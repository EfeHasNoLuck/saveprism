//! Tests of the GFF V3.2 codec.

use std::path::PathBuf;

use saveprism_core::{
    Codec, DecodeCtx, Decoded, Edit, IntWidth, Limits, NodeFlags, OwnedValue, Path, SniffCtx,
    Value, apply_edits,
};

use super::{GFF_PARSE, GFF_UNSUPPORTED, GFF_WRITE, GffCodec, HINT_PREFIX};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/bioware")
        .join(name);
    std::fs::read(path).expect("fixture is readable")
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    GffCodec
        .decode(bytes, &mut ctx)
        .expect("valid synthetic GFF")
}

fn exact(bytes: &[u8]) -> Decoded {
    let decoded = decode(bytes);
    assert_eq!(
        GffCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("unchanged GFF encodes"),
        bytes
    );
    decoded
}

fn edit(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).expect("valid path"),
            value,
        }],
        &Limits::default(),
    )
}

fn node<'a>(decoded: &'a Decoded, path: &str) -> &'a saveprism_core::Node {
    let id = decoded
        .doc
        .resolve(&Path::parse(path).expect("valid path"))
        .expect("path exists");
    decoded.doc.get(id).expect("node exists")
}

#[test]
fn character_round_trips_exactly() {
    let bytes = fixture("character.bic");
    let decoded = exact(&bytes);
    let root = decoded.doc.get(decoded.doc.root()).expect("root");
    assert_eq!(
        root.type_hint.as_ref().map(|hint| hint.as_str()),
        Some("GFF:root:BIC")
    );
    assert_eq!(
        node(&decoded, "/Level").value,
        Value::Int {
            v: 7,
            width: IntWidth::U8
        }
    );
    assert_eq!(
        node(&decoded, "/Gold").value,
        Value::Int {
            v: 500,
            width: IntWidth::U32
        }
    );
    assert_eq!(
        node(&decoded, "/Tag").value,
        Value::Str("mira_rune".to_owned())
    );
    assert_eq!(
        node(&decoded, "/Inventory/1/Name").value,
        Value::Str("Iron Key".to_owned())
    );
    assert_eq!(
        node(&decoded, "/Stats/Str").value,
        Value::Int {
            v: 14,
            width: IntWidth::U8
        }
    );
}

#[test]
fn strings_and_structs_are_labelled() {
    let decoded = exact(&fixture("character.bic"));
    assert!(
        node(&decoded, "/Tag")
            .type_hint
            .as_ref()
            .is_some_and(|hint| hint.as_str().starts_with(HINT_PREFIX))
    );
    assert!(
        node(&decoded, "/Stats")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(
        node(&decoded, "/Inventory")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
    assert!(!node(&decoded, "/Gold").flags.contains(NodeFlags::READ_ONLY));
    let loc = node(&decoded, "/FirstName");
    assert_eq!(
        loc.type_hint.as_ref().map(|hint| hint.as_str()),
        Some("GFF:cexolocstring")
    );
    assert_eq!(
        node(&decoded, "/FirstName/strings/0/text").value,
        Value::Str("Mira".to_owned())
    );
    assert!(
        node(&decoded, "/FirstName/strref")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

#[test]
fn a_number_edit_changes_only_its_bytes() {
    let bytes = fixture("character.bic");
    let mut decoded = exact(&bytes);
    edit(&mut decoded, "/Gold", OwnedValue::Int(501)).expect("edit gold");
    let written = GffCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("edited GFF encodes");
    assert_eq!(written.len(), bytes.len());
    let changed: Vec<usize> = bytes
        .iter()
        .zip(written.iter())
        .enumerate()
        .filter(|(_, (left, right))| left != right)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(changed.len(), 1, "only the low byte of the DWORD changes");
    let again = decode(&written);
    assert_eq!(
        node(&again, "/Gold").value,
        Value::Int {
            v: 501,
            width: IntWidth::U32
        }
    );
}

#[test]
fn a_same_length_string_edit_is_written() {
    let bytes = fixture("character.bic");
    let mut decoded = exact(&bytes);
    edit(
        &mut decoded,
        "/Tag",
        OwnedValue::Str("kira_rune".to_owned()),
    )
    .expect("edit tag");
    edit(
        &mut decoded,
        "/FirstName/strings/0/text",
        OwnedValue::Str("Kira".to_owned()),
    )
    .expect("edit first name");
    let written = GffCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("edited GFF encodes");
    let again = decode(&written);
    assert_eq!(
        node(&again, "/Tag").value,
        Value::Str("kira_rune".to_owned())
    );
    assert_eq!(
        node(&again, "/FirstName/strings/0/text").value,
        Value::Str("Kira".to_owned())
    );
}

#[test]
fn a_length_changing_string_edit_is_refused() {
    let bytes = fixture("character.bic");
    let mut decoded = exact(&bytes);
    edit(
        &mut decoded,
        "/Tag",
        OwnedValue::Str("much longer tag".to_owned()),
    )
    .expect("the document allows the edit");
    assert_eq!(
        GffCodec
            .encode(&decoded.doc, &decoded.params)
            .expect_err("a longer string must be refused")
            .code(),
        GFF_WRITE
    );
}

#[test]
fn an_out_of_range_number_is_refused() {
    let bytes = fixture("character.bic");
    let mut decoded = exact(&bytes);
    assert_eq!(
        edit(&mut decoded, "/Level", OwnedValue::Int(300))
            .expect_err("300 does not fit a BYTE")
            .code(),
        saveprism_core::codes::CORE_EDIT_FAILED
    );
    edit(&mut decoded, "/Weight", OwnedValue::Str("heavy".to_owned()))
        .expect("the document allows the type change");
    assert_eq!(
        GffCodec
            .encode(&decoded.doc, &decoded.params)
            .expect_err("a string cannot be written as a FLOAT")
            .code(),
        GFF_WRITE
    );
}

#[test]
fn module_ifo_is_read() {
    let decoded = exact(&fixture("module.ifo"));
    assert_eq!(
        node(&decoded, "/ModName").value,
        Value::Str("The Ashen Vale".to_owned())
    );
}

#[test]
fn truncated_and_unsupported_files_give_codes() {
    let bytes = fixture("character.bic");
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    assert_eq!(
        GffCodec
            .decode(bytes.get(..40).unwrap_or_default(), &mut ctx)
            .expect_err("truncated header")
            .code(),
        GFF_PARSE
    );
    let mut bad_version = bytes.clone();
    if let Some(byte) = bad_version.get_mut(7) {
        *byte = b'1';
    }
    assert_eq!(
        GffCodec
            .decode(&bad_version, &mut ctx)
            .expect_err("bad version")
            .code(),
        GFF_PARSE
    );
    let mut unknown = bytes;
    let field_offset = u32::from_le_bytes(
        unknown
            .get(16..20)
            .and_then(|bytes| bytes.try_into().ok())
            .expect("field offset"),
    ) as usize;
    if let Some(byte) = unknown.get_mut(field_offset) {
        *byte = 99;
    }
    assert_eq!(
        GffCodec
            .decode(&unknown, &mut ctx)
            .expect_err("unknown type")
            .code(),
        GFF_UNSUPPORTED
    );
}

#[test]
fn sniff_requires_the_version_and_a_plausible_header() {
    let ctx = SniffCtx {
        file_name: Some("character.bic"),
        ..SniffCtx::default()
    };
    let bytes = fixture("character.bic");
    assert!(GffCodec.sniff(&bytes, &ctx).value() > 0.9);
    assert_eq!(
        GffCodec.sniff(b"short", &ctx),
        saveprism_core::Confidence::NONE
    );
    let v4 = fixture("dao-save.das");
    assert_eq!(GffCodec.sniff(&v4, &ctx), saveprism_core::Confidence::NONE);
}
