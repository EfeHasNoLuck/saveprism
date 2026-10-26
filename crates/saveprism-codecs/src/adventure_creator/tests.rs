use saveprism_core::{
    Codec, Confidence, DecodeCtx, Edit, Limits, OwnedValue, Path, SniffCtx, Value,
};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/adventure-creator")
        .join(name);
    std::fs::read(path).expect("fixture exists")
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    AdventureCreatorCodec
        .decode(bytes, &mut ctx)
        .expect("synthetic AC save parses")
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    AdventureCreatorCodec
        .encode(&decoded.doc, &decoded.params)
        .expect("document writes")
}

fn round_trip(name: &str) -> (Vec<u8>, Decoded) {
    let bytes = fixture(name);
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes, "{name} must round trip exactly");
    (bytes, decoded)
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
    saveprism_core::apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: Path::parse(path).expect("path"),
            value,
        }],
        &Limits::default(),
    )
    .expect("edit applies");
}

fn value_at(decoded: &Decoded, path: &str) -> Value {
    let id = decoded
        .doc
        .resolve(&Path::parse(path).expect("path"))
        .expect("path exists");
    decoded.doc.get(id).expect("node exists").value.clone()
}

#[test]
fn json_xml_and_binary_fixtures_round_trip() {
    for name in [
        "village.json.save",
        "village.xml.save",
        "village.binary.save",
    ] {
        round_trip(name);
    }
}

#[test]
fn json_parts_are_split_and_kept_as_children() {
    let (_, decoded) = round_trip("village.json.save");
    assert_eq!(
        value_at(&decoded, "/0/data/mainData/previousSceneName"),
        Value::Str("Village".into())
    );
    assert_eq!(
        value_at(&decoded, "/1/data/sceneName"),
        Value::Str("Village".into())
    );
    assert_eq!(
        value_at(&decoded, "/2/data/sceneName"),
        Value::Str("Forest".into())
    );
}

#[test]
fn an_edit_rewrites_only_its_part() {
    let (bytes, mut decoded) = round_trip("village.json.save");
    set(
        &mut decoded,
        "/0/data/mainData/timeScale",
        OwnedValue::Float(2.5),
    );
    let out = encode(&decoded);
    let reread = decode(&out);
    assert_eq!(
        value_at(&reread, "/0/data/mainData/timeScale"),
        Value::Float {
            v: 2.5,
            width: saveprism_core::FloatWidth::F64
        }
    );
    // The scene parts are still byte-identical pieces of the original file.
    let text = std::str::from_utf8(&out).expect("UTF-8");
    let original = std::str::from_utf8(&bytes).expect("UTF-8");
    let tail = original.split_once("||").expect("divider").1;
    assert!(text.ends_with(tail));
}

#[test]
fn xml_and_binary_parts_are_editable() {
    let (_, mut xml) = round_trip("village.xml.save");
    set(
        &mut xml,
        "/0/data/SaveData/mainData/previousSceneName/#text",
        OwnedValue::Str("Harbour".into()),
    );
    let reread = decode(&encode(&xml));
    assert_eq!(
        value_at(&reread, "/0/data/SaveData/mainData/previousSceneName/#text"),
        Value::Str("Harbour".into())
    );
    assert_eq!(
        value_at(
            &reread,
            "/1/data/ArrayOfSingleLevelData/SingleLevelData/sceneName/#text"
        ),
        Value::Str("Village".into())
    );

    let (_, mut binary) = round_trip("village.binary.save");
    set(&mut binary, "/0/data/currentPlayerID", OwnedValue::Int(2));
    let reread = decode(&encode(&binary));
    assert_eq!(
        value_at(&reread, "/0/data/currentPlayerID"),
        Value::Int {
            v: 2,
            width: saveprism_core::IntWidth::I32
        }
    );
}

#[test]
fn sniff_claims_ac_saves_and_rejects_plain_json_and_xml() {
    let ctx = SniffCtx::default();
    for name in [
        "village.json.save",
        "village.xml.save",
        "village.binary.save",
    ] {
        let bytes = fixture(name);
        let confidence = AdventureCreatorCodec.sniff(&bytes, &ctx);
        assert!(
            confidence >= Confidence::new(0.9),
            "{name} sniffed at {}",
            confidence.value()
        );
    }
    for plain in [
        b"{\"mainData\":{\"a\":1}}".as_slice(),
        b"<SaveData><mainData/></SaveData>".as_slice(),
        b"{\"a\":1}".as_slice(),
    ] {
        assert_eq!(AdventureCreatorCodec.sniff(plain, &ctx), Confidence::NONE);
    }
}

#[test]
fn truncated_saves_are_errors_not_panics() {
    let limits = Limits::default();
    for bytes in [
        b"".as_slice(),
        b"||".as_slice(),
        b"{\"mainData\":{}}".as_slice(),
        b"{\"mainData\":{}}||{\"a\":1}||{\"b\":2}".as_slice(),
        b"{\"mainData\":{}}||not json".as_slice(),
    ] {
        let mut ctx = DecodeCtx::new(&limits);
        assert!(
            AdventureCreatorCodec.decode(bytes, &mut ctx).is_err(),
            "{bytes:?} must not decode"
        );
    }
}
