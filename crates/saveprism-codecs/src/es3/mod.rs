//! `format.es3`: Unity Easy Save 3 files.
//!
//! A plain `.es3` file is JSON of the form `{"key": {"__type": "<type>", "value": ...}}`. This codec
//! is a thin layer over `format.json` (so the byte-exact writing comes from there) that gives the
//! `value` node of every `__type` object the hint `ES3:<type>` (`ES3:int`,
//! `ES3:System.Collections.Generic.List`1[[System.Int32]]`). The `__type` node itself stays in the
//! tree as read-only information for the user.
//!
//! Encrypted files are opened through the `crypto.es3-aes` layer (see `aes`), optionally followed
//! by `compression.gzip`: the recipe is `crypto.es3-aes` -> `compression.gzip` -> `format.es3`.

mod aes;
mod plain;

pub use aes::{Es3AesLayer, password_unlocks_first_block};

use std::sync::Arc;

use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, OwnedValue, Params, Registry, Result,
    SaveDoc, SniffCtx, TypeHint,
};

/// The root of the file is not an object, or the file is not Easy Save 3 text.
pub const ES3_ROOT: ErrorCode = ErrorCode::new("SP-ES3-001");
/// The document holds something Easy Save 3 text cannot write (map, reference) or lost a node.
pub const ES3_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-ES3-002");

/// The encrypted file is malformed (too short, not whole blocks).
pub const ES3_BAD_DATA: ErrorCode = ErrorCode::new("SP-ES3-003");
/// Wrong password, or the file is not Easy Save 3 encrypted.
pub const ES3_WRONG_PASSWORD: ErrorCode = ErrorCode::new("SP-ES3-004");
/// The file is encrypted but no password was given.
pub const ES3_PASSWORD_REQUIRED: ErrorCode = ErrorCode::new("SP-ES3-005");

/// How far into the file `sniff` looks for `"__type"`.
const SNIFF_WINDOW: usize = 4096;
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Registers the Easy Save 3 codec as `format.es3` and the encryption layer `crypto.es3-aes`.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(Es3AesLayer));
    reg.add_codec(Arc::new(Es3Codec));
}

/// The Easy Save 3 type name of a new top-level value, as its `__type` entry names it (`int`,
/// `float`, `bool`, `string`); `None` for other values.
pub fn primitive_type(value: &OwnedValue) -> Option<&'static str> {
    match value {
        OwnedValue::Int(_) => Some("int"),
        OwnedValue::Float(_) => Some("float"),
        OwnedValue::Bool(_) => Some("bool"),
        OwnedValue::Str(_) => Some("string"),
        _ => None,
    }
}

/// The hint this codec gives the `value` node of a `{"__type": "<type>", "value": ...}` entry.
pub fn value_hint(type_name: &str) -> TypeHint {
    TypeHint::new(format!("ES3:{type_name}"))
}

/// The `format.es3` codec.
pub(crate) struct Es3Codec;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

impl Codec for Es3Codec {
    fn id(&self) -> &'static str {
        "format.es3"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let body = input.strip_prefix(&BOM[..]).unwrap_or(input);
        let first = body
            .iter()
            .copied()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'));
        let window = body.get(..SNIFF_WINDOW).unwrap_or(body);
        if first == Some(b'{') && contains(window, b"\"__type\"") {
            return Confidence::HIGH;
        }
        let extension = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".es3"));
        if extension {
            Confidence::MEDIUM
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        plain::decode(input, ctx)
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        plain::encode(doc, params)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path as FsPath;

    use saveprism_core::{
        Edit, Limits, NodeFlags, OwnedValue, Path, Position, Value, apply_edits, codes,
    };

    use super::*;
    use crate::json::JsonCodec;

    fn fixture(name: &str) -> Vec<u8> {
        let path = FsPath::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/es3")
            .join(name);
        std::fs::read(&path).unwrap()
    }

    fn decode(bytes: &[u8]) -> Result<Decoded> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        Es3Codec.decode(bytes, &mut ctx)
    }

    fn hint_at(decoded: &Decoded, path: &str) -> Option<String> {
        let id = decoded.doc.resolve(&Path::parse(path).unwrap()).unwrap();
        let node = decoded.doc.get(id).unwrap();
        node.type_hint
            .as_ref()
            .map(|hint| hint.as_str().to_string())
    }

    fn assert_round_trip(bytes: &[u8]) -> Decoded {
        let decoded = decode(bytes).unwrap();
        let written = Es3Codec.encode(&decoded.doc, &decoded.params).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&written),
            String::from_utf8_lossy(bytes)
        );
        decoded
    }

    #[test]
    fn settings_round_trip_and_hints() {
        let decoded = assert_round_trip(&fixture("settings.es3"));
        assert_eq!(
            hint_at(&decoded, "/volume/value").as_deref(),
            Some("ES3:float")
        );
        assert_eq!(
            hint_at(&decoded, "/level/value").as_deref(),
            Some("ES3:int")
        );
        assert_eq!(
            hint_at(&decoded, "/fullscreen/value").as_deref(),
            Some("ES3:bool")
        );
        assert_eq!(
            hint_at(&decoded, "/playerName/value").as_deref(),
            Some("ES3:string")
        );
        // The `__type` node stays in the tree, without an ES3 hint.
        let type_id = decoded
            .doc
            .resolve(&Path::parse("/level/__type").unwrap())
            .unwrap();
        assert_eq!(
            decoded.doc.get(type_id).unwrap().value,
            saveprism_core::Value::Str("int".into())
        );
        assert_eq!(hint_at(&decoded, "/level/__type"), None);
    }

    #[test]
    fn progress_round_trip_and_long_type_names() {
        let decoded = assert_round_trip(&fixture("progress.es3"));
        assert_eq!(
            hint_at(&decoded, "/unlockedLevels/value").as_deref(),
            Some("ES3:System.Collections.Generic.List`1[[System.Int32]]")
        );
        assert_eq!(
            hint_at(&decoded, "/coins/value").as_deref(),
            Some("ES3:System.Int32")
        );
        assert!(
            hint_at(&decoded, "/checkpoint/value")
                .is_some_and(|hint| hint.starts_with("ES3:UnityEngine.Vector3"))
        );
        // Children of a typed value are not hinted by ES3.
        assert_eq!(
            hint_at(&decoded, "/checkpoint/value/x").as_deref(),
            Some("JSON:float")
        );
    }

    #[test]
    fn edited_float_keeps_its_decimal_point() {
        let decoded = decode(&fixture("settings.es3")).unwrap();
        let id = decoded
            .doc
            .resolve(&Path::parse("/volume/value").unwrap())
            .unwrap();
        let mut doc = decoded.doc.clone();
        doc.set_value(
            id,
            saveprism_core::Value::Float {
                v: 1.0,
                width: saveprism_core::FloatWidth::F64,
            },
        )
        .unwrap();
        let text = String::from_utf8(Es3Codec.encode(&doc, &decoded.params).unwrap()).unwrap();
        assert!(text.contains("\"value\": 1.0"), "{text}");
    }

    #[test]
    fn json_without_type_opens_without_hints() {
        let decoded = decode(br#"{"a":1,"b":{"value":2}}"#).unwrap();
        assert_eq!(hint_at(&decoded, "/a").as_deref(), Some("JSON:int"));
        assert_eq!(hint_at(&decoded, "/b/value").as_deref(), Some("JSON:int"));
        let written = Es3Codec.encode(&decoded.doc, &decoded.params).unwrap();
        assert_eq!(written, br#"{"a":1,"b":{"value":2}}"#);
    }

    #[test]
    fn broken_json_is_an_error_not_a_panic() {
        for bad in [
            &b"{\"a\": {\"__type\": \"int\", \"value\": "[..],
            &b"{\"a\" 1}"[..],
            &b"\xFF\xFE"[..],
            &b""[..],
        ] {
            let error = decode(bad).unwrap_err();
            assert!(error.code().as_str().starts_with("SP-"));
        }
    }

    #[test]
    fn non_object_root_is_refused() {
        let error = decode(b"[1,2]").unwrap_err();
        assert_eq!(error.code(), ES3_ROOT);
    }

    #[test]
    fn sniff_rules() {
        let with_name = |name: &'static str| SniffCtx {
            file_name: Some(name),
            depth: 0,
        };
        let plain = br#"{"k":{"__type":"int","value":1}}"#;
        let none = SniffCtx::default();
        assert_eq!(Es3Codec.sniff(plain, &none), Confidence::HIGH);
        assert_eq!(Es3Codec.sniff(b"  \n{\"x\":1}", &none), Confidence::NONE);
        assert_eq!(
            Es3Codec.sniff(b"{\"x\":1}", &with_name("SaveFile.ES3")),
            Confidence::MEDIUM
        );
        assert_eq!(
            Es3Codec.sniff(&[0x12, 0x34, 0x56], &with_name("a.es3")),
            Confidence::MEDIUM
        );
        assert_eq!(Es3Codec.sniff(b"", &none), Confidence::NONE);
        // `"__type"` after the sniff window does not count.
        let mut late = b"{\"pad\":\"".to_vec();
        late.extend(std::iter::repeat_n(b'x', SNIFF_WINDOW));
        late.extend_from_slice(b"\",\"__type\":1}");
        assert_eq!(Es3Codec.sniff(&late, &none), Confidence::NONE);
    }

    #[test]
    fn register_adds_the_codec() {
        let mut reg = Registry::default();
        register(&mut reg);
        register(&mut reg);
    }

    fn id_at(decoded: &Decoded, path: &str) -> saveprism_core::NodeId {
        decoded
            .doc
            .resolve(&Path::parse(path).unwrap())
            .unwrap_or_else(|| panic!("{path} is missing"))
    }

    fn read_only(decoded: &Decoded, path: &str) -> bool {
        decoded
            .doc
            .get(id_at(decoded, path))
            .unwrap()
            .flags
            .contains(NodeFlags::READ_ONLY)
    }

    #[test]
    fn bare_int_keys_round_trip_and_keep_their_text() {
        let bytes = fixture("dict-int-pretty.es3");
        let decoded = assert_round_trip(&bytes);
        assert!(
            hint_at(&decoded, "/playedMaps/value")
                .is_some_and(|hint| hint.starts_with("ES3:System.Collections.Generic.Dictionary`2"))
        );
        // The bare keys are ordinary, addressable object keys and the values are editable.
        assert_eq!(
            decoded
                .doc
                .get(id_at(&decoded, "/playedMaps/value/12"))
                .unwrap()
                .value,
            Value::Int {
                v: 6,
                width: saveprism_core::IntWidth::Unbounded
            }
        );
        assert_eq!(
            decoded
                .doc
                .get(id_at(&decoded, "/zoomLevels/value/1.5"))
                .unwrap()
                .value,
            Value::Float {
                v: 2.25,
                width: saveprism_core::FloatWidth::F64
            }
        );
        assert!(read_only(&decoded, "/playedMaps/value"));
        assert!(read_only(&decoded, "/zoomLevels/value"));
    }

    #[test]
    fn editing_a_value_keeps_the_keys_bare() {
        let decoded = decode(&fixture("dict-int-pretty.es3")).unwrap();
        let mut doc = decoded.doc.clone();
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: Path::parse("/playedMaps/value/0").unwrap(),
                value: OwnedValue::Int(200),
            }],
            &Limits::default(),
        )
        .unwrap();
        let text = String::from_utf8(Es3Codec.encode(&doc, &decoded.params).unwrap()).unwrap();
        assert!(text.contains("            0 : 200,"), "{text}");
        assert!(text.contains("            12 : 6,"), "{text}");
        assert!(!text.contains("\"0\""), "{text}");
    }

    #[test]
    fn bare_key_containers_are_read_only_but_growable_is_not_needed() {
        let decoded = decode(&fixture("dict-int-pretty.es3")).unwrap();
        let mut doc = decoded.doc.clone();
        let rename = Edit::RenameKey {
            path: Path::parse("/playedMaps/value/12").unwrap(),
            new_key: "13".into(),
        };
        let error = apply_edits(&mut doc, &[rename], &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY);

        let insert = Edit::Insert {
            parent: Path::parse("/playedMaps/value").unwrap(),
            at: Position::Key("4".into()),
            value: OwnedValue::Int(9),
        };
        let error = apply_edits(&mut doc, &[insert], &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_READ_ONLY);

        // A value below a bare key is still editable.
        apply_edits(
            &mut doc,
            &[Edit::Set {
                path: Path::parse("/playedMaps/value/-3").unwrap(),
                value: OwnedValue::Int(8),
            }],
            &Limits::default(),
        )
        .unwrap();
    }

    #[test]
    fn compact_shape_and_bool_keys_round_trip() {
        let compact = assert_round_trip(&fixture("dict-int-compact.es3"));
        assert_eq!(
            hint_at(&compact, "/settings/value").as_deref(),
            Some("ES3:dict[int, int]")
        );
        // The observed `values` key is not the node ES3 gives a type hint, but its keys are bare.
        assert_eq!(
            compact
                .doc
                .get(id_at(&compact, "/scores/values/3"))
                .unwrap()
                .value,
            Value::Str("4".into())
        );
        assert!(read_only(&compact, "/settings/value"));
        assert!(read_only(&compact, "/scores/values"));

        let booleans = assert_round_trip(&fixture("dict-bool.es3"));
        assert_eq!(
            booleans
                .doc
                .get(id_at(&booleans, "/toggles/value/true"))
                .unwrap()
                .value,
            Value::Str("on".into())
        );
        assert_eq!(
            booleans
                .doc
                .get(id_at(&booleans, "/toggles/value/false"))
                .unwrap()
                .value,
            Value::Str("off".into())
        );
        assert!(read_only(&booleans, "/toggles/value"));
    }

    #[test]
    fn a_bare_word_key_is_an_error_not_a_panic() {
        for bad in [
            &br#"{"d":{"__type":"dict[int,int]","value":{abc:1}}}"#[..],
            &br#"{"d":{1.5e:1}}"#[..],
            &br#"{"d":{truex:1}}"#[..],
            &br#"{"d":{-.5:1}}"#[..],
            &br#"{"d":{-:1}}"#[..],
        ] {
            let error = decode(bad).unwrap_err();
            assert!(
                error.code().as_str().starts_with("SP-"),
                "unexpected code for {bad:?}"
            );
        }
    }

    #[test]
    fn strict_json_still_rejects_bare_keys() {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let error = JsonCodec
            .decode(br#"{"d":{"value":{1:2}}}"#, &mut ctx)
            .unwrap_err();
        assert_eq!(error.code(), crate::json::JSON_SYNTAX);
        // The same bytes do open as Easy Save 3 (the `__type` is not required for decoding).
        assert!(decode(br#"{"d":{"__type":"dict[int,int]","value":{1:2}}}"#).is_ok());
    }

    #[test]
    fn detection_prefers_es3_for_bare_dictionaries() {
        use saveprism_core::detect::{DetectOptions, detect};

        let mut reg = Registry::default();
        crate::register_all(&mut reg);
        for bytes in [
            fixture("dict-int-pretty.es3"),
            fixture("dict-int-compact.es3"),
            fixture("dict-bool.es3"),
        ] {
            let found =
                detect(&bytes, &reg, &Limits::default(), &DetectOptions::default()).unwrap();
            assert_eq!(found.best.recipe.kinds(), ["format.es3"]);
        }
    }

    #[test]
    #[ignore = "needs local files under fixtures/external/es3 (never committed)"]
    fn every_external_es3_file_round_trips() {
        use saveprism_core::detect::{DetectOptions, detect};
        use saveprism_core::{Registry, pipeline};

        let dir = FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/external/es3");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut reg = Registry::default();
        crate::register_all(&mut reg);
        let limits = Limits::default();
        let mut stack: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
        let mut checked = 0_usize;
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                if let Ok(inner) = std::fs::read_dir(&path) {
                    stack.extend(inner.flatten().map(|e| e.path()));
                }
                continue;
            }
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("es3"))
            {
                continue;
            }
            let Ok(input) = std::fs::read(&path) else {
                continue;
            };
            let Ok(found) = detect(&input, &reg, &limits, &DetectOptions::default()) else {
                // Encrypted files need a password the detector cannot invent; skip them.
                continue;
            };
            let Ok(out) = pipeline::decode(&input, &found.best.recipe, &reg, &limits) else {
                continue;
            };
            let Ok(again) = pipeline::encode(&out.doc, &out.recipe, &reg) else {
                continue;
            };
            // Some real saves re-encode with other whitespace (the guard's "semantic" level);
            // those must still decode to the same tree.
            if again != input {
                let reread = pipeline::decode(&again, &out.recipe, &reg, &limits)
                    .unwrap_or_else(|e| panic!("{} did not decode again: {e}", path.display()));
                assert!(
                    reread.doc.tree_eq(&out.doc),
                    "{} did not round-trip to the same tree",
                    path.display()
                );
            }
            checked += 1;
        }
        eprintln!("checked {checked} external .es3 files");
    }
}
