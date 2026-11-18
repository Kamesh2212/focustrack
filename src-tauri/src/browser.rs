//! Local API for the FocusTrack browser extensions (SRS 2.3, 2.6).
//!
//! The extension POSTs the active tab's *domain* (never URL paths, titles or
//! page content) to `http://127.0.0.1:17865/v1/heartbeat` whenever the active
//! tab or window focus changes, and every 30 s. The response carries the list
//! of domains to block (website limits with blocking enabled).
//!
//! Only requests from browser extensions are accepted:
//!   * bound to 127.0.0.1 only;
//!   * `Host` must be 127.0.0.1/localhost (blocks DNS rebinding);
//!   * an `Origin` header, if present, must be an extension origin;
//!   * heartbeats need an `X-FocusTrack` header, which forces a CORS preflight
//!     that ordinary web pages cannot pass.

use crate::tracker::Shared;
use chrono::{Local, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Method, Request, Response, Server};

pub const PORT: u16 = 17865;
/// A report older than this is ignored (extensions resend every 30 s).
pub const FRESH_SECS: i64 = 75;
pub const BROWSERS: &[&str] = &["chrome", "edge", "brave", "firefox", "opera", "vivaldi"];

#[derive(Debug, Clone)]
pub struct BrowserReport {
    pub domain: Option<String>,
    pub focused: bool,
    pub at: NaiveDateTime,
}

#[derive(Deserialize)]
struct Heartbeat {
    browser: String,
    domain: Option<String>,
    #[serde(default)]
    focused: bool,
}

#[derive(Serialize)]
struct HeartbeatReply<'a> {
    tracking: bool,
    blocked: &'a [String],
}

/// Lower-cases, strips `www.` and trailing dots, and rejects anything that is
/// not a plain host name.
pub fn normalize_domain(raw: &str) -> Option<String> {
    let d = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let d = d.strip_prefix("www.").unwrap_or(&d).to_string();
    let valid = !d.is_empty()
        && d.len() <= 253
        && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !d.starts_with('.')
        && !d.contains("..");
    valid.then_some(d)
}

fn json_response(status: u16, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let ct: Header = "Content-Type: application/json".parse().expect("static header");
    Response::from_string(body).with_status_code(status).with_header(ct)
}

fn header(req: &Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

fn now() -> NaiveDateTime {
    let t = Local::now().naive_local();
    t.with_nanosecond(0).unwrap_or(t)
}

fn handle(mut req: Request, shared: &Shared) {
    let origin_ok = header(&req, "Origin").map_or(true, |o| {
        o.starts_with("chrome-extension://") || o.starts_with("moz-extension://") || o.starts_with("extension://")
    });
    let host_ok = header(&req, "Host").map_or(false, |h| {
        h == format!("127.0.0.1:{PORT}") || h == format!("localhost:{PORT}")
    });
    if !origin_ok || !host_ok {
        let _ = req.respond(json_response(403, r#"{"error":"forbidden"}"#.into()));
        return;
    }

    let method = req.method().clone();
    let url = req.url().to_string();
    match (method, url.as_str()) {
        (Method::Get, "/v1/status") => {
            let body = serde_json::json!({
                "app": "focustrack",
                "version": env!("CARGO_PKG_VERSION"),
                "tracking": shared.website_tracking.load(Ordering::SeqCst),
            });
            let _ = req.respond(json_response(200, body.to_string()));
        }
        (Method::Post, "/v1/heartbeat") => {
            if header(&req, "X-FocusTrack").is_none() {
                let _ = req.respond(json_response(403, r#"{"error":"missing header"}"#.into()));
                return;
            }
            let mut body = String::new();
            if req.as_reader().take(16 * 1024).read_to_string(&mut body).is_err() {
                let _ = req.respond(json_response(400, r#"{"error":"bad body"}"#.into()));
                return;
            }
            let beat: Heartbeat = match serde_json::from_str(&body) {
                Ok(b) => b,
                Err(_) => {
                    let _ = req.respond(json_response(400, r#"{"error":"bad json"}"#.into()));
                    return;
                }
            };
            if !BROWSERS.contains(&beat.browser.as_str()) {
                let _ = req.respond(json_response(400, r#"{"error":"unknown browser"}"#.into()));
                return;
            }
            let tracking = shared.website_tracking.load(Ordering::SeqCst);
            if let Ok(mut map) = shared.browsers.lock() {
                let report = BrowserReport {
                    domain: if tracking { beat.domain.as_deref().and_then(normalize_domain) } else { None },
                    focused: beat.focused,
                    at: now(),
                };
                map.insert(beat.browser, report);
            }
            let blocked = shared.blocklist.lock().map(|b| b.clone()).unwrap_or_default();
            let reply = HeartbeatReply { tracking, blocked: &blocked };
            let _ = req.respond(json_response(200, serde_json::to_string(&reply).unwrap_or_default()));
        }
        // No CORS headers: web pages' preflights fail, which is the point.
        (Method::Options, _) => {
            let _ = req.respond(Response::empty(204));
        }
        _ => {
            let _ = req.respond(json_response(404, r#"{"error":"not found"}"#.into()));
        }
    }
}

pub fn spawn_server(shared: Arc<Shared>) {
    thread::Builder::new()
        .name("focustrack-browser-api".into())
        .spawn(move || {
            let server = match Server::http(("127.0.0.1", PORT)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[browser] cannot listen on 127.0.0.1:{PORT}: {e}");
                    return;
                }
            };
            shared.server_running.store(true, Ordering::SeqCst);
            for req in server.incoming_requests() {
                handle(req, &shared);
            }
            shared.server_running.store(false, Ordering::SeqCst);
        })
        .expect("failed to spawn browser API thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_normalized() {
        assert_eq!(normalize_domain("WWW.YouTube.com").as_deref(), Some("youtube.com"));
        assert_eq!(normalize_domain("github.com.").as_deref(), Some("github.com"));
        assert_eq!(normalize_domain("localhost").as_deref(), Some("localhost"));
        assert_eq!(normalize_domain(""), None);
        assert_eq!(normalize_domain("evil.com/path"), None);
        assert_eq!(normalize_domain("a..b"), None);
        assert_eq!(normalize_domain("<script>"), None);
    }
}
