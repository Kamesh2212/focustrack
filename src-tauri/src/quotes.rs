//! Motivational quotes shown when something is blocked (#49). The list lives in
//! `quotes/quotes.json` (one file, built into the app); the browser extension
//! gets its quote from the app (`GET /v1/quote`), so the two never drift apart.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const RAW: &str = include_str!("../../quotes/quotes.json");

/// How many recent quotes are kept out of the next pick.
const RECENT: usize = 5;

#[cfg(test)]
const THEMES: &[&str] = &[
    "consistency",
    "hard work",
    "discipline",
    "focus",
    "patience",
    "learning",
    "failure",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Quote {
    pub text: String,
    pub author: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub theme: String,
}

/// The "Show motivational quotes" setting (on by default).
static ENABLED: AtomicBool = AtomicBool::new(true);
static RECENTLY: Mutex<VecDeque<usize>> = Mutex::new(VecDeque::new());

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::SeqCst);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub fn all() -> &'static [Quote] {
    static LIST: OnceLock<Vec<Quote>> = OnceLock::new();
    LIST.get_or_init(|| serde_json::from_str(RAW).unwrap_or_default())
}

fn random() -> u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish()
}

/// Picks from `list` at random, skipping the indices in `recent` while
/// anything else fits; only quotes of at most `max_len` characters.
fn pick_from(list: &[Quote], recent: &VecDeque<usize>, max_len: usize, r: u64) -> Option<usize> {
    let fits: Vec<usize> = (0..list.len())
        .filter(|&i| list[i].text.chars().count() + list[i].author.chars().count() + 4 <= max_len)
        .collect();
    let fresh: Vec<usize> = fits
        .iter()
        .copied()
        .filter(|i| !recent.contains(i))
        .collect();
    let from = if fresh.is_empty() { &fits } else { &fresh };
    if from.is_empty() {
        None
    } else {
        Some(from[(r % from.len() as u64) as usize])
    }
}

/// A random quote (text plus author within `max_len` characters), never the
/// same as the last few shown. None when turned off or nothing fits.
pub fn pick(max_len: usize) -> Option<Quote> {
    if !enabled() {
        return None;
    }
    let list = all();
    let mut recent = RECENTLY.lock().ok()?;
    let i = pick_from(list, &recent, max_len, random())?;
    recent.push_back(i);
    while recent.len() > RECENT.min(list.len().saturating_sub(1)) {
        recent.pop_front();
    }
    Some(list[i].clone())
}

/// For a notification: `"Text" — Author`, or nothing.
pub fn for_notification() -> Option<String> {
    pick(120).map(|q| format!("\u{201c}{}\u{201d} \u{2014} {}", q.text, q.author))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
    }

    /// The checks #49 asks for: valid JSON, complete entries, no duplicates.
    #[test]
    fn the_quote_file_is_valid() {
        let list: Vec<Quote> = serde_json::from_str(RAW).expect("quotes/quotes.json is valid JSON");
        assert!(list.len() >= 25, "batch 1 has 25 quotes");
        let mut seen = std::collections::HashSet::new();
        let mut short = 0;
        for q in &list {
            assert!(
                !q.text.trim().is_empty() && !q.author.trim().is_empty(),
                "{q:?}"
            );
            assert!(q.text.chars().count() <= 200, "too long: {}", q.text);
            assert!(q.source.contains("http"), "needs a source link: {}", q.text);
            assert!(
                THEMES.contains(&q.theme.as_str()),
                "unknown theme {:?}",
                q.theme
            );
            assert!(seen.insert(normal(&q.text)), "duplicate: {}", q.text);
            if q.text.chars().count() + q.author.chars().count() + 4 <= 120 {
                short += 1;
            }
        }
        assert!(short >= 5, "some quotes must fit a notification");
        for t in THEMES {
            assert!(
                list.iter().filter(|q| q.theme == *t).count() <= 8,
                "too many {t}"
            );
        }
    }

    #[test]
    fn never_the_same_quote_twice_in_a_row() {
        let list = all();
        let mut recent = VecDeque::new();
        let mut last = None;
        for r in 0..200u64 {
            let i = pick_from(list, &recent, 1000, r.wrapping_mul(2654435761)).unwrap();
            assert_ne!(Some(i), last);
            recent.push_back(i);
            if recent.len() > RECENT {
                recent.pop_front();
            }
            last = Some(i);
        }
        // Notifications only get quotes that fit.
        let i = pick_from(list, &VecDeque::new(), 120, 7).unwrap();
        assert!(list[i].text.chars().count() + list[i].author.chars().count() + 4 <= 120);
        // Nothing fits: no quote.
        assert_eq!(pick_from(list, &VecDeque::new(), 10, 7), None);
        assert_eq!(pick_from(&[], &VecDeque::new(), 500, 7), None);
    }
}
