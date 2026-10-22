//! Tests of the desktop session on copies of the synthetic and keyed fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use saveprism_codecs::es3::{ES3_PASSWORD_REQUIRED, ES3_WRONG_PASSWORD};

use super::*;

const PASSWORD: &str = "synthetic-test-key";

/// A work directory with a backup directory next to it (never inside it).
struct Env {
    _root: tempfile::TempDir,
    work: PathBuf,
    backups: PathBuf,
}

fn env() -> Env {
    let root = tempfile::tempdir().unwrap();
    let work = root.path().join("work");
    let backups = root.path().join("backups");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(&backups).unwrap();
    Env {
        _root: root,
        work,
        backups,
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures")
        .join(relative)
}

/// Copies a fixture into the work directory and returns the copy's path.
fn copy_in(env: &Env, relative: &str) -> PathBuf {
    let source = fixture(relative);
    let target = env.work.join(source.file_name().unwrap());
    fs::copy(&source, &target).unwrap();
    target
}

fn open(env: &Env, path: &Path, password: Option<&str>) -> Result<Session, ApiError> {
    Session::open(
        path.to_str().unwrap(),
        password.map(str::to_string),
        Some(env.backups.clone()),
    )
}

fn open_json(env: &Env) -> Session {
    let path = copy_in(env, "synthetic/json/compact.json");
    open(env, &path, None).unwrap()
}

fn row(session: &Session, path: &str) -> NodeRow {
    let (parent, _) = path.rsplit_once('/').unwrap();
    let page = session.get_children(parent, 0, 500).unwrap();
    page.items.into_iter().find(|r| r.path == path).unwrap()
}

#[test]
fn json_rpgmaker_and_es3_open_with_an_exact_guard() {
    for relative in [
        "synthetic/json/compact.json",
        "synthetic/rpgmaker-mv/file1.rpgsave",
        "synthetic/es3/settings.es3",
    ] {
        let env = env();
        let path = copy_in(&env, relative);
        let session = open(&env, &path, None).unwrap();
        let info = session.info();
        assert_eq!(info.guard, GuardLevel::Exact, "{relative}");
        assert!(info.editable, "{relative}");
        assert!(info.warning.is_none(), "{relative}");
        assert!(info.root.child_count > 0, "{relative}");
        assert_eq!(info.file_name, path.file_name().unwrap().to_string_lossy());
    }
}

#[test]
fn children_are_paged_and_limited() {
    let env = env();
    let session = open_json(&env);
    assert_eq!(session.info().root.child_count, 5);

    let first = session.get_children("", 0, 2).unwrap();
    assert_eq!(first.total, 5);
    let keys: Vec<&str> = first.items.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["name", "level"]);

    let rest = session.get_children("", 2, 500).unwrap();
    let keys: Vec<&str> = rest.items.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["ratio", "items", "stats"]);

    // A limit above 500 is capped; an offset past the end gives an empty page.
    assert_eq!(session.get_children("", 0, 100_000).unwrap().items.len(), 5);
    assert!(session.get_children("", 9, 10).unwrap().items.is_empty());

    let items = session.get_children("/items", 0, 10).unwrap();
    assert_eq!(items.total, 2);
    assert_eq!(items.items[0].kind, NodeKind::Object);
    assert_eq!(items.items[0].child_count, 2);
    assert_eq!(items.items[0].path, "/items/0");

    let level = row(&session, "/level");
    assert_eq!(level.kind, NodeKind::Int);
    assert_eq!(level.preview, "42");
    assert!(level.editable);
    assert!(!row(&session, "/items").editable);

    let error = session.get_children("/missing", 0, 10).unwrap_err();
    assert_eq!(error.code, "SP-CORE-011");
}

#[test]
fn search_finds_keys_by_path_and_values_by_text() {
    let env = env();
    let session = open_json(&env);

    let by_path = session.search("STATS/hp", 0, 50).unwrap();
    let paths: Vec<&str> = by_path.items.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, ["/stats/hp"]);
    assert!(!by_path.truncated);

    let by_value = session.search("KALKAN", 0, 50).unwrap();
    let paths: Vec<&str> = by_value.items.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, ["/items/0/tags/1"]);

    let by_number = session.search("100.5", 0, 50).unwrap();
    assert_eq!(by_number.total, 1);

    // Paging over hits: "/items" matches the container and all its descendants.
    let all = session.search("/items", 0, 500).unwrap();
    assert!(all.total >= 4);
    let page = session.search("/items", 1, 2).unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.total, all.total);
    assert_eq!(page.items[0].path, all.items[1].path);

    assert!(session.search("", 0, 50).unwrap().items.is_empty());
    assert!(
        session
            .search("no-such-text", 0, 50)
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn edits_by_type_and_bad_input() {
    let env = env();
    let mut session = open_json(&env);

    let level = session.edit_value("/level", " 43 ").unwrap();
    assert_eq!(level.preview, "43");
    assert_eq!(level.kind, NodeKind::Int);
    let alive = session.edit_value("/stats/alive", "False").unwrap();
    assert_eq!(alive.preview, "false");
    let name = session.edit_value("/name", "Ada").unwrap();
    assert_eq!(name.preview, "Ada");
    let hp = session.edit_value("/stats/hp", "7.25").unwrap();
    assert_eq!(hp.preview, "7.25");

    let changes = session.pending_changes();
    assert_eq!(changes.len(), 4);
    assert_eq!(
        changes[0],
        Change {
            path: "/level".into(),
            old: "42".into(),
            new: "43".into()
        }
    );

    // Wrong type, wrong range, container, missing path.
    assert_eq!(
        session.edit_value("/level", "abc").unwrap_err().code,
        app_codes::BAD_VALUE
    );
    assert_eq!(
        session.edit_value("/level", "1.5").unwrap_err().code,
        app_codes::BAD_VALUE
    );
    let out_of_range = session
        .edit_value("/level", "99999999999999999999")
        .unwrap_err();
    assert_eq!(out_of_range.code, app_codes::OUT_OF_RANGE);
    assert!(!out_of_range.hint.is_empty());
    assert_eq!(
        session
            .edit_value("/stats/alive", "maybe")
            .unwrap_err()
            .code,
        app_codes::BAD_VALUE
    );
    assert_eq!(
        session.edit_value("/ratio", "nan").unwrap_err().code,
        app_codes::BAD_VALUE
    );
    assert_eq!(
        session.edit_value("/stats", "1").unwrap_err().code,
        app_codes::NOT_EDITABLE_NODE
    );
    assert_eq!(
        session.edit_value("/missing", "1").unwrap_err().code,
        "SP-CORE-011"
    );
    // Failed edits leave the value and the pending list alone.
    assert_eq!(row(&session, "/level").preview, "43");
    assert_eq!(session.pending_changes().len(), 4);

    // Editing a value back to what it was cancels its entry.
    session.edit_value("/level", "42").unwrap();
    assert_eq!(session.pending_changes().len(), 3);
}

#[test]
fn discard_restores_the_opened_values() {
    let env = env();
    let mut session = open_json(&env);
    session.edit_value("/level", "1").unwrap();
    session.edit_value("/name", "x").unwrap();
    session.discard_changes().unwrap();
    assert!(session.pending_changes().is_empty());
    assert_eq!(row(&session, "/level").preview, "42");
    assert_eq!(row(&session, "/name").preview, "Şükrü'nün kaydı");
}

#[test]
fn save_as_writes_a_new_file_and_keeps_the_original() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let before = fs::read(&path).unwrap();
    let mut session = open(&env, &path, None).unwrap();
    session.edit_value("/level", "43").unwrap();

    let target = env.work.join("copy.json");
    let result = session.save(target.to_str().unwrap(), false).unwrap();
    assert!(result.verified);
    assert!(result.backup_path.is_none());
    assert_eq!(result.written_path, target.to_string_lossy());
    assert_eq!(fs::read(&path).unwrap(), before);

    let reopened = open(&env, &target, None).unwrap();
    assert_eq!(row(&reopened, "/level").preview, "43");
    assert!(session.pending_changes().is_empty());
}

#[test]
fn overwrite_makes_a_backup_and_writes_the_new_value() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let before = fs::read(&path).unwrap();
    let mut session = open(&env, &path, None).unwrap();
    session.edit_value("/level", "99").unwrap();

    let result = session.save(path.to_str().unwrap(), false).unwrap();
    let backup = PathBuf::from(result.backup_path.unwrap());
    assert!(backup.starts_with(&env.backups));
    assert_eq!(fs::read(&backup).unwrap(), before);

    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(row(&reopened, "/level").preview, "99");
}

#[test]
fn save_as_onto_an_existing_file_needs_overwrite() {
    let env = env();
    let mut session = open_json(&env);
    let other = env.work.join("other.json");
    fs::write(&other, b"{\"keep\":true}").unwrap();
    session.edit_value("/level", "5").unwrap();

    let error = session.save(other.to_str().unwrap(), false).unwrap_err();
    assert_eq!(error.code, app_codes::TARGET_EXISTS);
    assert_eq!(fs::read(&other).unwrap(), b"{\"keep\":true}");
    assert_eq!(session.pending_changes().len(), 1);

    let result = session.save(other.to_str().unwrap(), true).unwrap();
    assert!(result.backup_path.is_some());
    let reopened = open(&env, &other, None).unwrap();
    assert_eq!(row(&reopened, "/level").preview, "5");
}

#[test]
fn encrypted_es3_asks_for_a_password() {
    let env = env();
    let path = copy_in(&env, "keyed/es3-aes/settings.es3");

    let missing = open(&env, &path, None).err().unwrap();
    assert_eq!(missing.code, ES3_PASSWORD_REQUIRED.as_str());
    let empty = open(&env, &path, Some("")).err().unwrap();
    assert_eq!(empty.code, ES3_PASSWORD_REQUIRED.as_str());
    let wrong = open(&env, &path, Some("not-the-key")).err().unwrap();
    assert_eq!(wrong.code, ES3_WRONG_PASSWORD.as_str());
    assert!(!wrong.message.contains("not-the-key"));

    let session = open(&env, &path, Some(PASSWORD)).unwrap();
    let info = session.info();
    assert!(info.editable);
    assert_eq!(info.codec, "format.es3");
    assert_eq!(
        info.recipe.first().map(String::as_str),
        Some("crypto.es3-aes")
    );
    assert!(info.root.child_count > 0);
}

#[test]
fn encrypted_gzip_es3_opens_with_the_password() {
    let env = env();
    let path = copy_in(&env, "keyed/es3-aes/progress-gzip.es3");
    let session = open(&env, &path, Some(PASSWORD)).unwrap();
    let info = session.info();
    assert_eq!(
        info.recipe,
        ["crypto.es3-aes", "compression.gzip", "format.es3"]
    );
    assert!(info.editable);
}

#[test]
fn a_missing_file_is_a_read_error() {
    let env = env();
    let error = open(&env, &env.work.join("nope.json"), None).err().unwrap();
    assert_eq!(error.code, "SP-IO-007");
    assert!(!error.hint.is_empty());
}
