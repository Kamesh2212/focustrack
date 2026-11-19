//! Hybrid logical clock (design §6.7).
//!
//! Shared definitions (limits, goals, …) are "last writer wins". Comparing
//! plain wall-clock times would let a device whose clock runs fast win every
//! conflict. An HLC never goes backwards and never falls behind any timestamp
//! the device has seen, so later edits always compare as later, even across
//! devices with clocks that disagree.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hlc {
    /// Milliseconds since the Unix epoch (physical part).
    pub wall_ms: u64,
    /// Breaks ties within the same millisecond.
    pub counter: u32,
    /// Device ID, the final tie-breaker so two devices never produce equal stamps.
    pub device: String,
}

impl Hlc {
    pub fn parse(s: &str) -> Option<Hlc> {
        let mut parts = s.splitn(3, ':');
        let wall_ms = parts.next()?.parse().ok()?;
        let counter = parts.next()?.parse().ok()?;
        let device = parts.next()?.to_string();
        if device.is_empty() {
            return None;
        }
        Some(Hlc {
            wall_ms,
            counter,
            device,
        })
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.wall_ms, self.counter, self.device)
    }
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.wall_ms, self.counter, &self.device).cmp(&(
            other.wall_ms,
            other.counter,
            &other.device,
        ))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares two HLC strings; unparsable stamps lose.
pub fn newer(a: &str, b: &str) -> bool {
    match (Hlc::parse(a), Hlc::parse(b)) {
        (Some(a), Some(b)) => a > b,
        (Some(_), None) => true,
        _ => false,
    }
}

/// A device's clock.
#[derive(Debug, Clone)]
pub struct Clock {
    device: String,
    last_wall: u64,
    counter: u32,
}

impl Clock {
    pub fn new(device: &str) -> Clock {
        Clock {
            device: device.to_string(),
            last_wall: 0,
            counter: 0,
        }
    }

    /// Restores a clock from the newest stamp it issued or saw.
    pub fn resume(device: &str, last: Option<&Hlc>) -> Clock {
        let mut c = Clock::new(device);
        if let Some(h) = last {
            c.last_wall = h.wall_ms;
            c.counter = h.counter;
        }
        c
    }

    /// A stamp for a local change at wall time `now_ms`.
    pub fn tick(&mut self, now_ms: u64) -> Hlc {
        if now_ms > self.last_wall {
            self.last_wall = now_ms;
            self.counter = 0;
        } else {
            self.counter += 1;
        }
        self.stamp()
    }

    /// Moves the clock past a stamp received from another device.
    pub fn observe(&mut self, remote: &Hlc, now_ms: u64) {
        let wall = now_ms.max(self.last_wall).max(remote.wall_ms);
        self.counter = if wall == self.last_wall && wall == remote.wall_ms {
            self.counter.max(remote.counter) + 1
        } else if wall == self.last_wall {
            self.counter + 1
        } else if wall == remote.wall_ms {
            remote.counter + 1
        } else {
            0
        };
        self.last_wall = wall;
    }

    fn stamp(&self) -> Hlc {
        Hlc {
            wall_ms: self.last_wall,
            counter: self.counter,
            device: self.device.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_orders() {
        let h = Hlc::parse("1795041234567:3:dev-a").unwrap();
        assert_eq!(h.to_string(), "1795041234567:3:dev-a");
        assert!(Hlc::parse("x:1:a").is_none());
        assert!(Hlc::parse("1:1:").is_none());
        assert!(newer("2:0:a", "1:9:z"));
        assert!(newer("1:1:a", "1:0:z"));
        assert!(newer("1:1:b", "1:1:a"));
        assert!(newer("1:0:a", "garbage"));
    }

    #[test]
    fn never_goes_backwards() {
        let mut c = Clock::new("a");
        let t1 = c.tick(1000);
        let t2 = c.tick(900); // wall clock moved back
        let t3 = c.tick(1000);
        assert!(t2 > t1 && t3 > t2);
    }

    #[test]
    fn edit_after_seeing_a_fast_clock_still_wins() {
        // Device B's clock is an hour fast.
        let mut b = Clock::new("b");
        let from_b = b.tick(1_000 + 3_600_000);
        // Device A sees B's edit, then the user edits on A.
        let mut a = Clock::new("a");
        a.observe(&from_b, 2_000);
        let from_a = a.tick(2_000);
        assert!(
            from_a > from_b,
            "the later edit wins despite B's fast clock"
        );
    }
}
