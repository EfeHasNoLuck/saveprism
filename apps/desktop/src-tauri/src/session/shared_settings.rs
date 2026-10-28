//! The Settings choices the MCP server needs, shared through a small file.
//!
//! The window keeps its settings in WebView storage, which a separate `saveprism-desktop.exe mcp`
//! process cannot read. So the window writes the few choices that decide where files go and how
//! long backups stay (backup folders, the app folder, backups to keep, days to keep them) to
//! `settings.json` in the app data folder (`%LOCALAPPDATA%\SavePrism` on Windows), and the MCP
//! server reads it before each open, save, backup list and restore. The file never moves, so
//! moving the app folder in Settings does not move it. A missing, oversized or broken file means
//! the defaults; folders that are not absolute paths are ignored.

use std::fs;
use std::path::{Path, PathBuf};

use saveprism_core::io::default_backup_root;
use serde::{Deserialize, Serialize};

use super::export::write_atomic;
use super::{ApiError, MAX_KEEP_BACKUPS, MAX_KEEP_DAYS};

/// Name of the shared settings file inside the app data folder.
const FILE_NAME: &str = "settings.json";
/// Largest shared settings file that is read, in bytes.
const MAX_BYTES: u64 = 64 * 1024;
/// Most backup folders taken from the file.
const MAX_FOLDERS: usize = 32;

/// The shared choices, as the window stores them (missing fields keep the defaults).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SharedSettings {
    /// Backup folders the player used, newest first (empty: the default folder).
    pub backup_folders: Vec<String>,
    /// The app folder the player picked, or none for the default.
    pub app_folder: Option<String>,
    /// Backups kept per file, or none for the default.
    pub keep_backups: Option<usize>,
    /// Days backups are kept, or none for the default.
    pub keep_backup_days: Option<u32>,
}

impl SharedSettings {
    /// The backup folders as paths: absolute ones only, no repeats, at most 32. Empty keeps the
    /// folder the session already has (see [`super::Session::set_backup_roots`]).
    pub fn backup_roots(&self) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> = Vec::new();
        for folder in &self.backup_folders {
            let path = PathBuf::from(folder.trim());
            if path.is_absolute() && !roots.contains(&path) {
                roots.push(path);
            }
            if roots.len() == MAX_FOLDERS {
                break;
            }
        }
        roots
    }

    /// The app folder as a path, when one is set and absolute.
    pub fn app_dir(&self) -> Option<PathBuf> {
        self.app_folder
            .as_deref()
            .map(|folder| PathBuf::from(folder.trim()))
            .filter(|path| path.is_absolute())
    }

    /// Backups kept per file, clamped to 1..=50.
    pub fn keep_backups(&self) -> Option<usize> {
        self.keep_backups
            .map(|count| count.clamp(1, MAX_KEEP_BACKUPS))
    }

    /// Days backups are kept, clamped to 1..=365.
    pub fn keep_days(&self) -> Option<u32> {
        self.keep_backup_days
            .map(|days| days.clamp(1, MAX_KEEP_DAYS))
    }
}

/// Where the shared settings file lives, if the system has an app data folder.
pub fn default_file() -> Option<PathBuf> {
    default_backup_root()?
        .parent()
        .map(|app| app.join(FILE_NAME))
}

/// Writes the settings as JSON (temporary file, then rename), creating the folder when needed.
pub fn write(file: &Path, settings: &SharedSettings) -> Result<(), ApiError> {
    if let Some(dir) = file.parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir).map_err(|_| ApiError::internal())?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| ApiError::internal())?;
    write_atomic(file, &bytes)
}

/// Reads the settings; anything unusable gives the defaults.
pub fn read(file: &Path) -> SharedSettings {
    match fs::metadata(file) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_BYTES => fs::read(file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default(),
        _ => SharedSettings::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_written_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested").join(FILE_NAME);
        let backups = dir.path().join("backups");
        let settings = SharedSettings {
            backup_folders: vec![backups.to_string_lossy().into_owned()],
            app_folder: Some(dir.path().to_string_lossy().into_owned()),
            keep_backups: Some(25),
            keep_backup_days: Some(90),
        };
        write(&file, &settings).unwrap();
        let back = read(&file);
        assert_eq!(back, settings);
        assert_eq!(back.backup_roots(), vec![backups]);
        assert_eq!(back.app_dir().as_deref(), Some(dir.path()));
        assert_eq!(back.keep_backups(), Some(25));
        assert_eq!(back.keep_days(), Some(90));
    }

    #[test]
    fn a_missing_broken_or_huge_file_gives_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        assert_eq!(read(&file), SharedSettings::default());
        fs::write(&file, b"{not json").unwrap();
        assert_eq!(read(&file), SharedSettings::default());
        fs::write(&file, br#"{"keepBackups": -3}"#).unwrap();
        assert_eq!(read(&file), SharedSettings::default());
        let huge = format!(r#"{{"appFolder": "{}"}}"#, "x".repeat(70_000));
        fs::write(&file, huge).unwrap();
        assert_eq!(read(&file), SharedSettings::default());
        assert_eq!(read(dir.path()), SharedSettings::default());
    }

    #[test]
    fn relative_and_repeated_folders_are_dropped_and_counts_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let absolute = dir.path().to_string_lossy().into_owned();
        let settings = SharedSettings {
            backup_folders: vec![
                "relative\\backups".to_string(),
                absolute.clone(),
                absolute.clone(),
                "  ".to_string(),
            ],
            app_folder: Some("relative".to_string()),
            keep_backups: Some(500),
            keep_backup_days: Some(0),
        };
        assert_eq!(settings.backup_roots(), vec![dir.path().to_path_buf()]);
        assert_eq!(settings.app_dir(), None);
        assert_eq!(settings.keep_backups(), Some(MAX_KEEP_BACKUPS));
        assert_eq!(settings.keep_days(), Some(1));
    }
}
