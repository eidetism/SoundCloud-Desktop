//! `scimg://` — every image the UI shows comes through here: fetched once
//! via the shared client (so the proxy applies), kept in a disk LRU, served
//! to the WebView. The frontend never points `<img>` at sndcdn directly.
//!
//! URL shape: `scimg://localhost/<size>/<base64url of the original URL>`
//! (on Windows the WebView sees it as `http://scimg.localhost/...`). The size
//! is applied by rewriting SoundCloud's size suffix (`-large.jpg` →
//! `-t200x200.jpg`), so each size is its own cache entry.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use regex::Regex;
use sha2::{Digest, Sha256};
use tauri::Manager;
use tauri::http::{Request, Response, StatusCode, header};

use crate::app::diagnostics;
use crate::net::NetState;
use crate::rt::AppHandle;
use crate::shared::{file_lru, hosts};

pub const SCHEME: &str = "scimg";
/// Sizes the SoundCloud CDN serves that the UI uses.
/// `original` is for profile banners (`visuals-…-original.jpg`).
const SIZES: [&str; 6] = [
    "t67x67", "large", "t200x200", "t300x300", "t500x500", "original",
];
const LIMIT_BYTES: u64 = 300 * 1024 * 1024;
/// Trim the cache after this many new files (and once at startup).
const TRIM_EVERY: u32 = 200;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Hits refresh a file's last-use time at most this often.
const MARK_USED_INTERVAL: Duration = Duration::from_secs(60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

static SIZE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-(?:large|original|crop|t\d+x\d+|badge|small|tiny|mini)\.(jpg|jpeg|png)$")
        .expect("valid size regex")
});

pub struct ImageCache {
    dir: PathBuf,
    writes: AtomicU32,
}

impl ImageCache {
    pub fn open(cache_dir: &Path) -> Self {
        let dir = cache_dir.join("images");
        let trim_dir = dir.clone();
        std::thread::Builder::new()
            .name("image-cache-trim".into())
            .spawn(move || trim(&trim_dir))
            .ok();
        Self {
            dir,
            writes: AtomicU32::new(0),
        }
    }
}

fn trim(dir: &Path) {
    let freed = file_lru::trim_to_limit(file_lru::collect_files(&[dir]), LIMIT_BYTES);
    if freed > 0 {
        diagnostics::log("INFO", format!("[images] trimmed {} KB", freed / 1024));
    }
}

/// The CDN URL for `path` (`/<size>/<base64url>`), or `None` if it isn't a
/// SoundCloud image request.
fn target(path: &str) -> Option<String> {
    let (size, encoded) = path.trim_start_matches('/').split_once('/')?;
    if !SIZES.contains(&size) {
        return None;
    }
    let raw = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded).ok()?).ok()?;
    let url = hosts::media_url(&raw).ok()?;
    if !url.host_str()?.ends_with(".sndcdn.com") {
        return None;
    }
    let url = url.as_str();
    // Labs artwork (`al.sndcdn.com/...-t500x500.jpg?q=…`) carries a query and
    // comes in one size; only plain CDN names get resized.
    Some(if url.contains('?') {
        url.to_string()
    } else {
        SIZE_SUFFIX
            .replace(url, |caps: &regex::Captures| {
                format!("-{size}.{}", &caps[1])
            })
            .into_owned()
    })
}

fn cache_path(dir: &Path, url: &str) -> PathBuf {
    let key = hex::encode(Sha256::digest(url.as_bytes()));
    dir.join(&key[..2]).join(key)
}

fn sniff(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

fn respond(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type);
    if status == StatusCode::OK {
        response = response.header(header::CACHE_CONTROL, "max-age=31536000, immutable");
    }
    response.body(body).expect("static response parts")
}

fn fail(status: StatusCode) -> Response<Vec<u8>> {
    respond(status, "text/plain", Vec::new())
}

/// The protocol handler; answers on a runtime task.
pub fn handle(app: &AppHandle, request: Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let app = app.clone();
    let path = request.uri().path().to_string();
    tauri::async_runtime::spawn(async move {
        responder.respond(serve(&app, &path).await);
    });
}

async fn serve(app: &AppHandle, path: &str) -> Response<Vec<u8>> {
    let Some(url) = target(path) else {
        return fail(StatusCode::BAD_REQUEST);
    };
    let cache = app.state::<ImageCache>();
    let file = cache_path(&cache.dir, &url);

    let cached = file.clone();
    let hit = tokio::task::spawn_blocking(move || {
        let data = std::fs::read(&cached).ok()?;
        let fresh = std::fs::metadata(&cached)
            .ok()
            .and_then(|meta| {
                SystemTime::now()
                    .duration_since(file_lru::last_used(&meta))
                    .ok()
            })
            .is_some_and(|age| age < MARK_USED_INTERVAL);
        if !fresh {
            file_lru::mark_used(&cached);
        }
        Some(data)
    })
    .await
    .ok()
    .flatten();
    if let Some(data) = hit
        && let Some(content_type) = sniff(&data)
    {
        return respond(StatusCode::OK, content_type, data);
    }

    let http = app.state::<NetState>().client();
    let response = match http.get(&url).timeout(FETCH_TIMEOUT).send().await {
        Ok(response) => response,
        Err(_) => return fail(StatusCode::BAD_GATEWAY),
    };
    if !response.status().is_success() {
        return fail(StatusCode::NOT_FOUND);
    }
    let data = match response.bytes().await {
        Ok(bytes) if bytes.len() <= MAX_IMAGE_BYTES => bytes.to_vec(),
        _ => return fail(StatusCode::BAD_GATEWAY),
    };
    let Some(content_type) = sniff(&data) else {
        return fail(StatusCode::BAD_GATEWAY);
    };

    let to_write = data.clone();
    let trim_now = cache.writes.fetch_add(1, Ordering::Relaxed) % TRIM_EVERY == TRIM_EVERY - 1;
    let dir = cache.dir.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = write_atomic(&file, &to_write) {
            diagnostics::warn(format!("[images] write failed: {error}"));
        }
        if trim_now {
            trim(&dir);
        }
    });
    respond(StatusCode::OK, content_type, data)
}

/// tmp -> rename, so a crash never leaves a half-written image.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(size: &str, url: &str) -> String {
        format!("/{size}/{}", URL_SAFE_NO_PAD.encode(url))
    }

    #[test]
    fn rewrites_the_size_suffix() {
        let url = "https://i1.sndcdn.com/artworks-YEW4my9nuF2wFJHt-gzT36A-large.jpg";
        assert_eq!(
            target(&path("t200x200", url)).as_deref(),
            Some("https://i1.sndcdn.com/artworks-YEW4my9nuF2wFJHt-gzT36A-t200x200.jpg")
        );
        let avatar = "https://i1.sndcdn.com/avatars-umZzBaqKvVP4YFnT-VjMkfg-t500x500.jpg";
        assert!(
            target(&path("t67x67", avatar))
                .unwrap()
                .ends_with("-t67x67.jpg")
        );
    }

    #[test]
    fn labs_artwork_is_served_as_is() {
        let url = "https://al.sndcdn.com/labs-e0ea3a45-0-t500x500.jpg?q=YXJ0d2";
        assert_eq!(target(&path("t200x200", url)).as_deref(), Some(url));
    }

    #[test]
    fn refuses_other_hosts_sizes_and_garbage() {
        let ok = "https://i1.sndcdn.com/a-large.jpg";
        assert!(target(&path("t9999x9999", ok)).is_none());
        assert!(target(&path("t200x200", "https://example.com/a-large.jpg")).is_none());
        assert!(
            target(&path(
                "t200x200",
                "https://playback.media-streaming.soundcloud.cloud/x"
            ))
            .is_none()
        );
        assert!(target("/t200x200/%%%").is_none());
        assert!(target("/t200x200").is_none());
    }

    #[test]
    fn sniffs_images_only() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(sniff(b"<html>"), None);
    }
}
