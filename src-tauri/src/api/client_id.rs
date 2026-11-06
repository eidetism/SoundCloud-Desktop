//! Public web `client_id`, scraped from the soundcloud.com page hydration
//! (logic ported from the donor's `track_cache/sc_anon`).

use std::sync::LazyLock;

use regex::Regex;

static HYDRATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""hydratable"\s*:\s*"apiClient"\s*,\s*"data"\s*:\s*\{\s*"id"\s*:\s*"([^"]+)""#)
        .expect("valid client_id regex")
});

/// Pull `client_id` out of `window.__sc_hydration` on a soundcloud.com page.
pub fn extract(html: &str) -> Option<String> {
    HYDRATION
        .captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_api_client_id_in_hydration() {
        let html = r#"<script>window.__sc_hydration = [{"hydratable":"anonymousId","data":"x"},{"hydratable":"apiClient","data":{"id":"AbC123xyz","isExpiring":false}}];</script>"#;
        assert_eq!(extract(html).as_deref(), Some("AbC123xyz"));
    }

    #[test]
    fn tolerates_whitespace() {
        let html = r#"{ "hydratable" : "apiClient" , "data" : { "id" : "id42" } }"#;
        assert_eq!(extract(html).as_deref(), Some("id42"));
    }

    #[test]
    fn missing_hydration_gives_none() {
        assert_eq!(extract("<html>captcha</html>"), None);
    }
}
