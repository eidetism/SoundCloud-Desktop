//! Opening links from track descriptions and "open in browser" in the
//! system browser. Only http(s) URLs leave the app.

use reqwest::Url;
use tauri_plugin_opener::OpenerExt;

use crate::rt::AppHandle;

fn browsable(raw: &str) -> Option<Url> {
    Url::parse(raw)
        .ok()
        .filter(|url| matches!(url.scheme(), "https" | "http") && url.host_str().is_some())
}

#[tauri::command]
pub fn app_open_link(app: AppHandle, url: String) -> Result<(), String> {
    let url = browsable(&url).ok_or("not a web link")?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| format!("open link: {e}"))
}

#[cfg(test)]
mod tests {
    use super::browsable;

    #[test]
    fn only_web_links_open() {
        assert!(browsable("https://soundcloud.com/a/b").is_some());
        assert!(browsable("http://example.com").is_some());
        for raw in [
            "file:///C:/Windows",
            "javascript:alert(1)",
            "ms-settings:",
            "soundcloud.com",
        ] {
            assert!(browsable(raw).is_none(), "{raw}");
        }
    }
}
