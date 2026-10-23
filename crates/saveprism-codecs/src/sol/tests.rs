use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, NodeFlags, OwnedValue, Path, Value, apply_edits,
};

use super::*;

fn header(version: u32, body: &[u8]) -> Vec<u8> {
    let mut out = vec![0, 0xbf];
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(b"TCSO\0\x04\0\0\0\0");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.push(b'x');
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(body);
    let len = u32::try_from(out.len() - 6).unwrap_or_default();
    out.get_mut(2..6)
        .expect("header length span")
        .copy_from_slice(&len.to_be_bytes());
    out
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    FlashSolCodec
        .decode(bytes, &mut ctx)
        .expect("valid synthetic LSO")
}
fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        FlashSolCodec.encode(&decoded.doc, &decoded.params).unwrap(),
        bytes
    );
    decoded
}
fn amf3_string(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = vec![u8::try_from((bytes.len() << 1) | 1).expect("short test string")];
    out.extend_from_slice(bytes);
    out
}

#[test]
fn sniff_requires_lso_magic() {
    let bytes = header(0, &[]);
    let ctx = SniffCtx {
        file_name: Some("x.sol"),
        ..SniffCtx::default()
    };
    assert!(FlashSolCodec.sniff(&bytes, &ctx).value() > 0.9);
}

#[test]
fn amf0_scalars_round_trip_and_edit() {
    let bytes = header(0, &[0, 1, b'n', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut decoded = exact(&bytes);
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/n").expect("path"),
            value: OwnedValue::Float(8.5),
        }],
        &Limits::default(),
    )
    .expect("edit");
    assert_ne!(
        FlashSolCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("encode"),
        bytes
    );
}

#[test]
fn amf3_integer_round_trip_and_range_guard() {
    let bytes = header(3, &[0x03, b'x', 0x04, 0x7f, 0]);
    let mut decoded = exact(&bytes);
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/x").expect("path"),
            value: OwnedValue::Int(42),
        }],
        &Limits::default(),
    )
    .expect("edit");
    assert_eq!(
        FlashSolCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("encode")
            .last(),
        Some(&0)
    );
    let id = decoded
        .doc
        .resolve(&Path::parse("/x").expect("path"))
        .expect("value path");
    let node = decoded.doc.get(id).expect("value node");
    assert!(!node.flags.contains(NodeFlags::READ_ONLY));
    assert!(matches!(node.value, Value::Int { .. }));
}

#[test]
fn rejects_truncated_and_unsupported_values() {
    let mut bytes = header(0, &[0]);
    assert_eq!(decode_error(&bytes).code(), SOL_PARSE);
    bytes = header(0, &[0, 1, b'x', 0x0d]);
    assert_eq!(decode_error(&bytes).code(), SOL_UNSUPPORTED);
}

#[test]
fn generated_fixtures_decode_exactly() {
    for name in ["amf0.sol", "amf3.sol"] {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/sol")
                .join(name),
        )
        .expect("fixture");
        exact(&bytes);
    }
    let amf0 = decode(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/sol/amf0.sol"),
        )
        .expect("fixture"),
    );
    let shared = amf0
        .doc
        .resolve(&Path::parse("/shared/hp").expect("path"))
        .expect("shared child");
    assert!(
        amf0.doc
            .get(shared)
            .expect("node")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    let amf3 = decode(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/sol/amf3.sol"),
        )
        .expect("fixture"),
    );
    let alias = amf3
        .doc
        .resolve(&Path::parse("/avatarAgain").expect("path"))
        .expect("object reference");
    assert!(matches!(
        amf3.doc.get(alias).expect("node").value,
        Value::Ref(_)
    ));
}

#[test]
fn fixture_scalars_edit_and_redecode() {
    let fixture = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/synthetic/sol")
                .join(name),
        )
        .expect("fixture")
    };
    let mut amf0 = exact(&fixture("amf0.sol"));
    apply_edits(
        &mut amf0.doc,
        &[
            Edit::Set {
                path: Path::parse("/score").expect("path"),
                value: OwnedValue::Float(77.25),
            },
            Edit::Set {
                path: Path::parse("/settings/music").expect("path"),
                value: OwnedValue::Bool(false),
            },
            Edit::Set {
                path: Path::parse("/title").expect("path"),
                value: OwnedValue::Str("edited title".into()),
            },
        ],
        &Limits::default(),
    )
    .expect("AMF0 edits");
    let bytes = FlashSolCodec
        .encode(&amf0.doc, &amf0.params)
        .expect("AMF0 encode");
    let after = decode(&bytes);
    assert!(
        matches!(after.doc.get(after.doc.resolve(&Path::parse("/score").expect("path")).expect("score")).expect("node").value, Value::Float { v, .. } if v == 77.25)
    );
    assert!(matches!(
        after
            .doc
            .get(
                after
                    .doc
                    .resolve(&Path::parse("/settings/music").expect("path"))
                    .expect("music")
            )
            .expect("node")
            .value,
        Value::Bool(false)
    ));
    assert!(
        matches!(after.doc.get(after.doc.resolve(&Path::parse("/title").expect("path")).expect("title")).expect("node").value, Value::Str(ref s) if s == "edited title")
    );

    let mut amf3 = exact(&fixture("amf3.sol"));
    apply_edits(
        &mut amf3.doc,
        &[
            Edit::Set {
                path: Path::parse("/negative").expect("path"),
                value: OwnedValue::Int(-42),
            },
            Edit::Set {
                path: Path::parse("/double").expect("path"),
                value: OwnedValue::Float(4.5),
            },
            Edit::Set {
                path: Path::parse("/inventory/0/kind").expect("path"),
                value: OwnedValue::Str("crate".into()),
            },
            Edit::Set {
                path: Path::parse("/vector/0").expect("path"),
                value: OwnedValue::Int(-12),
            },
        ],
        &Limits::default(),
    )
    .expect("AMF3 edits");
    let bytes = FlashSolCodec
        .encode(&amf3.doc, &amf3.params)
        .expect("AMF3 encode");
    let after = decode(&bytes);
    assert!(matches!(
        after
            .doc
            .get(
                after
                    .doc
                    .resolve(&Path::parse("/negative").expect("path"))
                    .expect("integer")
            )
            .expect("node")
            .value,
        Value::Int { v: -42, .. }
    ));
    assert!(
        matches!(after.doc.get(after.doc.resolve(&Path::parse("/double").expect("path")).expect("double")).expect("node").value, Value::Float { v, .. } if v == 4.5)
    );
    assert!(
        matches!(after.doc.get(after.doc.resolve(&Path::parse("/inventory/0/kind").expect("path")).expect("kind")).expect("node").value, Value::Str(ref s) if s == "crate")
    );
    assert!(matches!(
        after
            .doc
            .get(
                after
                    .doc
                    .resolve(&Path::parse("/vector/0").expect("path"))
                    .expect("vector item")
            )
            .expect("node")
            .value,
        Value::Int { v: -12, .. }
    ));
}

#[test]
fn amf3_reused_string_is_read_only_and_int_range_is_enforced() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/sol/amf3.sol"),
    )
    .expect("fixture");
    let mut decoded = exact(&bytes);
    let first = decoded
        .doc
        .resolve(&Path::parse("/firstText").expect("path"))
        .expect("string");
    assert!(
        decoded
            .doc
            .get(first)
            .expect("node")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/max29").expect("path"),
            value: OwnedValue::Int(1 << 28),
        }],
        &Limits::default(),
    )
    .expect("core accepts i32");
    assert_eq!(
        FlashSolCodec
            .encode(&decoded.doc, &decoded.params)
            .expect_err("AMF3 range")
            .code(),
        SOL_WRITE
    );
}

#[test]
fn amf3_string_edits_preserve_later_string_table_references() {
    let value = |name: &str, content: &[u8]| {
        let mut pair = amf3_string(name);
        pair.extend_from_slice(content);
        pair.push(0);
        pair
    };
    let populated = header(
        3,
        &[
            value("a", &[6, 3, b'x']),
            value("b", &[6, 3, b'y']),
            value("c", &[6, 6]),
        ]
        .concat(),
    );
    let mut decoded = exact(&populated);
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse("/a").expect("path"),
            value: OwnedValue::Str(String::new()),
        }],
        &Limits::default(),
    )
    .expect("edit");
    assert_eq!(
        FlashSolCodec
            .encode(&decoded.doc, &decoded.params)
            .expect_err("emptying the string shifts later refs")
            .code(),
        SOL_WRITE
    );

    let empty = header(
        3,
        &[
            value("a", &[6, 1]),
            value("b", &[6, 3, b'y']),
            value("c", &[6, 4]),
        ]
        .concat(),
    );
    let decoded = exact(&empty);
    let id = decoded
        .doc
        .resolve(&Path::parse("/a").expect("path"))
        .expect("empty string");
    assert!(
        decoded
            .doc
            .get(id)
            .expect("node")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

fn decode_error(bytes: &[u8]) -> Error {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    FlashSolCodec
        .decode(bytes, &mut ctx)
        .expect_err("input must fail")
}
