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
        GameView::Values { .. } | GameView::Sugarcube { .. } => None,
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
    let GameView::Values { rows } = session.game_view().unwrap() else {
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
    let GameView::Values { rows } = session.game_view().unwrap() else {
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

    session.save(path.to_str().unwrap(), false).unwrap();
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
    session.save(path.to_str().unwrap(), false).unwrap();
    session.edit_value("/level", "60").unwrap();
    session.save(path.to_str().unwrap(), false).unwrap();
    let saved = fs::read(&path).unwrap();

    let list = session.backups().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].created_ms >= list[1].created_ms);
    let oldest = list[1].clone();
    assert_eq!(oldest.byte_size, u64::try_from(first.len()).unwrap());

    // Unsaved edits are dropped by a restore.
    session.edit_value("/level", "70").unwrap();
    let restored = session.restore_backup(&oldest.name).unwrap();
    assert_eq!(fs::read(&path).unwrap(), first);
    assert_eq!(row(&restored, "/level").preview, "42");
    assert!(restored.pending_changes().is_empty());
    assert!(!restored.history_state().can_undo);

    // The file that was replaced is the newest backup now, so the restore can be taken back.
    let list = restored.backups().unwrap();
    assert_eq!(list.len(), 3);
    let back = restored.restore_backup(&list[0].name).unwrap();
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert_eq!(row(&back, "/level").preview, "60");
}

#[test]
fn restore_refuses_unknown_names_and_unreadable_backups() {
    let env = env();
    let path = copy_in(&env, "synthetic/json/compact.json");
    let mut session = open(&env, &path, None).unwrap();
    session.edit_value("/level", "50").unwrap();
    let result = session.save(path.to_str().unwrap(), false).unwrap();
    let saved = fs::read(&path).unwrap();

    for name in ["../compact.json", "source.txt", "1.bak", ""] {
        let error = session.restore_backup(name).err().unwrap();
        assert_eq!(error.code, app_codes::BACKUP_NOT_FOUND, "{name}");
    }

    // A backup that does not open as a save is never written.
    let backup = PathBuf::from(result.backup_path.unwrap());
    fs::write(&backup, b"\x00\x01 not a save").unwrap();
    let name = backup.file_name().unwrap().to_str().unwrap();
    let error = session.restore_backup(name).err().unwrap();
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
    session.save(path.to_str().unwrap(), false).unwrap();
    let view = rpg_view(&session).unwrap();
    assert!(view.switches.is_empty());
    assert_eq!(ids(&view.variables), [3, 4]);
}

// SugarCube (Twine) saves.

const STORY: &str = "synthetic/sugarcube/multi-moment.save";

fn story_view(session: &Session) -> Option<SugarCubeView> {
    match session.game_view().unwrap() {
        GameView::Sugarcube { view } => Some(*view),
        GameView::Rpgmaker { .. } | GameView::Values { .. } => None,
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

    session.save(path.to_str().unwrap(), false).unwrap();
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
    Session::open_code(text, Some(env.backups.clone()))
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
    let result = session.save(target.to_str().unwrap(), false).unwrap();
    assert!(result.backup_path.is_none());
    let info = session.info();
    assert!(!info.pasted);
    assert_eq!(PathBuf::from(&info.path), target);
    assert!(file.exists());
    assert!(file_names(&env.backups).is_empty());
    let reopened = open(&env, &target, None).unwrap();
    assert_eq!(story_value(&story_view(&reopened).unwrap(), "gold"), "1");
}
