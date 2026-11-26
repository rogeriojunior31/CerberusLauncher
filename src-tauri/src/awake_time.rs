//! A clock that stops while the PC sleeps, for play time: a game left open
//! overnight with the PC suspended didn't play those hours. On Windows it is
//! `QueryUnbiasedInterruptTime` (100 ns units, suspend left out); elsewhere a
//! monotonic clock.

use std::ops::{Add, Sub};
use std::time::Duration;

/// A moment on the awake clock; it behaves like `std::time::Instant`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AwakeInstant(u64);

impl AwakeInstant {
    pub fn now() -> Self {
        Self(ticks())
    }

    pub fn elapsed(&self) -> Duration {
        Self::now().saturating_duration_since(*self)
    }

    pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
        from_ticks(self.0.saturating_sub(earlier.0))
    }
}

impl Add<Duration> for AwakeInstant {
    type Output = Self;
    fn add(self, d: Duration) -> Self {
        Self(self.0.saturating_add((d.as_nanos() / 100).min(u128::from(u64::MAX)) as u64))
    }
}

impl Sub<Duration> for AwakeInstant {
    type Output = Self;
    fn sub(self, d: Duration) -> Self {
        Self(self.0.saturating_sub((d.as_nanos() / 100).min(u128::from(u64::MAX)) as u64))
    }
}

/// A duration measured on the wall clock (`ran`), less the sleep within it:
/// over the same stretch the wall clock moved `wall` and the awake one `awake`.
pub fn without_sleep(ran: Duration, wall: Duration, awake: Duration) -> Duration {
    ran.saturating_sub(wall.saturating_sub(awake))
}

fn from_ticks(t: u64) -> Duration {
    Duration::from_secs(t / 10_000_000) + Duration::from_nanos(t % 10_000_000 * 100)
}

#[cfg(windows)]
fn ticks() -> u64 {
    use windows_sys::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime;
    let mut t = 0u64;
    // SAFETY: a plain out-parameter.
    unsafe { QueryUnbiasedInterruptTime(&mut t) };
    t
}

#[cfg(not(windows))]
fn ticks() -> u64 {
    use std::sync::OnceLock;
    static BASE: OnceLock<std::time::Instant> = OnceLock::new();
    (BASE.get_or_init(std::time::Instant::now).elapsed().as_nanos() / 100) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_night_asleep_is_not_played() {
        let h = |n: u64| Duration::from_secs(n * 3600);
        // 15 s of play and 8 h with the PC suspended, the game open.
        assert_eq!(without_sleep(h(8) + Duration::from_secs(15), h(8) + Duration::from_secs(15), Duration::from_secs(15)), Duration::from_secs(15));
        // Awake all along: all of it counts.
        assert_eq!(without_sleep(h(2), h(2), h(2)), h(2));
    }

    #[test]
    fn behaves_like_an_instant() {
        let a = AwakeInstant::now();
        std::thread::sleep(Duration::from_millis(30));
        let b = AwakeInstant::now();
        assert!(b > a && b.saturating_duration_since(a) >= Duration::from_millis(20));
        assert_eq!(a.saturating_duration_since(b), Duration::ZERO);
        assert_eq!((a + Duration::from_secs(15)).saturating_duration_since(a), Duration::from_secs(15));
        assert!(a.elapsed() >= Duration::from_millis(20));
    }
}
