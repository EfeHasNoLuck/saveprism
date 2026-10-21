//! Safe writing: backup, temporary file, atomic replace, verification (signatures only for now).
//!
//! Note: `std::path::Path` is always written in full here so it is not confused with the
//! document [`Path`](crate::Path).

use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Error, Result, codes};

/// Options of [`safe_write`]. Use [`WriteOptions::new`] and adjust the public fields.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOptions {
    /// Directory that holds the backups (never next to the save).
    pub backup_root: PathBuf,
    /// Number of backups kept per file.
    pub keep_last: usize,
    /// Backups older than this are pruned, if set.
    pub max_age: Option<Duration>,
}

impl WriteOptions {
    /// Options with the default retention: the last 10 backups, at most 30 days old.
    pub fn new(backup_root: PathBuf) -> Self {
        Self {
            backup_root,
            keep_last: 10,
            max_age: Some(Duration::from_secs(30 * 24 * 60 * 60)),
        }
    }
}

/// What [`safe_write`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReport {
    /// The backup that was made, if the target existed.
    pub backup_path: Option<PathBuf>,
    /// Number of bytes written to the target.
    pub bytes_written: u64,
    /// Number of old backups removed.
    pub pruned: usize,
}

/// Writes `bytes` to `target` safely: backup, temporary file, atomic replace, then `verify` on the
/// bytes read back; on a failed verification the backup is restored.
pub fn safe_write(
    _target: &std::path::Path,
    _bytes: &[u8],
    _opts: &WriteOptions,
    _verify: &dyn Fn(&[u8]) -> Result<()>,
) -> Result<WriteReport> {
    Err(Error::new(
        codes::CORE_NOT_IMPLEMENTED,
        "safe_write is not implemented yet",
    ))
}

/// The application's backup directory (shared by the CLI and the desktop app), if one can be determined.
pub fn default_backup_root() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_options_defaults() {
        let opts = WriteOptions::new(PathBuf::from("backups"));
        assert_eq!(opts.keep_last, 10);
        assert_eq!(opts.max_age, Some(Duration::from_secs(30 * 24 * 3600)));
        assert_eq!(opts.backup_root, PathBuf::from("backups"));
    }

    #[test]
    fn safe_write_is_not_implemented_yet() {
        let opts = WriteOptions::new(PathBuf::from("backups"));
        let error = safe_write(
            std::path::Path::new("save.dat"),
            b"data",
            &opts,
            &|_bytes: &[u8]| Ok(()),
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_NOT_IMPLEMENTED);
    }

    #[test]
    fn default_backup_root_is_a_stub() {
        assert_eq!(default_backup_root(), None);
    }
}
