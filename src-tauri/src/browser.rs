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
#[serde(rename_all = "camelCase")]
struct HeartbeatReply<'a> {
    tracking: bool,
    /// Blocked because a daily limit was reached.
    blocked: &'a [String],
    /// Blocked because a focus session is running.
    focus_blocked: &'a [String],
    /// "Allow only" sessions: every other site is blocked.
    focus_allow_only: Option<&'a [String]>,
    focus_ends_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppRow {
    name: String,
    category: String,
    color: String,
    secs: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SiteRow {
    domain: String,
    category: String,
    color: String,
    secs: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    date: String,
    screen_secs: i64,
    active_secs: i64,
    web_secs: i64,
    top_sites: Vec<SiteRow>,
    top_apps: Vec<AppRow>,
    productive_secs: i64,
    neutral_secs: i64,
    distracting_secs: i64,
    limits: Vec<crate::limits::LimitStatus>,
    goals: Vec<crate::goals::GoalStatus>,
    focus: Option<crate::focus::FocusStatus>,
    focus_today_secs: i64,
}

/// Today's usage for the extension's popup and new-tab page.
fn summary(conn: &rusqlite::Connection, shared: &Shared) -> rusqlite::Result<Summary> {
    use crate::analytics as an;
    let day = an::fmt_day(now().date());
    let (active, idle) = an::totals(conn, &day, &day)?;
    let sites = an::site_usage(conn, &day, &day)?;
    let (focus_secs, _) = crate::focus::totals(conn, &day, &day)?;
    let cats = an::category_usage(conn, &day, &day)?;
    let (productive, distracting) = an::productive_split(&cats);
    Ok(Summary {
        top_apps: an::app_usage(conn, &day, &day)?
            .into_iter()
            .filter(|a| a.secs > 0)
            .take(8)
            .map(|a| AppRow {
                name: a.name,
                category: a.category,
                color: a.color,
                secs: a.secs,
            })
            .collect(),
        productive_secs: productive,
        distracting_secs: distracting,
        neutral_secs: (active - productive - distracting).max(0),
        screen_secs: active + idle,
        active_secs: active,
        web_secs: an::site_total(conn, &day, &day)?,
        top_sites: sites
            .into_iter()
            .filter(|s| s.secs > 0)
            .take(8)
            .map(|s| SiteRow {
                domain: s.domain,
                category: s.category,
                color: s.color,
                secs: s.secs,
            })
            .collect(),
        limits: crate::limits::limit_status(conn, &day)?
            .into_iter()
            .filter(|l| l.enabled)
            .collect(),
        goals: crate::goals::goal_status(conn, &day)?
            .into_iter()
            .filter(|g| g.enabled)
            .collect(),
        focus: crate::focus::status(shared),
        focus_today_secs: focus_secs,
        date: day,
    })
}

/// Lower-cases, strips `www.` and trailing dots, and rejects anything that is
/// not a plain host name.
pub fn normalize_domain(raw: &str) -> Option<String> {
    let d = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let d = d.strip_prefix("www.").unwrap_or(&d).to_string();
    let valid = !d.is_empty()
        && d.len() <= 253
        && d.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !d.starts_with('.')
        && !d.contains("..");
    valid.then_some(d)
}

/// Browser-extension origins we talk to.
fn is_extension_origin(o: &str) -> bool {
    o.starts_with("chrome-extension://")
        || o.starts_with("moz-extension://")
        || o.starts_with("extension://")
}

/// Which `Origin` values may use the API. Absent (non-browser or privileged
/// extension request) and `null` (Firefox can send an opaque origin for
/// extension requests) are allowed: every data endpoint also requires the
/// `X-FocusTrack` header, which forces a CORS preflight that web pages can't
/// pass because we only answer it for extension origins.
pub fn origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(o) => o == "null" || is_extension_origin(o),
    }
}

/// The origin to echo in CORS headers: extension origins only, never web pages.
pub fn cors_origin(origin: Option<&str>) -> Option<String> {
    origin
        .filter(|o| is_extension_origin(o))
        .map(str::to_string)
}

fn header_line(s: &str) -> Header {
    s.parse().expect("static header")
}

fn with_cors<R: std::io::Read>(
    mut r: Response<R>,
    cors: &Option<String>,
    preflight: bool,
) -> Response<R> {
    if let Some(o) = cors {
        r = r.with_header(header_line(&format!("Access-Control-Allow-Origin: {o}")));
        r = r.with_header(header_line("Vary: Origin"));
        if preflight {
            r = r
                .with_header(header_line(
                    "Access-Control-Allow-Methods: GET, POST, OPTIONS",
                ))
                .with_header(header_line(
                    "Access-Control-Allow-Headers: Content-Type, X-FocusTrack",
                ))
                .with_header(header_line("Access-Control-Allow-Private-Network: true"))
                .with_header(header_line("Access-Control-Max-Age: 600"));
        }
    }
    r
}

fn json_response(
    cors: &Option<String>,
    status: u16,
    body: String,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let ct: Header = "Content-Type: application/json"
        .parse()
        .expect("static header");
    let r = Response::from_string(body)
        .with_status_code(status)
        .with_header(ct);
    with_cors(r, cors, false)
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

fn handle(mut req: Request, shared: &Shared, conn: Option<&rusqlite::Connection>) {
    let origin = header(&req, "Origin");
    let origin_ok = origin_allowed(origin.as_deref());
    let cors = cors_origin(origin.as_deref());
    let host_ok = header(&req, "Host")
        .is_some_and(|h| h == format!("127.0.0.1:{PORT}") || h == format!("localhost:{PORT}"));
    #[cfg(debug_assertions)]
    eprintln!(
        "[browser] {} {} origin={:?} ok={}",
        req.method(),
        req.url(),
        header(&req, "Origin").map(|o| o.split("://").next().unwrap_or("").to_string()),
        origin_ok && host_ok
    );
    if !origin_ok || !host_ok {
        let _ = req.respond(json_response(&cors, 403, r#"{"error":"forbidden"}"#.into()));
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
            let _ = req.respond(json_response(&cors, 200, body.to_string()));
        }
        (Method::Get, "/v1/summary") => {
            // Usage data: extension-only (the header forces a CORS preflight for web pages).
            if header(&req, "X-FocusTrack").is_none() {
                let _ = req.respond(json_response(
                    &cors,
                    403,
                    r#"{"error":"missing header"}"#.into(),
                ));
                return;
            }
            let body = conn
                .ok_or_else(|| "database unavailable".to_string())
                .and_then(|c| summary(c, shared).map_err(|e| e.to_string()))
                .and_then(|s| serde_json::to_string(&s).map_err(|e| e.to_string()));
            let _ = match body {
                Ok(b) => req.respond(json_response(&cors, 200, b)),
                Err(e) => req.respond(json_response(
                    &cors,
                    500,
                    serde_json::json!({ "error": e }).to_string(),
                )),
            };
        }
        (Method::Post, "/v1/goals/toggle") | (Method::Post, "/v1/notes") => {
            if header(&req, "X-FocusTrack").is_none() {
                let _ = req.respond(json_response(
                    &cors,
                    403,
                    r#"{"error":"missing header"}"#.into(),
                ));
                return;
            }
            let mut body = String::new();
            if req
                .as_reader()
                .take(32 * 1024)
                .read_to_string(&mut body)
                .is_err()
            {
                let _ = req.respond(json_response(&cors, 400, r#"{"error":"bad body"}"#.into()));
                return;
            }
            let Some(conn) = conn else {
                let _ = req.respond(json_response(
                    &cors,
                    500,
                    r#"{"error":"database unavailable"}"#.into(),
                ));
                return;
            };
            let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            let result = if url == "/v1/notes" {
                let text = v
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    Err("empty note".to_string())
                } else {
                    let title: String =
                        text.lines().next().unwrap_or("").chars().take(60).collect();
                    let session = shared
                        .focus
                        .lock()
                        .ok()
                        .and_then(|f| f.as_ref().map(|a| a.session_id));
                    crate::notes::create(
                        conn,
                        crate::notes::QUICK_NOTEBOOK,
                        &title,
                        &text,
                        session,
                        now(),
                    )
                    .map(|n| serde_json::json!({ "id": n.id }))
                }
            } else {
                match v.get("id").and_then(|i| i.as_i64()) {
                    Some(id) => crate::goals::toggle_check(
                        conn,
                        id,
                        &crate::analytics::fmt_day(now().date()),
                    )
                    .map(|done| serde_json::json!({ "done": done }))
                    .map_err(|e| e.to_string()),
                    None => Err("missing id".to_string()),
                }
            };
            let _ = match result {
                Ok(v) => req.respond(json_response(&cors, 200, v.to_string())),
                Err(e) => req.respond(json_response(
                    &cors,
                    400,
                    serde_json::json!({ "error": e }).to_string(),
                )),
            };
        }
        (Method::Post, "/v1/heartbeat") => {
            if header(&req, "X-FocusTrack").is_none() {
                let _ = req.respond(json_response(
                    &cors,
                    403,
                    r#"{"error":"missing header"}"#.into(),
                ));
                return;
            }
            let mut body = String::new();
            if req
                .as_reader()
                .take(16 * 1024)
                .read_to_string(&mut body)
                .is_err()
            {
                let _ = req.respond(json_response(&cors, 400, r#"{"error":"bad body"}"#.into()));
                return;
            }
            let beat: Heartbeat = match serde_json::from_str(&body) {
                Ok(b) => b,
                Err(_) => {
                    let _ =
                        req.respond(json_response(&cors, 400, r#"{"error":"bad json"}"#.into()));
                    return;
                }
            };
            if !BROWSERS.contains(&beat.browser.as_str()) {
                let _ = req.respond(json_response(
                    &cors,
                    400,
                    r#"{"error":"unknown browser"}"#.into(),
                ));
                return;
            }
            let tracking = shared.website_tracking.load(Ordering::SeqCst);
            if let Ok(mut map) = shared.browsers.lock() {
                let report = BrowserReport {
                    domain: if tracking {
                        beat.domain.as_deref().and_then(normalize_domain)
                    } else {
                        None
                    },
                    focused: beat.focused,
                    at: now(),
                };
                map.insert(beat.browser, report);
            }
            let blocked = shared
                .blocklist
                .lock()
                .map(|b| b.clone())
                .unwrap_or_default();
            let focus = crate::focus::status(shared);
            let focus_blocked = shared
                .focus_blocklist
                .lock()
                .map(|b| b.clone())
                .unwrap_or_default();
            let allow_only = shared.focus_allow_sites.lock().ok().and_then(|b| b.clone());
            let reply = HeartbeatReply {
                tracking,
                blocked: &blocked,
                focus_blocked: &focus_blocked,
                focus_allow_only: allow_only.as_deref(),
                focus_ends_at: focus.map(|f| f.plan_ends_at),
            };
            let _ = req.respond(json_response(
                &cors,
                200,
                serde_json::to_string(&reply).unwrap_or_default(),
            ));
        }
        // No CORS headers: web pages' preflights fail, which is the point.
        // Preflights get CORS headers only for extension origins, so web pages fail them.
        (Method::Options, _) => {
            let _ = req.respond(with_cors(Response::empty(204), &cors, true));
        }
        _ => {
            let _ = req.respond(json_response(&cors, 404, r#"{"error":"not found"}"#.into()));
        }
    }
}

fn serve(server: &Server, shared: &Shared, conn: Option<&rusqlite::Connection>) {
    for req in server.incoming_requests() {
        handle(req, shared, conn);
    }
}

pub fn spawn_server(shared: Arc<Shared>, db_path: std::path::PathBuf) {
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
            let conn = crate::db::open(&db_path)
                .map_err(|e| eprintln!("[browser] database: {e}"))
                .ok();
            serve(&server, &shared, conn.as_ref());
            shared.server_running.store(false, Ordering::SeqCst);
        })
        .expect("failed to spawn browser API thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// Runs the real request handler on a spare port and talks raw HTTP to it,
    /// the way each kind of client would.
    #[test]
    fn http_behaviour_for_extensions_and_web_pages() {
        use std::io::Write;
        let server = Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        thread::spawn(move || {
            let shared = Shared::new(false, 300);
            let conn = crate::db::open_in_memory().unwrap();
            serve(&server, &shared, Some(&conn));
        });
        let send = |raw: &str| -> String {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            s.write_all(raw.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out
        };
        let host = format!("Host: 127.0.0.1:{PORT}");
        let get = |origin: &str, extra: &str| {
            send(&format!(
                "GET /v1/summary HTTP/1.1\r\n{host}\r\n{origin}{extra}Connection: close\r\n\r\n"
            ))
        };

        // Firefox: opaque origin with the header -> allowed.
        assert!(get("Origin: null\r\n", "X-FocusTrack: 1\r\n").starts_with("HTTP/1.1 200"));
        // Chrome/Firefox extension origin -> allowed, and CORS echoes the origin.
        let r = get("Origin: moz-extension://abc\r\n", "X-FocusTrack: 1\r\n");
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
        assert!(
            r.contains("Access-Control-Allow-Origin: moz-extension://abc"),
            "{r}"
        );
        // Preflight from an extension gets CORS approval…
        let pre = |origin: &str| {
            send(&format!(
                "OPTIONS /v1/summary HTTP/1.1\r\n{host}\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: GET\r\nAccess-Control-Request-Headers: x-focustrack\r\nConnection: close\r\n\r\n"
            ))
        };
        let ok = pre("chrome-extension://abc");
        assert!(
            ok.contains("Access-Control-Allow-Headers: Content-Type, X-FocusTrack"),
            "{ok}"
        );
        // …but web pages don't, so browsers never send their real request.
        assert!(!pre("https://evil.example").contains("Access-Control-Allow-Origin"));
        assert!(!pre("null").contains("Access-Control-Allow-Origin"));
        // Direct web-page requests and missing headers are refused.
        assert!(
            get("Origin: https://evil.example\r\n", "X-FocusTrack: 1\r\n")
                .starts_with("HTTP/1.1 403")
        );
        assert!(get("Origin: null\r\n", "").starts_with("HTTP/1.1 403"));
        let rebinding = send("GET /v1/summary HTTP/1.1\r\nHost: evil.example:17865\r\nX-FocusTrack: 1\r\nConnection: close\r\n\r\n");
        assert!(rebinding.starts_with("HTTP/1.1 403"));
    }

    #[test]
    fn origins_and_cors() {
        assert!(origin_allowed(None));
        assert!(
            origin_allowed(Some("null")),
            "Firefox may send an opaque origin"
        );
        assert!(origin_allowed(Some("moz-extension://abc")));
        assert!(origin_allowed(Some("chrome-extension://abc")));
        assert!(!origin_allowed(Some("https://evil.example")));
        assert!(!origin_allowed(Some("http://localhost:3000")));
        assert_eq!(
            cors_origin(Some("moz-extension://abc")).as_deref(),
            Some("moz-extension://abc")
        );
        assert_eq!(
            cors_origin(Some("null")),
            None,
            "never grant CORS to opaque origins"
        );
        assert_eq!(cors_origin(Some("https://evil.example")), None);
        assert_eq!(cors_origin(None), None);
    }

    #[test]
    fn domains_are_normalized() {
        assert_eq!(
            normalize_domain("WWW.YouTube.com").as_deref(),
            Some("youtube.com")
        );
        assert_eq!(
            normalize_domain("github.com.").as_deref(),
            Some("github.com")
        );
        assert_eq!(normalize_domain("localhost").as_deref(), Some("localhost"));
        assert_eq!(normalize_domain(""), None);
        assert_eq!(normalize_domain("evil.com/path"), None);
        assert_eq!(normalize_domain("a..b"), None);
        assert_eq!(normalize_domain("<script>"), None);
    }
}
