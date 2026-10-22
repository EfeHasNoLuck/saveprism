//! Command line tests: `run(...)` in process on files in a temporary directory.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use saveprism_cli::run;
use saveprism_core::{Limits, Recipe, Registry, Step, pipeline};

const JSON: &str = r#"{"name":"hero","level":7,"items":["sword","shield"],"gold":120}"#;

fn registry() -> Registry {
    let mut reg = Registry::default();
    saveprism_codecs::register_all(&mut reg);
    reg
}

/// The file bytes of `JSON` wrapped in `layers` (outermost first), made with the pipeline.
fn make_input(layers: &[&str]) -> Vec<u8> {
    let reg = registry();
    let plain = pipeline::decode(
        JSON.as_bytes(),
        &Recipe::new(vec![Step::bare("format.json")]),
        &reg,
        &Limits::default(),
    )
    .unwrap();
    let codec_params = plain.recipe.steps.last().unwrap().params.clone();
    let mut steps: Vec<Step> = layers.iter().map(|id| Step::bare(*id)).collect();
    steps.push(Step::new("format.json", codec_params));
    pipeline::encode(&plain.doc, &Recipe::new(steps), &reg).unwrap()
}

/// Runs the tool; returns (exit code, stdout, stderr).
fn cli(args: &[&OsString]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let all = std::iter::once(OsString::from("saveprism")).chain(args.iter().map(|a| (*a).clone()));
    let code = run(all, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

fn os(text: &str) -> OsString {
    OsString::from(text)
}

fn path_arg(path: &Path) -> OsString {
    path.as_os_str().to_owned()
}

fn count_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() { count_files(&path) } else { 1 }
        })
        .sum()
}

/// decode then encode: the new file must equal the original byte for byte.
fn assert_byte_identical_round_trip(layers: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let original = make_input(layers);
    let save = dir.path().join("save.dat");
    fs::write(&save, &original).unwrap();

    let json_out = dir.path().join("save.json");
    let (code, _, stderr) = cli(&[
        &os("decode"),
        &path_arg(&save),
        &os("-o"),
        &path_arg(&json_out),
        &os("--backup-dir"),
        &path_arg(&backups),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(fs::read(&json_out).unwrap(), JSON.as_bytes());
    let recipe_path: PathBuf = dir.path().join("save.recipe.toml");
    assert!(recipe_path.exists());

    let rebuilt = dir.path().join("rebuilt.dat");
    let (code, _, stderr) = cli(&[
        &os("encode"),
        &path_arg(&json_out),
        &os("--recipe"),
        &path_arg(&recipe_path),
        &os("-o"),
        &path_arg(&rebuilt),
        &os("--backup-dir"),
        &path_arg(&backups),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(fs::read(&rebuilt).unwrap(), original);
}

#[test]
fn base64_json_round_trips_byte_for_byte() {
    assert_byte_identical_round_trip(&["encoding.base64"]);
}

#[test]
fn lz_string_json_round_trips_byte_for_byte() {
    assert_byte_identical_round_trip(&["compression.lz-string"]);
}

#[test]
fn encoding_over_an_existing_file_makes_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let original = make_input(&["encoding.base64"]);
    let save = dir.path().join("save.dat");
    fs::write(&save, &original).unwrap();

    let json_out = dir.path().join("save.json");
    let recipe_path = dir.path().join("save.recipe.toml");
    let (code, _, stderr) = cli(&[
        &os("decode"),
        &path_arg(&save),
        &os("-o"),
        &path_arg(&json_out),
        &os("--backup-dir"),
        &path_arg(&backups),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let before = count_files(&backups);

    // Edit the level, then write over the original save.
    let edited = JSON.replace("\"level\":7", "\"level\":9");
    fs::write(&json_out, &edited).unwrap();
    let (code, _, stderr) = cli(&[
        &os("encode"),
        &path_arg(&json_out),
        &os("--recipe"),
        &path_arg(&recipe_path),
        &os("-o"),
        &path_arg(&save),
        &os("--backup-dir"),
        &path_arg(&backups),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(count_files(&backups) > before, "no backup was made");
    assert_ne!(fs::read(&save).unwrap(), original);
}

#[test]
fn inspect_json_is_valid_json_with_the_expected_recipe() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.dat");
    fs::write(&save, make_input(&["encoding.base64"])).unwrap();

    let (code, stdout, stderr) = cli(&[&os("inspect"), &path_arg(&save), &os("--json")]);
    assert_eq!(code, 0, "{stderr}");

    // Parse the output with the project's own JSON codec.
    let reg = registry();
    pipeline::decode(
        stdout.as_bytes(),
        &Recipe::new(vec![Step::bare("format.json")]),
        &reg,
        &Limits::default(),
    )
    .expect("inspect --json must print valid JSON");
    assert!(
        stdout.contains(r#""recipe":["encoding.base64","format.json"]"#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""overall":"exact""#), "{stdout}");

    let (code, stdout, _) = cli(&[&os("inspect"), &path_arg(&save)]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("encoding.base64 -> format.json"),
        "{stdout}"
    );
}

#[test]
fn a_broken_file_exits_with_one_and_an_sp_code() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("broken.dat");
    fs::write(&save, [0x00_u8, 0x01, 0x02, 0xFE, 0xFF, 0x80, 0x00]).unwrap();
    let (code, stdout, stderr) = cli(&[&os("inspect"), &path_arg(&save)]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stderr.contains("SP-"), "{stderr}");
}

#[test]
fn a_missing_file_exits_with_one() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.dat");
    let (code, _, stderr) = cli(&[&os("inspect"), &path_arg(&missing)]);
    assert_eq!(code, 1);
    assert!(stderr.contains("SP-IO-007"), "{stderr}");
}

#[test]
fn a_usage_error_exits_with_two() {
    let (code, _, stderr) = cli(&[&os("decode")]);
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

/// Synthetic Easy Save 3 file encrypted with the password `synthetic-test-key` (see fixtures/keyed).
fn keyed_settings() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/keyed/es3-aes/settings.es3")
}

fn write_candidates(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("candidates.txt");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn keys_try_prints_the_password_and_its_line() {
    let dir = tempfile::tempdir().unwrap();
    let list = write_candidates(dir.path(), "wrong-one\nwrong-two\nsynthetic-test-key\n");
    let (code, stdout, stderr) = cli(&[
        &os("keys"),
        &os("try"),
        &path_arg(&keyed_settings()),
        &os("--candidates"),
        &path_arg(&list),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "found: synthetic-test-key (line 3)\n");
}

#[test]
fn keys_try_without_the_password_exits_one_with_an_sp_code() {
    let dir = tempfile::tempdir().unwrap();
    let list = write_candidates(dir.path(), "wrong-one\n# not a candidate\nwrong-two\r\n");
    let (code, stdout, stderr) = cli(&[
        &os("keys"),
        &os("try"),
        &path_arg(&keyed_settings()),
        &os("--candidates"),
        &path_arg(&list),
    ]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("SP-CLI-002"), "{stderr}");
    // The candidates are never echoed.
    assert!(!stderr.contains("wrong-one"), "{stderr}");
}

#[test]
fn keys_try_with_no_candidates_exits_one_with_an_sp_code() {
    let dir = tempfile::tempdir().unwrap();
    let list = write_candidates(dir.path(), "# only a comment\n\n");
    let (code, stdout, stderr) = cli(&[
        &os("keys"),
        &os("try"),
        &path_arg(&keyed_settings()),
        &os("--candidates"),
        &path_arg(&list),
    ]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stderr.contains("SP-CLI-001"), "{stderr}");
}

#[test]
fn es3_plain_file_decodes_and_encodes_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let original_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic/es3/progress.es3");
    let original = fs::read(&original_path).unwrap();
    let save = dir.path().join("progress.es3");
    fs::write(&save, &original).unwrap();

    let json_out = dir.path().join("progress.json");
    let (code, _, stderr) = cli(&[
        &os("decode"),
        &path_arg(&save),
        &os("-o"),
        &path_arg(&json_out),
        &os("--backup-dir"),
        &path_arg(&backups),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let recipe_path = dir.path().join("progress.recipe.toml");
    assert!(recipe_path.exists());

    let rebuilt = dir.path().join("rebuilt.es3");
    let (code, _, stderr) = cli(&[
        &os("encode"),
        &path_arg(&json_out),
        &os("--recipe"),
        &path_arg(&recipe_path),
        &os("-o"),
        &path_arg(&rebuilt),
        &os("--backup-dir"),
        &path_arg(&backups),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(fs::read(&rebuilt).unwrap(), original);
}
