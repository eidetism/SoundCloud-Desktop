use std::sync::atomic::{AtomicBool, Ordering};

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime, Url, Webview};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_opener::OpenerExt;

use crate::app::{diagnostics, locale};

const APP_ORIGIN: (&str, &str) = ("http", "tauri.localhost");
const BROWSER_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];
const TITLE: &str = "Pulse";

static ASKING: AtomicBool = AtomicBool::new(false);

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navigation")
        .on_navigation(|webview, url| {
            // The soundcloud.com windows (sign-in, hidden site) browse the web.
            if [crate::auth::LOGIN_WINDOW, crate::auth::SITE_WINDOW].contains(&webview.label()) {
                return url.scheme() == "https" || is_blank(url);
            }
            if allowed(url, dev_server(webview).as_ref()) {
                return true;
            }
            diagnostics::warn(format!(
                "[Navigation] blocked {}://{}",
                url.scheme(),
                url.host_str().unwrap_or_default()
            ));
            offer_browser(webview.app_handle(), url);
            false
        })
        .build()
}

fn allowed(url: &Url, dev_server: Option<&Url>) -> bool {
    is_app(url) || is_blank(url) || dev_server.is_some_and(|dev| url.origin() == dev.origin())
}

fn is_app(url: &Url) -> bool {
    url.scheme() == APP_ORIGIN.0 && url.host_str() == Some(APP_ORIGIN.1) && url.port().is_none()
}

fn is_blank(url: &Url) -> bool {
    url.scheme() == "about" && matches!(url.path(), "blank" | "srcdoc")
}

fn dev_server<R: Runtime>(webview: &Webview<R>) -> Option<Url> {
    cfg!(dev)
        .then(|| webview.config().build.dev_url.clone())
        .flatten()
}

fn opens_in_browser(url: &Url) -> bool {
    BROWSER_SCHEMES.contains(&url.scheme())
}

fn offer_browser<R: Runtime>(app: &AppHandle<R>, url: &Url) {
    if !opens_in_browser(url) || ASKING.swap(true, Ordering::AcqRel) {
        return;
    }
    let target = url.to_string();
    let question = locale::pick(
        "Открыть ссылку в браузере?",
        "Open this link in your browser?",
    );
    let opener = app.clone();
    app.dialog()
        .message(format!("{question}\n\n{target}"))
        .title(TITLE)
        .buttons(MessageDialogButtons::OkCancel)
        .show(move |open| {
            ASKING.store(false, Ordering::Release);
            if open {
                let _ = opener.opener().open_url(target, None::<&str>);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    fn app_url(path: &str) -> Url {
        url(&format!("{}://{}{path}", APP_ORIGIN.0, APP_ORIGIN.1))
    }

    #[test]
    fn the_app_origin_and_blank_frames_stay_in_the_window() {
        for kept in [
            app_url("/"),
            app_url("/home"),
            url("about:blank"),
            url("about:srcdoc"),
        ] {
            assert!(allowed(&kept, None), "{kept}");
        }
    }

    #[test]
    fn other_origins_and_schemes_leave_the_window() {
        for blocked in [
            "tauri://localhost/",
            "https://tauri.localhost/",
            "http://tauri.localhost:8080/",
            "https://soundcloud.com/discover",
            "http://127.0.0.1:4455/overlay",
            "data:text/html,<p>x</p>",
            "file:///etc/passwd",
            "blob:https://evil.example/1",
        ] {
            assert!(!allowed(&url(blocked), None), "{blocked}");
        }
    }

    #[test]
    fn the_dev_server_counts_only_when_given() {
        let dev = url("http://localhost:1420");
        assert!(allowed(&url("http://localhost:1420/home"), Some(&dev)));
        assert!(!allowed(&url("http://localhost:1421/"), Some(&dev)));
        assert!(!allowed(&url("http://localhost:1420/home"), None));
    }

    #[test]
    fn only_web_and_contact_links_are_offered_to_the_browser() {
        for offered in [
            "https://soundcloud.com/x",
            "http://example.com",
            "mailto:a@b.c",
            "tel:+100",
        ] {
            assert!(opens_in_browser(&url(offered)), "{offered}");
        }
        for refused in [
            "file:///etc/passwd",
            "data:text/plain,x",
            "scimg://localhost/x",
            "javascript:alert(1)",
        ] {
            assert!(!opens_in_browser(&url(refused)), "{refused}");
        }
    }
}
