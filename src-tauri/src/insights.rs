//! Usage insights (SRS 3.1 / Phase 4): plain-language observations computed
//! locally from your own data with simple rules. Nothing leaves the PC.

use crate::analytics::{self as an};
use crate::categories::browser_family;
use crate::focus;
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Insight {
    /// `good` | `warn` | `info`
    pub tone: &'static str,
    pub text: String,
}

fn fmt(secs: i64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn hour12(h: usize) -> String {
    match h % 24 {
        0 => "12 AM".into(),
        12 => "12 PM".into(),
        h if h < 12 => format!("{h} AM"),
        h => format!("{} PM", h - 12),
    }
}

/// Week-over-week change for one app or site, if it's large enough to mention.
pub fn change(name: &str, kind: &str, this: i64, prev: i64) -> Option<(i64, Insight)> {
    if prev < 30 * 60 && this < 30 * 60 {
        return None;
    }
    if prev == 0 {
        return None;
    }
    let pct = ((this - prev) as f64 / prev as f64 * 100.0).round() as i64;
    if pct.abs() < 25 || (this - prev).abs() < 20 * 60 {
        return None;
    }
    let up = pct > 0;
    let tone = match (kind, up) {
        ("distracting", true) | ("productive", false) => "warn",
        ("distracting", false) | ("productive", true) => "good",
        _ => "info",
    };
    let text = format!(
        "Your {name} usage {} by {}% this week ({} vs {}).",
        if up { "increased" } else { "decreased" },
        pct.abs(),
        fmt(this),
        fmt(prev)
    );
    Some(((this - prev).abs(), Insight { tone, text }))
}

/// Start hour of the busiest `width`-hour window.
pub fn peak_window(hours: &[i64], width: usize) -> Option<usize> {
    if hours.iter().sum::<i64>() == 0 || width == 0 || width > hours.len() {
        return None;
    }
    (0..=hours.len() - width).max_by_key(|&s| (hours[s..s + width].iter().sum::<i64>(), std::cmp::Reverse(s)))
}

pub fn insights(conn: &Connection, today: NaiveDate) -> rusqlite::Result<Vec<Insight>> {
    let d = |n: i64| an::fmt_day(today - Duration::days(n));
    let (wk_from, wk_to, pw_from, pw_to) = (d(6), d(0), d(13), d(7));
    let mut out = Vec::new();

    // 1. Biggest week-over-week changes across websites and (non-browser) apps.
    let mut changes: Vec<(i64, Insight)> = Vec::new();
    let prev_sites: HashMap<i64, i64> = an::site_usage(conn, &pw_from, &pw_to)?.into_iter().map(|s| (s.id, s.secs)).collect();
    for s in an::site_usage(conn, &wk_from, &wk_to)? {
        changes.extend(change(&s.domain, &s.kind, s.secs, *prev_sites.get(&s.id).unwrap_or(&0)));
    }
    let prev_apps: HashMap<i64, i64> = an::app_usage(conn, &pw_from, &pw_to)?.into_iter().map(|a| (a.id, a.secs)).collect();
    for a in an::app_usage(conn, &wk_from, &wk_to)? {
        if browser_family(&a.exe_name).is_none() {
            changes.extend(change(&a.name, &a.kind, a.secs, *prev_apps.get(&a.id).unwrap_or(&0)));
        }
    }
    changes.sort_by(|a, b| b.0.cmp(&a.0));
    out.extend(changes.into_iter().take(2).map(|(_, i)| i));

    // 2. When you're most active.
    let spans = an::active_spans(conn, &wk_from, &wk_to)?;
    let (hours, _) = an::hour_buckets(&spans);
    if hours.iter().sum::<i64>() >= 2 * 3600 {
        if let Some(start) = peak_window(&hours, 4) {
            out.push(Insight {
                tone: "info",
                text: format!("You spend most of your computer time between {} and {}.", hour12(start), hour12(start + 4)),
            });
        }
    }

    // 3. Distracting time this week.
    let cats = an::category_usage(conn, &wk_from, &wk_to)?;
    let distracting: i64 = cats.iter().filter(|c| c.kind == "distracting").map(|c| c.secs).sum();
    if distracting >= 3600 {
        out.push(Insight {
            tone: if distracting >= 7 * 3600 { "warn" } else { "info" },
            text: format!("You spent {} on entertainment, social media and gaming apps this week.", fmt(distracting)),
        });
    }

    // 4. Productive apps: this month vs last month.
    let (m_from, pm_from, pm_to) = (d(29), d(59), d(30));
    let prod = |from: &str, to: &str| -> rusqlite::Result<i64> {
        Ok(an::category_usage(conn, from, to)?.iter().filter(|c| c.kind == "productive").map(|c| c.secs).sum())
    };
    let (this_m, prev_m) = (prod(&m_from, &wk_to)?, prod(&pm_from, &pm_to)?);
    if prev_m >= 2 * 3600 {
        let pct = ((this_m - prev_m) as f64 / prev_m as f64 * 100.0).round() as i64;
        if pct.abs() >= 10 {
            out.push(Insight {
                tone: if pct > 0 { "good" } else { "warn" },
                text: format!(
                    "Your productive app usage {} by {}% this month.",
                    if pct > 0 { "increased" } else { "decreased" },
                    pct.abs()
                ),
            });
        }
    }

    // 5. Focus sessions this week and the current streak.
    let (focus_secs, focus_done) = focus::totals(conn, &wk_from, &wk_to)?;
    if focus_done > 0 {
        let streak = focus::streak(conn, today)?;
        let streak_text = if streak >= 2 { format!(" You're on a {streak}-day focus streak.") } else { String::new() };
        out.push(Insight {
            tone: "good",
            text: format!(
                "You completed {focus_done} focus session{} ({} focused) this week.{streak_text}",
                if focus_done == 1 { "" } else { "s" },
                fmt(focus_secs)
            ),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_thresholds_and_tone() {
        let (_, i) = change("youtube.com", "distracting", 5 * 3600, 3 * 3600).unwrap();
        assert_eq!(i.tone, "warn");
        assert_eq!(i.text, "Your youtube.com usage increased by 67% this week (5h vs 3h).");
        assert_eq!(change("github.com", "productive", 4 * 3600, 2 * 3600).unwrap().1.tone, "good");
        assert!(change("x", "neutral", 3600, 3500).is_none(), "small change");
        assert!(change("x", "neutral", 600, 300).is_none(), "too little time");
        assert!(change("x", "neutral", 3600, 0).is_none(), "no baseline");
    }

    #[test]
    fn peak_window_finds_busiest_hours() {
        let mut h = [0i64; 24];
        for (i, v) in [(14, 50), (15, 60), (16, 55), (17, 40), (9, 30)] {
            h[i] = v;
        }
        assert_eq!(peak_window(&h, 4), Some(14));
        assert_eq!(peak_window(&[0; 24], 4), None);
        assert_eq!(hour12(14), "2 PM");
        assert_eq!(hour12(18), "6 PM");
        assert_eq!(hour12(0), "12 AM");
    }

    #[test]
    fn runs_on_empty_database() {
        let conn = crate::db::open_in_memory().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert!(insights(&conn, today).unwrap().is_empty());
    }
}
