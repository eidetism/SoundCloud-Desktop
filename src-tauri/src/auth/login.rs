//! The sign-in window: a plain, visible WebView2 on soundcloud.com/signin.
//!
//! Rules: the user types the credentials and solves any DataDome check
//! themselves. No User-Agent override, no script injection, no autofill.
//! The profile directory persists between runs (it keeps the `datadome`
//! cookie); only the window is destroyed. We only *read*: the `oauth_token`
//! cookie, the browser's own User-Agent, and navigation URLs to notice the
//! DataDome check.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{Manager, PhysicalPosition, Url, WebviewUrl, WebviewWindowBuilder};

use super::AuthState;
use super::browser_proxy::{self, BrowserRoute};
use super::forwarder::{self, Forwarder, Upstream};
use crate::api::{Api, ApiError};
use crate::app::{diagnostics, locale};
use crate::net::NetState;
use crate::rt::{AppHandle, WebviewWindow};
use crate::secrets::{self, Secret};
use crate::settings::SettingsStore;

pub const LABEL: &str = "login";
const HINT_LABEL: &str = "login-hint";
const SIGNIN_URL: &str = "https://soundcloud.com/signin";
const COOKIE_URL: &str = "https://soundcloud.com";
const TOKEN_COOKIE: &str = "oauth_token";
const PROFILE_DIR: &str = "login-profile";
const CAPTCHA_HOST: &str = "captcha-delivery.com";
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_AFTER_ERROR: Duration = Duration::from_secs(5);
const HINT_SIZE: (f64, f64) = (440.0, 56.0);
const HINT_TOP_OFFSET: f64 = 48.0;

/// The forwarder (if any) that lives exactly as long as the login window.
#[derive(Default)]
pub struct LoginSlot(Mutex<Option<Forwarder>>);

pub fn is_captcha(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == CAPTCHA_HOST || host.ends_with(&format!(".{CAPTCHA_HOST}")))
}

pub async fn open(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    let settings = app.state::<SettingsStore>().get();
    let proxy = match browser_proxy::route(&settings.network)? {
        BrowserRoute::Direct => None,
        BrowserRoute::Proxy(proxy) => Some(proxy),
        BrowserRoute::Forwarded(endpoint) => {
            let password = secrets::load(Secret::ProxyPassword)?.unwrap_or_default();
            let forwarder = forwarder::start(Upstream { endpoint, password })
                .await
                .map_err(|e| format!("login proxy forwarder: {e}"))?;
            let proxy = browser_proxy::forwarder_proxy(forwarder.port);
            *app.state::<AuthState>().login.0.lock().unwrap() = Some(forwarder);
            Some(proxy)
        }
    };

    let profile = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(PROFILE_DIR);
    let url = SIGNIN_URL.parse().expect("valid sign-in URL");
    let navigation_app = app.clone();
    let window = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url))
        .title(locale::pick("Вход в SoundCloud", "Sign in to SoundCloud"))
        .inner_size(480.0, 760.0)
        .center()
        .data_directory(profile)
        .additional_browser_args(&browser_proxy::args(proxy.as_deref()))
        .on_navigation(move |url| {
            if is_captcha(url.as_str()) {
                show_hint(&navigation_app);
            }
            true
        })
        .build()
        .map_err(|e| {
            stop_forwarder(&app);
            format!("login window: {e}")
        })?;

    watch_native(&app, &window);
    tauri::async_runtime::spawn(poll(app));
    Ok(())
}

/// Read the browser's User-Agent and watch iframe navigations for the
/// DataDome check, through WebView2's own API (nothing runs in the page).
fn watch_native(app: &AppHandle, window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings2;
    use webview2_com::{NavigationStartingEventHandler, take_pwstr};
    use windows::core::{Interface, PWSTR};

    let app = app.clone();
    let result = window.with_webview(move |webview| {
        let read = || -> windows::core::Result<()> {
            let core = unsafe { webview.controller().CoreWebView2()? };

            let settings: ICoreWebView2Settings2 = unsafe { core.Settings()? }.cast()?;
            let mut agent = PWSTR::null();
            unsafe { settings.UserAgent(&mut agent)? };
            remember_user_agent(&app, take_pwstr(agent));

            let frame_app = app.clone();
            let handler = NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut uri = PWSTR::null();
                    unsafe { args.Uri(&mut uri)? };
                    if is_captcha(&take_pwstr(uri)) {
                        show_hint(&frame_app);
                    }
                }
                Ok(())
            }));
            let mut token = Default::default();
            unsafe { core.add_FrameNavigationStarting(&handler, &mut token)? };
            Ok(())
        };
        if let Err(error) = read() {
            diagnostics::warn(format!("[login] webview2 hooks: {error}"));
        }
    });
    if let Err(error) = result {
        diagnostics::warn(format!("[login] with_webview: {error}"));
    }
}

fn remember_user_agent(app: &AppHandle, agent: String) {
    if agent.is_empty() {
        return;
    }
    let store = app.state::<SettingsStore>();
    if store.get().browser_user_agent.as_deref() == Some(agent.as_str()) {
        return;
    }
    match store.update(|settings| settings.browser_user_agent = Some(agent)) {
        Ok(()) => app.state::<NetState>().reload(&store.get()),
        Err(error) => diagnostics::warn(format!("[login] user agent: {error}")),
    }
}

/// "Complete the check, it's needed once" — a small borderless window pinned
/// over the top of the login window.
fn show_hint(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if app.get_webview_window(HINT_LABEL).is_some() {
            return;
        }
        let Some(login) = app.get_webview_window(LABEL) else {
            return;
        };
        let url = WebviewUrl::App("index.html?view=captcha-hint".into());
        let built = WebviewWindowBuilder::new(&app, HINT_LABEL, url)
            .title(locale::pick("Проверка", "Check"))
            .inner_size(HINT_SIZE.0, HINT_SIZE.1)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .parent(&login)
            .and_then(|builder| builder.build());
        let hint = match built {
            Ok(hint) => hint,
            Err(error) => {
                diagnostics::warn(format!("[login] hint window: {error}"));
                return;
            }
        };
        if let (Ok(position), Ok(size), Ok(scale)) = (
            login.outer_position(),
            login.outer_size(),
            login.scale_factor(),
        ) {
            let width = HINT_SIZE.0 * scale;
            let x = position.x as f64 + (size.width as f64 - width) / 2.0;
            let y = position.y as f64 + HINT_TOP_OFFSET * scale;
            let _ = hint.set_position(PhysicalPosition::new(x, y));
        }
        let _ = hint.show();
    });
}

/// Wait for the `oauth_token` cookie, confirm it with `/me`, then sign in and
/// destroy the window. Ends when the user closes the window.
async fn poll(app: AppHandle) {
    let mut rejected: Option<String> = None;
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let Some(window) = app.get_webview_window(LABEL) else {
            break;
        };
        let token = window
            .cookies_for_url(COOKIE_URL.parse().expect("valid cookie URL"))
            .ok()
            .and_then(|cookies| {
                cookies
                    .into_iter()
                    .find(|cookie| cookie.name() == TOKEN_COOKIE && !cookie.value().is_empty())
                    .map(|cookie| cookie.value().to_string())
            });
        let Some(token) = token else {
            continue;
        };
        if rejected.as_deref() == Some(token.as_str()) {
            continue;
        }

        let http = app.state::<NetState>().client();
        match app.state::<Api>().me(&http, &token).await {
            Ok(me) => {
                if let Err(error) = app.state::<AuthState>().sign_in(&app, token, me) {
                    diagnostics::error(format!("[login] {error}"));
                    continue;
                }
                let _ = window.destroy();
                break;
            }
            // A stale cookie from an old session: wait for a fresh one.
            Err(ApiError::Unauthorized) => rejected = Some(token),
            Err(error) => {
                diagnostics::warn(format!("[login] /me check failed: {error}"));
                tokio::time::sleep(RETRY_AFTER_ERROR).await;
            }
        }
    }
    if let Some(hint) = app.get_webview_window(HINT_LABEL) {
        let _ = hint.destroy();
    }
    stop_forwarder(&app);
}

fn stop_forwarder(app: &AppHandle) {
    app.state::<AuthState>().login.0.lock().unwrap().take();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_datadome_hosts() {
        assert!(is_captcha(
            "https://geo.captcha-delivery.com/captcha/?initialCid=x"
        ));
        assert!(is_captcha("https://captcha-delivery.com/"));
        assert!(!is_captcha("https://soundcloud.com/signin"));
        assert!(!is_captcha("https://captcha-delivery.com.evil.example/"));
        assert!(!is_captcha("https://notcaptcha-delivery.com/"));
        assert!(!is_captcha("not a url"));
    }
}
