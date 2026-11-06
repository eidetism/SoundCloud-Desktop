//! SoundCloud api-v2 client. Requests carry `client_id` (scraped from the web
//! page, refreshed on 401/403) and, when signed in, `Authorization: OAuth`.
//! All traffic goes through the shared client from `crate::net`.

mod client_id;
pub mod models;

use std::fmt;
use std::time::Duration;

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, RwLock};

use models::Me;

const API_BASE: &str = "https://api-v2.soundcloud.com";
const WEB_BASE: &str = "https://soundcloud.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// Delays before retrying a 429 / 5xx answer.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];
const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum ApiError {
    /// 401 even with a fresh `client_id`: the OAuth token is invalid.
    Unauthorized,
    Http(u16),
    Network(String),
    Decode(String),
    ClientId(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "unauthorized"),
            ApiError::Http(status) => write!(f, "HTTP {status}"),
            ApiError::Network(message) => write!(f, "network: {message}"),
            ApiError::Decode(message) => write!(f, "decode: {message}"),
            ApiError::ClientId(message) => write!(f, "client_id: {message}"),
        }
    }
}

#[derive(Default)]
pub struct Api {
    client_id: RwLock<Option<String>>,
    refresh: Mutex<()>,
}

impl Api {
    pub async fn me(&self, http: &reqwest::Client, token: &str) -> Result<Me, ApiError> {
        self.get_json(http, "/me", Some(token)).await
    }

    async fn client_id(&self, http: &reqwest::Client) -> Result<String, ApiError> {
        if let Some(id) = self.client_id.read().await.clone() {
            return Ok(id);
        }
        self.refresh_client_id(http, None).await
    }

    /// Re-scrape `client_id`. Concurrent callers coalesce: if another task
    /// already replaced `stale`, its result is reused.
    async fn refresh_client_id(
        &self,
        http: &reqwest::Client,
        stale: Option<&str>,
    ) -> Result<String, ApiError> {
        let _guard = self.refresh.lock().await;
        if let Some(current) = self.client_id.read().await.clone()
            && Some(current.as_str()) != stale
        {
            return Ok(current);
        }
        let html = http
            .get(WEB_BASE)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let id = client_id::extract(&html)
            .ok_or_else(|| ApiError::ClientId("not found on soundcloud.com".into()))?;
        *self.client_id.write().await = Some(id.clone());
        Ok(id)
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        http: &reqwest::Client,
        path: &str,
        token: Option<&str>,
    ) -> Result<T, ApiError> {
        let mut client_id = self.client_id(http).await?;
        let mut refreshed = false;
        let mut retries = RETRY_DELAYS.iter();
        loop {
            let mut request = http
                .get(format!("{API_BASE}{path}"))
                .query(&[("client_id", client_id.as_str())])
                .timeout(REQUEST_TIMEOUT);
            if let Some(token) = token {
                request = request.header("Authorization", format!("OAuth {token}"));
            }
            let response = request
                .send()
                .await
                .map_err(|e| ApiError::Network(e.to_string()))?;
            let status = response.status();
            if status.is_success() {
                let body = response
                    .bytes()
                    .await
                    .map_err(|e| ApiError::Network(e.to_string()))?;
                return serde_json::from_slice(&body).map_err(|e| ApiError::Decode(e.to_string()));
            }
            match status {
                // A stale client_id also answers 401/403: refresh it once first.
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN if !refreshed => {
                    refreshed = true;
                    client_id = self.refresh_client_id(http, Some(&client_id)).await?;
                }
                StatusCode::UNAUTHORIZED => return Err(ApiError::Unauthorized),
                StatusCode::TOO_MANY_REQUESTS => match retries.next() {
                    Some(delay) => {
                        tokio::time::sleep(retry_after(&response).unwrap_or(*delay)).await
                    }
                    None => return Err(ApiError::Http(status.as_u16())),
                },
                _ if status.is_server_error() => match retries.next() {
                    Some(delay) => tokio::time::sleep(*delay).await,
                    None => return Err(ApiError::Http(status.as_u16())),
                },
                _ => return Err(ApiError::Http(status.as_u16())),
            }
        }
    }
}

/// `Retry-After` in seconds, capped so a hostile value can't stall the app.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let seconds = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}
