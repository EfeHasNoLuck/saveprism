//! Tests of the desktop session on copies of the synthetic and keyed fixtures.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use saveprism_codecs::es3::{ES3_PASSWORD_REQUIRED, ES3_WRONG_PASSWORD};
use saveprism_codecs::layers::ZlibLayer;
use saveprism_codecs::renpy::sign;
use saveprism_core::{DecodeCtx, Layer, Limits};

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
    Session::open_with_keys(
        path.to_str().unwrap(),
        password.map(str::to_string),
        Some(env.backups.clone()),
        RenpyKeySource::At(None),
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
fn tyrano_saves_report_their_engine() {
    for relative in [
        "synthetic/tyrano/sample_tyrano_data.sav",
        "synthetic/tyrano/sample_tyrano_data_uri.sav",
        "synthetic/tyrano/sample_tyrano_sf.sav",
    ] {
        let env = env();
        let path = copy_in(&env, relative);
        let session = open(&env, &path, None).unwrap();
        let info = session.info();
        assert_eq!(info.guard, GuardLevel::Exact, "{relative}");
        assert!(info.editable, "{relative}");
        assert_eq!(info.engine.as_deref(), Some("tyranoscript"), "{relative}");
        assert_eq!(
            info.recipe,
            ["encoding.url-escape", "format.json"],
            "{relative}"
        );
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
            new: "43".into(),
            kind: ChangeKind::Set
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
    let result = session.save(target.to_str().unwrap(), false, None).unwrap();
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

    let result = session.save(path.to_str().unwrap(), false, None).unwrap();
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

    let error = session
        .save(other.to_str().unwrap(), false, None)
        .unwrap_err();
    assert_eq!(error.code, app_codes::TARGET_EXISTS);
    assert_eq!(fs::read(&other).unwrap(), b"{\"keep\":true}");
    assert_eq!(session.pending_changes().len(), 1);

    let result = session.save(other.to_str().unwrap(), true, None).unwrap();
    assert!(result.backup_path.is_some());
    let reopened = open(&env, &other, None).unwrap();
    assert_eq!(row(&reopened, "/level").preview, "5");
}

#[test]
fn save_refuses_a_file_that_changed_on_disk_and_leaves_it_alone() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();
    session.edit_value("/level", "99").unwrap();

    let written_by_another_program = b"{\"level\":7}".to_vec();
    fs::write(&path, &written_by_another_program).unwrap();
    let error = session
        .save(path.to_str().unwrap(), false, None)
        .unwrap_err();
    assert_eq!(error.code, app_codes::FILE_CHANGED);
    assert_eq!(
        error.message,
        "\"compact.json\" changed on disk after it was opened"
    );
    assert_eq!(fs::read(&path).unwrap(), written_by_another_program);
}

#[test]
fn save_writes_an_unchanged_file_and_a_second_save_after_it() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();

    session.edit_value("/level", "5").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let first = fs::read(&path).unwrap();

    session.edit_value("/level", "6").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    assert_ne!(fs::read(&path).unwrap(), first);
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(row(&reopened, "/level").preview, "6");
}

/// Names of the files in `dir`, sorted.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn export_json_file_refuses_the_open_save_and_folders() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let before = fs::read(&path).unwrap();
    let session = open(&env, &path, None).unwrap();

    // The open save, also spelled through `..` (only canonical paths match).
    let roundabout = env
        .work
        .join("..")
        .join("work")
        .join(path.file_name().unwrap());
    for target in [&path, &roundabout] {
        let error = session
            .export_json_file(target.to_str().unwrap())
            .unwrap_err();
        assert_eq!(error.code, app_codes::EXPORT_TARGET, "{}", target.display());
        assert!(!error.hint.is_empty());
    }
    // A folder, also one named like a JSON file, and any name that is not `.json`.
    let json_folder = env.work.join("folder.json");
    fs::create_dir(&json_folder).unwrap();
    let other_save = env.work.join("other.es3");
    let text_file = env.work.join("notes.txt");
    for target in [&env.work, &json_folder, &other_save, &text_file] {
        let error = session
            .export_json_file(target.to_str().unwrap())
            .unwrap_err();
        assert_eq!(error.code, app_codes::EXPORT_TARGET, "{}", target.display());
    }
    fs::remove_dir(&json_folder).unwrap();
    assert!(session.export_json_file("").is_err());

    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(file_names(&env.work), vec!["compact.json".to_string()]);
}

#[test]
fn export_json_file_writes_the_json_view_as_json() {
    let env = env();
    let mut session = open_json(&env);
    session.edit_value("/level", "7").unwrap();
    let target = env.work.join("compact-export.json");
    fs::write(&target, b"old text").unwrap();

    let written = session.export_json_file(target.to_str().unwrap()).unwrap();
    assert_eq!(written, target.to_string_lossy());
    let text = fs::read_to_string(&target).unwrap();
    assert_eq!(text, session.export_json().unwrap());
    assert!(text.contains("\"level\": 7"));
    // The file is plain JSON.
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["level"], 7);
    // The extension is checked without regard to case.
    let upper = env.work.join("UPPER.JSON");
    session.export_json_file(upper.to_str().unwrap()).unwrap();
    // No temporary file is left next to them, and the pending edit is still pending.
    assert_eq!(
        file_names(&env.work),
        vec![
            "UPPER.JSON".to_string(),
            "compact-export.json".to_string(),
            "compact.json".to_string()
        ]
    );
    assert_eq!(session.pending_changes().len(), 1);
}

/// The JSON view of `session` after `change`, as the editor would send it back.
fn edited(session: &Session, change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut json: serde_json::Value =
        serde_json::from_str(&session.export_json().unwrap()).unwrap();
    change(&mut json);
    serde_json::to_string_pretty(&json).unwrap()
}

#[test]
fn apply_json_changes_int_float_bool_and_text() {
    let env = env();
    let mut session = open_json(&env);
    let text = edited(&session, |json| {
        json["level"] = 43.into();
        json["ratio"] = 2.5.into();
        json["name"] = "Ada".into();
        json["stats"]["hp"] = 7.25.into();
        json["stats"]["alive"] = false.into();
    });
    assert_eq!(session.apply_json(&text).unwrap(), 5);

    assert_eq!(row(&session, "/level").preview, "43");
    assert_eq!(row(&session, "/ratio").preview, "2.5");
    assert_eq!(row(&session, "/name").preview, "Ada");
    assert_eq!(row(&session, "/stats/hp").preview, "7.25");
    assert_eq!(row(&session, "/stats/alive").preview, "false");
    // They are ordinary pending edits: listed with the opened value, undone one by one.
    let changes = session.pending_changes();
    assert_eq!(changes.len(), 5);
    let level = changes.iter().find(|c| c.path == "/level").unwrap();
    assert_eq!((level.old.as_str(), level.new.as_str()), ("42", "43"));
    session.edit_value("/level", "42").unwrap();
    assert_eq!(session.pending_changes().len(), 4);
    // Applying the opened values again cancels their entries.
    let back = edited(&session, |json| {
        json["ratio"] = 1.0.into();
        json["name"] = "Şükrü'nün kaydı".into();
    });
    assert_eq!(session.apply_json(&back).unwrap(), 2);
    assert_eq!(session.pending_changes().len(), 2);
}

#[test]
fn apply_json_of_the_same_view_changes_nothing() {
    let env = env();
    let mut session = open_json(&env);
    let text = session.export_json().unwrap();
    assert_eq!(session.apply_json(&text).unwrap(), 0);
    // Other spacing and `1` for the float `1.0` are no change either.
    let other = edited(&session, |json| json["ratio"] = 1.into());
    assert_eq!(session.apply_json(&other).unwrap(), 0);
    assert!(session.pending_changes().is_empty());
}

/// A change to the parsed JSON view.
type JsonChange = fn(&mut serde_json::Value);

#[test]
fn apply_json_refuses_structure_changes_and_applies_nothing() {
    let env = env();
    let mut session = open_json(&env);
    let cases: [(&str, JsonChange); 5] = [
        ("/stats/note", |json| {
            json["stats"].as_object_mut().unwrap().remove("note");
        }),
        ("/extra", |json| json["extra"] = 1.into()),
        ("/items/0/tags", |json| {
            json["items"][0]["tags"]
                .as_array_mut()
                .unwrap()
                .push("x".into());
        }),
        ("/stats", |json| json["stats"] = serde_json::json!([1, 2])),
        ("/stats/note", |json| json["stats"]["note"] = 0.into()),
    ];
    for (pointer, change) in cases {
        // A valid change rides along; it must not be applied either.
        let text = edited(&session, |json| {
            json["level"] = 7.into();
            change(json);
        });
        let error = session.apply_json(&text).unwrap_err();
        assert_eq!(error.code, app_codes::JSON_SHAPE, "{pointer}");
        assert!(
            error.message.starts_with(&format!("{pointer}: ")),
            "{}",
            error.message
        );
        assert!(!error.hint.is_empty());
        assert!(session.pending_changes().is_empty(), "{pointer}");
        assert_eq!(row(&session, "/level").preview, "42", "{pointer}");
    }
    let removed = edited(&session, |json| {
        json.as_object_mut().unwrap().remove("ratio");
    });
    let error = session.apply_json(&removed).unwrap_err();
    assert!(
        error
            .message
            .contains("adding or removing fields is not supported")
    );

    // A value of another type, and text that is not JSON.
    let typed = edited(&session, |json| json["level"] = "43".into());
    let error = session.apply_json(&typed).unwrap_err();
    assert_eq!(error.code, app_codes::BAD_VALUE);
    assert!(error.message.starts_with("/level: "));
    let error = session.apply_json("{\n  \"level\": 43,\n}").unwrap_err();
    assert_eq!(error.code, app_codes::JSON_TEXT);
    assert!(error.hint.contains("line 3"), "{}", error.hint);
    assert!(session.pending_changes().is_empty());
}

#[test]
fn apply_json_refuses_text_over_the_limit() {
    let env = env();
    let mut session = open_json(&env);
    let mut text = session.export_json().unwrap();
    text.push_str(&" ".repeat(export::MAX_EXPORT_BYTES));
    let error = session.apply_json(&text).unwrap_err();
    assert_eq!(error.code, codes::CORE_LIMIT_DECODED.as_str());
}

#[test]
fn apply_json_keeps_whole_numbers_whole_and_in_range() {
    let env = env();
    let mut session = open_json(&env);
    let text = session.export_json().unwrap();
    let huge = text.replace("\"level\": 42", "\"level\": 99999999999999999999999");
    let error = session.apply_json(&huge).unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert!(error.message.starts_with("/level: "), "{}", error.message);
    let decimal = edited(&session, |json| json["level"] = 1.5.into());
    assert_eq!(
        session.apply_json(&decimal).unwrap_err().code,
        app_codes::BAD_VALUE
    );
    assert!(session.pending_changes().is_empty());
}

#[test]
fn apply_json_checks_the_gold_limit_and_applies_nothing_on_error() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let mut session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    let gold = view.gold.path.clone();
    let hp = view.actors[0].hp.path.clone();

    let text = edited(&session, |json| {
        *json.pointer_mut(&hp).unwrap() = 1.into();
        *json.pointer_mut(&gold).unwrap() = 100_000_000.into();
    });
    let error = session.apply_json(&text).unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert!(
        error.message.starts_with(&format!("{gold}: ")),
        "{}",
        error.message
    );
    assert!(error.message.contains("99,999,999"));
    assert!(session.pending_changes().is_empty());
    assert_eq!(rpg_view(&session).unwrap().gold.value, "1250");

    let text = edited(&session, |json| {
        *json.pointer_mut(&gold).unwrap() = 2601.into();
    });
    assert_eq!(session.apply_json(&text).unwrap(), 1);
    let field = rpg_view(&session).unwrap().gold;
    assert_eq!(
        (field.value.as_str(), field.original.as_str()),
        ("2601", "1250")
    );
}

/// `{"name": "Ann", "inventory": <map {"sword": 3, 7: true}>, "name": "Bob", "blob": <3 bytes>,
/// "link": a reference to the first name, "none": null}`
fn map_doc() -> SaveDoc {
    use saveprism_core::{IntWidth, Node};
    let mut doc = SaveDoc::new();
    let mut add = |value: Value| doc.push(Node::new(value)).unwrap();
    let ann = add(Value::Str("Ann".into()));
    let sword = add(Value::Str("sword".into()));
    let three = add(Value::Int {
        v: 3,
        width: IntWidth::I32,
    });
    let seven = add(Value::Int {
        v: 7,
        width: IntWidth::I32,
    });
    let yes = add(Value::Bool(true));
    let inventory = add(Value::Map(vec![(sword, three), (seven, yes)]));
    let bob = add(Value::Str("Bob".into()));
    let blob = add(Value::Bytes(vec![1, 2, 3]));
    let link = add(Value::Ref(ann));
    let none = add(Value::Null);
    let root = add(Value::Object(vec![
        ("name".into(), ann),
        ("inventory".into(), inventory),
        ("name".into(), bob),
        ("blob".into(), blob),
        ("link".into(), link),
        ("none".into(), none),
    ]));
    doc.set_root(root);
    doc
}

fn map_session(env: &Env) -> Session {
    let mut session = open_json(env);
    session.doc = map_doc();
    session
}

#[test]
fn apply_json_changes_the_value_of_a_map_pair() {
    let env = env();
    let mut session = map_session(&env);
    let text = edited(&session, |json| {
        assert_eq!(json["inventory"][1]["key"], 7);
        json["inventory"][0]["value"] = 5.into();
        json["inventory"][1]["value"] = false.into();
    });
    assert_eq!(session.apply_json(&text).unwrap(), 2);
    let changes = session.pending_changes();
    assert_eq!(changes[0].path, "/inventory/0");
    assert_eq!(
        (changes[0].old.as_str(), changes[0].new.as_str()),
        ("3", "5")
    );
    assert_eq!(changes[1].path, "/inventory/1");

    // Map keys stay as they are.
    let text = edited(&session, |json| {
        json["name"] = "Zed".into();
        json["inventory"][0]["key"] = "axe".into();
    });
    let error = session.apply_json(&text).unwrap_err();
    assert_eq!(error.code, app_codes::JSON_SHAPE);
    assert!(
        error.message.starts_with("/inventory/0/key: "),
        "{}",
        error.message
    );
    let text = edited(&session, |json| {
        json["inventory"][0].as_object_mut().unwrap().remove("key");
    });
    assert_eq!(
        session.apply_json(&text).unwrap_err().code,
        app_codes::JSON_SHAPE
    );
    assert_eq!(session.pending_changes().len(), 2);
    assert_eq!(row(&session, "/name").preview, "Ann");
}

#[test]
fn apply_json_maps_repeated_keys_like_the_view() {
    let env = env();
    let mut session = map_session(&env);
    // `name (2)` is the second `name`: leaving it alone while the first changes is fine.
    let text = edited(&session, |json| {
        assert_eq!(json["name (2)"], "Bob");
        json["name"] = "Ada".into();
    });
    assert_eq!(session.apply_json(&text).unwrap(), 1);
    let rows = session.get_children("", 0, 10).unwrap().items;
    assert_eq!(
        (rows[0].key.as_str(), rows[0].preview.as_str()),
        ("name", "Ada")
    );
    assert_eq!(
        (rows[2].key.as_str(), rows[2].preview.as_str()),
        ("name", "Bob")
    );

    // The second one cannot be addressed by its path, so it is refused instead of changing the first.
    let text = edited(&session, |json| json["name (2)"] = "Cid".into());
    let error = session.apply_json(&text).unwrap_err();
    assert_eq!(error.code, app_codes::NOT_EDITABLE_NODE);
    assert!(
        error.message.starts_with("/name (2): "),
        "{}",
        error.message
    );
    assert_eq!(session.pending_changes().len(), 1);
}

#[test]
fn apply_json_refuses_placeholder_changes() {
    let env = env();
    let mut session = map_session(&env);
    let cases: [(&str, serde_json::Value); 3] = [
        ("/blob", "<4 bytes>".into()),
        ("/link", "-> #99".into()),
        ("/none", 0.into()),
    ];
    for (pointer, value) in cases {
        let text = edited(&session, |json| {
            json["name"] = "Zed".into();
            *json.pointer_mut(pointer).unwrap() = value;
        });
        let error = session.apply_json(&text).expect_err(pointer);
        assert_eq!(error.code, app_codes::JSON_SHAPE, "{pointer}");
        assert!(
            error.message.starts_with(&format!("{pointer}: ")),
            "{}",
            error.message
        );
        assert!(session.pending_changes().is_empty());
    }
}

#[test]
fn apply_json_handles_many_repeated_keys() {
    use saveprism_core::Node;
    let env = env();
    let mut session = open_json(&env);
    let mut doc = SaveDoc::new();
    let mut entries = Vec::new();
    for index in 0..20_000 {
        let id = doc.push(Node::new(Value::Str(index.to_string()))).unwrap();
        entries.push(("k".to_string(), id));
    }
    let root = doc.push(Node::new(Value::Object(entries))).unwrap();
    doc.set_root(root);
    session.doc = doc;
    let text = session.export_json().unwrap();
    assert!(text.contains("\"k (20000)\""));
    assert_eq!(session.apply_json(&text).unwrap(), 0);
    let text = edited(&session, |json| json["k"] = "first".into());
    assert_eq!(session.apply_json(&text).unwrap(), 1);
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

fn rpg_view(session: &Session) -> Option<RpgMakerView> {
    match session.game_view().unwrap() {
        GameView::Rpgmaker { view } => Some(*view),
        GameView::Values { .. }
        | GameView::Sugarcube { .. }
        | GameView::Witcher3 { .. }
        | GameView::Satisfactory { .. } => None,
    }
}

fn ids(entries: &[Entry]) -> Vec<u32> {
    entries.iter().map(|entry| entry.id).collect()
}

#[test]
fn mz_save_becomes_rpg_maker_sections() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();

    assert_eq!(view.engine, "mz");
    assert_eq!(view.gold.value, "1250");
    assert_eq!(view.gold.original, "1250");
    assert_eq!(view.gold.kind, FieldKind::Int);
    assert_eq!(view.gold.min, Some(0));
    assert_eq!(view.gold.max, Some(99_999_999));
    assert_eq!(view.gold.hint, "Whole number \u{b7} 0 to 99,999,999");
    assert!(view.gold.editable);

    assert_eq!(view.actors.len(), 1);
    let actor = &view.actors[0];
    assert_eq!(actor.name, "Reid");
    assert_eq!(actor.level.value, "7");
    assert_eq!(actor.level.min, Some(1));
    assert_eq!(actor.level.max, Some(99));
    assert_eq!(actor.hp.value, "450");
    assert_eq!(actor.hp.hint, "Whole number \u{b7} 0 or higher");
    assert_eq!(actor.mp.value, "90");
    assert_eq!(actor.exp.as_ref().map(|f| f.value.as_str()), Some("1820"));
    assert!(actor.bonuses.is_empty());

    assert_eq!(ids(&view.items), [1, 7]);
    assert_eq!(view.items[0].field.value, "5");
    assert_eq!(view.items[0].field.max, Some(99));
    assert_eq!(ids(&view.weapons), [1]);
    assert!(view.armors.is_empty());
    // Variable 2 is 0 and has no name, so it is left out.
    assert_eq!(ids(&view.variables), [1, 3, 4]);
    assert_eq!(view.variables[0].field.hint, "Whole number");
    assert_eq!(view.variables[1].field.kind, FieldKind::Float);
    assert_eq!(view.variables[2].field.kind, FieldKind::String);
    assert_eq!(ids(&view.switches), [1, 3]);
    assert_eq!(view.switches[0].field.hint, "On/off");

    // No data folder next to the save: no names.
    assert!(view.names_folder.is_none());
    assert!(view.items.iter().all(|entry| entry.name.is_none()));
}

#[test]
fn mv_save_becomes_rpg_maker_sections() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mv/file1.rpgsave");
    let session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    assert_eq!(view.engine, "mv");
    assert!(!view.gold.value.is_empty());
    assert!(!view.actors.is_empty());
    assert!(!view.actors[0].level.value.is_empty());
    assert!(!view.actors[0].hp.value.is_empty());
    assert!(!view.items.is_empty());
}

#[test]
fn names_come_from_the_data_folder_next_to_the_save_folder() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let data = env.work.parent().unwrap().join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(
        data.join("Items.json"),
        r#"[null,{"id":1,"name":"Potion"},{"id":7,"name":"Elixir"}]"#,
    )
    .unwrap();
    fs::write(
        data.join("System.json"),
        r#"{"variables":["","","Counter","","Gate"],"switches":["","Door"]}"#,
    )
    .unwrap();
    // A broken file never fails the view.
    fs::write(data.join("Weapons.json"), "not json").unwrap();

    let session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));
    assert_eq!(view.items[0].name.as_deref(), Some("Potion"));
    assert_eq!(view.items[1].name.as_deref(), Some("Elixir"));
    assert_eq!(view.weapons[0].name, None);
    // The named variable 2 is shown although its value is 0.
    assert_eq!(ids(&view.variables), [1, 2, 3, 4]);
    assert_eq!(view.variables[1].name.as_deref(), Some("Counter"));
    assert_eq!(view.variables[2].name, None);
    assert_eq!(view.switches[0].name.as_deref(), Some("Door"));
}

#[test]
fn names_also_come_from_a_data_folder_copied_next_to_the_save() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let data = env.work.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(
        data.join("Items.json"),
        r#"[null,{"id":1,"name":"Potion"}]"#,
    )
    .unwrap();

    let session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));
    assert_eq!(view.items[0].name.as_deref(), Some("Potion"));
}

#[test]
fn easy_save_file_becomes_a_values_list() {
    let env = env();
    let path = copy_in(&env, "synthetic/es3/progress.es3");
    let session = open(&env, &path, None).unwrap();
    let GameView::Values { rows, .. } = session.game_view().unwrap() else {
        unreachable!("an Easy Save 3 file has no RPG Maker view");
    };
    let coins = rows.iter().find(|r| r.name == "coins").unwrap();
    let field = coins.field.as_ref().unwrap();
    assert_eq!(field.type_name.as_deref(), Some("Int32"));
    assert_eq!(field.max, Some(2_147_483_647));
    assert_eq!(field.min, Some(-2_147_483_648));
    assert_eq!(field.value, "250");
    assert_eq!(coins.path, "/coins/value");

    let levels = rows.iter().find(|r| r.name == "unlockedLevels").unwrap();
    assert!(levels.field.is_none());
    assert_eq!(levels.children, 3);

    let info = session.field_info("/coins/value").unwrap();
    assert_eq!(info.type_name.as_deref(), Some("Int32"));
    assert_eq!(
        session.field_info("/unlockedLevels").unwrap_err().code,
        app_codes::NOT_EDITABLE_NODE
    );

    // The limit of the Easy Save 3 type is enforced on edit.
    let mut session = session;
    let error = session
        .edit_value("/coins/value", "3000000000")
        .unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert!(!error.hint.is_empty());
    session.edit_value("/coins/value", "300").unwrap();
}

#[test]
fn gold_edit_respects_the_rpg_maker_limit_and_keeps_the_original() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let mut session = open(&env, &path, None).unwrap();
    let gold_path = rpg_view(&session).unwrap().gold.path;

    let error = session.edit_value(&gold_path, "100000000").unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert!(error.hint.contains("99,999,999"));
    assert_eq!(rpg_view(&session).unwrap().gold.value, "1250");

    session.edit_value(&gold_path, "500").unwrap();
    let gold = rpg_view(&session).unwrap().gold;
    assert_eq!(gold.value, "500");
    assert_eq!(gold.original, "1250");
    assert_eq!(
        session.field_info(&gold_path).unwrap().max,
        Some(99_999_999)
    );
}

#[test]
fn a_decimal_gold_value_is_checked_against_the_limit_too() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let session = open(&env, &path, None).unwrap();
    let gold_path = saveprism_core::Path::parse(&rpg_view(&session).unwrap().gold.path).unwrap();
    // A node as the codec would give it if the game had saved gold as 1250.0.
    let node = saveprism_core::Node::new(Value::Float {
        v: 1250.0,
        width: saveprism_core::FloatWidth::F64,
    });
    let error = session
        .check_bounds(&gold_path, &node, &OwnedValue::Float(1.0e9))
        .unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert!(
        session
            .check_bounds(&gold_path, &node, &OwnedValue::Float(500.5))
            .is_ok()
    );
}

#[test]
fn a_plain_json_file_has_a_values_list_with_i64_limits() {
    let env = env();
    let session = open_json(&env);
    let GameView::Values { rows, .. } = session.game_view().unwrap() else {
        unreachable!("plain JSON has no RPG Maker view");
    };
    let level = rows.iter().find(|r| r.name == "level").unwrap();
    let field = level.field.as_ref().unwrap();
    assert_eq!(field.kind, FieldKind::Int);
    // Limits sent to the UI stay within JavaScript's exact integer range.
    assert_eq!(field.min, Some(-9_007_199_254_740_991));
    assert_eq!(field.max, Some(9_007_199_254_740_991));
    let stats = rows.iter().find(|r| r.name == "stats").unwrap();
    assert!(stats.field.is_none());
    assert!(stats.children > 0);
}

#[test]
fn a_missing_file_is_a_read_error() {
    let env = env();
    let error = open(&env, &env.work.join("nope.json"), None).err().unwrap();
    assert_eq!(error.code, "SP-IO-007");
    assert!(!error.hint.is_empty());
}

fn level_of(session: &Session) -> String {
    row(session, "/level").preview
}

#[test]
fn undo_and_redo_walk_single_edits() {
    let env = env();
    let mut session = open_json(&env);
    assert_eq!(
        session.history_state(),
        HistoryState {
            can_undo: false,
            can_redo: false
        }
    );
    // Nothing to undo or redo is not an error and changes nothing.
    let none = session.undo().unwrap();
    assert!(none.fields.is_empty() && !none.reload && none.pending == 0 && !none.state.can_undo);
    assert!(!session.redo().unwrap().state.can_redo);

    session.edit_value("/level", "43").unwrap();
    session.edit_value("/name", "Ada").unwrap();
    session.edit_value("/stats/hp", "7.25").unwrap();
    assert_eq!(session.pending_changes().len(), 3);

    // Undo twice: the last two edits are taken back, the result names the value and its state.
    let first = session.undo().unwrap();
    assert_eq!(first.fields.len(), 1);
    assert_eq!(first.fields[0].path, "/stats/hp");
    assert_eq!(first.fields[0].value, "100.5");
    assert_eq!(first.fields[0].original, "100.5");
    assert_eq!(first.pending, 2);
    assert!(first.state.can_undo && first.state.can_redo);
    let second = session.undo().unwrap();
    assert_eq!(second.fields[0].value, "Şükrü'nün kaydı");
    assert_eq!(second.pending, 1);
    assert_eq!(row(&session, "/name").preview, "Şükrü'nün kaydı");
    assert_eq!(level_of(&session), "43");
    assert_eq!(session.pending_changes().len(), 1);

    // Redo once: the name is back, the hit-points edit stays undone.
    let redone = session.redo().unwrap();
    assert_eq!(redone.fields[0].path, "/name");
    assert_eq!(redone.fields[0].value, "Ada");
    assert_eq!(redone.fields[0].original, "Şükrü'nün kaydı");
    assert_eq!(redone.pending, 2);
    assert!(redone.state.can_undo && redone.state.can_redo);
    assert_eq!(row(&session, "/name").preview, "Ada");
    assert_eq!(row(&session, "/stats/hp").preview, "100.5");

    // Undo down to the opened file, then there is nothing more to undo.
    session.undo().unwrap();
    let last = session.undo().unwrap();
    assert_eq!(last.pending, 0);
    assert!(!last.state.can_undo && last.state.can_redo);
    assert!(session.pending_changes().is_empty());
    assert_eq!(level_of(&session), "42");
    assert_eq!(session.undo().unwrap().pending, 0);
}

#[test]
fn setting_a_value_back_is_a_step_of_its_own() {
    let env = env();
    let mut session = open_json(&env);
    session.edit_value("/level", "43").unwrap();
    // The per-field undo is an ordinary edit back to the opened value.
    session.edit_value("/level", "42").unwrap();
    assert!(session.pending_changes().is_empty());
    // Failed and unchanged edits are no steps.
    assert!(session.edit_value("/level", "abc").is_err());
    session.edit_value("/level", "42").unwrap();

    session.undo().unwrap();
    assert_eq!(level_of(&session), "43");
    let changes = session.pending_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (changes[0].old.as_str(), changes[0].new.as_str()),
        ("42", "43")
    );
    session.undo().unwrap();
    assert_eq!(level_of(&session), "42");
    assert!(session.pending_changes().is_empty());
    assert!(!session.history_state().can_undo);
    session.redo().unwrap();
    session.redo().unwrap();
    assert_eq!(level_of(&session), "42");
    assert!(session.pending_changes().is_empty());
}

#[test]
fn a_json_apply_is_one_step() {
    let env = env();
    let mut session = open_json(&env);
    session.edit_value("/level", "50").unwrap();
    let text = edited(&session, |json| {
        json["level"] = 51.into();
        json["ratio"] = 2.5.into();
        json["name"] = "Ada".into();
        json["stats"]["alive"] = false.into();
    });
    assert_eq!(session.apply_json(&text).unwrap(), 4);
    assert_eq!(session.pending_changes().len(), 4);

    // One undo takes back all four values; the edit before the Apply stays.
    let undone = session.undo().unwrap();
    assert_eq!(undone.fields.len(), 4);
    assert_eq!(undone.pending, 1);
    assert_eq!(level_of(&session), "50");
    assert_eq!(row(&session, "/ratio").preview, "1.0");
    assert_eq!(row(&session, "/name").preview, "Şükrü'nün kaydı");
    assert_eq!(row(&session, "/stats/alive").preview, "true");
    let changes = session.pending_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "/level");

    // One redo does all four again.
    let redone = session.redo().unwrap();
    assert_eq!(redone.fields.len(), 4);
    assert_eq!(redone.pending, 4);
    assert_eq!(level_of(&session), "51");
    assert_eq!(row(&session, "/ratio").preview, "2.5");
    assert_eq!(row(&session, "/stats/alive").preview, "false");
    assert!(!redone.state.can_redo);
}

#[test]
fn undo_all_can_be_undone() {
    let env = env();
    let mut session = open_json(&env);
    session.edit_value("/level", "43").unwrap();
    session.edit_value("/name", "Ada").unwrap();
    session.discard_changes().unwrap();
    assert!(session.pending_changes().is_empty());
    assert_eq!(level_of(&session), "42");
    assert!(session.history_state().can_undo);

    // Undo all is one step: one undo brings both edits back.
    let back = session.undo().unwrap();
    assert_eq!(back.fields.len(), 2);
    assert_eq!(back.pending, 2);
    assert_eq!(level_of(&session), "43");
    assert_eq!(row(&session, "/name").preview, "Ada");
    // The steps before it are still there.
    session.undo().unwrap();
    session.undo().unwrap();
    assert!(session.pending_changes().is_empty());
    assert!(!session.history_state().can_undo);
    session.redo().unwrap();
    session.redo().unwrap();
    assert_eq!(level_of(&session), "43");
    // Redo of the Undo all step throws the edits away again.
    session.redo().unwrap();
    assert!(session.pending_changes().is_empty());
    assert_eq!(level_of(&session), "42");

    // Nothing pending: Undo all adds no step, so the next undo is still the Undo all step.
    session.discard_changes().unwrap();
    assert_eq!(session.undo().unwrap().pending, 2);
}

#[test]
fn a_new_action_clears_the_redo_steps() {
    let env = env();
    let mut session = open_json(&env);
    session.edit_value("/level", "43").unwrap();
    session.edit_value("/level", "44").unwrap();
    session.undo().unwrap();
    assert!(session.history_state().can_redo);

    session.edit_value("/name", "Ada").unwrap();
    assert!(!session.history_state().can_redo);
    assert_eq!(session.redo().unwrap().pending, 2);
    assert_eq!(level_of(&session), "43");

    // A JSON Apply and an Undo all clear it too.
    session.undo().unwrap();
    assert!(session.history_state().can_redo);
    let text = edited(&session, |json| json["ratio"] = 2.5.into());
    session.apply_json(&text).unwrap();
    assert!(!session.history_state().can_redo);
    session.undo().unwrap();
    assert!(session.history_state().can_redo);
    session.discard_changes().unwrap();
    assert!(!session.history_state().can_redo);

    // An edit that changes nothing is no action: the redo step stays.
    session.undo().unwrap();
    assert!(session.history_state().can_redo);
    let level = level_of(&session);
    session.edit_value("/level", &level).unwrap();
    assert!(session.history_state().can_redo);
}

#[test]
fn saving_clears_the_history() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();
    session.edit_value("/level", "43").unwrap();
    session.edit_value("/level", "44").unwrap();
    session.undo().unwrap();
    assert!(session.history_state().can_undo && session.history_state().can_redo);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    assert_eq!(
        session.history_state(),
        HistoryState {
            can_undo: false,
            can_redo: false
        }
    );
    // Undo after the save must not change the saved document.
    assert_eq!(session.undo().unwrap().pending, 0);
    assert_eq!(level_of(&session), "43");
    assert!(session.pending_changes().is_empty());
    // A reopened file starts with an empty history, too.
    let reopened = open(&env, &path, None).unwrap();
    assert!(!reopened.history_state().can_undo);
}

#[test]
fn the_history_keeps_at_most_200_steps() {
    let env = env();
    let mut session = open_json(&env);
    for value in 1..=250 {
        session
            .edit_value("/level", &(1000 + value).to_string())
            .unwrap();
    }
    let mut undone = 0;
    while session.history_state().can_undo {
        session.undo().unwrap();
        undone += 1;
    }
    assert_eq!(undone, 200);
    // The 50 oldest steps are gone: the value is the one before the 51st edit, not the opened one.
    assert_eq!(level_of(&session), "1050");
    assert_eq!(session.pending_changes().len(), 1);
}

#[test]
fn a_step_with_many_values_asks_the_ui_to_read_everything_again() {
    let env = env();
    let count = history::MAX_FIELDS + 100;
    let body: Vec<String> = (0..count).map(|index| format!("\"k{index}\":0")).collect();
    let path = env.work.join("many.json");
    fs::write(&path, format!("{{{}}}", body.join(","))).unwrap();
    let mut session = open(&env, &path, None).unwrap();

    let text = edited(&session, |json| {
        for value in json.as_object_mut().unwrap().values_mut() {
            *value = 5.into();
        }
    });
    assert_eq!(session.apply_json(&text).unwrap(), count);
    let undone = session.undo().unwrap();
    assert!(undone.reload);
    assert!(undone.fields.is_empty());
    assert_eq!(undone.pending, 0);
    assert!(session.pending_changes().is_empty());
    let redone = session.redo().unwrap();
    assert!(redone.reload);
    assert_eq!(redone.pending, count);
}

#[test]
fn edit_values_sets_many_values_as_one_undo_step() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let mut session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    let items: Vec<String> = view.items.iter().map(|e| e.field.path.clone()).collect();
    let switches: Vec<String> = view.switches.iter().map(|e| e.field.path.clone()).collect();
    assert_eq!(items.len(), 2);

    let result = session.edit_values(&items, "42").unwrap();
    assert_eq!(result.fields.len(), 2);
    assert!(result.fields.iter().all(|field| field.value == "42"));
    assert_eq!(result.pending, 2);
    assert!(result.state.can_undo);

    // Over the item limit (99): nothing changes.
    let error = session.edit_values(&items, "150").unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    let view = rpg_view(&session).unwrap();
    assert!(view.items.iter().all(|entry| entry.field.value == "42"));

    // A value that does not parse for one entry: nothing changes either.
    let error = session.edit_values(&switches, "7").unwrap_err();
    assert_eq!(error.code, app_codes::BAD_VALUE);

    let result = session.edit_values(&switches, "false").unwrap();
    assert!(result.fields.iter().all(|field| field.value == "false"));

    // One undo takes back the whole switch step, the next the whole item step.
    session.undo().unwrap();
    let view = rpg_view(&session).unwrap();
    assert!(
        view.switches
            .iter()
            .all(|entry| entry.field.value == entry.field.original)
    );
    assert!(view.items.iter().all(|entry| entry.field.value == "42"));
    session.undo().unwrap();
    assert!(session.pending_changes().is_empty());

    // Setting values to what they already are changes nothing and records no step.
    let state = session.history_state();
    let current: Vec<String> = view
        .items
        .iter()
        .map(|e| e.field.original.clone())
        .collect();
    let same = session.edit_values(&items[..1], &current[0]).unwrap();
    assert!(same.fields.is_empty());
    assert_eq!(session.history_state(), state);
}

#[test]
fn edit_values_refuses_read_only_files_and_missing_paths() {
    let env = env();
    let mut session = open_json(&env);
    let error = session
        .edit_values(&["/level".into(), "/missing".into()], "5")
        .unwrap_err();
    assert_eq!(error.code, "SP-CORE-011");
    assert!(session.pending_changes().is_empty());
}

#[test]
fn backups_are_listed_and_restored_with_a_backup_of_the_current_file() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let first = fs::read(&path).unwrap();
    let mut session = open(&env, &path, None).unwrap();
    assert!(session.backups().unwrap().is_empty());

    session.edit_value("/level", "50").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    session.edit_value("/level", "60").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let saved = fs::read(&path).unwrap();

    let list = session.backups().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].created_ms >= list[1].created_ms);
    let oldest = list[1].clone();
    assert_eq!(oldest.byte_size, u64::try_from(first.len()).unwrap());

    // Unsaved edits are dropped by a restore.
    session.edit_value("/level", "70").unwrap();
    let restored = session.restore_backup(&oldest.name, None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), first);
    assert_eq!(row(&restored, "/level").preview, "42");
    assert!(restored.pending_changes().is_empty());
    assert!(!restored.history_state().can_undo);

    // The file that was replaced is the newest backup now, so the restore can be taken back.
    let list = restored.backups().unwrap();
    assert_eq!(list.len(), 3);
    let back = restored.restore_backup(&list[0].name, None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert_eq!(row(&back, "/level").preview, "60");
}

#[test]
fn save_keeps_at_most_the_requested_backups_and_never_drops_the_newest() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();

    let mut last_backup = String::new();
    for level in 0..5 {
        session.edit_value("/level", &level.to_string()).unwrap();
        let result = session
            .save(path.to_str().unwrap(), false, Some(2))
            .unwrap();
        last_backup = PathBuf::from(result.backup_path.unwrap())
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
    }

    let list = session.backups().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, last_backup);
}

#[test]
fn restore_refuses_unknown_names_and_unreadable_backups() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();
    session.edit_value("/level", "50").unwrap();
    let result = session.save(path.to_str().unwrap(), false, None).unwrap();
    let saved = fs::read(&path).unwrap();

    for name in ["../compact.json", "source.txt", "1.bak", ""] {
        let error = session.restore_backup(name, None).err().unwrap();
        assert_eq!(error.code, app_codes::BACKUP_NOT_FOUND, "{name}");
    }

    // A backup that does not open as a save is never written.
    let backup = PathBuf::from(result.backup_path.unwrap());
    fs::write(&backup, b"\x00\x01 not a save").unwrap();
    let name = backup.file_name().unwrap().to_str().unwrap();
    let error = session.restore_backup(name, None).err().unwrap();
    assert_eq!(error.code, app_codes::BACKUP_UNREADABLE);
    assert_eq!(fs::read(&path).unwrap(), saved);
}

#[test]
fn unnamed_entries_set_to_empty_stay_in_the_list_until_saved() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let mut session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    let switches: Vec<String> = view.switches.iter().map(|e| e.field.path.clone()).collect();
    let variable = view.variables[0].field.path.clone();

    session.edit_values(&switches, "false").unwrap();
    session.edit_value(&variable, "0").unwrap();
    let view = rpg_view(&session).unwrap();
    assert_eq!(ids(&view.switches), [1, 3]);
    assert!(
        view.switches
            .iter()
            .all(|entry| entry.field.value == "false")
    );
    assert_eq!(ids(&view.variables), [1, 3, 4]);

    // Once saved, the empty unnamed entries are unused again and left out.
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let view = rpg_view(&session).unwrap();
    assert!(view.switches.is_empty());
    assert_eq!(ids(&view.variables), [3, 4]);
}

// SugarCube (Twine) saves.

const STORY: &str = "synthetic/sugarcube/multi-moment.save";

fn story_view(session: &Session) -> Option<SugarCubeView> {
    match session.game_view().unwrap() {
        GameView::Sugarcube { view } => Some(*view),
        GameView::Rpgmaker { .. }
        | GameView::Values { .. }
        | GameView::Witcher3 { .. }
        | GameView::Satisfactory { .. } => None,
    }
}

fn story_row<'a>(view: &'a SugarCubeView, name: &str) -> &'a ValueRow {
    view.variables
        .iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("no variable {name}"))
}

fn story_value(view: &SugarCubeView, name: &str) -> String {
    story_row(view, name).field.as_ref().unwrap().value.clone()
}

/// Writes `json` as a plain JSON file and opens it.
fn open_text(env: &Env, json: &str) -> Session {
    let path = env.work.join("story.json");
    fs::write(&path, json).unwrap();
    open(env, &path, None).unwrap()
}

/// The source JSON of the multi-moment fixture (see fixtures/README.md).
fn story_source() -> String {
    fs::read_to_string(fixture("tools/sugarcube/multi-moment.json")).unwrap()
}

fn json_of(session: &Session) -> serde_json::Value {
    serde_json::from_str(&session.export_json().unwrap()).unwrap()
}

fn json_get<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map.get(key),
        serde_json::Value::Array(items) => items.get(key.parse::<usize>().ok()?),
        _ => None,
    }
}

fn json_set(value: &mut serde_json::Value, key: &str, new: serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.insert(key.to_string(), new);
        }
        serde_json::Value::Array(items) => {
            if let Some(slot) = key.parse::<usize>().ok().and_then(|i| items.get_mut(i)) {
                *slot = new;
            }
        }
        _ => {}
    }
}

/// SugarCube's documented `Diff.patch` rules, written here as an independent reference: `0`
/// deletes, `[1, a, b]` removes array items a..=b, `[2, v]` copies `v`, an object patches deeper.
fn sc_patch(orig: &serde_json::Value, delta: &serde_json::Value) -> serde_json::Value {
    let mut out = orig.clone();
    let Some(entries) = delta.as_object() else {
        return out;
    };
    for (key, change) in entries {
        if change.as_f64() == Some(0.0) {
            if let serde_json::Value::Object(map) = &mut out {
                map.remove(key);
            }
            continue;
        }
        if let Some(op) = change.as_array() {
            let number = |i: usize| {
                op.get(i)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
            };
            match number(0) {
                Some(1) => {
                    let (from, to) = (number(1).unwrap(), number(2).unwrap());
                    if let serde_json::Value::Array(items) = &mut out {
                        items.drain(from..=to);
                    }
                }
                Some(2) => json_set(&mut out, key, op[1].clone()),
                _ => {}
            }
            continue;
        }
        let inner = sc_patch(json_get(&out, key).unwrap(), change);
        json_set(&mut out, key, inner);
    }
    out
}

/// Every moment of the save, rebuilt from `state.delta` with [`sc_patch`].
fn sc_moments(save: &serde_json::Value) -> Vec<serde_json::Value> {
    let delta = save["state"]["delta"].as_array().unwrap();
    let mut moments = vec![delta[0].clone()];
    for change in &delta[1..] {
        let next = sc_patch(moments.last().unwrap(), change);
        moments.push(next);
    }
    moments
}

/// The JSON value at a core path ("/state/delta/0/variables/gold").
fn json_at<'a>(value: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    path.split('/')
        .skip(1)
        .fold(value, |at, key| json_get(at, key).unwrap())
}

/// Every row of the view shows the value the game loads at the current moment.
fn assert_rows_match_the_game(session: &Session) {
    let view = story_view(session).unwrap();
    let save = json_of(session);
    let moments = sc_moments(&save);
    let current = &moments[view.index]["variables"];
    assert_eq!(view.variables.len(), current.as_object().unwrap().len());
    for row in &view.variables {
        let want = &current[row.name.as_str()];
        if row
            .note
            .as_ref()
            .is_some_and(|n| n.kind == NoteKind::Changed)
        {
            continue; // shown without a value
        }
        let found = json_at(&save, &row.path);
        assert_eq!(found, want, "{}", row.name);
        if let Some(field) = &row.field {
            let text = match want {
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            assert_eq!(field.value, text, "{}", row.name);
        }
    }
}

#[test]
fn sugarcube_save_becomes_a_story_view() {
    let env = env();
    let path = copy_in(&env, STORY);
    let session = open(&env, &path, None).unwrap();
    assert_eq!(session.info().guard, GuardLevel::Exact);
    assert_eq!(session.info().engine.as_deref(), Some("sugarcube"));
    let view = story_view(&session).unwrap();
    assert_eq!(view.title.as_deref(), Some("Lake shore"));
    assert_eq!(view.passage.as_deref(), Some("Lake"));
    assert_eq!(view.date, Some(1_760_000_000_000));
    assert_eq!((view.moments, view.index), (5, 3));

    let names: Vec<&str> = view.variables.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "name", "gold", "hasKey", "mood", "hp", "bag", "stats", "flags", "met", "visited",
            "since", "rival", "lantern"
        ],
        "torch was deleted in moment 2"
    );

    // Scalars at their last writer: the first moment, or a copy of a later one (not moment 4,
    // which comes after the one the game resumes at).
    for (name, path, value) in [
        ("name", "/state/delta/0/variables/name", "Ash"),
        ("gold", "/state/delta/3/variables/gold/1", "140"),
        ("hasKey", "/state/delta/2/variables/hasKey/1", "true"),
        ("mood", "/state/delta/3/variables/mood/1", "brave"),
        ("hp", "/state/delta/0/variables/hp", "10.5"),
        ("lantern", "/state/delta/1/variables/lantern/1", "3"),
    ] {
        let row = story_row(&view, name);
        let field = row.field.as_ref().unwrap();
        assert_eq!(
            (row.path.as_str(), field.path.as_str()),
            (path, path),
            "{name}"
        );
        assert_eq!(field.value, value, "{name}");
        assert!(field.editable, "{name}");
        assert!(row.note.is_none(), "{name}");
    }

    // An object nothing later touched is a group of the first moment.
    let stats = story_row(&view, "stats");
    assert_eq!(stats.path, "/state/delta/0/variables/stats");
    assert_eq!((stats.children, stats.field.is_none()), (2, true));
    assert!(stats.note.is_none());

    // Partly changed values and revive tuples are read-only notes.
    let note = |name: &str| {
        let row = story_row(&view, name);
        assert!(row.field.is_none() && row.children == 0, "{name}");
        (row.path.clone(), row.note.clone().unwrap())
    };
    let (path, bag) = note("bag");
    assert_eq!(path, "/state/delta/1/variables/bag");
    assert_eq!(bag.kind, NoteKind::Changed);
    assert_eq!(note("flags").1.kind, NoteKind::Changed);
    let (path, met) = note("met");
    assert_eq!(path, "/state/delta/2/variables/met/1");
    assert_eq!((met.kind, met.count), (NoteKind::Map, Some(3)));
    let visited = note("visited").1;
    assert_eq!((visited.kind, visited.count), (NoteKind::Set, Some(1)));
    let since = note("since").1;
    assert_eq!(since.kind, NoteKind::Date);
    assert_eq!(since.text.as_deref(), Some("2025-10-09T12:00:00.000Z"));
    assert_eq!(note("rival").1.kind, NoteKind::Empty);

    assert_rows_match_the_game(&session);
}

#[test]
fn one_moment_sugarcube_save() {
    let env = env();
    let path = copy_in(&env, "synthetic/sugarcube/one-moment.save");
    let session = open(&env, &path, None).unwrap();
    let view = story_view(&session).unwrap();
    assert_eq!(view.passage.as_deref(), Some("Start"));
    assert_eq!((view.moments, view.index), (1, 0));
    assert_eq!(story_value(&view, "gold"), "120");
    assert_eq!(story_row(&view, "bag").children, 2);
    assert_eq!(story_row(&view, "seen").children, 1);
    assert_rows_match_the_game(&session);
}

#[test]
fn sugarcube_edit_changes_the_current_value_and_nothing_else() {
    let env = env();
    let path = copy_in(&env, STORY);
    let mut session = open(&env, &path, None).unwrap();
    let before = json_of(&session);
    let view = story_view(&session).unwrap();
    let gold = story_row(&view, "gold").path.clone();
    let name = story_row(&view, "name").path.clone();
    session.edit_value(&gold, "150").unwrap();
    session.edit_value(&name, "Kai").unwrap();
    session
        .edit_value("/state/delta/0/variables/stats/dex", "9")
        .unwrap();
    let view = story_view(&session).unwrap();
    assert_eq!(story_value(&view, "gold"), "150");
    assert_eq!(
        story_row(&view, "gold").field.as_ref().unwrap().original,
        "140"
    );

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    let view = story_view(&reopened).unwrap();
    assert_eq!(story_value(&view, "gold"), "150");
    assert_eq!(story_value(&view, "name"), "Kai");
    assert_rows_match_the_game(&reopened);

    // The file differs from the original in exactly the three edited values.
    let after = json_of(&reopened);
    let mut expected = before.clone();
    expected["state"]["delta"][3]["variables"]["gold"][1] = 150.into();
    expected["state"]["delta"][0]["variables"]["name"] = "Kai".into();
    expected["state"]["delta"][0]["variables"]["stats"]["dex"] = 9.into();
    assert_eq!(after, expected);

    // What the game sees: the current moment changed; the moments around it keep their own values.
    let moments = sc_moments(&after);
    let gold_at = |i: usize| moments[i]["variables"]["gold"].clone();
    assert_eq!(
        (gold_at(2), gold_at(3), gold_at(4)),
        (95.into(), 150.into(), 999.into())
    );
    assert_eq!(moments[3]["variables"]["name"], "Kai");
    assert_eq!(moments[4]["variables"]["name"], "Ashe");
    assert_eq!(moments[3]["variables"]["stats"]["dex"], 9);
    let old = sc_moments(&before);
    for key in ["hasKey", "mood", "bag", "flags", "met", "lantern"] {
        assert_eq!(
            moments[3]["variables"][key], old[3]["variables"][key],
            "{key}"
        );
    }
}

#[test]
fn sugarcube_read_only_rows_cannot_be_edited() {
    let env = env();
    let path = copy_in(&env, STORY);
    let mut session = open(&env, &path, None).unwrap();
    let view = story_view(&session).unwrap();
    let read_only: Vec<&ValueRow> = view.variables.iter().filter(|r| r.note.is_some()).collect();
    assert_eq!(read_only.len(), 6);
    for row in read_only {
        assert!(row.field.is_none(), "{}", row.name);
        let error = session.edit_value(&row.path, "1").unwrap_err();
        assert_eq!(error.code, app_codes::NOT_EDITABLE_NODE, "{}", row.name);
        assert!(session.field_info(&row.path).is_err(), "{}", row.name);
    }
    assert!(session.pending_changes().is_empty());
}

#[test]
fn sugarcube_index_picks_the_moment() {
    let env = env();
    let source = story_source();
    assert!(source.contains("\"index\":3,"));
    let with_index = |text: &str| source.replace("\"index\":3,", text);

    // Moment 1: gold was copied there, the torch still exists, the bag is partly changed.
    let session = open_text(&env, &with_index("\"index\":1,"));
    let view = story_view(&session).unwrap();
    assert_eq!((view.index, view.passage.as_deref()), (1, Some("Forest")));
    assert_eq!(story_value(&view, "gold"), "95");
    assert_eq!(story_value(&view, "torch"), "true");
    assert_eq!(
        story_row(&view, "bag").note.as_ref().unwrap().kind,
        NoteKind::Changed
    );
    assert_rows_match_the_game(&session);

    // Moment 0: everything is in the first moment; the bag is a group again.
    let session = open_text(&env, &with_index("\"index\":0,"));
    let view = story_view(&session).unwrap();
    assert_eq!(view.passage.as_deref(), Some("Start"));
    assert_eq!(story_row(&view, "bag").children, 2);
    assert!(view.variables.iter().all(|r| r.name != "lantern"));
    assert_rows_match_the_game(&session);

    // Missing, negative, too large or not a number: the last moment.
    for text in ["", "\"index\":-1,", "\"index\":99,", "\"index\":\"2\","] {
        let session = open_text(&env, &with_index(text));
        let view = story_view(&session).unwrap();
        assert_eq!(view.index, 4, "{text}");
        assert_eq!(view.passage.as_deref(), Some("Village"), "{text}");
        assert_eq!(story_value(&view, "gold"), "999", "{text}");
        assert_eq!(story_value(&view, "name"), "Ashe", "{text}");
        assert_eq!(
            story_row(&view, "stats").note.as_ref().unwrap().kind,
            NoteKind::Changed
        );
    }
}

#[test]
fn odd_sugarcube_shapes_fall_back_to_the_values_list() {
    let env = env();
    for json in [
        r#"{"state":{"delta":[]}}"#,
        r#"{"state":{"delta":[5]}}"#,
        r#"{"state":{"delta":[{"title":"A"}]}}"#,
        r#"{"state":{"delta":[{"variables":3}]}}"#,
        r#"{"state":{"delta":{"0":{"variables":{}}}}}"#,
        r#"{"state":"x"}"#,
        r#"[{"state":{"delta":[{"variables":{}}]}}]"#,
        r#"{"state":{"index":1,"delta":[{"variables":{"a":1}},7]}}"#,
        r#"{"state":{"index":1,"delta":[{"variables":{"a":1}},{"variables":[2,{"a":2}]}]}}"#,
        r#"{"state":{"delta":[{"variables":{"a":1},"variables":{"a":2}}]}}"#,
        r#"{"state":{"delta":[{"variables":{"a":1}}]},"state":{"delta":[]}}"#,
        "42",
    ] {
        let session = open_text(&env, json);
        assert!(story_view(&session).is_none(), "{json}");
        assert!(session.info().engine.is_none(), "{json}");
    }
}

#[test]
fn odd_sugarcube_values_are_read_only_and_never_stale() {
    let env = env();
    let json = r#"{"desc":"Turn 4","type":3,"date":1760000000000.5,"id":"x","state":{"index":2,"delta":[
        {"title":"A","variables":{"a":1,"b":2,"c":3,"d":4,"e":5,"f":6,"g":7,"h":8,"h":9,"u":["(revive:)",["undefined"]]}},
        null,
        {"title":[2,5],"variables":{"a":[9,1],"b":"x","c":[2,5,6],"d":[2,7],"d":[2,8],"f":0.0,"g":[3,1760000000000]}},
        {"variables":{"e":[2,50]}}
    ]}}"#;
    let session = open_text(&env, json);
    let view = story_view(&session).unwrap();
    assert_eq!(view.title.as_deref(), Some("Turn 4"));
    assert_eq!(view.date, None, "a decimal date is not shown");
    assert_eq!(
        view.passage, None,
        "a title copy that is not text is unknown"
    );
    for name in ["a", "b", "c", "d", "g", "h"] {
        let row = story_row(&view, name);
        assert!(row.field.is_none(), "{name}");
        assert_eq!(row.note.as_ref().unwrap().kind, NoteKind::Changed, "{name}");
    }
    assert!(view.variables.iter().all(|r| r.name != "f"), "0.0 deletes");
    assert_eq!(
        story_row(&view, "u").note.as_ref().unwrap().kind,
        NoteKind::Code
    );
    // `e` is changed only after the current moment.
    let e = story_row(&view, "e").field.as_ref().unwrap();
    assert_eq!(
        (e.value.as_str(), e.path.as_str()),
        ("5", "/state/delta/0/variables/e")
    );
}

#[test]
fn sugarcube_variables_are_capped() {
    let env = env();
    let names: Vec<String> = (0..10_050).map(|i| format!("\"v{i}\":{i}")).collect();
    let json = format!(
        r#"{{"state":{{"index":0,"delta":[{{"title":"A","variables":{{{}}}}}]}}}}"#,
        names.join(",")
    );
    let session = open_text(&env, &json);
    let view = story_view(&session).unwrap();
    assert_eq!(view.variables.len(), 10_000);
    assert_eq!(story_value(&view, "v9999"), "9999");
}

// Pasted save codes.

/// The pasted-codes folder of a test environment: next to its backup folder.
fn pasted_folder(env: &Env) -> PathBuf {
    env.backups.parent().unwrap().join("pasted")
}

fn paste(env: &Env, text: &str) -> Result<Session, ApiError> {
    Session::open_code_with_source(text, Some(env.backups.clone()), RenpyKeySource::At(None))
}

fn fixture_text(relative: &str) -> String {
    fs::read_to_string(fixture(relative)).unwrap()
}

/// Decodes a copied code with the core alone (no session) and returns the value text at `path`.
fn decoded_value(code: &str, path: &str) -> String {
    let mut reg = Registry::default();
    saveprism_codecs::register_all(&mut reg);
    let limits = Limits::default();
    let found = detect(code.as_bytes(), &reg, &limits, &DetectOptions::default()).unwrap();
    let out = pipeline::decode(code.as_bytes(), &found.best.recipe, &reg, &limits).unwrap();
    let id = out
        .doc
        .resolve(&saveprism_core::Path::parse(path).unwrap())
        .unwrap();
    value_text(out.doc.get(id).unwrap()).unwrap()
}

#[test]
fn a_pasted_mv_code_is_edited_and_copied_back_as_a_code() {
    let env = env();
    let text = fixture_text("synthetic/rpgmaker-mv/file1.rpgsave");
    let mut session = paste(&env, &text).unwrap();
    let info = session.info();
    assert!(info.pasted);
    assert!(info.text_code);
    assert!(info.editable);
    assert_eq!(info.guard, GuardLevel::Exact);
    assert_eq!(info.engine.as_deref(), Some("rpgmaker-mv"));
    let file = PathBuf::from(&info.path);
    assert_eq!(file.parent().unwrap(), pasted_folder(&env));
    assert_eq!(fs::read_to_string(&file).unwrap(), text.trim());

    // Unchanged, the code comes back as it was pasted.
    assert_eq!(session.copy_code().unwrap(), text.trim());

    let gold = rpg_view(&session).unwrap().gold.path;
    session.edit_value(&gold, "4321").unwrap();
    let copied = session.copy_code().unwrap();
    assert_ne!(copied, text.trim());
    assert_eq!(decoded_value(&copied, &gold), "4321");

    // Copying is not a save: the edit is still pending, the history and the file are unchanged.
    assert_eq!(session.pending_changes().len(), 1);
    assert!(session.history_state().can_undo);
    assert_eq!(fs::read_to_string(&file).unwrap(), text.trim());

    // The copied code pastes again with the new gold.
    let again = paste(&env, &copied).unwrap();
    assert_eq!(rpg_view(&again).unwrap().gold.value, "4321");
}

#[test]
fn a_pasted_sugarcube_code_keeps_its_story_view() {
    let env = env();
    let text = fixture_text(STORY);
    let mut session = paste(&env, &text).unwrap();
    assert!(session.info().pasted);
    assert_eq!(session.info().engine.as_deref(), Some("sugarcube"));
    let view = story_view(&session).unwrap();
    let gold = story_row(&view, "gold").path.clone();
    session.edit_value(&gold, "777").unwrap();

    let copied = session.copy_code().unwrap();
    assert_eq!(decoded_value(&copied, &gold), "777");
    assert_eq!(session.pending_changes().len(), 1);
    let again = paste(&env, &copied).unwrap();
    assert_eq!(story_value(&story_view(&again).unwrap(), "gold"), "777");
}

#[test]
fn whitespace_around_a_code_is_fine() {
    let env = env();
    let text = fixture_text("synthetic/rpgmaker-mv/file1.rpgsave");
    let session = paste(&env, &format!("\r\n  \t{text}\n\n ")).unwrap();
    let info = session.info();
    assert_eq!(info.guard, GuardLevel::Exact);
    assert_eq!(fs::read_to_string(&info.path).unwrap(), text.trim());
    assert_eq!(session.copy_code().unwrap(), text.trim());
}

#[test]
fn empty_and_bad_codes_are_refused_and_leave_no_file() {
    let env = env();
    for empty in ["", "   ", "\r\n\t"] {
        let error = paste(&env, empty).err().unwrap();
        assert_eq!(error.code, app_codes::EMPTY_CODE, "{empty:?}");
    }
    assert!(!pasted_folder(&env).exists());

    // A code that opens first, so the folder exists and holds one file.
    let good = paste(&env, &fixture_text(STORY)).unwrap();
    let kept = file_names(&pasted_folder(&env));
    assert_eq!(kept.len(), 1);

    for garbage in [
        "this is not a save code",
        "N4IgzgnmAuCmC2IBcoD6YCGA3WBRAdhgEYA2sAJstAE4CusANCKvLPrQcWZUjfU6jYBjAPa18cap1IUqdRsyIZo0MgGEx",
        "\u{1}\u{2}\u{3}",
    ] {
        let error = paste(&env, garbage).err().unwrap();
        assert!(!error.message.is_empty(), "{garbage}");
        assert!(
            error.hint.contains("whole code"),
            "{garbage}: {}",
            error.hint
        );
        assert_eq!(file_names(&pasted_folder(&env)), kept, "{garbage}");
    }
    // The document opened before is untouched.
    assert!(PathBuf::from(good.info().path).exists());
}

#[test]
fn binary_formats_refuse_copy_code() {
    let env = env();
    let path = copy_in(&env, "keyed/es3-aes/settings.es3");
    let session = open(&env, &path, Some(PASSWORD)).unwrap();
    assert!(!session.info().text_code);
    assert!(!session.info().pasted);
    let error = session.copy_code().err().unwrap();
    assert_eq!(error.code, app_codes::NOT_TEXT_CODE);
    assert!(error.hint.contains("Save as"));

    // RPG Maker MZ files are a deflate stream: UTF-8 in the file, but not a code to paste.
    let path = copy_in(&env, "synthetic/rpgmaker-mz/file1.rmmzsave");
    let session = open(&env, &path, None).unwrap();
    assert!(!session.info().text_code);
    assert_eq!(
        session.copy_code().err().unwrap().code,
        app_codes::NOT_TEXT_CODE
    );
}

#[test]
fn text_files_opened_normally_can_be_copied_as_a_code() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mv/file1.rpgsave");
    let mut session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert!(info.text_code);
    assert!(!info.pasted);
    // A file's own name tells the engine; only pasted codes get it from the layers.
    assert_eq!(info.engine, None);
    let gold = rpg_view(&session).unwrap().gold.path;
    session.edit_value(&gold, "55").unwrap();
    assert_eq!(decoded_value(&session.copy_code().unwrap(), &gold), "55");
    assert_eq!(session.pending_changes().len(), 1);
}

#[test]
fn only_the_newest_pasted_codes_are_kept_and_other_files_stay() {
    let env = env();
    let dir = pasted_folder(&env);
    fs::create_dir_all(dir.join("code-5.txt.d")).unwrap();
    for stamp in 1..=15 {
        fs::write(dir.join(format!("code-{stamp}.txt")), "old").unwrap();
    }
    let foreign = ["notes.txt", "code-abc.txt", "code-7.json", "Code-8.txt"];
    for name in foreign {
        fs::write(dir.join(name), "mine").unwrap();
    }

    let session = paste(&env, &fixture_text(STORY)).unwrap();
    let opened = PathBuf::from(session.info().path);
    let opened_name = opened.file_name().unwrap().to_string_lossy().into_owned();
    let mut codes: Vec<String> = file_names(&dir)
        .into_iter()
        .filter(|name| name.starts_with("code-") && name.ends_with(".txt"))
        .filter(|name| name != "code-abc.txt")
        .collect();
    assert_eq!(codes.len(), MAX_PASTED);
    codes.retain(|name| *name != opened_name);
    let mut expected: Vec<String> = (7..=15).map(|n| format!("code-{n}.txt")).collect();
    expected.sort();
    assert_eq!(codes, expected);
    for name in foreign {
        assert!(dir.join(name).exists(), "{name}");
    }
    assert!(dir.join("code-5.txt.d").is_dir());
}

#[test]
fn the_code_just_pasted_is_kept_even_with_newer_names_around() {
    let env = env();
    let dir = pasted_folder(&env);
    fs::create_dir_all(&dir).unwrap();
    let future = 99_999_999_999_000_u64;
    for step in 0..12 {
        fs::write(dir.join(format!("code-{}.txt", future + step)), "x").unwrap();
    }
    let session = paste(&env, &fixture_text(STORY)).unwrap();
    let opened = PathBuf::from(session.info().path);
    assert!(opened.exists());
    assert_eq!(file_names(&dir).len(), MAX_PASTED);
    assert!(dir.join(format!("code-{}.txt", future + 11)).exists());
    assert!(!dir.join(format!("code-{future}.txt")).exists());
}

#[test]
fn many_pastes_keep_ten_codes() {
    let env = env();
    let text = fixture_text(STORY);
    let mut last = PathBuf::new();
    for _ in 0..(MAX_PASTED + 3) {
        last = PathBuf::from(paste(&env, &text).unwrap().info().path);
    }
    assert_eq!(file_names(&pasted_folder(&env)).len(), MAX_PASTED);
    assert!(last.exists());
}

#[test]
fn the_pasted_folder_is_never_the_backup_folder() {
    let env = env();
    let mut session = paste(&env, &fixture_text(STORY)).unwrap();
    let file = PathBuf::from(session.info().path);
    assert!(!file.starts_with(&env.backups));
    assert_ne!(file.parent().unwrap(), env.backups);
    assert!(file_names(&env.backups).is_empty());

    // "Save as file" makes it an ordinary file; the pasted copy stays where it was.
    let view = story_view(&session).unwrap();
    session
        .edit_value(&story_row(&view, "gold").path, "1")
        .unwrap();
    let target = env.work.join("story.save");
    let result = session.save(target.to_str().unwrap(), false, None).unwrap();
    assert!(result.backup_path.is_none());
    let info = session.info();
    assert!(!info.pasted);
    assert_eq!(PathBuf::from(&info.path), target);
    assert!(file.exists());
    assert!(file_names(&env.backups).is_empty());
    let reopened = open(&env, &target, None).unwrap();
    assert_eq!(story_value(&story_view(&reopened).unwrap(), "gold"), "1");
}

// RPG Maker XP, VX and VX Ace saves (Ruby Marshal).

const XP_SAVE: &str = "synthetic/rpgmaker-xp/Save1.rxdata";
const VX_SAVE: &str = "synthetic/rpgmaker-vx/Save1.rvdata";
const VXACE_SAVE: &str = "synthetic/rpgmaker-vxace/Save03.rvdata2";

/// Copies a synthetic game database (`fixtures/<relative>/*`) to `Data` next to the copied save.
fn copy_data(env: &Env, relative: &str) -> PathBuf {
    let target = env.work.join("Data");
    fs::create_dir_all(&target).unwrap();
    for entry in fs::read_dir(fixture(relative)).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
    }
    target
}

fn values(entries: &[Entry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| entry.field.value.as_str())
        .collect()
}

fn names(entries: &[Entry]) -> Vec<Option<&str>> {
    entries.iter().map(|entry| entry.name.as_deref()).collect()
}

fn bonuses(actor: &Actor) -> Vec<(&str, &str)> {
    actor
        .bonuses
        .iter()
        .map(|bonus| (bonus.key.as_str(), bonus.field.value.as_str()))
        .collect()
}

fn actor_names(view: &RpgMakerView) -> Vec<&str> {
    view.actors
        .iter()
        .map(|actor| actor.name.as_str())
        .collect()
}

fn open_rgss(env: &Env, relative: &str) -> Session {
    let path = copy_in(env, relative);
    let session = open(env, &path, None).unwrap();
    assert_eq!(session.info().guard, GuardLevel::Exact, "{relative}");
    session
}

/// The view of a copied save with a copied `Data` folder next to it, and that folder.
fn view_with_data(save: &str, data: &str) -> (RpgMakerView, String) {
    let env = env();
    let folder = copy_data(&env, data);
    let view = rpg_view(&open_rgss(&env, save)).unwrap();
    (view, folder.to_string_lossy().into_owned())
}

#[test]
fn xp_save_becomes_rpg_maker_sections() {
    let env = env();
    let view = rpg_view(&open_rgss(&env, XP_SAVE)).unwrap();

    assert_eq!(view.engine, "xp");
    assert_eq!(view.gold.value, "5000");
    assert_eq!(view.gold.max, Some(9_999_999));
    assert_eq!(view.gold.hint, "Whole number \u{b7} 0 to 9,999,999");
    assert!(view.gold.editable);

    assert_eq!(actor_names(&view), ["Aluxes", "Basil"]);
    let actor = &view.actors[0];
    assert_eq!(actor.id, 1);
    assert_eq!(actor.level.value, "7");
    assert_eq!(actor.level.max, Some(99));
    assert_eq!(actor.hp.value, "741");
    assert_eq!(actor.mp.value, "534");
    assert_eq!(actor.exp.as_ref().map(|f| f.value.as_str()), Some("500"));
    assert_eq!(
        bonuses(actor),
        [
            ("maxHp", "0"),
            ("maxMp", "0"),
            ("strength", "0"),
            ("dexterity", "0"),
            ("agility", "0"),
            ("intelligence", "0"),
        ]
    );
    // The party keeps copies of the actors, but the game loads them from Game_Actors (dump 7).
    assert_eq!(actor.hp.path, "/7/@data/1/@hp");
    assert_eq!(actor.mp.path, "/7/@data/1/@sp");
    assert_eq!(view.actors[1].level.path, "/7/@data/2/@level");
    assert!(actor.hp.editable);

    assert_eq!(ids(&view.items), [1, 4]);
    assert_eq!(values(&view.items), ["3", "1"]);
    assert_eq!(view.items[0].field.max, Some(99));
    assert_eq!(ids(&view.weapons), [1]);
    assert!(view.armors.is_empty());
    // Variable 2 is 0 and has no name, so it is left out.
    assert_eq!(ids(&view.variables), [1, 3, 4]);
    assert_eq!(values(&view.variables), ["100", "-7", "8589934592"]);
    assert_eq!(ids(&view.switches), [1, 3]);
    assert_eq!(view.switches[0].field.kind, FieldKind::Bool);

    // No Data folder next to the save: no names.
    assert!(view.names_folder.is_none());
    assert!(view.items.iter().all(|entry| entry.name.is_none()));
}

#[test]
fn vx_save_becomes_rpg_maker_sections() {
    let env = env();
    let view = rpg_view(&open_rgss(&env, VX_SAVE)).unwrap();

    assert_eq!(view.engine, "vx");
    assert_eq!(view.gold.value, "9000");
    assert_eq!(view.gold.max, Some(9_999_999));

    // The party keeps actor ids (1 and 3); the name of actor 3 is not UTF-8.
    assert_eq!(actor_names(&view), ["Ralph", "Benn\u{fffd}tt"]);
    let actor = &view.actors[0];
    assert_eq!(actor.level.value, "9");
    assert_eq!(actor.hp.value, "612");
    assert_eq!(actor.mp.value, "88");
    assert_eq!(actor.exp.as_ref().map(|f| f.value.as_str()), Some("2500"));
    assert_eq!(
        bonuses(actor),
        [
            ("maxHp", "50"),
            ("maxMp", "0"),
            ("attack", "5"),
            ("defense", "3"),
            ("spirit", "0"),
            ("agility", "2"),
        ]
    );
    assert_eq!(view.actors[1].hp.path, "/9/@data/3/@hp");
    assert_eq!(view.actors[1].mp.path, "/9/@data/3/@mp");

    assert_eq!(ids(&view.items), [1, 9]);
    assert_eq!(values(&view.items), ["3", "2"]);
    assert_eq!(ids(&view.weapons), [2]);
    assert_eq!(ids(&view.armors), [5]);
    assert_eq!(ids(&view.variables), [1, 3, 4]);
    assert_eq!(view.variables[2].field.kind, FieldKind::String);
    assert_eq!(ids(&view.switches), [2, 4]);
    assert!(view.names_folder.is_none());
}

#[test]
fn vxace_save_becomes_rpg_maker_sections() {
    let env = env();
    let view = rpg_view(&open_rgss(&env, VXACE_SAVE)).unwrap();

    assert_eq!(view.engine, "vxace");
    assert_eq!(view.gold.value, "1250");
    assert_eq!(view.gold.max, Some(99_999_999));

    assert_eq!(actor_names(&view), ["Eric", "Natalie"]);
    let eric = &view.actors[0];
    assert_eq!(eric.level.value, "12");
    assert_eq!(eric.hp.value, "451");
    assert_eq!(eric.mp.value, "80");
    // `@exp` is {2 => 100, 1 => 4200} and the class is 1.
    let exp = eric.exp.as_ref().unwrap();
    assert_eq!(exp.value, "4200");
    assert_eq!(exp.path, "/1/actors/@data/1/@exp/1");
    assert!(exp.editable);
    assert_eq!(
        bonuses(eric),
        [
            ("maxHp", "50"),
            ("maxMp", "10"),
            ("attack", "5"),
            ("defense", "4"),
            ("magicAttack", "3"),
            ("magicDefense", "2"),
            ("agility", "1"),
            ("luck", "0"),
        ]
    );

    // The slot of Natalie in Game_Actors is a link to the object kept in Game_System.
    let natalie = &view.actors[1];
    assert_eq!(natalie.hp.value, "452");
    assert_eq!(natalie.hp.path, "/1/system/@companion/@hp");
    assert!(natalie.hp.editable);
    assert_eq!(natalie.bonuses.len(), 8);

    assert_eq!(ids(&view.items), [1, 2]);
    assert_eq!(values(&view.items), ["5", "1"]);
    assert_eq!(view.items[0].field.path, "/1/party/@items/0");
    assert_eq!(ids(&view.weapons), [1]);
    assert!(view.armors.is_empty());
    assert_eq!(ids(&view.variables), (1..=10).collect::<Vec<u32>>());
    assert_eq!(view.variables[5].field.kind, FieldKind::Float);
    assert_eq!(ids(&view.switches), [1, 4]);
    assert!(view.names_folder.is_none());

    // The first VX Ace save has no stat bonuses.
    let view = rpg_view(&open_rgss(&env, "synthetic/rpgmaker-vxace/Save01.rvdata2")).unwrap();
    assert_eq!(view.engine, "vxace");
    assert_eq!(actor_names(&view), ["Eric", "Natalie"]);
    assert!(view.actors[0].bonuses.is_empty());
}

#[test]
fn rgss_names_come_from_the_data_folder_next_to_the_save() {
    let (view, data) = view_with_data(XP_SAVE, "synthetic/rpgmaker-xp/Data");
    assert_eq!(view.names_folder, Some(data));
    assert_eq!(
        names(&view.items),
        [Some("Herb Tonic"), Some("Phoenix Ash")]
    );
    assert_eq!(names(&view.weapons), [Some("Bronze Sword")]);
    // The named variable 2 is shown although its value is 0.
    assert_eq!(ids(&view.variables), [1, 2, 3, 4]);
    assert_eq!(
        names(&view.variables),
        [
            Some("Gold Found"),
            Some("Steps Taken"),
            None,
            Some("Big Number")
        ]
    );
    assert_eq!(ids(&view.switches), [1, 2, 3]);
    assert_eq!(view.switches[1].name.as_deref(), Some("Lamp Lit"));

    let (view, data) = view_with_data(VX_SAVE, "synthetic/rpgmaker-vx/Data");
    assert_eq!(view.names_folder, Some(data));
    assert_eq!(names(&view.items), [Some("Herb"), Some("Smelling Salts")]);
    assert_eq!(names(&view.weapons), [Some("Long Spear")]);
    assert_eq!(names(&view.armors), [Some("Round Shield")]);
    assert_eq!(
        names(&view.variables),
        [Some("Coins"), Some("Day"), None, None]
    );
    assert_eq!(ids(&view.switches), [1, 2, 4]);
    assert_eq!(view.switches[0].name.as_deref(), Some("Bell Rung"));

    let (view, data) = view_with_data(VXACE_SAVE, "synthetic/rpgmaker-vxace/Data");
    assert_eq!(view.names_folder, Some(data));
    assert_eq!(
        names(&view.items),
        [Some("Healing Herb"), Some("Ether Drop")]
    );
    assert_eq!(names(&view.weapons), [Some("\u{c9}p\u{e9}e courte")]);
    assert_eq!(ids(&view.variables), (1..=11).collect::<Vec<u32>>());
    assert_eq!(view.variables[10].name.as_deref(), Some("Zero Counter"));
    assert_eq!(
        names(&view.switches),
        [Some("Door Open"), Some("Lights Off"), Some("Met Natalie")]
    );
}

#[test]
fn rgss_names_also_come_from_the_folder_above_and_only_for_the_same_engine() {
    // A save in a subfolder of the game: `<game>/Saves/Save1.rxdata` and `<game>/Data`.
    let env = env();
    let saves = env.work.join("Saves");
    fs::create_dir_all(&saves).unwrap();
    let path = saves.join("Save1.rxdata");
    fs::copy(fixture(XP_SAVE), &path).unwrap();
    let data = copy_data(&env, "synthetic/rpgmaker-xp/Data");
    let view = rpg_view(&open(&env, &path, None).unwrap()).unwrap();
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));

    // VX Ace data files next to an XP save are not read.
    let (view, _) = view_with_data(XP_SAVE, "synthetic/rpgmaker-vxace/Data");
    assert!(view.names_folder.is_none());
}

#[test]
fn rgss_edits_through_the_view_survive_a_save() {
    for relative in [XP_SAVE, VX_SAVE, VXACE_SAVE] {
        let env = env();
        let path = copy_in(&env, relative);
        let mut session = open(&env, &path, None).unwrap();
        let view = rpg_view(&session).unwrap();
        let last = view.actors.len() - 1;
        session.edit_value(&view.gold.path, "4321").unwrap();
        session.edit_value(&view.items[0].field.path, "42").unwrap();
        session
            .edit_value(&view.actors[last].hp.path, "999")
            .unwrap();
        session.save(path.to_str().unwrap(), false, None).unwrap();

        let reopened = open(&env, &path, None).unwrap();
        let info = reopened.info();
        assert_eq!(info.guard, GuardLevel::Exact, "{relative}");
        assert!(info.editable, "{relative}");
        let again = rpg_view(&reopened).unwrap();
        assert_eq!(again.gold.value, "4321", "{relative}");
        assert_eq!(again.items[0].field.value, "42", "{relative}");
        assert_eq!(again.actors[last].hp.value, "999", "{relative}");
        assert_eq!(again.actors[last].hp.path, view.actors[last].hp.path);
    }
}

#[test]
fn rgss_limits_are_enforced() {
    for (relative, max, over) in [
        (XP_SAVE, 9_999_999, "10000000"),
        (VX_SAVE, 9_999_999, "10000000"),
        (VXACE_SAVE, 99_999_999, "100000000"),
    ] {
        let env = env();
        let mut session = open_rgss(&env, relative);
        let view = rpg_view(&session).unwrap();
        let gold = view.gold.path.clone();

        let error = session.edit_value(&gold, over).unwrap_err();
        assert_eq!(error.code, app_codes::OUT_OF_RANGE, "{relative}");
        assert_eq!(session.field_info(&gold).unwrap().max, Some(max));
        session.edit_value(&gold, &max.to_string()).unwrap();

        let count = view.items[0].field.path.clone();
        let error = session.edit_value(&count, "100").unwrap_err();
        assert_eq!(error.code, app_codes::OUT_OF_RANGE, "{relative}");
        let level = view.actors[0].level.path.clone();
        let error = session.edit_value(&level, "0").unwrap_err();
        assert_eq!(error.code, app_codes::OUT_OF_RANGE, "{relative}");
    }

    // JSON Apply checks the same limit.
    let env = env();
    let mut session = open_rgss(&env, XP_SAVE);
    let gold = rpg_view(&session).unwrap().gold.path;
    let text = edited(&session, |json| {
        *json.pointer_mut(&gold).unwrap() = 10_000_000.into();
    });
    let error = session.apply_json(&text).unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert!(error.message.contains("9,999,999"), "{}", error.message);
    assert!(session.pending_changes().is_empty());
}

// RPG Maker 2000 and 2003 saves (LCF).

const SAVE_2K: &str = "synthetic/rpgmaker-2k/Save01.lsd";
const SAVE_2K3: &str = "synthetic/rpgmaker-2k3/Save01.lsd";
const DB_2K: &str = "synthetic/rpgmaker-2k/RPG_RT.ldb";
const DB_2K3: &str = "synthetic/rpgmaker-2k3/RPG_RT.ldb";

/// Opens a copy of `save`, with a copy of `database` next to it as `RPG_RT.ldb`.
fn open_lcf(env: &Env, save: &str, database: Option<&str>) -> Session {
    if let Some(database) = database {
        fs::copy(fixture(database), env.work.join("RPG_RT.ldb")).unwrap();
    }
    let path = copy_in(env, save);
    let session = open(env, &path, None).unwrap();
    assert_eq!(session.info().guard, GuardLevel::Exact, "{save}");
    session
}

#[test]
fn an_rpg_rt_ini_encoding_hint_is_used_and_reported() {
    let env = env();
    let save = open_lcf(&env, SAVE_2K, None);
    assert_eq!(save.info().text_encoding.as_deref(), Some("windows-1252"));

    let path = copy_in(&env, SAVE_2K);
    fs::write(
        env.work.join("RPG_RT.ini"),
        b"[EasyRPG]\r\nEncoding=932\r\n",
    )
    .unwrap();
    let hinted = open(&env, &path, None).unwrap();
    assert_eq!(hinted.info().text_encoding.as_deref(), Some("shift_jis"));
    assert_eq!(hinted.info().guard, GuardLevel::Exact);
}

#[test]
fn a_2k_save_becomes_rpg_maker_sections() {
    let env = env();
    let view = rpg_view(&open_lcf(&env, SAVE_2K, None)).unwrap();

    assert_eq!(view.engine, "2k");
    assert_eq!(view.gold.value, "5000");
    assert_eq!(view.gold.path, "/inventory/gold");
    assert_eq!(view.gold.max, Some(999_999));
    assert!(view.gold.editable);

    // Actor 1 uses its database name, which is unknown without RPG_RT.ldb.
    assert_eq!(actor_names(&view), ["Actor #1", "Mira"]);
    let actor = &view.actors[0];
    assert_eq!(actor.level.value, "12");
    assert_eq!(actor.level.path, "/actors/0/level");
    assert_eq!(actor.level.max, Some(99));
    assert_eq!(actor.hp.value, "250");
    assert_eq!(actor.mp.value, "40");
    assert_eq!(actor.exp.as_ref().map(|f| f.value.as_str()), Some("3400"));
    assert_eq!(
        bonuses(actor),
        [
            ("maxHp", "10"),
            ("maxMp", "5"),
            ("attack", "3"),
            ("defense", "2"),
            ("spirit", "0"),
            ("agility", "1"),
        ]
    );
    assert_eq!(view.actors[1].hp.path, "/actors/1/current_hp");

    // Without the database every entry is an item.
    assert_eq!(ids(&view.items), [1, 2, 5, 7]);
    assert_eq!(values(&view.items), ["3", "1", "2", "99"]);
    assert_eq!(view.items[3].field.path, "/inventory/item_counts/3");
    assert_eq!(view.items[0].field.max, Some(99));
    assert!(view.weapons.is_empty() && view.armors.is_empty());
    // Variable 3 is 0 and has no name, so it is left out; index 0 is variable 1.
    assert_eq!(ids(&view.variables), [1, 2, 4, 5, 6]);
    assert_eq!(
        values(&view.variables),
        ["100", "-7", "20000000", "-123456789", "2147483647"]
    );
    assert_eq!(view.variables[0].field.path, "/system/variables/0");
    assert_eq!(ids(&view.switches), [1, 3]);
    assert_eq!(view.switches[0].field.kind, FieldKind::Bool);
    assert!(view.names_folder.is_none());
}

#[test]
fn lcf_names_and_item_types_come_from_rpg_rt_ldb() {
    let env = env();
    let view = rpg_view(&open_lcf(&env, SAVE_2K, Some(DB_2K))).unwrap();
    assert_eq!(
        view.names_folder,
        Some(env.work.to_string_lossy().into_owned())
    );
    assert_eq!(view.engine, "2k");
    assert_eq!(actor_names(&view), ["Alex", "Mira"]);
    assert_eq!(ids(&view.items), [1, 7]);
    // A Windows-1252 name.
    assert_eq!(names(&view.items), [Some("Potion"), Some("\u{c9}lixir")]);
    assert_eq!(ids(&view.weapons), [2]);
    assert_eq!(names(&view.weapons), [Some("Bronze Sword")]);
    assert_eq!(ids(&view.armors), [5]);
    assert_eq!(view.armors[0].field.path, "/inventory/item_counts/2");
    // The named variable 3 is shown although it is 0.
    assert_eq!(ids(&view.variables), [1, 2, 3, 4, 5, 6]);
    assert_eq!(view.variables[2].name.as_deref(), Some("Zero Counter"));
    assert_eq!(ids(&view.switches), [1, 2, 3]);
    assert_eq!(view.switches[1].name.as_deref(), Some("Lamp Lit"));

    // A 2003 database makes the save a 2003 one.
    let env = self::env();
    let view = rpg_view(&open_lcf(&env, SAVE_2K, Some(DB_2K3))).unwrap();
    assert_eq!(view.engine, "2k3");
}

#[test]
fn a_2k3_save_without_gold_shows_zero_read_only() {
    let env = env();
    let session = open_lcf(&env, SAVE_2K3, Some(DB_2K3));
    let view = rpg_view(&session).unwrap();
    assert_eq!(view.engine, "2k3");
    assert_eq!(view.gold.value, "0");
    assert!(!view.gold.editable);
    // The party is actors 3 and 1; actor 1 takes its name from the database.
    assert_eq!(actor_names(&view), ["Kai", "Alex"]);
    assert_eq!(view.actors[0].level.value, "20");
    assert_eq!(ids(&view.weapons), [2]);
    assert_eq!(names(&view.items), [Some("Ether")]);
    assert_eq!(values(&view.variables), ["-1", "16777217", "7"]);
    assert_eq!(ids(&view.variables), [1, 2, 5]);
    assert_eq!(ids(&view.switches), [2, 3]);
    assert_eq!(view.switches[0].name.as_deref(), Some("Gate Open"));

    // The 2003 fields alone make it a 2003 save.
    let env = self::env();
    let view = rpg_view(&open_lcf(&env, SAVE_2K3, None)).unwrap();
    assert_eq!(view.engine, "2k3");
    assert_eq!(actor_names(&view), ["Kai", "Actor #1"]);
}

#[test]
fn lcf_edits_through_the_view_survive_a_save() {
    let env = env();
    let mut session = open_lcf(&env, SAVE_2K, Some(DB_2K));
    let view = rpg_view(&session).unwrap();
    session.edit_value(&view.gold.path, "999999").unwrap();
    session.edit_value(&view.items[0].field.path, "42").unwrap();
    session
        .edit_value(&view.variables[0].field.path, "16777300")
        .unwrap();
    session
        .edit_value(&view.switches[1].field.path, "true")
        .unwrap();
    session
        .edit_value(&view.actors[1].level.path, "99")
        .unwrap();
    session.edit_value(&view.actors[0].hp.path, "1000").unwrap();
    let path = env.work.join("Save01.lsd");
    session.save(path.to_str().unwrap(), false, None).unwrap();

    let reopened = open(&env, &path, None).unwrap();
    let info = reopened.info();
    assert_eq!(info.guard, GuardLevel::Exact);
    assert!(info.editable);
    let again = rpg_view(&reopened).unwrap();
    assert_eq!(again.gold.value, "999999");
    assert_eq!(again.items[0].field.value, "42");
    assert_eq!(again.variables[0].field.value, "16777300");
    assert_eq!(again.switches[1].field.value, "true");
    assert_eq!(again.actors[1].level.value, "99");
    assert_eq!(again.actors[0].hp.value, "1000");
    assert_eq!(actor_names(&again), ["Alex", "Mira"]);
}

#[test]
fn lcf_limits_are_enforced() {
    let env = env();
    let mut session = open_lcf(&env, SAVE_2K, None);
    let view = rpg_view(&session).unwrap();
    let gold = view.gold.path.clone();
    let error = session.edit_value(&gold, "1000000").unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    assert_eq!(session.field_info(&gold).unwrap().max, Some(999_999));
    let error = session
        .edit_value(&view.items[0].field.path, "100")
        .unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    let error = session
        .edit_value(&view.actors[0].level.path, "0")
        .unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    // A variable is a 32-bit number.
    let error = session
        .edit_value(&view.variables[0].field.path, "2147483648")
        .unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);

    // The database itself opens as a plain list of values.
    let env = self::env();
    let path = copy_in(&env, DB_2K);
    let session = open(&env, &path, None).unwrap();
    assert_eq!(session.info().guard, GuardLevel::Exact);
    assert!(matches!(
        session.game_view().unwrap(),
        GameView::Values { .. }
    ));
}

// Ren'Py saves.

const RENPY_SAVE: &str = "synthetic/renpy/renpy8-0/1-1-LT1.save";
const RENPY_SIGNED: &str = "synthetic/renpy/renpy8-1-signed/1-1-LT1.save";
const RENPY_7: &str = "synthetic/renpy/renpy7/1-1-LT1.save";
const RENPY_PERSISTENT: &str = "synthetic/renpy/persistent/persistent";
const RENPY_PERSISTENT_SIGNED: &str = "synthetic/renpy/persistent-signed/persistent";

fn value_rows(session: &Session) -> Vec<ValueRow> {
    let GameView::Values { rows, .. } = session.game_view().unwrap() else {
        panic!("not a Values list");
    };
    rows
}

fn value_row<'a>(rows: &'a [ValueRow], name: &str) -> &'a ValueRow {
    rows.iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

#[test]
fn renpy_save_lists_the_game_variables_first() {
    let env = env();
    let path = copy_in(&env, RENPY_SAVE);
    let session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert_eq!(info.engine.as_deref(), Some("renpy"));
    assert_eq!(info.guard, GuardLevel::Exact);
    assert!(info.editable);
    assert_eq!(info.notice, None);
    assert_eq!(
        info.recipe,
        vec!["container.renpy-save", "format.python-pickle"]
    );

    let rows = value_rows(&session);
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "backpack",
            "bank_balance",
            "companion_name",
            "door_open",
            "flags_seen",
            "hero_name",
            "huge_number",
            "inventory",
            "karma",
            "last_choice",
            "met_eileen",
            "money",
            "motto",
            "mystery.clue_count",
            "player_name",
            "position",
            "stats",
            "steps",
            "visited",
            "_last_say_who",
            "_window",
            "mouse_visible",
            "nvl_list",
            "save_name",
        ]
    );
    let money = value_row(&rows, "money").field.as_ref().unwrap();
    assert_eq!(money.value, "120");
    assert_eq!(money.path, "/0/store.money");
    assert!(money.editable);
    let player = value_row(&rows, "player_name").field.as_ref().unwrap();
    assert!(!player.editable);
    // The second name of a shared value points at its first place.
    let companion = value_row(&rows, "companion_name").field.as_ref().unwrap();
    assert_eq!(companion.path, "/0/store.player_name");
    assert!(!companion.editable);
    let backpack = value_row(&rows, "backpack");
    assert_eq!(backpack.path, "/0/store.inventory");
    assert_eq!(backpack.children, 3);
    assert_eq!(value_row(&rows, "stats").children, 2);
    assert_eq!(
        value_row(&rows, "last_choice").note.as_ref().unwrap().kind,
        NoteKind::Empty
    );
    assert!(
        !value_row(&rows, "huge_number")
            .field
            .as_ref()
            .unwrap()
            .editable
    );
}

#[test]
fn renpy_edits_survive_a_save() {
    for relative in [RENPY_SAVE, RENPY_7] {
        let env = env();
        let path = copy_in(&env, relative);
        let mut session = open(&env, &path, None).unwrap();
        session.edit_value("/0/store.money", "5000").unwrap();
        session
            .edit_value("/0/store.hero_name", "Ada \u{e7}a\u{11f}")
            .unwrap();
        session.edit_value("/0/store.stats/luck", "0.5").unwrap();
        let flag = if relative == RENPY_7 {
            "/0/store.met_cat"
        } else {
            "/0/store.met_eileen"
        };
        session.edit_value(flag, "false").unwrap();
        session.save(path.to_str().unwrap(), false, None).unwrap();

        let reopened = open(&env, &path, None).unwrap();
        assert_eq!(reopened.info().guard, GuardLevel::Exact, "{relative}");
        let rows = value_rows(&reopened);
        assert_eq!(
            value_row(&rows, "money").field.as_ref().unwrap().value,
            "5000"
        );
        assert_eq!(
            value_row(&rows, "hero_name").field.as_ref().unwrap().value,
            "Ada \u{e7}a\u{11f}"
        );
        let luck = reopened.field_info("/0/store.stats/luck").unwrap();
        assert_eq!(luck.value, "0.5");
        assert_eq!(reopened.field_info(flag).unwrap().value, "false");
    }
}

#[test]
fn a_signed_renpy_save_tells_the_player_to_trust_it_and_is_written_unsigned() {
    let env = env();
    let path = copy_in(&env, RENPY_SIGNED);
    let mut session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert_eq!(info.notice.as_deref(), Some(NOTICE_SIGNED_SAVE));
    assert!(info.warning.unwrap().contains("Answer Yes"));
    assert!(info.editable);
    session.edit_value("/0/store.money", "777").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().notice, None);
    assert_eq!(reopened.field_info("/0/store.money").unwrap().value, "777");
}

#[test]
fn a_signed_renpy_persistent_is_read_only() {
    let env = env();
    let path = copy_in(&env, RENPY_PERSISTENT_SIGNED);
    let mut session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert_eq!(info.engine.as_deref(), Some("renpy"));
    assert_eq!(info.notice.as_deref(), Some(NOTICE_SIGNED_PERSISTENT));
    assert!(!info.editable);
    let error = session.edit_value("/play_count", "9").unwrap_err();
    assert_eq!(error.code, app_codes::READ_ONLY_DOCUMENT);
    assert!(error.message.contains("persistent"));
    let error = session
        .save(path.to_str().unwrap(), false, None)
        .unwrap_err();
    assert_eq!(error.code, app_codes::READ_ONLY_DOCUMENT);
    assert!(!value_rows(&session)[0].field.as_ref().unwrap().editable);
}

#[test]
fn an_unsigned_renpy_persistent_lists_and_saves_its_fields() {
    let env = env();
    let path = copy_in(&env, RENPY_PERSISTENT);
    let mut session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert_eq!(info.engine.as_deref(), Some("renpy"));
    assert_eq!(info.notice, None);
    assert!(info.editable);
    let names: Vec<String> = value_rows(&session)
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(
        names,
        [
            "best_time",
            "ending_good",
            "play_count",
            "player_title",
            "_changed",
            "_preferences",
            "_seen_ever",
        ]
    );
    session.edit_value("/play_count", "9").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.field_info("/play_count").unwrap().value, "9");
}

// A synthetic Ren'Py key pair (SEC1 private key and SPKI public key), the two fields
// `savetoken.create_token` writes to `security_keys.txt`. Not a real player's key.
const RENPY_KEY_TEXT: &str = "signing-key \
MHcCAQEEIHYeYqUSRQdnOtTkdbq1IB+USR19c3ph/EYnOlYi7s7joAoGCCqGSM49AwEHoUQDQgAEXtIZeHL7RF/vKerE96+NB+YwoEB477I5QSJ6rmvVzAm/QQZFT3WcZJFoS3BUTGH6JkbbRgH5+uWxD51/+Gch7g== \
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEXtIZeHL7RF/vKerE96+NB+YwoEB477I5QSJ6rmvVzAm/QQZFT3WcZJFoS3BUTGH6JkbbRgH5+uWxD51/+Gch7g==\n";

fn key_file(env: &Env) -> PathBuf {
    let path = env.work.join("security_keys.txt");
    fs::write(&path, RENPY_KEY_TEXT).unwrap();
    path
}

fn open_with_key(env: &Env, path: &Path, key: &Path) -> Result<Session, ApiError> {
    Session::open_with_keys(
        path.to_str().unwrap(),
        None,
        Some(env.backups.clone()),
        RenpyKeySource::At(Some(key.to_path_buf())),
    )
}

#[test]
fn a_signed_renpy_save_is_signed_with_the_players_key() {
    let env = env();
    let key = key_file(&env);
    let path = copy_in(&env, RENPY_SIGNED);
    let mut session = open_with_key(&env, &path, &key).unwrap();
    let info = session.info();
    assert_eq!(info.notice.as_deref(), Some(NOTICE_KEY_SIGNED));
    assert!(info.warning.unwrap().contains("your own Ren'Py key"));
    assert!(info.editable);
    session.edit_value("/0/store.money", "888").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();

    let reopened = open_with_key(&env, &path, &key).unwrap();
    assert_eq!(reopened.info().notice.as_deref(), Some(NOTICE_KEY_SIGNED));
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    assert_eq!(reopened.field_info("/0/store.money").unwrap().value, "888");
    // Without a key the same saved file is still a signed save: the game would ask.
    let plain = open(&env, &path, None).unwrap();
    assert_eq!(plain.info().notice.as_deref(), Some(NOTICE_SIGNED_SAVE));
}

#[test]
fn an_old_renpy_save_without_a_signatures_entry_stays_unsigned() {
    let env = env();
    let key = key_file(&env);
    let path = copy_in(&env, RENPY_SAVE);
    let mut session = open_with_key(&env, &path, &key).unwrap();
    assert_eq!(session.info().notice, None);
    session.edit_value("/0/store.money", "321").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    // Ren'Py before 7.6 does not check signatures, so there is no entry to sign.
    let reopened = open_with_key(&env, &path, &key).unwrap();
    assert_eq!(reopened.info().notice, None);
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    assert_eq!(reopened.field_info("/0/store.money").unwrap().value, "321");
}

#[test]
fn a_signed_renpy_persistent_with_the_players_key_is_editable_and_signed() {
    let env = env();
    let key = key_file(&env);
    let path = copy_in(&env, RENPY_PERSISTENT_SIGNED);
    let mut session = open_with_key(&env, &path, &key).unwrap();
    let info = session.info();
    assert_eq!(info.notice.as_deref(), Some(NOTICE_KEY_SIGNED_PERSISTENT));
    assert!(info.warning.unwrap().contains("persistent data"));
    assert!(info.editable);
    session.edit_value("/play_count", "9").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();

    // The signature after the zlib stream verifies the uncompressed data, as
    // `renpy.persistent.load` checks it.
    let written = fs::read(&path).unwrap();
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let (data, params) = ZlibLayer.decode(&written, &mut ctx).unwrap();
    let trailing = params.bytes("trailing").unwrap();
    let signatures = std::str::from_utf8(trailing).unwrap();
    assert!(!signatures.is_empty());
    assert!(sign::verify_data(&data, signatures));

    let reopened = open_with_key(&env, &path, &key).unwrap();
    assert_eq!(
        reopened.info().notice.as_deref(),
        Some(NOTICE_KEY_SIGNED_PERSISTENT)
    );
    assert_eq!(reopened.field_info("/play_count").unwrap().value, "9");
}

/// A key source that counts how often it is asked, and the counter.
fn recording(key: Option<&Path>) -> (RenpyKeySource, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let source = RenpyKeySource::Recording {
        calls: Arc::clone(&calls),
        path: key.map(Path::to_path_buf),
    };
    (source, calls)
}

fn asked(calls: &Arc<AtomicUsize>) -> usize {
    calls.load(Ordering::SeqCst)
}

fn open_recorded(env: &Env, path: &Path, source: &RenpyKeySource) -> Result<Session, ApiError> {
    Session::open_with_keys(
        path.to_str().unwrap(),
        None,
        Some(env.backups.clone()),
        source.clone(),
    )
}

/// The `trailing` bytes after the zlib stream of a `persistent` file.
fn persistent_signature(bytes: &[u8]) -> Vec<u8> {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let (_, params) = ZlibLayer.decode(bytes, &mut ctx).unwrap();
    params
        .opt_bytes("trailing")
        .unwrap()
        .unwrap_or_default()
        .to_vec()
}

#[test]
fn only_a_signed_renpy_file_asks_for_the_key_file() {
    let env = env();
    let key = key_file(&env);
    let (source, calls) = recording(Some(&key));
    // Other formats, and Ren'Py files that carry no signature, never touch the key source.
    for relative in [
        "synthetic/json/compact.json",
        "synthetic/rpgmaker-mv/file1.rpgsave",
        "synthetic/es3/settings.es3",
        RENPY_SAVE,
        RENPY_7,
        RENPY_PERSISTENT,
    ] {
        let path = copy_in(&env, relative);
        open_recorded(&env, &path, &source).unwrap();
        assert_eq!(asked(&calls), 0, "{relative}");
    }
    // So does a pasted code.
    let code = fixture_text("synthetic/json/compact.json");
    Session::open_code_with_source(&code, Some(env.backups.clone()), source.clone()).unwrap();
    assert_eq!(asked(&calls), 0);
    // A signed Ren'Py save or persistent file asks, once.
    for (index, relative) in [RENPY_SIGNED, RENPY_PERSISTENT_SIGNED]
        .into_iter()
        .enumerate()
    {
        let path = copy_in(&env, relative);
        let session = open_recorded(&env, &path, &source).unwrap();
        assert_eq!(asked(&calls), index + 1, "{relative}");
        assert!(session.renpy_key.is_some(), "{relative}");
    }
}

#[test]
fn a_restored_backup_keeps_the_key_source_of_the_session() {
    let env = env();
    let key = key_file(&env);
    let (source, calls) = recording(Some(&key));
    let path = copy_in(&env, RENPY_PERSISTENT_SIGNED);
    let mut session = open_recorded(&env, &path, &source).unwrap();
    session.edit_value("/play_count", "9").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let backup = session.backups().unwrap().remove(0);
    let restored = session.restore_backup(&backup.name, None).unwrap();
    // The restored file went through the session's own source, not the real one.
    assert_eq!(asked(&calls), 2);
    assert_eq!(
        restored.info().notice.as_deref(),
        Some(NOTICE_KEY_SIGNED_PERSISTENT)
    );
}

#[test]
fn a_signed_persistent_nobody_changed_keeps_its_signature_bytes() {
    let env = env();
    let key = key_file(&env);
    let path = copy_in(&env, RENPY_PERSISTENT_SIGNED);
    let original = fs::read(&path).unwrap();
    let target = path.to_str().unwrap();
    let mut session = open_with_key(&env, &path, &key).unwrap();

    // Saved with no edit: the signature text the file had stays as it was (the deflate bytes of this
    // fixture are not reproduced exactly, so the whole file is not compared).
    let signature = persistent_signature(&original);
    assert!(!signature.is_empty());
    session.save(target, false, None).unwrap();
    assert_eq!(persistent_signature(&fs::read(&path).unwrap()), signature);
    // An edit that is taken back leaves the data as it was.
    let before = session.field_info("/play_count").unwrap().value;
    session.edit_value("/play_count", "9").unwrap();
    session.edit_value("/play_count", &before).unwrap();
    session.save(target, false, None).unwrap();
    assert_eq!(persistent_signature(&fs::read(&path).unwrap()), signature);

    // A real edit is signed with the player's key.
    session.edit_value("/play_count", "9").unwrap();
    session.save(target, false, None).unwrap();
    let edited = fs::read(&path).unwrap();
    let edited_signature = persistent_signature(&edited);
    assert!(!edited_signature.is_empty());
    assert_ne!(edited_signature, signature);
    // Saving again with no edit keeps what the file holds now, not the signature it was opened with.
    session.save(target, false, None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), edited);
}

#[test]
fn an_unsigned_renpy_persistent_is_not_signed_even_with_the_players_key() {
    let env = env();
    let key = key_file(&env);
    let path = copy_in(&env, RENPY_PERSISTENT);
    let mut session = open_with_key(&env, &path, &key).unwrap();
    assert_eq!(session.info().notice, None);
    session.edit_value("/play_count", "9").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    assert!(persistent_signature(&fs::read(&path).unwrap()).is_empty());

    // Even a session that holds the key for it (it does not look one up) writes it unsigned.
    let mut forced = open(&env, &path, None).unwrap();
    forced.renpy_key = Some(key);
    forced.edit_value("/play_count", "10").unwrap();
    forced.save(path.to_str().unwrap(), false, None).unwrap();
    assert!(persistent_signature(&fs::read(&path).unwrap()).is_empty());
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().notice, None);
    assert_eq!(reopened.field_info("/play_count").unwrap().value, "10");
}

#[test]
fn the_key_text_never_enters_the_recipe_of_a_write() {
    let env = env();
    let key = key_file(&env);
    let mut session = open_with_key(&env, &copy_in(&env, RENPY_SIGNED), &key).unwrap();
    session.edit_value("/0/store.money", "5").unwrap();
    let recipe = session.renpy_recipe(&session.recipe).unwrap();
    let printed = format!("{recipe:?}");
    // The private half of the key is the start of the SEC1 DER; none of it may show up.
    assert!(printed.contains("signature"));
    for secret in [
        "MHcCAQEEIHYeYqUSRQdn",
        "HYeYqUSRQdnOtTkdbq1IB",
        "signing-key",
    ] {
        assert!(!printed.contains(secret), "{secret}");
    }
}

#[test]
fn a_key_file_over_64_kib_is_treated_as_no_key() {
    let env = env();
    let big = env.work.join("big_security_keys.txt");
    fs::write(&big, format!("{RENPY_KEY_TEXT}{}", "#".repeat(64 * 1024))).unwrap();
    let path = copy_in(&env, RENPY_PERSISTENT_SIGNED);
    let session = open_with_key(&env, &path, &big).unwrap();
    assert_eq!(
        session.info().notice.as_deref(),
        Some(NOTICE_SIGNED_PERSISTENT)
    );
    assert!(!session.info().editable);
    let path = copy_in(&env, RENPY_SIGNED);
    let session = open_with_key(&env, &path, &big).unwrap();
    assert_eq!(session.info().notice.as_deref(), Some(NOTICE_SIGNED_SAVE));
    assert!(session.renpy_key.is_none());
}

// Naninovel save slots.
const NANINOVEL_SAVE: &str = "synthetic/naninovel/GameSave001.json";
const NANINOVEL_GZIP: &str = "synthetic/naninovel/GameSave001-gzip.nson";
const NANINOVEL_DEFLATE: &str = "synthetic/naninovel/GameSave001-deflate.nson";

#[test]
fn read_only_top_level_rows_show_a_preview() {
    let env = env();
    let path = copy_in(&env, "synthetic/sol/amf3.sol");
    let session = open(&env, &path, None).unwrap();
    let rows = value_rows(&session);
    let plain: Vec<&ValueRow> = rows
        .iter()
        .filter(|row| row.field.is_none() && row.children == 0 && row.note.is_none())
        .collect();
    assert!(!plain.is_empty());
    assert!(plain.iter().all(|row| !row.preview.is_empty()), "{plain:?}");
}

#[test]
fn naninovel_lists_custom_variables_first() {
    let env = env();
    let path = copy_in(&env, NANINOVEL_SAVE);
    let session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert_eq!(info.engine.as_deref(), Some("naninovel"));
    assert_eq!(info.guard, GuardLevel::Exact);
    assert!(info.editable);
    assert_eq!(info.recipe, vec!["format.naninovel"]);

    let rows = value_rows(&session);
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "coins",
            "g_cleared",
            "has_key",
            "hero_name",
            "objectJsonMap",
            "playbackSpot",
            "rollbackStackJson",
            "thumbnailBase64",
        ]
    );
    let coins = value_row(&rows, "coins").field.as_ref().unwrap();
    assert_eq!(coins.value, "50");
    assert_eq!(
        coins.path,
        "/objectJsonMap/values/0/LocalVariables/0/value/numericValue"
    );
    assert!(coins.editable);
    let name = value_row(&rows, "hero_name").field.as_ref().unwrap();
    assert_eq!(name.value, "Aster");
    assert_eq!(name.kind, FieldKind::String);
    let cleared = value_row(&rows, "g_cleared").field.as_ref().unwrap();
    assert_eq!(cleared.kind, FieldKind::Bool);
    assert_eq!(value_row(&rows, "objectJsonMap").children, 2);
}

#[test]
fn naninovel_gzip_and_deflate_slots_open() {
    for (relative, recipe) in [
        (NANINOVEL_GZIP, vec!["compression.gzip", "format.naninovel"]),
        (
            NANINOVEL_DEFLATE,
            vec!["compression.deflate", "format.naninovel"],
        ),
    ] {
        let env = env();
        let path = copy_in(&env, relative);
        let session = open(&env, &path, None).unwrap();
        let info = session.info();
        assert_eq!(info.engine.as_deref(), Some("naninovel"), "{relative}");
        assert_eq!(info.recipe, recipe, "{relative}");
        assert!(info.editable, "{relative}");
        let rows = value_rows(&session);
        let coins = value_row(&rows, "coins").field.as_ref().unwrap();
        assert_eq!(coins.value, "50", "{relative}");
    }
}

// Adding and removing values, and picking the game's data folder by hand.

/// Copies every file of a fixture folder into `target`.
fn copy_folder(source: &str, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(fixture(source)).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
    }
}

/// Writes the smallest MV/MZ database (only what the names and the catalog need).
fn write_mv_data(folder: &Path) {
    fs::create_dir_all(folder).unwrap();
    fs::write(
        folder.join("Items.json"),
        r#"[null,{"id":1,"name":"Potion"},{"id":3,"name":"Hi-Potion"},{"id":7,"name":"Elixir"}]"#,
    )
    .unwrap();
    fs::write(
        folder.join("System.json"),
        r#"{"variables":["","Steps"],"switches":["","Door"]}"#,
    )
    .unwrap();
}

fn rpgmaker_view(view: GameView) -> RpgMakerView {
    match view {
        GameView::Rpgmaker { view } => *view,
        _ => panic!("not an RPG Maker view"),
    }
}

#[test]
fn naninovel_edits_survive_a_save() {
    let env = env();
    let path = copy_in(&env, NANINOVEL_SAVE);
    let mut session = open(&env, &path, None).unwrap();
    session
        .edit_value(
            "/objectJsonMap/values/0/LocalVariables/0/value/numericValue",
            "900",
        )
        .unwrap();
    session
        .edit_value(
            "/objectJsonMap/values/0/LocalVariables/1/value/stringValue",
            "Nova",
        )
        .unwrap();
    session
        .edit_value(
            "/objectJsonMap/values/0/LocalVariables/2/value/booleanValue",
            "false",
        )
        .unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();

    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    let rows = value_rows(&reopened);
    assert_eq!(
        value_row(&rows, "coins").field.as_ref().unwrap().value,
        "900"
    );
    assert_eq!(
        value_row(&rows, "hero_name").field.as_ref().unwrap().value,
        "Nova"
    );
    assert_eq!(
        value_row(&rows, "has_key").field.as_ref().unwrap().value,
        "false"
    );
    // The untouched service state keeps its original text.
    assert_eq!(
        reopened
            .field_info("/objectJsonMap/values/1/capacity")
            .unwrap()
            .value,
        "20"
    );
}

#[test]
fn a_data_folder_can_be_picked_for_mv_by_the_game_or_data_folder() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mv/file1.rpgsave");
    let game = env.work.join("picked-game");
    let data = game.join("data");
    write_mv_data(&data);
    let mut session = open(&env, &path, None).unwrap();
    assert!(rpg_view(&session).unwrap().names_folder.is_none());

    // The game folder: its `data` folder is found.
    let view = rpgmaker_view(session.set_data_folder(game.to_str().unwrap()).unwrap());
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));
    assert_eq!(view.items[0].name.as_deref(), Some("Potion"));
    assert!(view.can_add.items);
    assert_eq!(view.catalog.items.len(), 3);

    // The data folder itself is accepted too.
    let view = rpgmaker_view(session.set_data_folder(data.to_str().unwrap()).unwrap());
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));
    assert_eq!(view.items[0].name.as_deref(), Some("Potion"));
    assert_eq!(
        rpg_view(&session).unwrap().items[0].name.as_deref(),
        Some("Potion")
    );
}

#[test]
fn a_data_folder_can_be_picked_for_vx_ace_by_the_game_or_data_folder() {
    let env = env();
    let mut session = open_rgss(&env, VXACE_SAVE);
    assert!(rpg_view(&session).unwrap().names_folder.is_none());
    let game = env.work.join("picked-vxace");
    let data = game.join("Data");
    copy_folder("synthetic/rpgmaker-vxace/Data", &data);

    let view = rpgmaker_view(session.set_data_folder(game.to_str().unwrap()).unwrap());
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));
    assert_eq!(view.items[0].name.as_deref(), Some("Healing Herb"));
    assert!(view.can_add.items);
    assert!(!view.catalog.items.is_empty());

    let view = rpgmaker_view(session.set_data_folder(data.to_str().unwrap()).unwrap());
    assert_eq!(view.names_folder, Some(data.to_string_lossy().into_owned()));
}

#[test]
fn a_wrong_data_folder_says_what_to_pick() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mv/file1.rpgsave");
    let mut session = open(&env, &path, None).unwrap();
    let empty = env.work.join("empty");
    fs::create_dir_all(&empty).unwrap();

    let error = session
        .set_data_folder(empty.to_str().unwrap())
        .unwrap_err();
    assert_eq!(error.code, app_codes::DATA_FOLDER);
    assert!(error.message.contains("no game data files"));
    assert!(error.hint.contains("Items.json"));

    let file = env.work.join("not-a-folder");
    fs::write(&file, b"x").unwrap();
    let error = session.set_data_folder(file.to_str().unwrap()).unwrap_err();
    assert_eq!(error.code, app_codes::DATA_FOLDER);
    assert!(error.message.contains("is not a folder"));

    // A file without RPG Maker sections has nothing to name.
    let mut json = open_json(&env);
    let error = json.set_data_folder(empty.to_str().unwrap()).unwrap_err();
    assert_eq!(error.code, app_codes::NO_GAME_SECTIONS);
}

#[test]
fn an_mv_item_can_be_added_undone_and_saved() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mv/file1.rpgsave");
    let mut session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    assert!(view.can_add.items);
    assert_eq!(ids(&view.items), [1, 2, 5]);

    let result = session.add_inventory("items", 3, "2").unwrap();
    assert!(result.structural);
    assert_eq!(result.pending, 1);
    let view = rpg_view(&session).unwrap();
    assert_eq!(ids(&view.items), [1, 2, 3, 5]);
    let added = view.items.iter().find(|entry| entry.id == 3).unwrap();
    assert_eq!(added.field.value, "2");
    assert!(added.field.added);
    assert!(added.removable);
    let change = session
        .pending_changes()
        .into_iter()
        .find(|change| change.path == added.field.path)
        .unwrap();
    assert_eq!(change.kind, ChangeKind::Added);

    // An id that is there already is refused.
    let error = session.add_inventory("items", 3, "1").unwrap_err();
    assert_eq!(error.code, app_codes::ENTRY_EXISTS);

    let result = session.undo().unwrap();
    assert!(result.structural);
    assert_eq!(result.pending, 0);
    assert_eq!(ids(&rpg_view(&session).unwrap().items), [1, 2, 5]);

    assert!(session.redo().unwrap().structural);
    assert_eq!(ids(&rpg_view(&session).unwrap().items), [1, 2, 3, 5]);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    let view = rpg_view(&reopened).unwrap();
    assert_eq!(ids(&view.items), [1, 2, 3, 5]);
    assert_eq!(
        view.items
            .iter()
            .find(|entry| entry.id == 3)
            .unwrap()
            .field
            .value,
        "2"
    );
}

#[test]
fn an_xp_item_can_be_added_undone_and_saved() {
    let env = env();
    let path = copy_in(&env, XP_SAVE);
    let mut session = open_rgss(&env, XP_SAVE);
    let view = rpg_view(&session).unwrap();
    assert!(view.can_add.items);
    assert_eq!(ids(&view.items), [1, 4]);

    let result = session.add_inventory("items", 2, "5").unwrap();
    assert!(result.structural);
    let view = rpg_view(&session).unwrap();
    assert_eq!(ids(&view.items), [1, 2, 4]);
    let added = view.items.iter().find(|entry| entry.id == 2).unwrap();
    assert_eq!(added.field.value, "5");
    assert!(added.field.added);
    // An XP hash gives back only the last entry; existing ones stay set to 0 instead.
    assert!(!added.removable);

    assert!(session.undo().unwrap().structural);
    assert_eq!(ids(&rpg_view(&session).unwrap().items), [1, 4]);
    assert!(session.redo().unwrap().structural);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    let view = rpg_view(&reopened).unwrap();
    assert_eq!(ids(&view.items), [1, 2, 4]);
    assert_eq!(
        view.items
            .iter()
            .find(|entry| entry.id == 2)
            .unwrap()
            .field
            .value,
        "5"
    );
}

#[test]
fn a_vx_ace_item_can_be_added_undone_and_saved() {
    let env = env();
    let path = copy_in(&env, VXACE_SAVE);
    let mut session = open_rgss(&env, VXACE_SAVE);
    let view = rpg_view(&session).unwrap();
    assert!(view.can_add.items);
    assert_eq!(ids(&view.items), [1, 2]);

    assert!(session.add_inventory("items", 3, "4").unwrap().structural);
    let view = rpg_view(&session).unwrap();
    assert_eq!(ids(&view.items), [1, 2, 3]);
    assert_eq!(
        view.items
            .iter()
            .find(|entry| entry.id == 3)
            .unwrap()
            .field
            .value,
        "4"
    );

    assert!(session.undo().unwrap().structural);
    assert_eq!(ids(&rpg_view(&session).unwrap().items), [1, 2]);
    assert!(session.redo().unwrap().structural);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    assert_eq!(ids(&rpg_view(&reopened).unwrap().items), [1, 2, 3]);
}

fn values_view(session: &Session) -> (Vec<ValueRow>, Option<String>) {
    match session.game_view().unwrap() {
        GameView::Values { rows, add_parent } => (rows, add_parent),
        _ => panic!("not a Values list"),
    }
}

#[test]
fn a_json_value_can_be_added_removed_and_undone() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();
    let (_, parent) = values_view(&session);
    assert_eq!(parent.as_deref(), Some(""));
    let alive = |session: &Session| {
        session
            .get_children("/stats", 0, 500)
            .unwrap()
            .items
            .iter()
            .any(|row| row.path == "/stats/alive")
    };
    assert!(alive(&session));

    assert!(
        session
            .add_value("", "nickname", FieldKind::String, "Kai")
            .unwrap()
            .structural
    );
    let (rows, _) = values_view(&session);
    let nickname = rows.iter().find(|row| row.name == "nickname").unwrap();
    let field = nickname.field.as_ref().unwrap();
    assert_eq!((field.value.as_str(), field.added), ("Kai", true));
    assert_eq!(session.pending_changes()[0].kind, ChangeKind::Added);

    // Undo of an added value takes it out; redo puts it back.
    assert!(session.undo().unwrap().structural);
    assert!(
        !values_view(&session)
            .0
            .iter()
            .any(|row| row.name == "nickname")
    );
    assert!(session.redo().unwrap().structural);

    // An added value can be removed; nothing stays pending then.
    let result = session.remove_value("/nickname").unwrap();
    assert!(result.structural);
    assert_eq!(result.pending, 0);
    assert!(
        !values_view(&session)
            .0
            .iter()
            .any(|row| row.name == "nickname")
    );

    // A value of the file is removed and comes back with undo.
    session
        .add_value("", "nickname", FieldKind::String, "Kai")
        .unwrap();
    let result = session.remove_value("/stats/alive").unwrap();
    assert!(result.structural);
    assert_eq!(result.pending, 2); // the new nickname and the removed value
    assert!(!alive(&session));
    let removed = session
        .pending_changes()
        .into_iter()
        .find(|change| change.path == "/stats/alive")
        .unwrap();
    assert_eq!(removed.kind, ChangeKind::Removed);

    assert!(session.undo().unwrap().structural);
    assert!(alive(&session));

    session.remove_value("/stats/alive").unwrap();
    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    let (rows, _) = values_view(&reopened);
    assert!(rows.iter().any(|row| row.name == "nickname"));
    assert!(!alive(&reopened));
}

#[test]
fn a_value_can_be_added_to_a_json_object() {
    let env = env();
    let mut session = open_json(&env);
    values_view(&session);
    session
        .add_value("/stats", "score", FieldKind::Int, "5")
        .unwrap();
    let page = session.get_children("/stats", 0, 500).unwrap();
    let score = page
        .items
        .iter()
        .find(|row| row.path == "/stats/score")
        .unwrap();
    assert!(!score.addable);
    let field = session.field_info("/stats/score").unwrap();
    assert_eq!((field.value.as_str(), field.added), ("5", true));

    // A name that is there already is refused.
    let error = session
        .add_value("/stats", "hp", FieldKind::Int, "1")
        .unwrap_err();
    assert_eq!(error.code, app_codes::ENTRY_EXISTS);
}

#[test]
fn an_es3_value_can_be_added_and_saved() {
    let env = env();
    let path = copy_in(&env, "synthetic/es3/progress.es3");
    let mut session = open(&env, &path, None).unwrap();
    let (_, parent) = values_view(&session);
    assert_eq!(parent.as_deref(), Some(""));

    assert!(
        session
            .add_value("", "newScore", FieldKind::Int, "7")
            .unwrap()
            .structural
    );
    let (rows, _) = values_view(&session);
    let row = value_row(&rows, "newScore");
    let field = row.field.as_ref().unwrap();
    assert_eq!(field.value, "7");
    assert_eq!(field.type_name.as_deref(), Some("Int32"));
    assert!(field.added);
    assert!(row.removable);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    let (rows, _) = values_view(&reopened);
    assert_eq!(
        value_row(&rows, "newScore").field.as_ref().unwrap().value,
        "7"
    );
}

#[test]
fn a_sugarcube_variable_can_be_added_and_saved() {
    let env = env();
    let path = copy_in(&env, "synthetic/sugarcube/one-moment.save");
    let mut session = open(&env, &path, None).unwrap();
    let view = story_view(&session).unwrap();
    let parent = view.add_parent.clone().unwrap();

    assert!(
        session
            .add_value(&parent, "newFlag", FieldKind::Bool, "true")
            .unwrap()
            .structural
    );
    let view = story_view(&session).unwrap();
    assert_eq!(story_value(&view, "newFlag"), "true");
    let row = story_row(&view, "newFlag");
    assert!(row.removable);
    assert!(row.field.as_ref().unwrap().added);

    assert!(session.undo().unwrap().structural);
    assert!(
        !story_view(&session)
            .unwrap()
            .variables
            .iter()
            .any(|row| row.name == "newFlag")
    );
    assert!(session.redo().unwrap().structural);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    assert_eq!(
        story_value(&story_view(&reopened).unwrap(), "newFlag"),
        "true"
    );
}

#[test]
fn an_mv_item_can_be_removed_undone_and_saved() {
    let env = env();
    let path = copy_in(&env, "synthetic/rpgmaker-mv/file1.rpgsave");
    let mut session = open(&env, &path, None).unwrap();
    let view = rpg_view(&session).unwrap();
    let potion = view.items.iter().find(|entry| entry.id == 1).unwrap();
    assert!(potion.removable);
    let item_path = potion.field.path.clone();

    let result = session.remove_value(&item_path).unwrap();
    assert!(result.structural);
    assert_eq!(result.pending, 1);
    assert_eq!(ids(&rpg_view(&session).unwrap().items), [2, 5]);
    let change = session.pending_changes().into_iter().next().unwrap();
    assert_eq!(change.kind, ChangeKind::Removed);

    assert!(session.undo().unwrap().structural);
    assert_eq!(ids(&rpg_view(&session).unwrap().items), [1, 2, 5]);
    assert!(session.redo().unwrap().structural);

    session.save(path.to_str().unwrap(), false, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    assert_eq!(reopened.info().guard, GuardLevel::Exact);
    assert_eq!(ids(&rpg_view(&reopened).unwrap().items), [2, 5]);
}

const TERRARIA_PLAYER: &str = "synthetic/terraria/player-decrypted.plr";
const TERRARIA_WORLD: &str = "synthetic/terraria/world.wld";

/// Replaces the first occurrence of the little-endian `old` numbers with the `new` ones.
fn patch_i32s(bytes: &mut [u8], old: &[i32], new: &[i32]) {
    let find: Vec<u8> = old.iter().flat_map(|value| value.to_le_bytes()).collect();
    let put: Vec<u8> = new.iter().flat_map(|value| value.to_le_bytes()).collect();
    let at = bytes
        .windows(find.len())
        .position(|window| window == find)
        .unwrap();
    bytes[at..at + put.len()].copy_from_slice(&put);
}

#[test]
fn a_terraria_file_with_values_no_real_file_has_opens_read_only_with_a_reason() {
    let env = env();
    for relative in [TERRARIA_PLAYER, TERRARIA_WORLD] {
        let path = copy_in(&env, relative);
        let info = open(&env, &path, None).unwrap().info();
        assert!(info.editable, "{relative}");
        assert_eq!(info.notice, None, "{relative}");
    }

    // Life, max life, mana, max mana are 140, 140, 60, 60; a max life of 5000 is no real value.
    let path = copy_in(&env, TERRARIA_PLAYER);
    let mut bytes = fs::read(&path).unwrap();
    patch_i32s(&mut bytes, &[140, 140, 60, 60], &[140, 5000, 60, 60]);
    fs::write(&path, &bytes).unwrap();
    let mut session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert!(!info.editable);
    assert_eq!(info.notice.as_deref(), Some("terraria-layout"));
    assert!(info.warning.unwrap().contains("not recognized"));
    let error = session.edit_value("/life", "100").unwrap_err();
    assert_eq!(error.code, app_codes::READ_ONLY_DOCUMENT);
    let error = session
        .save(path.to_str().unwrap(), false, None)
        .unwrap_err();
    assert_eq!(error.code, app_codes::READ_ONLY_DOCUMENT);

    // Height 1200 and width 8400: a world with no width is not one.
    let path = copy_in(&env, TERRARIA_WORLD);
    let mut bytes = fs::read(&path).unwrap();
    patch_i32s(&mut bytes, &[1200, 8400], &[1200, 0]);
    fs::write(&path, &bytes).unwrap();
    let session = open(&env, &path, None).unwrap();
    let info = session.info();
    assert!(!info.editable);
    assert_eq!(info.notice.as_deref(), Some("terraria-layout"));
}

#[test]
fn an_encrypted_player_that_looks_like_another_format_still_opens_as_a_player() {
    // The ciphertext made with this key reads as XOR-obfuscated Ruby Marshal to the detector (a
    // weak guess); the Terraria name and shape must win over it.
    const KEY: &str = "SynthKy8";
    let env = env();
    let plain = fs::read(fixture(TERRARIA_PLAYER)).unwrap();
    let mut params = Params::new();
    params.set("password", KEY);
    let encrypted = saveprism_codecs::terraria::aes::TerrariaPlayerLayer
        .encode(&plain, &params)
        .unwrap();
    for name in ["Mira.plr", "Mira.plr.bak"] {
        let path = env.work.join(name);
        fs::write(&path, &encrypted).unwrap();
        let session = open(&env, &path, Some(KEY)).unwrap();
        let info = session.info();
        assert_eq!(info.codec, "format.terraria-player", "{name}");
        assert_eq!(info.guard, GuardLevel::Exact, "{name}");
        assert!(info.editable, "{name}");
        // Without the key it asks for one instead of showing the detector's guess.
        let error = open(&env, &path, None).err().unwrap();
        assert_eq!(error.code, "SP-TERRARIA-006", "{name}");
    }
}

// The Witcher 3 Gwent collection.

const WITCHER3_SAVE: &str = "synthetic/witcher3/ManualSave_invented.sav";

fn witcher3_view(session: &Session) -> Witcher3View {
    match session.game_view().unwrap() {
        GameView::Witcher3 { view } => *view,
        other => panic!("a Witcher 3 save must open with the Gwent view, got {other:?}"),
    }
}

#[test]
fn witcher3_saves_list_the_gwent_cards_and_decks() {
    let env = self::env();
    let path = copy_in(&env, WITCHER3_SAVE);
    let session = open(&env, &path, None).unwrap();
    let view = witcher3_view(&session);
    assert!(
        !view.rows.is_empty(),
        "the Values list stays below the collection"
    );
    let gwent = view.gwent.expect("the fixture has a Gwent manager");
    let cards: Vec<(u32, bool, &str)> = gwent
        .cards
        .iter()
        .map(|card| (card.index, card.leader, card.copies.value.as_str()))
        .collect();
    assert_eq!(
        cards,
        vec![(3, false, "4"), (4, false, "2"), (1001, true, "1")]
    );
    for card in &gwent.cards {
        assert!(card.copies.editable, "card {} copies", card.index);
        assert_eq!(card.copies.min, Some(0));
        assert_eq!(card.copies.max, Some(i64::from(i32::MAX)));
    }
    assert_eq!(gwent.decks.len(), 2);
    assert_eq!(gwent.decks[0].leader, Some(4001));
    assert_eq!(gwent.decks[1].leader, Some(2001));
    assert!(gwent.decks[0].cards.is_empty());
    assert_eq!(gwent.decks[1].cards, vec![3, 4]);
    assert_eq!(gwent.decks[1].card_count, 2);
}

#[test]
fn a_gwent_copy_count_of_a_repeated_record_edits_and_saves() {
    let env = self::env();
    let path = copy_in(&env, WITCHER3_SAVE);
    let mut session = open(&env, &path, None).unwrap();
    let second = witcher3_view(&session).gwent.unwrap().cards[1]
        .copies
        .clone();
    assert!(
        second.path.ends_with("SBCollectionCard#2/numCopies"),
        "{}",
        second.path
    );

    let error = session.edit_value(&second.path, "-1").unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    session.edit_value(&second.path, "3").unwrap();
    let after = witcher3_view(&session).gwent.unwrap();
    assert_eq!(after.cards[1].copies.value, "3");
    assert_eq!(after.cards[1].copies.original, "2");
    assert_eq!(after.cards[0].copies.value, "4", "the first record stays");

    session.save(path.to_str().unwrap(), true, None).unwrap();
    let reopened = open(&env, &path, None).unwrap();
    let cards = witcher3_view(&reopened).gwent.unwrap().cards;
    assert_eq!(cards[1].copies.value, "3");
    assert_eq!(cards[0].copies.value, "4");
}

#[test]
fn a_float_shows_its_number_not_a_codec_marker() {
    let float = |lexeme: Option<&str>| {
        let mut node = saveprism_core::Node::new(Value::Float {
            v: 12.5,
            width: saveprism_core::FloatWidth::F64,
        });
        node.lexeme = lexeme.map(Box::from);
        value_text(&node).unwrap()
    };
    assert_eq!(float(Some("sii:original")), "12.5");
    assert_eq!(float(Some("12.50")), "12.50");
    assert_eq!(float(None), "12.5");
}

#[test]
fn a_witcher3_float_kept_as_original_bytes_shows_its_number() {
    let env = self::env();
    let path = copy_in(&env, WITCHER3_SAVE);
    let session = open(&env, &path, None).unwrap();
    let weight = session.field_info("/Variables/W3Player/weight").unwrap();
    assert_eq!(weight.kind, FieldKind::Float);
    assert_eq!(weight.value, "71.5");
    assert!(weight.editable);
}

#[test]
fn litedb_fields_kept_by_an_index_say_so() {
    let env = self::env();
    let path = copy_in(&env, "synthetic/litedb/game.db");
    let session = open(&env, &path, None).unwrap();
    for (field, reason, editable) in [
        ("/player/0/_id", Some(ReadOnlyReason::Index), false),
        ("/player/0/gold", Some(ReadOnlyReason::Index), false),
        ("/player/0/level", None, true),
        // Text is read-only for its length, not for an index.
        ("/player/0/name", None, false),
    ] {
        let info = session.field_info(field).unwrap();
        assert_eq!(info.read_only_reason, reason, "{field}");
        assert_eq!(info.editable, editable, "{field}");
    }
}

// The Satisfactory inventories.

const SATISFACTORY_SAVE: &str = "synthetic/satisfactory/invented.sav";

fn satisfactory_view(session: &Session) -> SatisfactoryView {
    match session.game_view().unwrap() {
        GameView::Satisfactory { view } => *view,
        other => panic!("a Satisfactory save must open with its view, got {other:?}"),
    }
}

#[test]
fn satisfactory_saves_list_player_and_storage_inventories() {
    let env = self::env();
    let path = copy_in(&env, SATISFACTORY_SAVE);
    let session = open(&env, &path, None).unwrap();
    let view = satisfactory_view(&session);
    assert!(!view.rows.is_empty(), "the Values list stays below");
    assert_eq!(view.inventories.len(), 2);
    let player = &view.inventories[0];
    assert_eq!(player.kind, InventoryKind::Player);
    assert_eq!(player.label, "Player inventory");
    let stacks: Vec<(&str, &str)> = player
        .stacks
        .iter()
        .map(|stack| (stack.name.as_str(), stack.count.value.as_str()))
        .collect();
    assert_eq!(
        stacks,
        vec![("Invented Plate", "40"), ("Invented Wire", "7")]
    );
    for stack in &player.stacks {
        assert!(stack.count.path.ends_with("/NumItems"));
        assert!(stack.count.editable);
        assert_eq!(stack.count.min, Some(0));
        assert_eq!(stack.count.max, Some(500));
        assert!(stack.count.hint.contains("500 is the largest stack size"));
    }
    let storage = &view.inventories[1];
    assert_eq!(storage.kind, InventoryKind::Storage);
    assert_eq!(storage.label, "Storage Container Mk1");
    assert_eq!(storage.stacks.len(), 1);
    assert_eq!(storage.stacks[0].name, "Invented Ore");
    assert_eq!(storage.stacks[0].count.value, "120");
}

#[test]
fn a_satisfactory_stack_count_edits_and_undoes() {
    let env = self::env();
    let path = copy_in(&env, SATISFACTORY_SAVE);
    let mut session = open(&env, &path, None).unwrap();
    let count = satisfactory_view(&session).inventories[0].stacks[0]
        .count
        .clone();

    let error = session.edit_value(&count.path, "501").unwrap_err();
    assert_eq!(error.code, app_codes::OUT_OF_RANGE);
    session.edit_value(&count.path, "55").unwrap();
    let edited = satisfactory_view(&session).inventories[0].stacks[0]
        .count
        .clone();
    assert_eq!(edited.value, "55");
    assert_eq!(edited.original, "40");
    assert_eq!(session.pending_changes().len(), 1);

    session.undo().unwrap();
    let undone = satisfactory_view(&session).inventories[0].stacks[0]
        .count
        .clone();
    assert_eq!(undone.value, "40");
    assert!(session.pending_changes().is_empty());
}

fn collect_real_saves(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_real_saves(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sav"))
        {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "reads the local, git-ignored fixtures/external/satisfactory saves"]
fn real_satisfactory_saves_list_a_player_inventory_and_edit_one_count() {
    let root = fixture("external/satisfactory");
    let mut saves = Vec::new();
    collect_real_saves(&root, &mut saves);
    if saves.is_empty() {
        eprintln!("no real Satisfactory saves under {}", root.display());
        return;
    }

    for (index, source) in saves.iter().enumerate() {
        let env = self::env();
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("save.sav");
        let path = env.work.join(format!("{index}-{name}"));
        fs::copy(source, &path).unwrap();

        let mut session = open(&env, &path, None).unwrap();
        let started = std::time::Instant::now();
        let view = satisfactory_view(&session);
        let build = started.elapsed();
        let player = view
            .inventories
            .iter()
            .find(|inventory| inventory.kind == InventoryKind::Player)
            .unwrap_or_else(|| panic!("{} has no player inventory", source.display()));
        assert!(
            !player.stacks.is_empty(),
            "{} has an empty player inventory",
            source.display()
        );
        assert!(player.stacks.iter().all(|stack| !stack.name.is_empty()));
        let stack = player.stacks[0].count.clone();
        let old: i64 = stack.value.parse().unwrap();
        let new = if old < 500 { old + 1 } else { old - 1 };
        assert_ne!(old, new, "{} has no room to edit a count", source.display());

        session.edit_value(&stack.path, &new.to_string()).unwrap();
        let pending = session.pending_changes();
        assert_eq!(pending.len(), 1, "{}", source.display());
        assert_eq!(pending[0].path, stack.path);

        session.save(path.to_str().unwrap(), true, None).unwrap();
        let reopened = open(&env, &path, None).unwrap();
        let again = satisfactory_view(&reopened);
        let count = again
            .inventories
            .iter()
            .flat_map(|inventory| &inventory.stacks)
            .find(|candidate| candidate.count.path == stack.path)
            .unwrap_or_else(|| panic!("{} lost the edited stack", source.display()));
        assert_eq!(count.count.value, new.to_string());
        eprintln!(
            "{}: {} inventories ({} player, {} storage, {} other), {} stacks, view built in {:?}",
            source.display(),
            again.inventories.len(),
            again
                .inventories
                .iter()
                .filter(|inventory| inventory.kind == InventoryKind::Player)
                .count(),
            again
                .inventories
                .iter()
                .filter(|inventory| inventory.kind == InventoryKind::Storage)
                .count(),
            again
                .inventories
                .iter()
                .filter(|inventory| inventory.kind == InventoryKind::Other)
                .count(),
            again
                .inventories
                .iter()
                .map(|inventory| inventory.stacks.len())
                .sum::<usize>(),
            build
        );
    }
}
