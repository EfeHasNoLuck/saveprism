//! Integration tests of `safe_write` (backup, atomic replace, verification, pruning).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use saveprism_core::io::{WriteOptions, find_backup, list_backups, safe_write};
use saveprism_core::{Error, Result, codes};

fn ok(_bytes: &[u8]) -> Result<()> {
    Ok(())
}

fn reject(_bytes: &[u8]) -> Result<()> {
    Err(Error::new(codes::CORE_EDIT_FAILED, "rejected by test"))
}

/// Save dir and a separate backup root inside one temporary directory.
fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let saves = tmp.path().join("saves");
    let backups = tmp.path().join("backups");
    fs::create_dir_all(&saves).unwrap();
    (tmp, saves, backups)
}

fn backup_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(folders) = fs::read_dir(root) {
        for folder in folders.flatten() {
            for file in fs::read_dir(folder.path()).unwrap().flatten() {
                if file.path().extension().is_some_and(|e| e == "bak") {
                    found.push(file.path());
                }
            }
        }
    }
    found.sort();
    found
}

fn only_backup_folder(root: &Path) -> PathBuf {
    let folders: Vec<_> = fs::read_dir(root).unwrap().flatten().collect();
    assert_eq!(folders.len(), 1);
    folders[0].path()
}

fn no_temp_leftovers(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !(name.starts_with(".saveprism-") && name.ends_with(".tmp")),
            "leftover temporary file: {name}"
        );
    }
}

#[test]
fn new_file_is_written_without_a_backup() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    let opts = WriteOptions::new(backups.clone());

    let report = safe_write(&target, b"hello", &opts, &ok).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"hello");
    assert_eq!(report.backup_path, None);
    assert_eq!(report.bytes_written, 5);
    assert_eq!(report.pruned, 0);
    assert!(backup_files(&backups).is_empty());
    no_temp_leftovers(&saves);
}

#[test]
fn existing_file_is_backed_up_with_its_old_bytes() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    fs::write(&target, b"old bytes").unwrap();
    let opts = WriteOptions::new(backups.clone());

    let report = safe_write(&target, b"new bytes", &opts, &ok).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"new bytes");
    let backup = report.backup_path.unwrap();
    assert_eq!(fs::read(&backup).unwrap(), b"old bytes");
    assert!(backup.starts_with(&backups));
    assert_eq!(backup.extension().unwrap(), "bak");

    let folder = only_backup_folder(&backups);
    let name = folder.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(name.len(), 16);
    assert!(name.bytes().all(|b| b.is_ascii_hexdigit()));
    let source = fs::read_to_string(folder.join("source.txt")).unwrap();
    assert!(source.contains("slot1.sav"));
    no_temp_leftovers(&saves);
}

#[test]
fn backups_form_a_ring_of_ten() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    fs::write(&target, b"v0").unwrap();
    let opts = WriteOptions::new(backups.clone());

    let mut pruned = 0;
    for i in 1..=12 {
        let report = safe_write(&target, format!("v{i}").as_bytes(), &opts, &ok).unwrap();
        pruned += report.pruned;
    }

    assert_eq!(backup_files(&backups).len(), 10);
    assert_eq!(pruned, 2);
    assert_eq!(fs::read(&target).unwrap(), b"v12");
}

#[test]
fn old_backups_are_pruned_but_the_newest_and_strangers_stay() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    fs::write(&target, b"v0").unwrap();
    let mut opts = WriteOptions::new(backups.clone());

    safe_write(&target, b"v1", &opts, &ok).unwrap();
    let folder = only_backup_folder(&backups);
    fs::write(folder.join("1000.bak"), b"ancient").unwrap();
    fs::write(folder.join("2000.bak"), b"ancient").unwrap();
    fs::write(folder.join("notes.txt"), b"mine").unwrap();

    opts.max_age = Some(Duration::from_secs(24 * 3600));
    let report = safe_write(&target, b"v2", &opts, &ok).unwrap();
    assert_eq!(report.pruned, 2);
    assert!(!folder.join("1000.bak").exists());
    assert!(!folder.join("2000.bak").exists());
    assert!(folder.join("notes.txt").exists());
    assert!(folder.join("source.txt").exists());
    assert_eq!(backup_files(&backups).len(), 2);

    // A tiny max_age removes everything but the newest backup.
    std::thread::sleep(Duration::from_millis(30));
    opts.max_age = Some(Duration::ZERO);
    let report = safe_write(&target, b"v3", &opts, &ok).unwrap();
    assert_eq!(report.pruned, 2);
    let left = backup_files(&backups);
    assert_eq!(left.len(), 1);
    assert_eq!(Some(&left[0]), report.backup_path.as_ref());
}

#[test]
fn failed_verification_restores_the_original_bytes() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    fs::write(&target, b"original").unwrap();
    let opts = WriteOptions::new(backups.clone());

    let error = safe_write(&target, b"broken", &opts, &reject).unwrap_err();

    assert_eq!(error.code(), codes::IO_VERIFY_RESTORED);
    assert_eq!(error.code().as_str(), "SP-IO-004");
    assert!(error.report().contains("rejected by test"));
    assert_eq!(fs::read(&target).unwrap(), b"original");
    no_temp_leftovers(&saves);
}

#[test]
fn failed_verification_of_a_new_file_removes_it() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("fresh.sav");
    let opts = WriteOptions::new(backups.clone());

    let error = safe_write(&target, b"broken", &opts, &reject).unwrap_err();

    assert_eq!(error.code(), codes::CORE_EDIT_FAILED);
    assert!(!target.exists());
    assert!(backup_files(&backups).is_empty());
    no_temp_leftovers(&saves);
}

#[test]
fn read_only_target_is_an_error_not_a_panic() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("locked.sav");
    fs::write(&target, b"locked").unwrap();
    let mut permissions = fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&target, permissions.clone()).unwrap();
    let opts = WriteOptions::new(backups.clone());

    let result = safe_write(&target, b"new", &opts, &ok);

    // Restore write access so the temporary directory can be removed.
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&target, permissions).unwrap();

    let error = result.unwrap_err();
    assert_eq!(error.code(), codes::IO_REPLACE);
    assert_eq!(fs::read(&target).unwrap(), b"locked");
    no_temp_leftovers(&saves);
}

#[test]
fn backup_root_next_to_the_save_is_refused() {
    let (_tmp, saves, _backups) = setup();
    let target = saves.join("slot1.sav");
    fs::write(&target, b"data").unwrap();
    let opts = WriteOptions::new(saves.clone());

    let error = safe_write(&target, b"new", &opts, &ok).unwrap_err();

    assert_eq!(error.code(), codes::IO_NO_BACKUP_ROOT);
    assert_eq!(fs::read(&target).unwrap(), b"data");
}

#[cfg(unix)]
#[test]
fn existing_file_keeps_its_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    fs::write(&target, b"old bytes").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    let opts = WriteOptions::new(backups);

    safe_write(&target, b"new bytes", &opts, &ok).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"new bytes");
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn backups_are_listed_newest_first_and_found_by_name() {
    let (_tmp, saves, backups) = setup();
    let target = saves.join("slot1.sav");
    let opts = WriteOptions::new(backups.clone());
    assert!(list_backups(&backups, &target).is_empty());

    fs::write(&target, b"v1").unwrap();
    safe_write(&target, b"v2", &opts, &ok).unwrap();
    safe_write(&target, b"v3!", &opts, &ok).unwrap();
    let folder = only_backup_folder(&backups);
    fs::write(folder.join("notes.bak.txt"), b"stranger").unwrap();

    let list = list_backups(&backups, &target);
    assert_eq!(list.len(), 2);
    assert!(list[0].created_ms >= list[1].created_ms);
    assert_eq!(fs::read(&list[0].path).unwrap(), b"v2");
    assert_eq!(list[0].byte_size, 2);
    assert_eq!(fs::read(&list[1].path).unwrap(), b"v1");

    let found = find_backup(&backups, &target, &list[1].name).unwrap();
    assert_eq!(found, list[1].path);
    // Only backup names are accepted: no other files, no paths out of the folder.
    assert_eq!(find_backup(&backups, &target, "source.txt"), None);
    assert_eq!(find_backup(&backups, &target, "notes.bak.txt"), None);
    assert_eq!(find_backup(&backups, &target, "../1.bak"), None);
    assert_eq!(find_backup(&backups, &target, "1.bak"), None);
    // Another file has its own folder.
    assert!(list_backups(&backups, &saves.join("slot2.sav")).is_empty());
}
