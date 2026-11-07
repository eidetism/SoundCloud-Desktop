//! Moving the offline folder to another parent. Files are renamed when the
//! target is on the same volume and copied (through `.part`) otherwise; the
//! old copies go only after everything arrived and the index was written.
//! A target that already holds a `Pulse Offline` folder (an earlier install)
//! is merged in.

use std::io;
use std::path::{Path, PathBuf};

use super::index::{self, Index};
use super::{PART, ROOT, entry_path};

#[derive(Debug, PartialEq)]
pub enum Refusal {
    Same,
    /// One folder inside the other.
    Nested,
    NotWritable,
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::Same => "same",
            Refusal::Nested => "nested",
            Refusal::NotWritable => "not_writable",
        }
    }
}

/// Check moving `old_root` to `<parent>/Pulse Offline`; returns the new root.
pub fn target(old_root: &Path, parent: &Path) -> Result<PathBuf, Refusal> {
    let new_root = parent.join(ROOT);
    let same = |a: &Path, b: &Path| {
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        canon(a) == canon(b)
    };
    if same(old_root, &new_root) {
        return Err(Refusal::Same);
    }
    if new_root.starts_with(old_root) || old_root.starts_with(&new_root) {
        return Err(Refusal::Nested);
    }
    std::fs::create_dir_all(&new_root).map_err(|_| Refusal::NotWritable)?;
    let probe = new_root.join(".probe");
    std::fs::write(&probe, b"").map_err(|_| Refusal::NotWritable)?;
    let _ = std::fs::remove_file(probe);
    Ok(new_root)
}

/// Move every file of `index` from `old_root` to `new_root` and merge in an
/// index already there. Returns the merged index (written to `new_root`);
/// the old folder is cleaned up only after that.
pub fn move_all(
    old_root: &Path,
    new_root: &Path,
    index: &Index,
    mut progress: impl FnMut(u64, u64),
) -> io::Result<Index> {
    let total: u64 = index.tracks.values().map(|e| e.bytes).sum();
    let mut done = 0;
    progress(done, total);
    for entry in index.tracks.values() {
        let from = entry_path(old_root, entry);
        let to = entry_path(new_root, entry);
        transfer(&from, &to)?;
        done += entry.bytes;
        progress(done, total);
    }
    let mut merged = Index::load(&new_root.join(index::FILE));
    merged
        .tracks
        .retain(|id, e| !index.tracks.contains_key(id) && entry_path(new_root, e).is_file());
    merged
        .tracks
        .extend(index.tracks.iter().map(|(id, e)| (*id, e.clone())));
    merged.save(&new_root.join(index::FILE))?;
    for entry in index.tracks.values() {
        let _ = std::fs::remove_file(entry_path(old_root, entry));
    }
    let _ = std::fs::remove_file(old_root.join(index::FILE));
    for dir in [
        old_root.join(super::SAVED),
        old_root.join(super::RECENT),
        old_root.to_path_buf(),
    ] {
        let _ = std::fs::remove_dir(dir);
    }
    Ok(merged)
}

/// Rename, or copy through `.part` across volumes. The source stays until
/// the caller removes it.
fn transfer(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let part = to.with_extension(PART);
    std::fs::copy(from, &part)?;
    std::fs::rename(&part, to)
}

#[cfg(test)]
mod tests {
    use super::super::index::Entry;
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pulse-relocate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn entry(id: u64, saved: bool) -> Entry {
        Entry {
            id,
            urn: String::new(),
            title: format!("t{id}"),
            user_id: None,
            username: String::new(),
            user_permalink: String::new(),
            user_avatar_url: None,
            artwork_url: None,
            duration_ms: 1,
            permalink_url: String::new(),
            waveform_url: None,
            bytes: 3,
            saved,
            added_at: 0,
            played_at: 0,
        }
    }

    fn put(root: &Path, index: &mut Index, e: Entry) {
        let path = entry_path(root, &e);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, [1, 2, 3]).unwrap();
        index.tracks.insert(e.id, e);
    }

    #[test]
    fn targets_are_checked() {
        let base = temp("target");
        let old = base.join("a").join(ROOT);
        std::fs::create_dir_all(&old).unwrap();
        assert_eq!(target(&old, &base.join("a")), Err(Refusal::Same));
        assert_eq!(target(&old, &old), Err(Refusal::Nested));
        assert_eq!(target(&old, &base.join("b")), Ok(base.join("b").join(ROOT)));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn files_move_and_an_existing_folder_is_merged() {
        let base = temp("move");
        let old = base.join("old").join(ROOT);
        let new = base.join("new").join(ROOT);
        let mut ours = Index::default();
        put(&old, &mut ours, entry(1, true));
        put(&old, &mut ours, entry(2, false));
        let mut theirs = Index::default();
        put(&new, &mut theirs, entry(3, true));
        theirs.tracks.insert(4, entry(4, true)); // listed, file missing
        theirs.save(&new.join(index::FILE)).unwrap();

        let mut seen = Vec::new();
        let merged = move_all(&old, &new, &ours, |done, total| seen.push((done, total))).unwrap();
        assert_eq!(seen, [(0, 6), (3, 6), (6, 6)]);
        assert_eq!(merged.tracks.keys().copied().collect::<Vec<_>>(), [1, 2, 3]);
        assert!(new.join("saved/1.mp4").is_file() && new.join("recent/2.mp4").is_file());
        assert_eq!(Index::load(&new.join(index::FILE)), merged);
        assert!(!old.exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
