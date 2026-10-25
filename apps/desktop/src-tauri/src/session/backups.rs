//! The backups of the open file: list them and put one back.
//!
//! A restore is a save of the backup's bytes: the core safe write backs up the current file first
//! (so a restore can itself be taken back from the list), writes through a temporary file and an
//! atomic replace, and checks that the written bytes are the backup's bytes. A backup that does not
//! open as a save is never written. The file is then opened again, so pending edits and the edit
//! history are gone; the UI asks before it restores over unsaved edits.

use std::fs;

use saveprism_core::io::{WriteOptions, find_backup, list_backups, safe_write};
use saveprism_core::{Error, codes};
use serde::Serialize;

use super::{ApiError, Session, app_codes, clamp_keep_backups, decode_file, io_error, quote};

/// One backup of the open file, newest first in the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRow {
    /// File name of the backup; pass it back to `restore_backup`.
    pub name: String,
    /// When the backup was made, in milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// Size of the backup in bytes.
    pub byte_size: u64,
}

impl Session {
    /// The backups of the open file, newest first (empty when it was never saved over).
    pub fn backups(&self) -> Result<Vec<BackupRow>, ApiError> {
        let root = self.backup_root()?;
        Ok(list_backups(&root, &self.path)
            .into_iter()
            .map(|backup| BackupRow {
                name: backup.name,
                created_ms: backup.created_ms,
                byte_size: backup.byte_size,
            })
            .collect())
    }

    /// Puts the backup `name` of the open file back in place of the file and returns the file opened
    /// again. Pending edits are lost. The backup must open as a save with the same password;
    /// otherwise nothing is written. `keep_backups` is the number of backups kept for the file,
    /// clamped to 1..=50 (10 when missing).
    pub fn restore_backup(
        &self,
        name: &str,
        keep_backups: Option<usize>,
    ) -> Result<Session, ApiError> {
        let root = self.backup_root()?;
        let backup = find_backup(&root, &self.path, name).ok_or_else(|| {
            ApiError::new(
                app_codes::BACKUP_NOT_FOUND,
                format!("there is no backup \"{}\" of this file", quote(name)),
                "Open the backups list again. Old backups are removed after 30 days or when a file has more backups than the setting keeps.",
            )
        })?;
        let meta = fs::metadata(&backup).map_err(|error| io_error(&backup, error))?;
        self.limits
            .check_input(meta.len())
            .map_err(|error| error.context(format!("while reading {}", backup.display())))?;
        let bytes = fs::read(&backup).map_err(|error| io_error(&backup, error))?;
        decode_file(
            &self.reg,
            &self.limits,
            &self.path,
            &bytes,
            self.password.as_deref(),
        )
        .map_err(|error| {
            ApiError::new(
                app_codes::BACKUP_UNREADABLE,
                format!(
                    "the backup does not open as a save ({}: {})",
                    error.code().as_str(),
                    error.message()
                ),
                "Nothing was written. Pick another backup.",
            )
        })?;

        let mut opts = WriteOptions::new(root);
        opts.keep_last = clamp_keep_backups(keep_backups);
        let verify = |written: &[u8]| -> saveprism_core::Result<()> {
            if written == bytes.as_slice() {
                Ok(())
            } else {
                Err(Error::new(
                    codes::CORE_EDIT_FAILED,
                    "the written file is not the same as the backup",
                ))
            }
        };
        safe_write(&self.path, &bytes, &opts, &verify)?;
        let mut restored = Session::open_path(
            self.path.clone(),
            self.password.clone(),
            self.backup_root.clone(),
            self.renpy_keys.clone(),
            None,
        )?;
        restored.pasted = self.pasted;
        Ok(restored)
    }
}
