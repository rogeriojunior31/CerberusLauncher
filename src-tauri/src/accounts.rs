//! One owner for each account's session. The library and the achievements
//! used to renew the same refresh token each on their own; a store that
//! retires the old refresh token when it hands a new one (Epic and Microsoft
//! rotate them) then saw one of the two fail, and the account looked signed
//! out. Here the session is kept with the refresh token that came with it, one
//! renewal runs at a time (the others wait for it and get its session), and a
//! different refresh token (signed out, another account) never gets the old one.

use std::future::Future;

use cerberus_shared::tr;

/// Why a store didn't hand a session, by its answer: only a token taken back
/// or out of date (400, 401, 403) asks the player to sign in again; a pause
/// asked (429) or the store down (5xx) is a wait, and the account stays.
/// `store` is the store's name, `where_` where to sign in again.
pub fn refusal(store: &str, where_: &str, status: u16) -> String {
    match status {
        429 => tr!("A {} pediu uma pausa. O Cerberus tenta de novo mais tarde, sem sair da conta.", store),
        500..=599 => tr!("A {} está fora do ar agora ({}). O Cerberus tenta de novo mais tarde, sem sair da conta.", store, status),
        _ => crate::achievements::needs_login(&tr!("A {} recusou o acesso: entre de novo em {}.", store, where_)),
    }
}
use std::time::{Duration, Instant};

/// A session kept for a while, renewed by one caller at a time.
pub struct Kept<T> {
    /// When renewed, the refresh token used, the one it gave, and the session.
    inner: tokio::sync::Mutex<Option<(Instant, String, String, T)>>,
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
        if let Some((at, used, next, session)) = kept.as_ref() {
            // The token read before the renewal that rotated it (`used`) is the
            // same account as the new one: renewing with it again would be refused.
            if (refresh == next || refresh == used) && at.elapsed() < fresh_for {
                return Ok(session.clone());
            }
        }
        let (session, next) = renew().await?;
        *kept = Some((Instant::now(), refresh.to_string(), next, session.clone()));
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
    fn only_a_token_taken_back_asks_to_sign_in_again() {
        use crate::achievements::needs_login;
        let again = |status| super::refusal("Epic", "Configurações", status);
        for status in [400, 401, 403] {
            assert!(again(status).starts_with(&needs_login("")), "{status}: sign in again");
        }
        for status in [429, 500, 503] {
            assert!(!again(status).starts_with(&needs_login("")), "{status}: a wait, the account stays");
        }
        assert!(again(503).contains("503"));
    }

    #[test]
    fn askers_holding_the_rotated_token_get_the_new_session() {
        static KEPT: Kept<String> = Kept::new();
        static CALLS: AtomicU32 = AtomicU32::new(0);
        // Both read "r1" before the first renewal rotated it to "r2".
        let ask = |refresh: &'static str| async move {
            KEPT.get(refresh, Duration::from_secs(60), || async {
                CALLS.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(("access".to_string(), "r2".to_string()))
            })
            .await
        };
        let all = tauri::async_runtime::block_on(async { futures_util::future::join_all([ask("r1"), ask("r1"), ask("r2")]).await });
        assert!(all.iter().all(|r| r.as_deref() == Ok("access")));
        assert_eq!(CALLS.load(Ordering::SeqCst), 1, "the old token isn't renewed a second time");
    }

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
