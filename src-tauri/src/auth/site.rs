//! A hidden WebView2 on soundcloud.com for the API writes DataDome guards
//! (likes): from a non-browser client they get `403 x-datadome: protected`.
//!
//! The window shares the login profile (session and `datadome` cookies) and
//! the login window's proxy route. The request runs inside the page, so the
//! site's own DataDome script adds its client id, exactly as on the web.
//! Only the HTTP status comes back, read through the DevTools protocol: no
//! IPC is opened to soundcloud.com, no User-Agent override, nothing is
//! clicked or filled in. The window is created on demand and destroyed when
//! idle, so it costs nothing in the tray.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use super::AuthState;
use super::forwarder::Forwarder;
use super::login;
use crate::app::diagnostics;
use crate::rt::{AppHandle, WebviewWindow};

pub const LABEL: &str = "site";
const HOME_URL: &str = "https://soundcloud.com/";
/// The window goes away after this long without a request.
const IDLE: Duration = Duration::from_secs(120);
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const READY_POLL: Duration = Duration::from_millis(250);
/// Time for the page's scripts (DataDome among them) after `load`.
const SETTLE: Duration = Duration::from_millis(1500);
const READY_CHECK: &str =
    "document.readyState === 'complete' && location.hostname === 'soundcloud.com'";

#[derive(Default)]
pub struct SiteSlot {
    /// One request at a time; also guards creating the window.
    busy: tokio::sync::Mutex<()>,
    forwarder: Mutex<Option<Forwarder>>,
    generation: AtomicU64,
}

/// Send `method url` from inside soundcloud.com with `Authorization: OAuth
/// token`; returns the HTTP status (0 if the request didn't complete).
pub async fn request_status(
    app: &AppHandle,
    method: &str,
    url: &str,
    token: &str,
) -> Result<u16, String> {
    let auth = app.state::<AuthState>();
    let _busy = auth.site.busy.lock().await;
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => open(app).await?,
    };
    let status = evaluate(&window, &request_script(method, url, token))
        .await
        .and_then(|value| status_of(&value));
    schedule_close(app);
    status
}

/// Close the window now (sign-out).
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
    app.state::<AuthState>()
        .site
        .forwarder
        .lock()
        .unwrap()
        .take();
}

async fn open(app: &AppHandle) -> Result<WebviewWindow, String> {
    let (args, forwarder) = login::browser_args(app).await?;
    *app.state::<AuthState>().site.forwarder.lock().unwrap() = forwarder;
    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::External(HOME_URL.parse().expect("valid home URL")),
    )
    .title("SoundCloud")
    .visible(false)
    .focused(false)
    .skip_taskbar(true)
    .data_directory(login::profile_dir(app)?)
    .additional_browser_args(&args)
    .build()
    .map_err(|e| {
        close(app);
        format!("site window: {e}")
    })?;
    mute(&window);

    let started = tokio::time::Instant::now();
    loop {
        tokio::time::sleep(READY_POLL).await;
        if evaluate(&window, READY_CHECK)
            .await
            .ok()
            .as_ref()
            .and_then(result_value)
            == Some(&Value::Bool(true))
        {
            break;
        }
        if started.elapsed() > READY_TIMEOUT {
            close(app);
            return Err("soundcloud.com did not load".into());
        }
    }
    tokio::time::sleep(SETTLE).await;
    Ok(window)
}

/// The page never plays anything, but it is a full soundcloud.com tab.
fn mute(window: &WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    use windows::core::Interface;

    let result = window.with_webview(|webview| {
        let set = || -> windows::core::Result<()> {
            let core: ICoreWebView2_8 = unsafe { webview.controller().CoreWebView2()? }.cast()?;
            unsafe { core.SetIsMuted(true) }
        };
        if let Err(error) = set() {
            diagnostics::warn(format!("[site] mute: {error}"));
        }
    });
    if let Err(error) = result {
        diagnostics::warn(format!("[site] with_webview: {error}"));
    }
}

fn schedule_close(app: &AppHandle) {
    let generation = app
        .state::<AuthState>()
        .site
        .generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(IDLE).await;
        let auth = app.state::<AuthState>();
        let Ok(_busy) = auth.site.busy.try_lock() else {
            return;
        };
        if auth.site.generation.load(Ordering::SeqCst) == generation {
            close(&app);
        }
    });
}

/// `Runtime.evaluate` with `awaitPromise`; returns the protocol's JSON reply.
/// The expression may hold the token: it is never logged.
async fn evaluate(window: &WebviewWindow, expression: &str) -> Result<Value, String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::{HSTRING, w};

    let params = json!({
        "expression": expression,
        "awaitPromise": true,
        "returnByValue": true,
    })
    .to_string();
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    window
        .with_webview(move |webview| {
            let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                move |status, reply| {
                    let _ = tx.send(status.map(|()| reply).map_err(|e| e.to_string()));
                    Ok(())
                },
            ));
            let call = || -> windows::core::Result<()> {
                let core = unsafe { webview.controller().CoreWebView2()? };
                unsafe {
                    core.CallDevToolsProtocolMethod(
                        w!("Runtime.evaluate"),
                        &HSTRING::from(params.as_str()),
                        &handler,
                    )
                }
            };
            if let Err(error) = call() {
                diagnostics::warn(format!("[site] devtools call: {error}"));
            }
        })
        .map_err(|e| format!("site webview: {e}"))?;
    let reply = rx
        .await
        .map_err(|_| "site devtools call dropped".to_string())??;
    serde_json::from_str(&reply).map_err(|e| format!("site devtools reply: {e}"))
}

fn result_value(reply: &Value) -> Option<&Value> {
    reply.get("result")?.get("value")
}

fn status_of(reply: &Value) -> Result<u16, String> {
    if reply.get("exceptionDetails").is_some() {
        return Err("site request threw".into());
    }
    result_value(reply)
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .ok_or_else(|| "site request: no status".into())
}

/// An XHR like the web app's own, so DataDome's listener tags it. Resolves
/// to the status; 0 when the request fails at the network level.
fn request_script(method: &str, url: &str, token: &str) -> String {
    let method = Value::from(method);
    let url = Value::from(url);
    let auth = Value::from(format!("OAuth {token}"));
    format!(
        "new Promise((resolve) => {{ \
           const xhr = new XMLHttpRequest(); \
           xhr.open({method}, {url}); \
           xhr.setRequestHeader('Authorization', {auth}); \
           xhr.setRequestHeader('Accept', 'application/json, text/javascript, */*; q=0.1'); \
           xhr.onload = () => resolve(xhr.status); \
           xhr.onerror = () => resolve(0); \
           xhr.send(); \
         }})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_quotes_its_inputs() {
        let script = request_script("PUT", "https://api-v2.soundcloud.com/x?a=1", "t\"'");
        assert!(script.contains(r#"xhr.open("PUT", "https://api-v2.soundcloud.com/x?a=1")"#));
        assert!(script.contains(r#""OAuth t\"'""#));
    }

    #[test]
    fn reads_the_status_from_the_devtools_reply() {
        let ok = json!({"result": {"type": "number", "value": 200}});
        assert_eq!(status_of(&ok), Ok(200));
        let threw = json!({"result": {"type": "object"}, "exceptionDetails": {}});
        assert!(status_of(&threw).is_err());
        assert!(status_of(&json!({"result": {"type": "undefined"}})).is_err());
    }
}
