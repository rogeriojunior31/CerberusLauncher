//! One owner for each account's session. The library and the achievements
//! used to renew the same refresh token each on their own; a store that
//! retires the old refresh token when it hands a new one (Epic and Microsoft
//! rotate them) then saw one of the two fail, and the account looked signed
//! out. Here the session is kept with the refresh token that came with it, one
//! renewal runs at a time (the others wait for it and get its session), and a
//! different refresh token (signed out, another account) never gets the old one.

use std::future::Future;
use std::time::{Duration, Instant};

/// A session kept for a while, renewed by one caller at a time.
pub struct Kept<T> {
    inner: tokio::sync::Mutex<Option<(Instant, String, T)>>,
}

impl<T: Clone> Kept<T> {
    pub const fn new() -> Self {
        Self { inner: tokio::sync::Mutex::const_new(None) }
    }

    /// The kept session when it came from `refresh` (the refresh token the
    /// account has now) and is younger than `fresh_for`; else `renew` is
    /// called, once, and its session and new refresh token are kept.
    pub async fn get<F, Fut>(&self, refresh: &str, fresh_for: Duration, renew: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(T, String), String>>,
    {
        // Held while renewing: whoever asks meanwhile waits, then finds it kept.
        let mut kept = self.inner.lock().await;
        if let Some((at, from, session)) = kept.as_ref() {
            if from == refresh && at.elapsed() < fresh_for {
                return Ok(session.clone());
            }
        }
        let (session, next) = renew().await?;
        *kept = Some((Instant::now(), next, session.clone()));
        Ok(session)
    }
}

impl<T: Send + 'static> Kept<T> {
    /// Forgets the kept session (signed out).
    pub fn forget(&'static self) {
        if let Ok(mut kept) = self.inner.try_lock() {
            *kept = None;
        } else {
            // Renewing right now: forgotten as soon as it ends (never waited for here).
            tauri::async_runtime::spawn(async move { *self.inner.lock().await = None });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kept;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[test]
    fn five_at_once_renew_once() {
        static KEPT: Kept<String> = Kept::new();
        static CALLS: AtomicU32 = AtomicU32::new(0);
        let ask = || async {
            KEPT.get("r1", Duration::from_secs(60), || async {
                CALLS.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(("access".to_string(), "r1".to_string()))
            })
            .await
        };
        let all = tauri::async_runtime::block_on(async { futures_util::future::join_all((0..5).map(|_| ask())).await });
        assert!(all.iter().all(|r| r.as_deref() == Ok("access")));
        assert_eq!(CALLS.load(Ordering::SeqCst), 1, "one renewal for five askers");
    }

    #[test]
    fn another_refresh_token_never_gets_the_old_session() {
        static KEPT: Kept<String> = Kept::new();
        let kept = &KEPT;
        let got = |refresh: &'static str, access: &'static str| {
            tauri::async_runtime::block_on(kept.get(refresh, Duration::from_secs(60), || async move { Ok((access.to_string(), refresh.to_string())) }))
        };
        assert_eq!(got("old", "a1").unwrap(), "a1");
        assert_eq!(got("old", "unused").unwrap(), "a1", "same account: kept");
        assert_eq!(got("other", "a2").unwrap(), "a2", "signed in with another account: renewed");
        kept.forget();
        assert_eq!(got("other", "a3").unwrap(), "a3", "signed out: forgotten");
    }
}
