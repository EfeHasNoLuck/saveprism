//! Safe writing: backup, temporary file, atomic replace, verification.
//!
//! Note: `std::path::Path` is always written in full here so it is not confused with the
//! document [`Path`](crate::Path).

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result, codes};

/// Highest same-millisecond counter tried before a backup is given up.
const MAX_BACKUP_COLLISIONS: u32 = 10_000;

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

/// One backup of a file, as [`list_backups`] finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupInfo {
    /// Full path of the backup file.
    pub path: PathBuf,
    /// File name of the backup (`<unix_ms>.bak` or `<unix_ms>-<n>.bak`).
    pub name: String,
    /// When the backup was made, in milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// Size of the backup in bytes.
    pub byte_size: u64,
}

/// The backups of `target` under `backup_root`, newest first. A missing folder gives an empty list;
/// files that are not backups are left out.
pub fn list_backups(backup_root: &std::path::Path, target: &std::path::Path) -> Vec<BackupInfo> {
    let Ok(entries) = fs::read_dir(backup_folder(backup_root, target)) else {
        return Vec::new();
    };
    let mut backups: Vec<(u64, u32, BackupInfo)> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(fs::Metadata::is_file)?;
            let name = entry.file_name().to_str()?.to_string();
            let (ms, counter) = parse_backup_name(&name)?;
            let info = BackupInfo {
                path: entry.path(),
                name,
                created_ms: ms,
                byte_size: meta.len(),
            };
            Some((ms, counter, info))
        })
        .collect();
    backups.sort_by_key(|&(ms, counter, _)| std::cmp::Reverse((ms, counter)));
    backups.into_iter().map(|(_, _, info)| info).collect()
}

/// The backup of `target` called `name`, if `name` is a backup file name and that file exists.
/// Only names made by the backup itself are accepted, so `name` cannot point outside the folder;
/// a symbolic link is not a backup either, so it cannot lead out of the folder.
pub fn find_backup(
    backup_root: &std::path::Path,
    target: &std::path::Path,
    name: &str,
) -> Option<PathBuf> {
    parse_backup_name(name)?;
    let path = backup_folder(backup_root, target).join(name);
    fs::symlink_metadata(&path)
        .is_ok_and(|meta| meta.is_file())
        .then_some(path)
}

/// Writes `bytes` to `target` safely: backup, temporary file, atomic replace, then `verify` on the
/// bytes read back; on a failed verification the backup is restored.
///
/// An existing target is always backed up first (policy A). A target that does not exist yet gets
/// no backup; if its verification fails the new file is removed again.
pub fn safe_write(
    target: &std::path::Path,
    bytes: &[u8],
    opts: &WriteOptions,
    verify: &dyn Fn(&[u8]) -> Result<()>,
) -> Result<WriteReport> {
    let dir = parent_dir(target);
    if same_directory(&dir, &opts.backup_root) {
        return Err(Error::new(
            codes::IO_NO_BACKUP_ROOT,
            format!(
                "the backup directory {} is the same as the directory of the target",
                opts.backup_root.display()
            ),
        ));
    }

    let existed = match fs::metadata(target) {
        Ok(meta) if meta.is_dir() => {
            return Err(Error::new(
                codes::IO_REPLACE,
                format!("the target {} is a directory", target.display()),
            ));
        }
        Ok(meta) if meta.permissions().readonly() => {
            return Err(Error::new(
                codes::IO_REPLACE,
                format!("the target {} is read-only", target.display()),
            ));
        }
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(Error::new(
                codes::IO_READ,
                format!("cannot inspect the target {}", target.display()),
            )
            .with_source(error));
        }
    };

    let backup_path = if existed {
        Some(make_backup(target, opts)?)
    } else {
        None
    };

    atomic_write(target, &dir, bytes)?;

    let verified = fs::read(target)
        .map_err(|error| {
            Error::new(
                codes::IO_READ,
                format!("cannot read back {}", target.display()),
            )
            .with_source(error)
        })
        .and_then(|written| verify(&written));
    if let Err(failure) = verified {
        return Err(recover(target, &dir, backup_path.as_deref(), failure));
    }

    let pruned = prune_all(&opts.backup_root, opts, now_ms());
    Ok(WriteReport {
        backup_path,
        bytes_written: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        pruned,
    })
}

/// The application's backup directory (shared by the CLI and the desktop app), if one can be determined.
pub fn default_backup_root() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name);
    if cfg!(windows) {
        backup_root_windows(var("LOCALAPPDATA"))
    } else if cfg!(target_os = "macos") {
        backup_root_macos(var("HOME"))
    } else {
        backup_root_linux(var("XDG_DATA_HOME"), var("HOME"))
    }
}

/// Treats an unset or empty environment value as missing.
fn non_empty(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

#[allow(dead_code)]
fn backup_root_windows(local_app_data: Option<OsString>) -> Option<PathBuf> {
    non_empty(local_app_data).map(|base| base.join("SavePrism").join("backups"))
}

#[allow(dead_code)]
fn backup_root_macos(home: Option<OsString>) -> Option<PathBuf> {
    non_empty(home).map(|base| {
        base.join("Library")
            .join("Application Support")
            .join("SavePrism")
            .join("backups")
    })
}

#[allow(dead_code)]
fn backup_root_linux(xdg_data_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = non_empty(xdg_data_home)
        .filter(|path| path.is_absolute())
        .or_else(|| non_empty(home).map(|h| h.join(".local").join("share")))?;
    Some(base.join("saveprism").join("backups"))
}

/// FNV-1a 64-bit hash (stable across Rust versions, unlike `DefaultHasher`).
fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Milliseconds since the Unix epoch (0 if the clock is before it).
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The directory that holds the target (`.` for a bare file name).
fn parent_dir(target: &std::path::Path) -> PathBuf {
    match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Canonical form when the path exists, otherwise an absolute form, otherwise the path itself.
fn normalized(path: &std::path::Path) -> PathBuf {
    fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn same_directory(a: &std::path::Path, b: &std::path::Path) -> bool {
    normalized(a) == normalized(b)
}

/// The folder that holds the backups of `target`: `<backup_root>/<fnv64-hex(full path)>`.
fn backup_folder(backup_root: &std::path::Path, target: &std::path::Path) -> PathBuf {
    let full_text = normalized(target).to_string_lossy().into_owned();
    backup_root.join(format!("{:016x}", fnv1a_64(full_text.as_bytes())))
}

/// Copies the current target into `<backup_root>/<fnv64-hex(full path)>/<unix_ms>.bak`.
fn make_backup(target: &std::path::Path, opts: &WriteOptions) -> Result<PathBuf> {
    let backup_error = |message: String, source: std::io::Error| {
        Error::new(codes::IO_BACKUP, message).with_source(source)
    };

    let old = fs::read(target).map_err(|error| {
        backup_error(
            format!("cannot read {} for the backup", target.display()),
            error,
        )
    })?;

    let full_text = normalized(target).to_string_lossy().into_owned();
    let folder = backup_folder(&opts.backup_root, target);
    fs::create_dir_all(&folder).map_err(|error| {
        backup_error(
            format!("cannot create the backup directory {}", folder.display()),
            error,
        )
    })?;

    let ms = now_ms();
    for counter in 0..MAX_BACKUP_COLLISIONS {
        let name = if counter == 0 {
            format!("{ms}.bak")
        } else {
            format!("{ms}-{counter}.bak")
        };
        let path = folder.join(name);
        let opened = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path);
        let mut file = match opened {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(backup_error(
                    format!("cannot create the backup {}", path.display()),
                    error,
                ));
            }
        };
        let written = file.write_all(&old).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = written {
            let _ = fs::remove_file(&path);
            return Err(backup_error(
                format!("cannot write the backup {}", path.display()),
                error,
            ));
        }
        fs::write(folder.join("source.txt"), full_text.as_bytes()).map_err(|error| {
            backup_error(
                format!("cannot write source.txt in {}", folder.display()),
                error,
            )
        })?;
        return Ok(path);
    }
    Err(Error::new(
        codes::IO_BACKUP,
        format!("no free backup name in {}", folder.display()),
    ))
}

/// Sets the permission bits of `temp` to those of `target`. Does nothing if `target` does not exist yet.
#[cfg(unix)]
fn copy_existing_mode(target: &std::path::Path, temp: &std::path::Path) -> std::io::Result<()> {
    match fs::metadata(target) {
        Ok(meta) => fs::set_permissions(temp, meta.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Writes `bytes` to a temporary file next to `target`, syncs it and atomically replaces `target`.
/// The temporary file never outlives a failure. On Unix the permission bits of an existing target
/// are copied to the new file; the owner and symbolic links of the target are not preserved.
fn atomic_write(target: &std::path::Path, dir: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let temp_error = |message: &str, source: std::io::Error| {
        Error::new(
            codes::IO_TEMP_WRITE,
            format!("{message} in {}", dir.display()),
        )
        .with_source(source)
    };

    let mut temp = tempfile::Builder::new()
        .prefix(".saveprism-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .map_err(|error| temp_error("cannot create a temporary file", error))?;
    temp.write_all(bytes)
        .map_err(|error| temp_error("cannot write the temporary file", error))?;
    temp.flush()
        .map_err(|error| temp_error("cannot flush the temporary file", error))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| temp_error("cannot sync the temporary file", error))?;
    #[cfg(unix)]
    copy_existing_mode(target, temp.path()).map_err(|error| {
        temp_error(
            "cannot copy the permissions of the target to the temporary file",
            error,
        )
    })?;
    temp.persist(target).map_err(|failed| {
        // `failed.file` is dropped here, which deletes the temporary file.
        Error::new(
            codes::IO_REPLACE,
            format!("cannot replace {}", target.display()),
        )
        .with_source(failed.error)
    })?;
    Ok(())
}

/// Handles a failed verification: restores the backup, or removes a file that did not exist before.
fn recover(
    target: &std::path::Path,
    dir: &std::path::Path,
    backup: Option<&std::path::Path>,
    failure: Error,
) -> Error {
    let Some(backup) = backup else {
        // The target is new: leave nothing behind and report the original problem.
        let _ = fs::remove_file(target);
        return failure;
    };

    let restored = fs::read(backup)
        .map_err(|error| {
            Error::new(
                codes::IO_READ,
                format!("cannot read the backup {}", backup.display()),
            )
            .with_source(error)
        })
        .and_then(|old| atomic_write(target, dir, &old));

    let failure_text = format!("verification failed: {}", failure.report());
    match restored {
        Ok(()) => Error::new(
            codes::IO_VERIFY_RESTORED,
            format!(
                "verification of {} failed; the previous content was restored from {}",
                target.display(),
                backup.display()
            ),
        )
        .context(failure_text)
        .with_source(failure),
        Err(restore_failure) => Error::new(
            codes::IO_RESTORE_FAILED,
            format!(
                "verification of {} failed and restoring failed too; the backup is at {}",
                target.display(),
                backup.display()
            ),
        )
        .context(failure_text)
        .context(format!("restore failed: {}", restore_failure.report()))
        .with_source(restore_failure),
    }
}

/// Parses `<digits>.bak` or `<digits>-<digits>.bak` into `(unix_ms, counter)`.
fn parse_backup_name(name: &str) -> Option<(u64, u32)> {
    fn digits(text: &str) -> bool {
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
    }
    let stem = name.strip_suffix(".bak")?;
    match stem.split_once('-') {
        None => digits(stem)
            .then(|| stem.parse().ok())
            .flatten()
            .map(|ms| (ms, 0)),
        Some((ms, counter)) => {
            if digits(ms) && digits(counter) {
                Some((ms.parse().ok()?, counter.parse().ok()?))
            } else {
                None
            }
        }
    }
}

/// Prunes every backup folder under `root`; returns the number of removed backups.
fn prune_all(root: &std::path::Path, opts: &WriteOptions, now_ms: u64) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            removed += prune_folder(&entry.path(), opts, now_ms);
        }
    }
    removed
}

/// Prunes one backup folder: removes backups older than `max_age` or outside the newest
/// `keep_last`; the newest backup always stays. Unrecognised files are never touched.
fn prune_folder(folder: &std::path::Path, opts: &WriteOptions, now_ms: u64) -> usize {
    let Ok(entries) = fs::read_dir(folder) else {
        return 0;
    };
    let mut backups: Vec<(u64, u32, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name();
            let (ms, counter) = parse_backup_name(name.to_str()?)?;
            Some((ms, counter, entry.path()))
        })
        .collect();
    backups.sort();

    let total = backups.len();
    let keep_from = total.saturating_sub(opts.keep_last.max(1));
    let max_age_ms = opts
        .max_age
        .map(|age| u64::try_from(age.as_millis()).unwrap_or(u64::MAX));

    let mut removed = 0;
    for (index, (ms, _, path)) in backups.iter().enumerate() {
        if index + 1 == total {
            continue;
        }
        let too_old = max_age_ms.is_some_and(|limit| now_ms.saturating_sub(*ms) > limit);
        if (index < keep_from || too_old) && fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    removed
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
    fn fnv1a_64_matches_reference_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn backup_names_are_parsed_strictly() {
        assert_eq!(
            parse_backup_name("1700000000000.bak"),
            Some((1_700_000_000_000, 0))
        );
        assert_eq!(parse_backup_name("5-3.bak"), Some((5, 3)));
        assert_eq!(parse_backup_name("source.txt"), None);
        assert_eq!(parse_backup_name("abc.bak"), None);
        assert_eq!(parse_backup_name("-1.bak"), None);
        assert_eq!(parse_backup_name("+1.bak"), None);
        assert_eq!(parse_backup_name("1-.bak"), None);
        assert_eq!(parse_backup_name(".bak"), None);
    }

    #[test]
    fn windows_backup_root_uses_local_app_data() {
        assert_eq!(
            backup_root_windows(Some(OsString::from("base"))),
            Some(PathBuf::from("base").join("SavePrism").join("backups"))
        );
        assert_eq!(backup_root_windows(None), None);
        assert_eq!(backup_root_windows(Some(OsString::new())), None);
    }

    #[test]
    fn macos_backup_root_uses_application_support() {
        assert_eq!(
            backup_root_macos(Some(OsString::from("home"))),
            Some(
                PathBuf::from("home")
                    .join("Library")
                    .join("Application Support")
                    .join("SavePrism")
                    .join("backups")
            )
        );
        assert_eq!(backup_root_macos(None), None);
    }

    #[test]
    fn linux_backup_root_prefers_an_absolute_xdg_data_home() {
        let xdg = std::env::temp_dir().join("xdg");
        assert_eq!(
            backup_root_linux(
                Some(xdg.clone().into_os_string()),
                Some(OsString::from("home"))
            ),
            Some(xdg.join("saveprism").join("backups"))
        );
        assert_eq!(
            backup_root_linux(None, Some(OsString::from("home"))),
            Some(
                PathBuf::from("home")
                    .join(".local")
                    .join("share")
                    .join("saveprism")
                    .join("backups")
            )
        );
        assert_eq!(backup_root_linux(None, None), None);
    }

    #[test]
    fn default_backup_root_ends_with_backups_when_known() {
        if let Some(root) = default_backup_root() {
            assert_eq!(root.file_name().and_then(|n| n.to_str()), Some("backups"));
        }
    }
}
