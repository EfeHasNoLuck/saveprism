//! Folder choices and the background photo of the Settings page.
//!
//! The app keeps its own files (pasted codes, the background photo) in an app folder and the
//! backups in a backup folder. Both may be moved from Settings; the checks live here so they can be
//! tested without a window: a picked folder must exist, be writable, and must never be the folder of
//! the open save (that would put SavePrism's files next to the player's save). Moving the app folder
//! moves the pasted codes and the background photo to the new place. Nothing here writes a save.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::prelude::*;
use saveprism_core::io::default_backup_root;
use serde::Serialize;

use super::{ApiError, app_codes};

/// Largest background photo accepted, in bytes (20 MB).
pub const MAX_BACKGROUND_BYTES: u64 = 20 * 1024 * 1024;

/// The folders SavePrism uses when the player has not changed them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultPaths {
    /// Folder of the pasted codes, settings and the background photo.
    pub app_dir: String,
    /// Folder of the backups.
    pub backup_dir: String,
}

/// The background photo as the window can show it: a data URL and the file's name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImage {
    /// File name inside the background folder (`background.png`, `.jpg` or `.webp`).
    pub name: String,
    /// MIME type of the image.
    pub mime: String,
    /// `data:<mime>;base64,...`
    pub data_url: String,
}

/// The default folders, from the app's backup root.
pub fn default_paths() -> Result<DefaultPaths, ApiError> {
    let backup = default_backup_root().ok_or_else(|| {
        ApiError::new(
            app_codes::FOLDER,
            "no app data folder is known for this system",
            "Pick a folder in Settings.",
        )
    })?;
    let app = backup.parent().ok_or_else(|| {
        ApiError::new(
            app_codes::FOLDER,
            "no app folder is known for this system",
            "Pick a folder in Settings.",
        )
    })?;
    Ok(DefaultPaths {
        app_dir: app.to_string_lossy().into_owned(),
        backup_dir: backup.to_string_lossy().into_owned(),
    })
}

fn folder_error(message: String, hint: &str) -> ApiError {
    ApiError::new(app_codes::FOLDER, message, hint)
}

/// True when the two paths name the same folder (canonical when both exist).
fn same_folder(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => a == b,
    }
}

/// Checks a folder the player picked: it must be an existing, writable directory and not the folder
/// of the save that is open. Returns the canonical path.
pub fn check_folder(folder: &Path, save_path: Option<&Path>) -> Result<PathBuf, ApiError> {
    if !folder.exists() {
        return Err(folder_error(
            format!("\"{}\" does not exist", folder.display()),
            "Pick a folder that already exists.",
        ));
    }
    if !folder.is_dir() {
        return Err(folder_error(
            format!("\"{}\" is not a folder", folder.display()),
            "Pick a folder, not a file.",
        ));
    }
    if let Some(save) = save_path
        && let Some(parent) = save.parent()
        && same_folder(folder, parent)
    {
        return Err(folder_error(
            format!("\"{}\" is the folder of the open save", folder.display()),
            "Pick another folder. SavePrism never puts its own files next to a save.",
        ));
    }
    let probe = tempfile::Builder::new()
        .prefix(".saveprism-check-")
        .suffix(".tmp")
        .tempfile_in(folder)
        .map_err(|error| {
            folder_error(
                format!("\"{}\" cannot be written to: {error}", folder.display()),
                "Pick a folder you can write to.",
            )
        })?;
    // Drop removes the probe file again.
    drop(probe);
    fs::canonicalize(folder).map_err(|error| {
        folder_error(
            format!("\"{}\" cannot be used: {error}", folder.display()),
            "Pick another folder.",
        )
    })
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), ApiError> {
    fs::create_dir_all(to).map_err(|error| {
        folder_error(
            format!("cannot create \"{}\": {error}", to.display()),
            "Pick a folder you can write to.",
        )
    })?;
    let entries = fs::read_dir(from).map_err(|error| {
        folder_error(
            format!("cannot read \"{}\": {error}", from.display()),
            "Pick another folder.",
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            folder_error(
                format!("cannot read \"{}\": {error}", from.display()),
                "Pick another folder.",
            )
        })?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|error| {
                folder_error(
                    format!("cannot copy to \"{}\": {error}", target.display()),
                    "Pick a folder you can write to.",
                )
            })?;
        }
    }
    Ok(())
}

/// Moves the app's own folders (`pasted`, `background`) from `old` to `new`; a missing source is
/// fine. Returns the canonical new folder.
pub fn set_app_folder(
    new_folder: &Path,
    old_folder: Option<&Path>,
    save_path: Option<&Path>,
) -> Result<PathBuf, ApiError> {
    let new = check_folder(new_folder, save_path)?;
    if let Some(old) = old_folder
        && old.exists()
        && !same_folder(old, &new)
    {
        for name in ["pasted", "background"] {
            let source = old.join(name);
            if source.is_dir() {
                let target = new.join(name);
                if target.exists() {
                    copy_dir(&source, &target)?;
                    fs::remove_dir_all(&source).map_err(|error| {
                        folder_error(
                            format!("cannot clean \"{}\": {error}", source.display()),
                            "The files were copied; you may remove the old folder by hand.",
                        )
                    })?;
                } else {
                    fs::rename(&source, &target).or_else(|_| {
                        copy_dir(&source, &target)?;
                        fs::remove_dir_all(&source).map_err(|error| {
                            folder_error(
                                format!("cannot clean \"{}\": {error}", source.display()),
                                "The files were copied; you may remove the old folder by hand.",
                            )
                        })
                    })?;
                }
            }
        }
    }
    Ok(new)
}

/// The background folder inside the app folder.
pub fn background_dir(app_dir: &Path) -> PathBuf {
    app_dir.join("background")
}

/// The image type of `bytes` by its first bytes, or `None` when it is not a PNG, JPEG or WebP.
pub fn background_kind(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("png", "image/png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("jpg", "image/jpeg"))
    } else if bytes.get(0..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(("webp", "image/webp"))
    } else {
        None
    }
}

/// Refuses an image above the size limit.
pub fn check_background_size(len: u64) -> Result<(), ApiError> {
    if len > MAX_BACKGROUND_BYTES {
        return Err(ApiError::new(
            app_codes::BACKGROUND,
            format!("the image is larger than 20 MB ({len} bytes)"),
            "Pick a smaller image.",
        ));
    }
    Ok(())
}

fn existing_background(app_dir: &Path) -> Option<(PathBuf, &'static str)> {
    let dir = background_dir(app_dir);
    for (ext, mime) in [
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("webp", "image/webp"),
    ] {
        let path = dir.join(format!("background.{ext}"));
        if path.is_file() {
            return Some((path, mime));
        }
    }
    None
}

fn read_image(path: &Path, mime: &'static str, name: String) -> Result<BackgroundImage, ApiError> {
    let bytes = fs::read(path).map_err(|error| {
        ApiError::new(
            app_codes::BACKGROUND,
            format!("cannot read \"{}\": {error}", path.display()),
            "Choose the image again.",
        )
    })?;
    Ok(BackgroundImage {
        name,
        mime: mime.to_string(),
        data_url: format!("data:{mime};base64,{}", BASE64_STANDARD.encode(bytes)),
    })
}

/// The background photo currently stored in the app folder, when there is one.
pub fn load_background(app_dir: &Path) -> Result<Option<BackgroundImage>, ApiError> {
    match existing_background(app_dir) {
        Some((path, mime)) => {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            read_image(&path, mime, name).map(Some)
        }
        None => Ok(None),
    }
}

/// Copies `source` into the app folder as the background photo, replacing the previous one.
pub fn import_background(source: &Path, app_dir: &Path) -> Result<BackgroundImage, ApiError> {
    let meta = fs::metadata(source).map_err(|error| {
        ApiError::new(
            app_codes::BACKGROUND,
            format!("cannot read \"{}\": {error}", source.display()),
            "Pick the image again.",
        )
    })?;
    if !meta.is_file() {
        return Err(ApiError::new(
            app_codes::BACKGROUND,
            format!("\"{}\" is not a file", source.display()),
            "Pick an image file (PNG, JPEG or WebP).",
        ));
    }
    check_background_size(meta.len())?;
    let bytes = fs::read(source).map_err(|error| {
        ApiError::new(
            app_codes::BACKGROUND,
            format!("cannot read \"{}\": {error}", source.display()),
            "Pick the image again.",
        )
    })?;
    check_background_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    let (ext, mime) = background_kind(&bytes).ok_or_else(|| {
        ApiError::new(
            app_codes::BACKGROUND,
            "the file is not a PNG, JPEG or WebP image",
            "Pick a PNG, JPEG or WebP image.",
        )
    })?;

    let dir = background_dir(app_dir);
    fs::create_dir_all(&dir).map_err(|error| {
        folder_error(
            format!("cannot create \"{}\": {error}", dir.display()),
            "Pick another app folder.",
        )
    })?;
    for old in ["png", "jpg", "jpeg", "webp"] {
        let path = dir.join(format!("background.{old}"));
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                folder_error(
                    format!("cannot replace \"{}\": {error}", path.display()),
                    "Pick another app folder.",
                )
            })?;
        }
    }

    let name = format!("background.{ext}");
    let target = dir.join(&name);
    let mut temp = tempfile::Builder::new()
        .prefix(".saveprism-bg-")
        .suffix(".tmp")
        .tempfile_in(&dir)
        .map_err(|error| {
            folder_error(
                format!("cannot write in \"{}\": {error}", dir.display()),
                "Pick another app folder.",
            )
        })?;
    temp.write_all(&bytes).map_err(|error| {
        folder_error(
            format!("cannot write the image: {error}"),
            "Pick another app folder.",
        )
    })?;
    temp.persist(&target).map_err(|error| {
        folder_error(
            format!("cannot keep the image: {}", error.error),
            "Pick another app folder.",
        )
    })?;

    Ok(BackgroundImage {
        name,
        mime: mime.to_string(),
        data_url: format!("data:{mime};base64,{}", BASE64_STANDARD.encode(&bytes)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{clamp_keep_backups, clamp_keep_days};

    fn temp_dir(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("saveprism-folders-{name}-"))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn retention_settings_never_drop_below_one() {
        assert_eq!(clamp_keep_backups(Some(0)), 1);
        assert_eq!(clamp_keep_backups(Some(51)), 50);
        assert_eq!(clamp_keep_backups(None), 10);
        assert_eq!(clamp_keep_days(Some(0)), 1);
        assert_eq!(clamp_keep_days(Some(366)), 365);
        assert_eq!(clamp_keep_days(None), 30);
    }

    #[test]
    fn refuses_the_folder_of_the_save() {
        let dir = temp_dir("save");
        let save = dir.path().join("slot1.json");
        fs::write(&save, b"{}").unwrap();
        let error = check_folder(dir.path(), Some(&save)).err().unwrap();
        assert_eq!(error.code, app_codes::FOLDER);
    }

    #[test]
    fn refuses_a_file_and_a_missing_folder() {
        let dir = temp_dir("file");
        let file = dir.path().join("note.txt");
        fs::write(&file, b"x").unwrap();
        assert!(check_folder(&file, None).is_err());
        assert!(check_folder(&dir.path().join("missing"), None).is_err());
    }

    #[test]
    fn accepts_a_writable_folder() {
        let dir = temp_dir("ok");
        let picked = dir.path().join("backups");
        fs::create_dir(&picked).unwrap();
        let canonical = check_folder(&picked, None).unwrap();
        assert!(canonical.is_dir());
    }

    #[test]
    fn app_folder_move_carries_pasted_codes_and_the_photo() {
        let dir = temp_dir("move");
        let old = dir.path().join("old");
        let new = dir.path().join("new");
        fs::create_dir_all(old.join("pasted")).unwrap();
        fs::create_dir_all(old.join("background")).unwrap();
        fs::create_dir(&new).unwrap();
        fs::write(old.join("pasted").join("code-1.txt"), b"hello").unwrap();
        fs::write(old.join("background").join("background.png"), b"png").unwrap();
        let moved = set_app_folder(&new, Some(&old), None).unwrap();
        assert_eq!(moved, fs::canonicalize(&new).unwrap());
        assert!(new.join("pasted").join("code-1.txt").is_file());
        assert!(new.join("background").join("background.png").is_file());
        assert!(!old.join("pasted").exists());
    }

    #[test]
    fn background_copy_replaces_the_previous_image() {
        let dir = temp_dir("background");
        let app = dir.path().join("app");
        fs::create_dir(&app).unwrap();
        let png = dir.path().join("one.png");
        fs::write(&png, b"\x89PNG\r\n\x1a\nsmall").unwrap();
        let first = import_background(&png, &app).unwrap();
        assert_eq!(first.name, "background.png");
        assert_eq!(first.mime, "image/png");
        assert!(first.data_url.starts_with("data:image/png;base64,"));
        let webp = dir.path().join("two.webp");
        fs::write(&webp, b"RIFF0000WEBPsmall").unwrap();
        let second = import_background(&webp, &app).unwrap();
        assert_eq!(second.name, "background.webp");
        assert!(!background_dir(&app).join("background.png").exists());
        assert!(background_dir(&app).join("background.webp").is_file());
        assert_eq!(
            load_background(&app).unwrap().unwrap().name,
            "background.webp"
        );
    }

    #[test]
    fn background_must_be_a_known_image_under_the_size_limit() {
        let dir = temp_dir("bad");
        let app = dir.path().join("app");
        fs::create_dir(&app).unwrap();
        let text = dir.path().join("note.txt");
        fs::write(&text, b"not an image").unwrap();
        assert_eq!(
            import_background(&text, &app).unwrap_err().code,
            app_codes::BACKGROUND
        );
        assert!(check_background_size(MAX_BACKGROUND_BYTES).is_ok());
        assert_eq!(
            check_background_size(MAX_BACKGROUND_BYTES + 1)
                .unwrap_err()
                .code,
            app_codes::BACKGROUND
        );
    }
}
