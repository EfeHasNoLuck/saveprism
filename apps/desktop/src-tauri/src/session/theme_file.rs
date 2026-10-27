//! Reading and writing SavePrism theme files (Settings → Appearance).
//!
//! A theme file is data only: the web side validates the JSON (format, version, colours, ranges)
//! and this module only moves bytes. Writes use the same rules as "Export .json": never over the
//! open save, never a folder, a name ending in `.saveprism-theme.json`, and a temporary file
//! renamed into place so a failed write leaves nothing behind. Reads accept any regular file of at
//! most 64 KB; the content check happens before anything is applied.

use std::fs;
use std::path::{Path as FsPath, PathBuf};

use super::export::write_atomic;
use super::{ApiError, app_codes, same_file};

/// Largest theme file, in bytes (the brief: at most 64 KB).
pub const MAX_THEME_BYTES: u64 = 64 * 1024;
/// The ending every exported theme file must have.
pub const THEME_SUFFIX: &str = ".saveprism-theme.json";

fn theme_error(message: String, hint: &str) -> ApiError {
    ApiError::new(app_codes::THEME_FILE, message, hint)
}

fn is_theme_name(path: &FsPath) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(THEME_SUFFIX)
        })
}

/// Writes `text` to `target` and returns the written path. `save` is the open save file, which the
/// write never replaces.
pub fn write(target: &str, text: &str, save: Option<&FsPath>) -> Result<String, ApiError> {
    let path = PathBuf::from(target);
    if save.is_some_and(|save| same_file(&path, save)) {
        return Err(theme_error(
            format!("\"{}\" is the open save file", path.display()),
            "Pick another file name. The export never replaces the save.",
        ));
    }
    if !is_theme_name(&path) {
        return Err(theme_error(
            format!("\"{}\" does not end in {THEME_SUFFIX}", path.display()),
            "Pick a file name that ends in .saveprism-theme.json.",
        ));
    }
    if fs::metadata(&path).is_ok_and(|meta| meta.is_dir()) {
        return Err(theme_error(
            format!("\"{}\" is a folder", path.display()),
            "Pick a file name inside the folder.",
        ));
    }
    if text.len() as u64 > MAX_THEME_BYTES {
        return Err(theme_error(
            "the theme file is larger than 64 KB".to_string(),
            "Use a smaller theme file.",
        ));
    }
    write_atomic(&path, text.as_bytes())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Reads a theme file of at most 64 KB. Folders and other non-regular files are refused.
pub fn read(source: &str) -> Result<String, ApiError> {
    let path = PathBuf::from(source);
    let meta = fs::metadata(&path).map_err(|error| {
        theme_error(
            format!("cannot read \"{}\": {error}", path.display()),
            "Pick an existing theme file.",
        )
    })?;
    if !meta.is_file() {
        return Err(theme_error(
            format!("\"{}\" is not a file", path.display()),
            "Pick a theme file.",
        ));
    }
    if meta.len() > MAX_THEME_BYTES {
        return Err(theme_error(
            "the theme file is larger than 64 KB".to_string(),
            "Use a smaller theme file.",
        ));
    }
    let bytes = fs::read(&path).map_err(|error| {
        theme_error(
            format!("cannot read \"{}\": {error}", path.display()),
            "Pick another theme file.",
        )
    })?;
    String::from_utf8(bytes).map_err(|_| {
        theme_error(
            format!("\"{}\" is not UTF-8 text", path.display()),
            "Pick a SavePrism theme file.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_theme_file_is_written_and_read_back() {
        let dir = work();
        let target = dir.path().join("blue.saveprism-theme.json");
        let text = r#"{"format":"saveprism-theme","version":1}"#;
        let written = write(target.to_str().unwrap(), text, None).unwrap();
        assert_eq!(written, target.to_string_lossy());
        assert_eq!(read(&written).unwrap(), text);
    }

    #[test]
    fn writing_refuses_the_open_save() {
        let dir = work();
        let save = dir.path().join("slot.saveprism-theme.json");
        fs::write(&save, b"save").unwrap();
        let error = write(save.to_str().unwrap(), "{}", Some(&save)).unwrap_err();
        assert_eq!(error.code, app_codes::THEME_FILE);
        assert_eq!(fs::read(&save).unwrap(), b"save");
    }

    #[test]
    fn writing_refuses_other_names_folders_and_large_text() {
        let dir = work();
        let wrong = dir.path().join("blue.json");
        assert_eq!(
            write(wrong.to_str().unwrap(), "{}", None).unwrap_err().code,
            app_codes::THEME_FILE
        );
        assert_eq!(
            write(dir.path().to_str().unwrap(), "{}", None)
                .unwrap_err()
                .code,
            app_codes::THEME_FILE
        );
        let large = dir.path().join("large.saveprism-theme.json");
        let text = "x".repeat(MAX_THEME_BYTES as usize + 1);
        assert_eq!(
            write(large.to_str().unwrap(), &text, None)
                .unwrap_err()
                .code,
            app_codes::THEME_FILE
        );
        assert!(!large.exists());
    }

    #[test]
    fn names_are_not_case_sensitive() {
        let dir = work();
        let target = dir.path().join("BLUE.SAVEPRISM-THEME.JSON");
        write(target.to_str().unwrap(), "{}", None).unwrap();
        assert!(target.exists());
    }

    #[test]
    fn reading_refuses_folders_missing_files_and_large_files() {
        let dir = work();
        assert_eq!(
            read(dir.path().to_str().unwrap()).unwrap_err().code,
            app_codes::THEME_FILE
        );
        let missing = dir.path().join("missing.saveprism-theme.json");
        assert_eq!(
            read(missing.to_str().unwrap()).unwrap_err().code,
            app_codes::THEME_FILE
        );
        let large = dir.path().join("large.saveprism-theme.json");
        fs::write(&large, "x".repeat(MAX_THEME_BYTES as usize + 1)).unwrap();
        assert_eq!(
            read(large.to_str().unwrap()).unwrap_err().code,
            app_codes::THEME_FILE
        );
    }

    #[test]
    fn reading_refuses_text_that_is_not_utf8() {
        let dir = work();
        let path = dir.path().join("bad.saveprism-theme.json");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(
            read(path.to_str().unwrap()).unwrap_err().code,
            app_codes::THEME_FILE
        );
    }
}
