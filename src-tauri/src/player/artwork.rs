//! Artwork for the system media overlay. Windows would fetch an https cover
//! itself, past the app's proxy settings, so the cover is downloaded through
//! the shared client and handed over as a local file.

use std::path::{Path, PathBuf};

use crate::api::models::Track;
use crate::shared::hosts;

const MAX_COVER_BYTES: usize = 2 * 1024 * 1024;

/// A larger variant of a SoundCloud image URL: api-v2 hands out `-large`
/// (100×100); the CDN serves other sizes under the same name.
pub fn sized(url: &str, size: &str) -> String {
    url.replacen("-large.", &format!("-{size}."), 1)
}

/// The track's artwork, or the uploader's avatar when it has none.
pub fn of(track: &Track) -> Option<String> {
    track
        .artwork_url
        .clone()
        .or_else(|| track.user.as_ref()?.avatar_url.clone())
        .map(|url| sized(&url, "t500x500"))
}

/// Download the cover into `dir` (cleared first, so only the current cover
/// stays on disk) and return the file.
pub async fn cover_file(http: &reqwest::Client, url: &str, dir: &Path, id: u64) -> Option<PathBuf> {
    let url = hosts::media_url(url).ok()?;
    let response = http.get(url).send().await.ok()?.error_for_status().ok()?;
    let bytes = response.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_COVER_BYTES {
        return None;
    }
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok()?;
        let file = dir.join(format!("{id}.jpg"));
        std::fs::write(&file, &bytes).ok()?;
        Some(file)
    })
    .await
    .ok()?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_the_cdn_for_a_bigger_size() {
        assert_eq!(
            sized(
                "https://i1.sndcdn.com/artworks-YEW4my9nuF2wFJHt-gzT36A-large.jpg",
                "t500x500"
            ),
            "https://i1.sndcdn.com/artworks-YEW4my9nuF2wFJHt-gzT36A-t500x500.jpg"
        );
        let other = "https://al.sndcdn.com/labs-e0ea3a45-0-t500x500.jpg?q=1";
        assert_eq!(sized(other, "t500x500"), other);
    }
}
