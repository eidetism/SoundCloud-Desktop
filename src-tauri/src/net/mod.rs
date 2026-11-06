//! The single HTTP client. Every request to SoundCloud (API, audio, images)
//! goes through `NetState::client()`, built from the network settings and
//! rebuilt whenever they change.

mod check;
pub mod proxy;

use std::sync::RwLock;
use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::app::diagnostics;
use crate::secrets::{self, Secret};
use crate::settings::{Settings, SettingsStore};
use check::CheckResult;
use proxy::NetworkSettings;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub struct NetState {
    client: RwLock<reqwest::Client>,
}

impl NetState {
    /// Build from saved settings. A broken proxy config (or unreadable
    /// password) falls back to a direct client so the app still starts; the
    /// settings screen shows the error on the next save or check.
    pub fn init(settings: &Settings) -> Self {
        Self {
            client: RwLock::new(client_for(settings)),
        }
    }

    /// Rebuild from settings after a change outside the network form
    /// (e.g. a newly captured browser User-Agent).
    pub fn reload(&self, settings: &Settings) {
        *self.client.write().unwrap() = client_for(settings);
    }

    /// The shared client. Cheap to clone (reference-counted).
    pub fn client(&self) -> reqwest::Client {
        self.client.read().unwrap().clone()
    }
}

fn client_for(settings: &Settings) -> reqwest::Client {
    let ua = settings.browser_user_agent.as_deref();
    proxy_password(&settings.network)
        .and_then(|password| build_client(&settings.network, password.as_deref(), ua))
        .unwrap_or_else(|error| {
            diagnostics::warn(format!("[net] {error}; starting with a direct connection"));
            build_client(&NetworkSettings::default(), None, ua)
                .expect("direct client must always build")
        })
}

/// Stored proxy password, needed only when the active proxy has a username.
fn proxy_password(settings: &NetworkSettings) -> Result<Option<String>, String> {
    match settings.active_proxy() {
        Some(proxy) if proxy.has_credentials() => secrets::load(Secret::ProxyPassword),
        _ => Ok(None),
    }
}

pub fn build_client(
    settings: &NetworkSettings,
    password: Option<&str>,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT);
    if let Some(user_agent) = user_agent {
        builder = builder.user_agent(user_agent);
    }
    if let Some(endpoint) = settings.active_proxy() {
        let mut proxy = reqwest::Proxy::all(endpoint.url()?).map_err(|e| e.to_string())?;
        if endpoint.has_credentials() {
            proxy = proxy.basic_auth(&endpoint.username, password.unwrap_or_default());
        }
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|e| format!("http client: {e}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettingsView {
    settings: NetworkSettings,
    has_password: bool,
}

#[tauri::command]
pub fn net_settings_get(store: State<'_, SettingsStore>) -> Result<NetworkSettingsView, String> {
    Ok(NetworkSettingsView {
        settings: store.get().network,
        has_password: secrets::load(Secret::ProxyPassword)?.is_some(),
    })
}

/// What to do with the stored proxy password on save.
#[derive(serde::Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "camelCase")]
pub enum PasswordChange {
    Keep,
    Set(String),
    Clear,
}

impl PasswordChange {
    /// The password the new client should use.
    fn resolve(&self) -> Result<Option<String>, String> {
        match self {
            PasswordChange::Keep => secrets::load(Secret::ProxyPassword),
            PasswordChange::Set(value) => Ok(Some(value.clone())),
            PasswordChange::Clear => Ok(None),
        }
    }
}

/// Validate, build the new client, then persist the password and settings.
/// Nothing is saved if the client can't be built.
#[tauri::command]
pub fn net_settings_set(
    settings: NetworkSettings,
    password: PasswordChange,
    store: State<'_, SettingsStore>,
    net: State<'_, NetState>,
) -> Result<NetworkSettingsView, String> {
    let user_agent = store.get().browser_user_agent;
    let client = build_client(
        &settings,
        password.resolve()?.as_deref(),
        user_agent.as_deref(),
    )?;
    match &password {
        PasswordChange::Keep => {}
        PasswordChange::Set(value) => secrets::store(Secret::ProxyPassword, value)?,
        PasswordChange::Clear => secrets::delete(Secret::ProxyPassword)?,
    }
    store.update(|current| current.network = settings.clone())?;
    *net.client.write().unwrap() = client;
    diagnostics::log("INFO", format!("[net] mode set to {:?}", settings.mode));
    net_settings_get(store)
}

/// Check reachability of SoundCloud hosts. With `draft`, checks those
/// (unsaved) settings on a throwaway client instead of the active one.
#[tauri::command]
pub async fn net_check(
    draft: Option<NetworkSettings>,
    password: Option<PasswordChange>,
    store: State<'_, SettingsStore>,
    net: State<'_, NetState>,
) -> Result<Vec<CheckResult>, String> {
    let (client, secret) = match &draft {
        Some(settings) => {
            let secret = password.unwrap_or(PasswordChange::Keep).resolve()?;
            let user_agent = store.get().browser_user_agent;
            let client = build_client(settings, secret.as_deref(), user_agent.as_deref())?;
            (client, secret)
        }
        None => (net.client(), None),
    };
    let results = check::run(&client, secret.as_deref()).await;
    for result in &results {
        diagnostics::log("INFO", format!("[net] check {result}"));
    }
    Ok(results)
}
