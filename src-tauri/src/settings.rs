//! App settings persisted as JSON in the app data dir. Never holds secrets:
//! those live in the Credential Manager (`crate::secrets`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::net::proxy::NetworkSettings;

const FILE: &str = "settings.json";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub network: NetworkSettings,
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
