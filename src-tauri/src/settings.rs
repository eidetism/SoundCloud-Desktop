//! App settings persisted as JSON in the app data dir. Never holds secrets:
//! those live in the Credential Manager (`crate::secrets`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::net::proxy::NetworkSettings;

const FILE: &str = "settings.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub network: NetworkSettings,
    /// User-Agent of this machine's WebView2, read natively from the login
    /// window. Rust requests send it so the session looks like the browser
    /// that created it.
    pub browser_user_agent: Option<String>,
    /// Player volume, percent.
    pub volume: f64,
    /// Show the current track as a Discord "Listening to" status.
    pub discord_rich_presence: bool,
    pub theme: Theme,
    /// Even out loudness between tracks.
    pub normalize_volume: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// The settings the UI edits directly (network has its own commands).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSettings {
    pub theme: Theme,
    pub discord_rich_presence: bool,
    pub normalize_volume: bool,
}

#[tauri::command]
pub fn settings_ui_get(store: tauri::State<'_, SettingsStore>) -> UiSettings {
    let settings = store.get();
    UiSettings {
        theme: settings.theme,
        discord_rich_presence: settings.discord_rich_presence,
        normalize_volume: settings.normalize_volume,
    }
}

#[tauri::command]
pub fn settings_ui_set(
    app: crate::rt::AppHandle,
    store: tauri::State<'_, SettingsStore>,
    audio: tauri::State<'_, crate::audio::state::AudioState>,
    ui: UiSettings,
) -> Result<UiSettings, String> {
    let discord_changed = store.get().discord_rich_presence != ui.discord_rich_presence;
    store.update(|settings| {
        settings.theme = ui.theme;
        settings.discord_rich_presence = ui.discord_rich_presence;
        settings.normalize_volume = ui.normalize_volume;
    })?;
    crate::audio::engine::set_normalization(&audio, ui.normalize_volume);
    if discord_changed {
        crate::player::send(&app, crate::player::Command::RefreshPresence);
    }
    Ok(settings_ui_get(store))
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            network: NetworkSettings::default(),
            browser_user_agent: None,
            volume: 60.0,
            discord_rich_presence: true,
            theme: Theme::System,
            normalize_volume: true,
        }
    }
}

/// Managed state: the in-memory settings plus their file.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn open(data_dir: &Path) -> Self {
        let path = path(data_dir);
        let current = Mutex::new(load(&path));
        Self { path, current }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    /// Apply `change` and persist; memory is updated only if the write succeeds.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut current = self.current.lock().unwrap();
        let mut next = current.clone();
        change(&mut next);
        save(&self.path, &next)?;
        *current = next;
        Ok(())
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// Missing or unreadable file falls back to defaults: a corrupt settings file
/// must never keep the app from starting.
pub fn load(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Atomic write: tmp -> fsync -> rename, so a crash leaves the old or the new file.
pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let written = std::fs::File::create(&tmp).and_then(|mut file| {
        file.write_all(&bytes)?;
        file.sync_all()
    });
    if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("failed to save settings: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::proxy::{NetMode, ProxyScheme};

    #[test]
    fn empty_json_gives_direct_mode() {
        let settings: Settings = serde_json::from_slice(b"{}").unwrap();
        assert_eq!(settings.network.mode, NetMode::Direct);
        assert_eq!(settings.network.proxy.scheme, ProxyScheme::Socks5);
        assert_eq!(settings.volume, 60.0);
        assert!(settings.discord_rich_presence);
    }

    #[test]
    fn settings_round_trip_through_a_file() {
        let dir = std::env::temp_dir().join(format!("scd-settings-{}", std::process::id()));
        let file = path(&dir);
        let mut settings = Settings::default();
        settings.network.mode = NetMode::Proxy;
        settings.network.proxy.host = "10.0.0.1".into();
        settings.network.proxy.port = 1080;
        save(&file, &settings).unwrap();

        let loaded = load(&file);
        assert_eq!(loaded.network.mode, NetMode::Proxy);
        assert_eq!(loaded.network.proxy.host, "10.0.0.1");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("scd-settings-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = path(&dir);
        std::fs::write(&file, b"{not json").unwrap();
        assert_eq!(load(&file).network.mode, NetMode::Direct);
        std::fs::remove_dir_all(&dir).ok();
    }
}
