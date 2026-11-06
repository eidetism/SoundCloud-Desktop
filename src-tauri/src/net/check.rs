//! "Check connection": one request per SoundCloud host, reporting latency or
//! the error. Any HTTP status counts as reachable — these probe URLs answer
//! 401/403/404 by design; what matters is that the host responded.

use std::error::Error as _;
use std::fmt;
use std::time::{Duration, Instant};

use serde::Serialize;

const TIMEOUT: Duration = Duration::from_secs(10);
const TARGETS: [(&str, &str); 2] = [
    ("api-v2.soundcloud.com", "https://api-v2.soundcloud.com/"),
    ("i1.sndcdn.com", "https://i1.sndcdn.com/"),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    host: &'static str,
    /// HTTP status when the host answered.
    status: Option<u16>,
    /// Time to response headers, ms.
    latency_ms: Option<u64>,
    error: Option<CheckError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckError {
    kind: ErrorKind,
    message: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    Timeout,
    Connect,
    Other,
}

impl fmt::Display for CheckResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.error, self.status, self.latency_ms) {
            (Some(error), _, _) => write!(f, "{}: {:?} {}", self.host, error.kind, error.message),
            (None, Some(status), Some(ms)) => write!(f, "{}: HTTP {status} in {ms} ms", self.host),
            _ => write!(f, "{}: no result", self.host),
        }
    }
}

pub async fn run(client: &reqwest::Client, password: Option<&str>) -> Vec<CheckResult> {
    let probes = TARGETS.map(|(host, url)| probe(client, host, url, password));
    let [a, b] = probes;
    let (a, b) = tokio::join!(a, b);
    vec![a, b]
}

async fn probe(
    client: &reqwest::Client,
    host: &'static str,
    url: &str,
    password: Option<&str>,
) -> CheckResult {
    let started = Instant::now();
    match client.get(url).timeout(TIMEOUT).send().await {
        Ok(response) => CheckResult {
            host,
            status: Some(response.status().as_u16()),
            latency_ms: Some(started.elapsed().as_millis() as u64),
            error: None,
        },
        Err(error) => CheckResult {
            host,
            status: None,
            latency_ms: None,
            error: Some(CheckError {
                kind: kind_of(&error),
                message: scrub(&describe(&error), password),
            }),
        },
    }
}

fn kind_of(error: &reqwest::Error) -> ErrorKind {
    if error.is_timeout() {
        ErrorKind::Timeout
    } else if error.is_connect() {
        ErrorKind::Connect
    } else {
        ErrorKind::Other
    }
}

/// The full cause chain: reqwest's top-level message alone ("error sending
/// request") hides the useful part (proxy refused, auth failed, DNS...).
fn describe(error: &reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !parts.iter().any(|part| part.contains(&text)) {
            parts.push(text);
        }
        source = cause.source();
    }
    parts.join(": ")
}

/// Belt and braces: never let the proxy password reach the UI or the log.
fn scrub(message: &str, password: Option<&str>) -> String {
    match password {
        Some(secret) if secret.len() >= 3 => message.replace(secret, "***"),
        _ => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_hides_the_password() {
        assert_eq!(
            scrub("auth for user:hunter2 failed", Some("hunter2")),
            "auth for user:*** failed"
        );
    }

    #[test]
    fn scrub_leaves_short_or_missing_passwords_alone() {
        assert_eq!(scrub("a:b", Some("b")), "a:b");
        assert_eq!(scrub("text", None), "text");
    }

    #[test]
    fn result_display_shows_status_and_latency() {
        let ok = CheckResult {
            host: "h",
            status: Some(401),
            latency_ms: Some(120),
            error: None,
        };
        assert_eq!(ok.to_string(), "h: HTTP 401 in 120 ms");
    }
}
