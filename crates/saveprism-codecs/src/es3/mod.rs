//! `format.es3`: Unity Easy Save 3 files.
//!
//! A plain `.es3` file is JSON of the form `{"key": {"__type": "<type>", "value": ...}}`. This codec
//! is a thin layer over `format.json` (so the byte-exact writing comes from there) that gives the
//! `value` node of every `__type` object the hint `ES3:<type>` (`ES3:int`,
//! `ES3:System.Collections.Generic.List`1[[System.Int32]]`). The `__type` node itself stays in the
//! tree as read-only information for the user.
//!
//! Encrypted files (AES, optional GZip) are not handled here yet; the password layer will be added
//! next to `plain`.

mod plain;

use std::sync::Arc;

use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, ErrorCode, Params, Registry, Result, SaveDoc, SniffCtx,
};

/// The root of the file is not an object, or the file is not Easy Save 3 text.
pub const ES3_ROOT: ErrorCode = ErrorCode::new("SP-ES3-001");
/// The document holds something Easy Save 3 text cannot write (map, reference) or lost a node.
pub const ES3_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-ES3-002");

/// How far into the file `sniff` looks for `"__type"`.
const SNIFF_WINDOW: usize = 4096;
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Registers the Easy Save 3 codec as `format.es3`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(Es3Codec));
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

    use saveprism_core::{Limits, Path};

    use super::*;

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
}
