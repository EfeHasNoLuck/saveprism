use proptest::prelude::*;
use saveprism_core::edit::build_doc;
use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId,
    OwnedValue, ParamValue, Params, Path, Registry, SaveDoc, SniffCtx, Value, apply_edits, codes,
};

use super::*;

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    JsonCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    JsonCodec.encode(&decoded.doc, &decoded.params).unwrap()
}

fn regular(decoded: &Decoded) -> bool {
    decoded.params.bool("regular").unwrap()
}

fn assert_identical(input: &[u8]) {
    let decoded = decode(input);
    assert_eq!(
        String::from_utf8_lossy(&encode(&decoded)),
        String::from_utf8_lossy(input)
    );
    assert!(regular(&decoded), "not regular: {:?}", decoded.params);
}

fn node_at<'a>(decoded: &'a Decoded, path: &str) -> &'a Node {
    let id = decoded.doc.resolve(&Path::parse(path).unwrap()).unwrap();
    decoded.doc.get(id).unwrap()
}

fn hint_at<'a>(decoded: &'a Decoded, path: &str) -> Option<&'a str> {
    node_at(decoded, path)
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str())
}

/// The sample tree in pretty form: `unit` per level, `nl` as line ending.
fn pretty(unit: &str, nl: &str) -> String {
    let i1 = unit;
    let i2 = unit.repeat(2);
    format!(
        "{{{nl}{i1}\"a\": 1,{nl}{i1}\"b\": [{nl}{i2}1,{nl}{i2}2.5,{nl}{i2}\"x\"{nl}{i1}],{nl}{i1}\"c\": {{{nl}{i2}\"d\": null{nl}{i1}}},{nl}{i1}\"e\": [],{nl}{i1}\"f\": {{}}{nl}}}"
    )
}

#[test]
fn compact_javascript_output_is_identical() {
    assert_identical(br#"{"a":1,"b":[1,2.5,"x"],"c":{"d":null}}"#);
    assert_identical(br#"[true,false,null,-3,"y"]"#);
}

#[test]
fn python_default_output_is_identical() {
    let input = br#"{"a": 1, "b": [1, 2]}"#;
    assert_identical(input);
    let decoded = decode(input);
    assert_eq!(decoded.params.str("item_sep").unwrap(), ", ");
    assert_eq!(decoded.params.str("key_sep").unwrap(), ": ");
}

#[test]
fn indented_output_is_identical() {
    assert_identical(pretty("  ", "\n").as_bytes());
    assert_identical(pretty("    ", "\n").as_bytes());
    assert_identical(pretty("\t", "\n").as_bytes());
    assert_identical(pretty("  ", "\r\n").as_bytes());
    assert_identical(format!("{}\n", pretty("  ", "\n")).as_bytes());
    assert_identical(format!("{}\r\n", pretty("\t", "\r\n")).as_bytes());
}

#[test]
fn profile_parameters_are_inferred() {
    let two = decode(pretty("  ", "\n").as_bytes());
    assert_eq!(two.params.get("indent"), Some(&ParamValue::Int(2)));
    assert_eq!(two.params.str("newline").unwrap(), "lf");
    let tab = decode(format!("{}\r\n", pretty("\t", "\r\n")).as_bytes());
    assert_eq!(tab.params.str("indent").unwrap(), "tab");
    assert_eq!(tab.params.str("newline").unwrap(), "crlf");
    assert!(tab.params.bool("trailing_newline").unwrap());
    let compact = decode(br#"{"a":1}"#);
    assert_eq!(compact.params.str("indent").unwrap(), "none");
    assert!(!compact.params.bool("bom").unwrap());
}

#[test]
fn bom_trailing_newline_and_empty_containers() {
    assert_identical("\u{feff}{\"a\":1}\n".as_bytes());
    assert_identical("\u{feff}[1,2]".as_bytes());
    assert_identical(b"[]");
    assert_identical(b"{}");
    assert_identical(br#"{"a":[],"b":{}}"#);
    assert_identical(b"[[],{}]\n");
    assert_identical(br#"{"a": [ ], "b": { }}"#);
    assert!(decode("\u{feff}[]".as_bytes()).params.bool("bom").unwrap());
}

#[test]
fn repeated_keys_text_and_scalar_roots_are_identical() {
    assert_identical(br#"{"a":1,"a":2,"b":3,"a":4}"#);
    assert_identical("{\"ad\":\"Şişli çığ ğüöı İ\",\"e\":\"😀 emoji\"}".as_bytes());
    assert_identical(b"123");
    assert_identical(b"\"text\"\n");
    assert_identical(b"null");
    assert_identical(b"true");
    let decoded = decode(br#"{"a":1,"a":2}"#);
    let Value::Object(entries) = &decoded.doc.get(decoded.doc.root()).unwrap().value else {
        panic!("not an object");
    };
    assert_eq!(entries.len(), 2);
}

#[test]
fn number_and_string_lexemes_come_back_unchanged() {
    let digits = "1234567890123456789012345678901234567890";
    let accent_escape = "\\u00e9";
    let text = format!(r#"[1.0,1e3,1E+2,-0,{digits},"{accent_escape}","\/","\u001F","a\tb"]"#);
    assert_identical(text.as_bytes());

    let decoded = decode(text.as_bytes());
    assert!(matches!(
        node_at(&decoded, "/0").value,
        Value::Float { v, .. } if v == 1.0
    ));
    assert_eq!(node_at(&decoded, "/1").lexeme.as_deref(), Some("1e3"));
    assert_eq!(node_at(&decoded, "/2").lexeme.as_deref(), Some("1E+2"));
    let minus_zero = node_at(&decoded, "/3");
    assert!(matches!(minus_zero.value, Value::Int { v: 0, .. }));
    assert_eq!(minus_zero.lexeme.as_deref(), Some("-0"));
    let big = node_at(&decoded, "/4");
    assert!(matches!(big.value, Value::Float { .. }));
    assert_eq!(big.lexeme.as_deref(), Some(digits));
    let accent = node_at(&decoded, "/5");
    assert_eq!(accent.value, Value::Str("é".into()));
    assert_eq!(
        accent.lexeme.as_deref(),
        Some(format!("\"{accent_escape}\"").as_str())
    );
    assert_eq!(node_at(&decoded, "/6").value, Value::Str("/".into()));
    assert_eq!(node_at(&decoded, "/6").lexeme.as_deref(), Some(r#""\/""#));
    // A canonical escape needs no lexeme.
    assert!(node_at(&decoded, "/8").lexeme.is_none());
}

#[test]
fn integers_and_floats_get_their_hints() {
    let decoded = decode(b"[7,2.5,1e2]");
    assert_eq!(hint_at(&decoded, "/0"), Some("JSON:int"));
    assert_eq!(hint_at(&decoded, "/1"), Some("JSON:float"));
    assert_eq!(hint_at(&decoded, "/2"), Some("JSON:float"));
    assert!(matches!(
        node_at(&decoded, "/0").value,
        Value::Int {
            v: 7,
            width: IntWidth::Unbounded
        }
    ));
    assert!(matches!(
        node_at(&decoded, "/1").value,
        Value::Float {
            width: FloatWidth::F64,
            ..
        }
    ));
}

#[test]
fn unpaired_surrogates_are_kept_read_only() {
    let input = "[\"\\ud83d\",\"😀\"]".as_bytes();
    assert_identical(input);
    let decoded = decode(input);
    let lone = node_at(&decoded, "/0");
    assert_eq!(lone.value, Value::Str("\u{FFFD}".into()));
    assert_eq!(lone.lexeme.as_deref(), Some(r#""\ud83d""#));
    assert!(lone.flags.contains(NodeFlags::READ_ONLY));
    let pair = node_at(&decoded, "/1");
    assert_eq!(pair.value, Value::Str("😀".into()));
    assert!(!pair.flags.contains(NodeFlags::READ_ONLY));
}

#[test]
fn spans_cover_the_value_bytes() {
    let decoded = decode("\u{feff}[10,\"ab\"]".as_bytes());
    let span = node_at(&decoded, "/1").span.unwrap();
    assert_eq!((span.start, span.end), (7, 11));
    let root = node_at(&decoded, "").span.unwrap();
    assert_eq!((root.start, root.end), (3, 12));
}

#[test]
fn setting_a_float_keeps_the_decimal_point() {
    let mut decoded = decode(br#"{"a":1.0,"b":"x\/y","c":3}"#);
    let edits = [Edit::Set {
        path: Path::parse("/a").unwrap(),
        value: OwnedValue::Int(2),
    }];
    apply_edits(&mut decoded.doc, &edits, &Limits::default()).unwrap();
    assert_eq!(
        String::from_utf8(encode(&decoded)).unwrap(),
        r#"{"a":2.0,"b":"x\/y","c":3}"#
    );

    let edits = [Edit::Set {
        path: Path::parse("/b").unwrap(),
        value: OwnedValue::Str("é\n\"".into()),
    }];
    apply_edits(&mut decoded.doc, &edits, &Limits::default()).unwrap();
    assert_eq!(
        String::from_utf8(encode(&decoded)).unwrap(),
        "{\"a\":2.0,\"b\":\"é\\n\\\"\",\"c\":3}"
    );
}

#[test]
fn irregular_whitespace_is_semantically_equal() {
    let input = br#"{ "a" :1,"b":  2}"#;
    let decoded = decode(input);
    assert!(!regular(&decoded));
    let written = encode(&decoded);
    assert_ne!(written.as_slice(), &input[..]);
    let again = decode(&written);
    assert!(again.doc.tree_eq(&decoded.doc));

    // A key with a non-canonical escape is normalised the same way.
    let escaped = decode(br#"{"a\/b":1}"#);
    assert!(!regular(&escaped));
    assert!(decode(&encode(&escaped)).doc.tree_eq(&escaped.doc));
}

#[test]
fn very_deep_nesting_does_not_overflow_the_stack() {
    let depth = 10_000;
    let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let limits = Limits {
        max_depth: 20_000,
        ..Limits::default()
    };
    let decoded = decode_with(text.as_bytes(), &limits).unwrap();
    assert!(regular(&decoded));
    assert_eq!(encode(&decoded), text.as_bytes());
}

#[test]
fn depth_limit_is_enforced() {
    let limits = Limits {
        max_depth: 3,
        ..Limits::default()
    };
    assert!(decode_with(b"[[[1]]]", &limits).is_ok());
    let error = decode_with(b"[[[[1]]]]", &limits).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
    let objects = decode_with(br#"{"a":{"a":{"a":{"a":1}}}}"#, &limits).unwrap_err();
    assert_eq!(objects.code(), codes::CORE_LIMIT_DEPTH);
    let default_limit = "[".repeat(600) + &"]".repeat(600);
    assert_eq!(
        decode_with(default_limit.as_bytes(), &Limits::default())
            .unwrap_err()
            .code(),
        codes::CORE_LIMIT_DEPTH
    );
}

#[test]
fn node_limit_is_enforced() {
    let limits = Limits {
        max_nodes: 4,
        ..Limits::default()
    };
    let error = decode_with(b"[1,2,3,4,5]", &limits).unwrap_err();
    assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
}

#[test]
fn syntax_errors_carry_code_and_offset() {
    let cases: [(&[u8], u64); 8] = [
        (br#"{"a":[1,2"#, 9),
        (b"[1,]", 3),
        (b"[01]", 2),
        (b"", 0),
        (br#"{"a" 1}"#, 5),
        (b"[\"a\nb\"]", 3),
        (b"[tru]", 1),
        (b"[1.]", 3),
    ];
    for (input, offset) in cases {
        let error = decode_with(input, &Limits::default()).unwrap_err();
        assert_eq!(
            error.code(),
            JSON_SYNTAX,
            "{:?}",
            String::from_utf8_lossy(input)
        );
        assert_eq!(
            error.offset(),
            Some(offset),
            "{:?}",
            String::from_utf8_lossy(input)
        );
    }
}

#[test]
fn trailing_data_is_reported() {
    let error = decode_with(br#"{"a":1} x"#, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), JSON_TRAILING);
    assert_eq!(error.offset(), Some(8));
    let two = decode_with(b"1 2", &Limits::default()).unwrap_err();
    assert_eq!(two.code(), JSON_TRAILING);
    assert_eq!(two.offset(), Some(2));
}

#[test]
fn invalid_utf8_is_reported_with_the_byte_offset() {
    let error = decode_with(&[b'[', b'"', 0xFF, b'"', b']'], &Limits::default()).unwrap_err();
    assert_eq!(error.code(), JSON_UTF8);
    assert_eq!(error.offset(), Some(2));
    let with_bom = decode_with(
        &[0xEF, 0xBB, 0xBF, b'[', b'"', 0xFF, b'"', b']'],
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(with_bom.code(), JSON_UTF8);
    assert_eq!(with_bom.offset(), Some(5));
}

#[test]
fn syntax_error_offsets_include_the_bom() {
    let error = decode_with("\u{feff}[1,]".as_bytes(), &Limits::default()).unwrap_err();
    assert_eq!(error.code(), JSON_SYNTAX);
    assert_eq!(error.offset(), Some(6));
}

#[test]
fn a_key_with_an_unpaired_surrogate_is_refused() {
    let error = decode_with(br#"{"\ud800":1}"#, &Limits::default()).unwrap_err();
    assert_eq!(error.code(), JSON_LONE_SURROGATE);
}

#[test]
fn unwritable_values_are_refused() {
    let limits = Limits::default();
    let write = |value: &OwnedValue| {
        let doc = build_doc(value, &limits).unwrap();
        JsonCodec.encode(&doc, &Params::new())
    };
    for value in [
        OwnedValue::Bytes(vec![1, 2]),
        OwnedValue::Float(f64::NAN),
        OwnedValue::Float(f64::INFINITY),
        OwnedValue::Array(vec![OwnedValue::Float(f64::NEG_INFINITY)]),
    ] {
        assert_eq!(write(&value).unwrap_err().code(), JSON_UNWRITABLE);
    }

    let mut doc = SaveDoc::new();
    let key = doc.push(Node::new(Value::Str("k".into()))).unwrap();
    let item = doc.push(Node::new(Value::Null)).unwrap();
    let map = doc.push(Node::new(Value::Map(vec![(key, item)]))).unwrap();
    doc.set_root(map);
    assert_eq!(
        JsonCodec.encode(&doc, &Params::new()).unwrap_err().code(),
        JSON_UNWRITABLE
    );

    let mut doc = SaveDoc::new();
    let target = doc.push(Node::new(Value::Null)).unwrap();
    let reference = doc.push(Node::new(Value::Ref(target))).unwrap();
    doc.set_root(reference);
    assert_eq!(
        JsonCodec.encode(&doc, &Params::new()).unwrap_err().code(),
        JSON_UNWRITABLE
    );
}

#[test]
fn a_containment_cycle_is_refused() {
    let mut doc = SaveDoc::new();
    let id = doc.reserve().unwrap();
    doc.finish(id, Node::new(Value::Array(vec![id]))).unwrap();
    doc.set_root(id);
    assert_eq!(
        JsonCodec.encode(&doc, &Params::new()).unwrap_err().code(),
        JSON_UNWRITABLE
    );
}

#[test]
fn float_text_follows_javascript() {
    use super::write::float_text;
    assert_eq!(float_text(2.5, false).as_deref(), Some("2.5"));
    assert_eq!(float_text(2.0, false).as_deref(), Some("2"));
    assert_eq!(float_text(2.0, true).as_deref(), Some("2.0"));
    assert_eq!(float_text(0.0, false).as_deref(), Some("0"));
    assert_eq!(float_text(-0.0, true).as_deref(), Some("-0.0"));
    assert_eq!(float_text(1e21, true).as_deref(), Some("1e+21"));
    assert_eq!(float_text(1.5e-7, true).as_deref(), Some("1.5e-7"));
    assert_eq!(float_text(1e-7, true).as_deref(), Some("1e-7"));
    assert_eq!(float_text(1e-6, true).as_deref(), Some("0.000001"));
    assert_eq!(
        float_text(1e20, false).as_deref(),
        Some("100000000000000000000")
    );
    assert_eq!(float_text(-1.5e300, true).as_deref(), Some("-1.5e+300"));
    assert_eq!(float_text(f64::NAN, true), None);
    assert_eq!(float_text(f64::INFINITY, true), None);
}

#[test]
fn unsupported_profile_parameters_are_refused() {
    let doc = SaveDoc::new();
    let mut params = Params::new();
    params.set("indent", "wide");
    assert_eq!(
        JsonCodec.encode(&doc, &params).unwrap_err().code(),
        codes::CORE_INVALID_PARAM
    );
    let mut params = Params::new();
    params.set("newline", "cr");
    assert_eq!(
        JsonCodec.encode(&doc, &params).unwrap_err().code(),
        codes::CORE_INVALID_PARAM
    );
    // An empty parameter table means the compact default.
    assert_eq!(JsonCodec.encode(&doc, &Params::new()).unwrap(), b"null");
}

#[test]
fn sniff_prefers_json_looking_text() {
    let ctx = SniffCtx::default();
    assert!(JsonCodec.sniff(br#"  {"a":1}"#, &ctx) > JsonCodec.sniff(b"{ truncated", &ctx));
    assert!(JsonCodec.sniff(b"[1]", &ctx).value() >= 0.7);
    assert_eq!(JsonCodec.sniff(b"hello", &ctx).value(), 0.0);
    assert_eq!(JsonCodec.sniff(b"", &ctx).value(), 0.0);
    let named = SniffCtx {
        file_name: Some("Save.JSON"),
        depth: 0,
    };
    assert!(JsonCodec.sniff(b"[1]", &named) > JsonCodec.sniff(b"[1]", &ctx));
    assert!(JsonCodec.sniff(b"hello", &named).value() > 0.0);
}

#[test]
fn register_adds_format_json() {
    let mut reg = Registry::default();
    register(&mut reg);
    assert!(reg.is_codec("format.json"));
    register(&mut reg);
    assert_eq!(
        reg.codec("format.json").map(|c| c.id()),
        Some("format.json")
    );
}

fn arb_value() -> impl Strategy<Value = OwnedValue> {
    let leaf = prop_oneof![
        Just(OwnedValue::Null),
        any::<bool>().prop_map(OwnedValue::Bool),
        any::<i64>().prop_map(|v| OwnedValue::Int(i128::from(v))),
        any::<i128>().prop_map(OwnedValue::Int),
        any::<f64>()
            .prop_filter("finite", |v| v.is_finite())
            .prop_map(OwnedValue::Float),
        any::<String>().prop_map(OwnedValue::Str),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(OwnedValue::Array),
            prop::collection::vec((any::<String>(), inner), 0..6).prop_map(OwnedValue::Object),
        ]
    })
}

/// True when the decoded node means the same as `expected`. A whole-number float reads back as
/// an integer (JSON has no separate float type).
fn means(doc: &SaveDoc, id: NodeId, expected: &OwnedValue) -> bool {
    let Some(node) = doc.get(id) else {
        return false;
    };
    match (&node.value, expected) {
        (Value::Null, OwnedValue::Null) => true,
        (Value::Bool(a), OwnedValue::Bool(b)) => a == b,
        (Value::Int { v: a, .. }, OwnedValue::Int(b)) => a == b,
        (Value::Int { v: a, .. }, OwnedValue::Float(b)) => *a as f64 == *b,
        (Value::Float { v: a, .. }, OwnedValue::Float(b)) => a == b,
        (Value::Str(a), OwnedValue::Str(b)) => a == b,
        (Value::Array(ids), OwnedValue::Array(items)) => {
            ids.len() == items.len()
                && ids
                    .iter()
                    .zip(items)
                    .all(|(id, item)| means(doc, *id, item))
        }
        (Value::Object(entries), OwnedValue::Object(items)) => {
            entries.len() == items.len()
                && entries
                    .iter()
                    .zip(items)
                    .all(|((key, id), (name, item))| key == name && means(doc, *id, item))
        }
        _ => false,
    }
}

proptest! {
    #[test]
    fn built_documents_survive_encode_and_decode(value in arb_value()) {
        let limits = Limits::default();
        let doc = build_doc(&value, &limits).unwrap();
        let bytes = JsonCodec.encode(&doc, &Params::new()).unwrap();

        let first = decode(&bytes);
        prop_assert!(regular(&first));
        prop_assert!(means(&first.doc, first.doc.root(), &value));

        // Decoding adds `JSON:*` hints that `build_doc` does not set, so the decoded tree is
        // compared with a second round: same bytes, equal trees.
        let again = encode(&first);
        prop_assert_eq!(&bytes, &again);
        let second = decode(&again);
        prop_assert!(first.doc.tree_eq(&second.doc));
    }
}
