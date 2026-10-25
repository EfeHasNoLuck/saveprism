//! Save codes: text a browser game gives the player to copy and paste instead of a file.
//!
//! [`Session::open_code`] writes the pasted text to a file of its own in the app data folder
//! (`<app data>/pasted/code-<unix ms>.txt`, next to the backup folder, never next to a save) and
//! opens it the normal way, so detection, the round-trip guard, edits, undo and the JSON view all
//! work unchanged. A code that does not open is removed again; only the newest [`MAX_PASTED`] codes
//! are kept. [`Session::copy_code`] encodes the document (pending edits included), checks every
//! edited value in the result like a save does and returns it as text. Nothing is written by a copy.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path as FsPath, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use saveprism_core::io::default_backup_root;
use saveprism_core::{Error, Limits, codes, pipeline};

use super::{ApiError, Session, app_codes, with_password};

/// Folder of the pasted codes, next to the backup folder.
const PASTED_DIR: &str = "pasted";
/// Most pasted codes kept on disk (the newest ones).
pub const MAX_PASTED: usize = 10;
const CODE_PREFIX: &str = "code-";
const CODE_SUFFIX: &str = ".txt";
/// Names tried when a pasted code of the same millisecond already exists.
const NAME_TRIES: u64 = 1000;
/// What a player can do about a code that does not open.
const PASTE_HINT: &str = "Check that you copied the whole code, from its first character to its last. SavePrism reads codes such as LZ-String Base64 (RPG Maker MV, Twine SugarCube) and JSON.";

/// True when `bytes` are text a player can copy and paste: UTF-8 without control characters
/// other than tab and line breaks, and not empty.
pub(super) fn is_text_code(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            !text.is_empty()
                && !text
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
        }
        Err(_) => false,
    }
}

/// The folder of the pasted codes: next to the backup folder, inside the app data folder.
pub(super) fn pasted_dir(backup_root: &FsPath) -> Result<PathBuf, ApiError> {
    match backup_root.parent() {
        Some(app_data) if !app_data.as_os_str().is_empty() => Ok(app_data.join(PASTED_DIR)),
        _ => Err(Error::new(
            codes::IO_NO_BACKUP_ROOT,
            format!(
                "no app data folder is known for pasted codes (backup folder \"{}\")",
                backup_root.display()
            ),
        )
        .into()),
    }
}

/// The time stamp of a pasted code's file name (`code-<digits>.txt`), else `None`.
fn code_stamp(name: &str) -> Option<u64> {
    let digits = name.strip_prefix(CODE_PREFIX)?.strip_suffix(CODE_SUFFIX)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

fn temp_error(dir: &FsPath, source: std::io::Error) -> ApiError {
    Error::new(
        codes::IO_TEMP_WRITE,
        format!(
            "cannot write a temporary file in \"{}\": {source}",
            dir.display()
        ),
    )
    .with_source(source)
    .into()
}

/// Writes `bytes` to a new `code-<unix ms>.txt` in `dir` through a temporary file and a rename that
/// never replaces an existing file. Returns the new file's path. `dir` must be a real folder: a
/// symbolic link or junction there could lead the write and the pruning somewhere else.
fn write_new_code(dir: &FsPath, bytes: &[u8]) -> Result<PathBuf, ApiError> {
    fs::create_dir_all(dir).map_err(|source| temp_error(dir, source))?;
    let real_dir = fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_dir());
    if !real_dir {
        return Err(Error::new(
            codes::IO_TEMP_WRITE,
            format!(
                "\"{}\" is a link or not a folder, so pasted codes are not written there",
                dir.display()
            ),
        )
        .into());
    }
    let mut temp = tempfile::Builder::new()
        .prefix(".saveprism-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .map_err(|source| temp_error(dir, source))?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|source| temp_error(dir, source))?;
    let start = now_ms();
    for step in 0..NAME_TRIES {
        let target = dir.join(format!(
            "{CODE_PREFIX}{}{CODE_SUFFIX}",
            start.saturating_add(step)
        ));
        match temp.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(failed) if failed.error.kind() == ErrorKind::AlreadyExists => temp = failed.file,
            // Dropping the temporary file deletes it.
            Err(failed) => {
                return Err(Error::new(
                    codes::IO_REPLACE,
                    format!("cannot write \"{}\": {}", target.display(), failed.error),
                )
                .with_source(failed.error)
                .into());
            }
        }
    }
    Err(Error::new(
        codes::IO_REPLACE,
        format!(
            "no free file name for a pasted code in \"{}\"",
            dir.display()
        ),
    )
    .into())
}

/// Removes all but the newest [`MAX_PASTED`] pasted codes in `dir`. `keep` (the code just opened)
/// is never removed, and files with other names are never touched. Best effort: a file that cannot
/// be removed stays.
fn prune_codes(dir: &FsPath, keep: &FsPath) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let keep_name = keep.file_name();
    let mut older: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            if Some(name.as_os_str()) == keep_name {
                return None;
            }
            let stamp = code_stamp(name.to_str()?)?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then(|| (stamp, entry.path()))
        })
        .collect();
    older.sort_by(|a, b| b.cmp(a));
    for (_, path) in older.into_iter().skip(MAX_PASTED.saturating_sub(1)) {
        let _ = fs::remove_file(path);
    }
}

impl Session {
    /// Opens a pasted save code. Surrounding whitespace is trimmed; empty text and text over the
    /// input limit are refused. The text is written as UTF-8 to its own file in the pasted-codes
    /// folder next to the backup folder (`backup_root` as in [`Session::open`]) and opened from
    /// there. A code that does not open is removed again and the open error comes back with a hint
    /// for pasted codes. Only the newest [`MAX_PASTED`] codes are kept.
    pub fn open_code(text: &str, backup_root: Option<PathBuf>) -> Result<Session, ApiError> {
        Session::open_code_with_source(text, backup_root, super::RenpyKeySource::Default)
    }

    /// [`Session::open_code`] with an explicit Ren'Py key source, so tests never read the player's
    /// real `security_keys.txt`.
    pub(super) fn open_code_with_source(
        text: &str,
        backup_root: Option<PathBuf>,
        keys: super::RenpyKeySource,
    ) -> Result<Session, ApiError> {
        let code = text.trim();
        if code.is_empty() {
            return Err(ApiError::new(
                app_codes::EMPTY_CODE,
                "the pasted text is empty",
                "Paste the save code your game gave you, then open it.",
            ));
        }
        Limits::default()
            .check_input(u64::try_from(code.len()).unwrap_or(u64::MAX))
            .map_err(|error| error.context("while reading a pasted code"))?;
        let root = backup_root
            .clone()
            .or_else(default_backup_root)
            .ok_or_else(|| {
                ApiError::from(Error::new(
                    codes::IO_NO_BACKUP_ROOT,
                    "no app data folder is known for this system",
                ))
            })?;
        let dir = pasted_dir(&root)?;
        let path = write_new_code(&dir, code.as_bytes())?;
        match Session::open_path(path.clone(), None, backup_root, keys, None) {
            Ok(mut session) => {
                session.pasted = true;
                prune_codes(&dir, &path);
                Ok(session)
            }
            Err(error) => {
                let _ = fs::remove_file(&path);
                Err(ApiError {
                    hint: PASTE_HINT.to_string(),
                    ..error
                })
            }
        }
    }

    /// The document as a save code (pending edits included): encoded with the session's recipe,
    /// decoded again with every edited value checked, and returned as text. Formats that are not
    /// text (binary or encrypted saves) are refused. Nothing is written and the pending edits stay.
    pub fn copy_code(&self) -> Result<String, ApiError> {
        if !self.editable {
            return Err(self.read_only_error());
        }
        let recipe = with_password(&self.recipe, self.password.as_deref());
        let bytes = pipeline::encode(&self.doc, &recipe, &self.reg)?;
        self.verify_edits(&recipe, &bytes)?;
        if !is_text_code(&bytes) {
            return Err(ApiError::new(
                app_codes::NOT_TEXT_CODE,
                "this save format is not text, so it has no code to copy",
                "Use Save as file instead.",
            ));
        }
        String::from_utf8(bytes).map_err(|_| ApiError::internal())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_codes_are_utf8_without_control_characters() {
        assert!(is_text_code(b"N4IgzgnmAuCmC2IBcoD6YCGA3WBRAdhgEYA2sAJstAE4CusANCKvLPrQcWZUjfU6jYBjAPa18cap1IUqdRsyIZo0MgGExE5AEYATAIDuAS3wbx0HdoGwwQjA"));
        assert!(is_text_code("{\n\t\"name\": \"Şükrü\"\r\n}".as_bytes()));
        assert!(!is_text_code(b""));
        assert!(!is_text_code(b"\x78\x01\x65"));
        assert!(!is_text_code(&[0xff, 0xfe, 0x41]));
    }

    #[test]
    fn only_code_names_have_a_stamp() {
        assert_eq!(
            code_stamp("code-1761234567890.txt"),
            Some(1_761_234_567_890)
        );
        for name in [
            "code-.txt",
            "code-12.json",
            "code-1a.txt",
            "notes.txt",
            "code-12.txt.bak",
        ] {
            assert_eq!(code_stamp(name), None, "{name}");
        }
    }

    #[test]
    fn the_pasted_folder_sits_next_to_the_backup_folder() {
        let root = PathBuf::from("C:/Users/u/AppData/Local/SavePrism/backups");
        assert_eq!(
            pasted_dir(&root).unwrap(),
            PathBuf::from("C:/Users/u/AppData/Local/SavePrism/pasted")
        );
        assert!(pasted_dir(FsPath::new("backups")).is_err());
    }
}
