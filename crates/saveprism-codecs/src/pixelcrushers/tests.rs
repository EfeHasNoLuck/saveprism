use saveprism_core::{
    Codec, Confidence, DecodeCtx, Edit, Limits, OwnedValue, Path, SniffCtx, Value,
};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/synthetic/pixelcrushers")
        .join(name);
    std::fs::read(path).expect("fixture exists")
}

fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    PixelCrushersCodec
        .decode(bytes, &mut ctx)
        .expect("synthetic Pixel Crushers save parses")
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    PixelCrushersCodec
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
fn fixtures_round_trip_and_inline_json_and_lua_records() {
    for name in ["save0.dat", "save1.dat"] {
        round_trip(name);
    }
    let (_, decoded) = round_trip("save0.dat");
    assert_eq!(
        value_at(&decoded, "/saveRecords/0/data/health"),
        Value::Int {
            v: 87,
            width: saveprism_core::IntWidth::Unbounded
        }
    );
    assert_eq!(
        value_at(&decoded, "/saveRecords/1/data/Variable[\"Hero\"]"),
        Value::Str("Mira".into())
    );
    assert_eq!(
        value_at(&decoded, "/saveRecords/2/data"),
        Value::Str("not json, not lua".into())
    );
}

#[test]
fn an_edit_rewrites_its_record_and_keeps_the_others() {
    let (bytes, mut decoded) = round_trip("save0.dat");
    set(
        &mut decoded,
        "/saveRecords/0/data/health",
        OwnedValue::Int(120),
    );
    set(
        &mut decoded,
        "/saveRecords/1/data/Variable[\"Gold\"]",
        OwnedValue::Int(900),
    );
    let out = encode(&decoded);
    let reread = decode(&out);
    assert_eq!(
        value_at(&reread, "/saveRecords/0/data/health"),
        Value::Int {
            v: 120,
            width: saveprism_core::IntWidth::Unbounded
        }
    );
    assert_eq!(
        value_at(&reread, "/saveRecords/1/data/Variable[\"Gold\"]"),
        Value::Int {
            v: 900,
            width: saveprism_core::IntWidth::Unbounded
        }
    );
    assert_eq!(
        value_at(&reread, "/saveRecords/2/data"),
        Value::Str("not json, not lua".into())
    );
    // A record that was not edited keeps its exact nested text.
    let original = std::str::from_utf8(&bytes).expect("UTF-8");
    let out_text = std::str::from_utf8(&out).expect("UTF-8");
    let marker = "Variable[\\\"Hero\\\"]=\\\"Mira\\\"";
    assert!(original.contains(marker) && out_text.contains(marker));
}

#[test]
fn lua_reader_accepts_only_data() {
    let text = "Variable[\"Gold\"]=10; Variable[\"Name\"]=\"a\\\"b\"; T={1,2,[\"k\"]=true,nil};";
    let limits = Limits::default();
    let ctx = DecodeCtx::new(&limits);
    let doc = lua::parse(text, &ctx).expect("literal data parses");
    assert_eq!(
        lua::write_nested(text, &doc, doc.root()).expect("writes"),
        text.as_bytes()
    );
    for bad in [
        "Variable[\"Gold\"]=os.time();",
        "print(\"x\")",
        "Variable[\"X\"]=1+2;",
        "Variable[\"X\"]=function() end;",
        "Variable[\"X\"]=\"unterminated",
    ] {
        assert!(lua::parse(bad, &ctx).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn sniff_claims_pixel_crushers_saves_and_rejects_plain_json() {
    let ctx = SniffCtx::default();
    for name in ["save0.dat", "save1.dat"] {
        let bytes = fixture(name);
        let confidence = PixelCrushersCodec.sniff(&bytes, &ctx);
        assert!(
            confidence >= Confidence::new(0.9),
            "{name} at {}",
            confidence.value()
        );
    }
    for plain in [
        b"{\"records\":[{\"data\":\"x\"}]}".as_slice(),
        b"{\"a\":1}".as_slice(),
        b"[\"data\"]".as_slice(),
    ] {
        assert_eq!(PixelCrushersCodec.sniff(plain, &ctx), Confidence::NONE);
    }
}

#[test]
fn encrypted_and_broken_saves_fail_clearly() {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let error = PixelCrushersCodec
        .decode(b"\x1f\x8b\x08\x00secret", &mut ctx)
        .expect_err("binary data is not a Pixel Crushers save");
    assert_eq!(error.code(), PC_PARSE);
    assert!(error.to_string().contains("encrypted"));
    for bytes in [
        b"".as_slice(),
        b"{}".as_slice(),
        b"{\"saveRecords\":[{\"key\":\"a\"}]}".as_slice(),
    ] {
        let mut ctx = DecodeCtx::new(&limits);
        assert!(
            PixelCrushersCodec.decode(bytes, &mut ctx).is_err(),
            "{bytes:?}"
        );
    }
}
