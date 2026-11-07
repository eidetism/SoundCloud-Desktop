//! A hidden WebView2 on soundcloud.com for the writes DataDome guards (likes,
//! playlist edits, GraphQL mutations): from a non-browser client they get
//! `403 x-datadome: protected`.
//!
//! The window shares the login profile (session and `datadome` cookies) and
//! the login window's proxy route. The request runs inside the page, so the
//! site's own DataDome script adds its client id, exactly as on the web.
//! Only our own request's HTTP status and JSON body come back, read through
//! the DevTools protocol: no IPC is opened to soundcloud.com, no User-Agent override, nothing is
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

/// What our request inside the page got back.
#[derive(Debug, PartialEq)]
pub struct SiteResponse {
    /// 0 if the request didn't complete.
    pub status: u16,
    /// The body, when it is JSON.
    pub body: Option<Value>,
}

/// Send `method url` (with an optional JSON `body`) from inside
/// soundcloud.com with `Authorization: OAuth token`.
pub async fn request(
    app: &AppHandle,
    method: &str,
    url: &str,
    token: &str,
    body: Option<&Value>,
) -> Result<SiteResponse, String> {
    let auth = app.state::<AuthState>();
    let _busy = auth.site.busy.lock().await;
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => open(app).await?,
    };
    let response = evaluate(&window, &request_script(method, url, token, body))
        .await
        .and_then(|value| response_of(&value));
    schedule_close(app);
    response
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

fn response_of(reply: &Value) -> Result<SiteResponse, String> {
    if reply.get("exceptionDetails").is_some() {
        return Err("site request threw".into());
    }
    let value = result_value(reply).ok_or("site request: no result")?;
    let status = value
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .ok_or("site request: no status")?;
    let body = value
        .get("body")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok());
    Ok(SiteResponse { status, body })
}

/// An XHR like the web app's own, so DataDome's listener tags it. Resolves
/// to `{status, body}` (the response text); status 0 when the request fails
/// at the network level.
fn request_script(method: &str, url: &str, token: &str, body: Option<&Value>) -> String {
    let method = Value::from(method);
    let url = Value::from(url);
    let auth = Value::from(format!("OAuth {token}"));
    let (content_type, payload) = match body {
        Some(body) => (
            "xhr.setRequestHeader('Content-Type', 'application/json'); ",
            Value::from(body.to_string()),
        ),
        None => ("", Value::Null),
    };
    format!(
        "new Promise((resolve) => {{            const xhr = new XMLHttpRequest();            xhr.open({method}, {url});            xhr.setRequestHeader('Authorization', {auth});            xhr.setRequestHeader('Accept', 'application/json, text/javascript, */*; q=0.1');            {content_type}           xhr.onload = () => resolve({{ status: xhr.status, body: xhr.responseText }});            xhr.onerror = () => resolve({{ status: 0, body: null }});            xhr.send({payload});          }})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_quotes_its_inputs() {
        let script = request_script("PUT", "https://api-v2.soundcloud.com/x?a=1", "t\"'", None);
        assert!(script.contains(r#"xhr.open("PUT", "https://api-v2.soundcloud.com/x?a=1")"#));
        assert!(script.contains(r#""OAuth t\"'""#));
        assert!(script.contains("xhr.send(null)"));
        assert!(!script.contains("Content-Type"));
    }

    #[test]
    fn script_sends_a_json_body_as_a_quoted_string() {
        let body = json!({"q": "it's \"x\""});
        let script = request_script(
            "POST",
            "https://graph.soundcloud.com/graphql",
            "t",
            Some(&body),
        );
        assert!(script.contains("'Content-Type', 'application/json'"));
        assert!(script.contains(r#"xhr.send("{\"q\":\"it's \\\"x\\\"\"}")"#));
    }

    #[test]
    fn reads_status_and_json_body_from_the_devtools_reply() {
        let ok =
            json!({"result": {"type": "object", "value": {"status": 200, "body": "{\"a\":1}"}}});
        assert_eq!(
            response_of(&ok),
            Ok(SiteResponse {
                status: 200,
                body: Some(json!({"a": 1}))
            })
        );
        let html = json!({"result": {"value": {"status": 403, "body": "<html>"}}});
        assert_eq!(
            response_of(&html),
            Ok(SiteResponse {
                status: 403,
                body: None
            })
        );
        let threw = json!({"result": {"type": "object"}, "exceptionDetails": {}});
        assert!(response_of(&threw).is_err());
        assert!(response_of(&json!({"result": {"type": "undefined"}})).is_err());
    }
}
