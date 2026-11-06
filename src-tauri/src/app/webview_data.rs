use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::app::{APP_IDENTIFIER, diagnostics};

const LOCK_FILE: &str = "instance.lock";
const TRASH_MARK: &str = ".trash-";

const CACHE_NAMES: &[&str] = &["EBWebView"];

static INSTANCE_LOCK: OnceLock<File> = OnceLock::new();

pub fn wipe_before_launch() {
    if !hold_instance_lock() {
        return;
    }
    let trash: Vec<PathBuf> = targets()
        .iter()
        .flat_map(|target| leftovers(target).into_iter().chain(move_aside(target)))
        .collect();
    if trash.is_empty() {
        return;
    }
    diagnostics::log(
        "INFO",
        format!("[WebviewData] wiping {} entries", trash.len()),
    );
    let _ = std::thread::Builder::new()
        .name("webview-data-wipe".into())
        .spawn(move || trash.iter().for_each(|path| remove(path)));
}

fn hold_instance_lock() -> bool {
    let Some(dir) = dirs::data_local_dir().map(|dir| dir.join(APP_IDENTIFIER)) else {
        return false;
    };
    if fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let Ok(file) = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_FILE))
    else {
        return false;
    };
    file.try_lock().is_ok() && INSTANCE_LOCK.set(file).is_ok()
}

fn targets() -> Vec<PathBuf> {
    let mut targets = Vec::new();
    if let Some(cache) = dirs::cache_dir().map(|dir| dir.join(APP_IDENTIFIER)) {
        targets.extend(CACHE_NAMES.iter().map(|name| cache.join(name)));
    }
    targets
}

fn move_aside(target: &Path) -> Option<PathBuf> {
    let name = target.file_name()?.to_str()?;
    let aside = target.with_file_name(format!("{name}{TRASH_MARK}{}", std::process::id()));
    fs::rename(target, &aside).ok()?;
    Some(aside)
}

fn leftovers(target: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(name)) = (target.parent(), target.file_name().and_then(|n| n.to_str()))
    else {
        return Vec::new();
    };
    let prefix = format!("{name}{TRASH_MARK}");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(&prefix))
        })
        .map(|entry| entry.path())
        .collect()
}

fn remove(path: &Path) {
    let is_dir = fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    let result = if is_dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    if let Err(err) = result {
        diagnostics::warn(format!("[WebviewData] {}: {err}", path.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("scd-webview-{nanos}"));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn moves_the_target_aside_and_keeps_neighbours() {
        let root = TempRoot::new();
        let target = root.0.join("EBWebView");
        fs::create_dir_all(target.join("Default")).unwrap();
        fs::create_dir_all(root.0.join("audio")).unwrap();

        let aside = move_aside(&target).unwrap();
        remove(&aside);

        assert!(!target.exists());
        assert!(!aside.exists());
        assert!(root.0.join("audio").exists());
    }

    #[test]
    fn missing_target_is_skipped() {
        let root = TempRoot::new();
        assert!(move_aside(&root.0.join("EBWebView")).is_none());
    }

    #[test]
    fn leftovers_match_only_their_own_target() {
        let root = TempRoot::new();
        fs::create_dir_all(root.0.join("Cache.db.trash-1")).unwrap();
        fs::write(root.0.join("Cache.db-wal.trash-2"), b"").unwrap();
        fs::write(root.0.join("Cache.db"), b"").unwrap();

        let found = leftovers(&root.0.join("Cache.db"));

        assert_eq!(found, vec![root.0.join("Cache.db.trash-1")]);
    }
}
