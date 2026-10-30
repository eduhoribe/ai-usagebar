use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cache::{Cache, acquire_lock_async};
use crate::error::{AppError, Result};
use crate::vendor::{MAX_BODY_BYTES, read_body_capped};

use super::types::{Snapshot, parse_response};

// ProductionEnvironment in Junie CLI 3893.6. Never take the host from a token.
pub const BASE_URL: &str = "https://ingrazzio-cloud-prod.labs.jb.gg";
pub type FetchOutcome = crate::outcome::Outcome<Snapshot>;

#[derive(Serialize, Deserialize)]
struct Cached {
    account: String,
    snapshot: Snapshot,
}

pub async fn fetch_snapshot(api_key: &str, cache: &Cache, ttl: Duration) -> Result<FetchOutcome> {
    fetch_snapshot_at(api_key, cache, ttl, BASE_URL).await
}

/// Injected endpoint and cache; tests never access a real login or service.
async fn fetch_snapshot_at(
    api_key: &str,
    cache: &Cache,
    ttl: Duration,
    base: &str,
) -> Result<FetchOutcome> {
    cache.ensure_dir()?;
    let _lock = acquire_lock_async(&cache.lock_path(), Duration::from_secs(15)).await?;
    let account = crate::cache::fingerprint_of(api_key);
    let parse_cache = |bytes: &[u8]| -> Result<Snapshot> {
        let cached: Cached = serde_json::from_slice(bytes)
            .map_err(|_| AppError::Schema("Junie balance cache is invalid".into()))?;
        if cached.account != account {
            return Err(AppError::Schema(
                "Junie cache belongs to a different key".into(),
            ));
        }
        cached.snapshot.validate()
    };
    if let Some(bytes) = cache.fresh_payload(ttl)?
        && let Ok(snapshot) = parse_cache(&bytes)
    {
        return Ok(crate::outcome::Outcome::cached(snapshot, cache, false));
    }
    match request(api_key, base).await {
        Ok(snapshot) => {
            cache.write_payload(&serde_json::to_vec(&Cached {
                account,
                snapshot: snapshot.clone(),
            })?)?;
            Ok(crate::outcome::Outcome::fresh(snapshot))
        }
        Err(error) if error.is_transient() => {
            crate::outcome::fallback(cache, None, error, parse_cache)
        }
        Err(error) => {
            cache.mark_stale();
            let status = match &error {
                AppError::Http { status, .. } => *status,
                _ => 0,
            };
            let diagnostic = cache.write_last_error(status, &error.to_string());
            crate::outcome::fallback(cache, Some(diagnostic), error, parse_cache)
        }
    }
}

async fn request(key: &str, base: &str) -> Result<Snapshot> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .get(format!("{}/auth/test", base.trim_end_matches('/')))
        .bearer_auth(key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| AppError::Transport("Junie balance request failed".into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Http {
            status: status.as_u16(),
            body: if matches!(status.as_u16(), 401 | 403) {
                "Junie key rejected; create a Junie API key at https://junie.jetbrains.com/cli"
                    .into()
            } else {
                format!("Junie balance endpoint returned HTTP {}", status.as_u16())
            },
        });
    }
    parse_response(&read_body_capped(response, MAX_BODY_BYTES).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "junie-test-canary";
    const BODY: &str = r#"{"balanceLeft":42.5,"balanceUnit":"USD","licenseType":"AIP","username":"private-canary","token":"private-canary"}"#;

    #[tokio::test]
    async fn caches_only_projected_metrics_and_reuses_the_fresh_payload() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/auth/test")
            .match_header("authorization", format!("Bearer {KEY}").as_str())
            .with_body(BODY)
            .expect(1)
            .create_async()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("junie"));
        for _ in 0..2 {
            let outcome = fetch_snapshot_at(KEY, &cache, Duration::from_secs(60), &server.url())
                .await
                .unwrap();
            assert_eq!(outcome.snapshot.balance, 42.5);
        }
        mock.assert_async().await;
        let payload = String::from_utf8(cache.maybe_payload().unwrap().unwrap()).unwrap();
        assert!(!payload.contains(KEY));
        assert!(!payload.contains("private-canary"));
    }

    #[tokio::test]
    async fn failures_keep_the_last_good_balance_but_never_cross_keys() {
        let mut server = mockito::Server::new_async().await;
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("junie"));
        let good = server
            .mock("GET", "/auth/test")
            .with_body(BODY)
            .create_async()
            .await;
        fetch_snapshot_at(KEY, &cache, Duration::ZERO, &server.url())
            .await
            .unwrap();
        good.remove_async().await;
        let bad = server
            .mock("GET", "/auth/test")
            .with_status(503)
            .expect(2)
            .with_body(KEY)
            .create_async()
            .await;
        let outcome = fetch_snapshot_at(KEY, &cache, Duration::ZERO, &server.url())
            .await
            .unwrap();
        assert_eq!(outcome.snapshot.balance, 42.5);
        assert!(outcome.stale);
        let error = fetch_snapshot_at("other-key", &cache, Duration::from_secs(60), &server.url())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("503"));
        assert!(!error.to_string().contains(KEY));
        bad.assert_async().await;
    }

    #[tokio::test]
    async fn redirects_and_auth_errors_never_expose_response_text() {
        let mut server = mockito::Server::new_async().await;
        let mut target = mockito::Server::new_async().await;
        let leak = target.mock("GET", "/leak").expect(0).create_async().await;
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path().join("junie"));
        for status in [302, 401, 403, 429] {
            let mock = server
                .mock("GET", "/auth/test")
                .with_status(status)
                .with_header("location", &format!("{}/leak", target.url()))
                .with_body(KEY)
                .create_async()
                .await;
            let error = fetch_snapshot_at(KEY, &cache, Duration::ZERO, &server.url())
                .await
                .unwrap_err();
            assert!(!error.to_string().contains(KEY));
            mock.assert_async().await;
            mock.remove_async().await;
        }
        leak.assert_async().await;
    }
}
