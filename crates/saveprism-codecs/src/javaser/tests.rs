use super::*;
use proptest::prelude::*;
use saveprism_core::{DecodeCtx, Limits, Path, SniffCtx, Value};

fn sample() -> Vec<u8> {
    let mut b = vec![0xac, 0xed, 0, 5, 0x73, 0x72, 0, 1, b'X'];
    b.extend_from_slice(&0_u64.to_be_bytes());
    b.extend_from_slice(&[2, 0, 1, b'I', 0, 4, b'g', b'o', b'l', b'd', 0x78, 0x70]);
    b.extend_from_slice(&17_i32.to_be_bytes());
    b
}
fn decode(bytes: &[u8]) -> Decoded {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    JavaSerializationCodec
        .decode(bytes, &mut ctx)
        .expect("decode")
}

#[test]
fn object_fields_round_trip_and_patch_exact_width() {
    let bytes = sample();
    let mut decoded = decode(&bytes);
    assert_eq!(
        JavaSerializationCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    let path = Path::parse("/0/gold").unwrap();
    let id = decoded.doc.resolve(&path).unwrap();
    decoded
        .doc
        .set_value(
            id,
            Value::Int {
                v: 0x12345678,
                width: saveprism_core::IntWidth::I32,
            },
        )
        .unwrap();
    let edited = JavaSerializationCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_eq!(&edited[edited.len() - 4..], &0x12345678_i32.to_be_bytes());
    assert!(
        JavaSerializationCodec
            .decode(&edited, &mut DecodeCtx::new(&Limits::default()))
            .unwrap()
            .doc
            .get(id)
            .is_some(),
    );
    decoded
        .doc
        .set_value(
            id,
            Value::Int {
                v: i128::from(i32::MAX) + 1,
                width: saveprism_core::IntWidth::I32,
            },
        )
        .unwrap();
    assert_eq!(
        JavaSerializationCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap_err()
            .code(),
        JAVA_WRITE
    );
}

#[test]
fn malformed_and_unsupported_inputs_are_errors() {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    assert_eq!(
        JavaSerializationCodec
            .decode(b"", &mut ctx)
            .unwrap_err()
            .code(),
        JAVA_PARSE
    );
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    assert_eq!(
        JavaSerializationCodec
            .decode(&[0xac, 0xed, 0, 5, 0x73], &mut ctx)
            .unwrap_err()
            .code(),
        JAVA_PARSE
    );
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    assert_eq!(
        JavaSerializationCodec
            .decode(&[0xac, 0xed, 0, 5, 0x7b], &mut ctx)
            .unwrap_err()
            .code(),
        JAVA_UNSUPPORTED
    );
}

#[test]
fn sniff_checks_stream_magic() {
    assert_eq!(
        JavaSerializationCodec.sniff(&[0xac, 0xed, 0, 5], &SniffCtx::default()),
        Confidence::new(0.99)
    );
    assert_eq!(
        JavaSerializationCodec.sniff(&[0xac, 0xed, 0, 4], &SniffCtx::default()),
        Confidence::NONE
    );
}

#[test]
fn primitive_arrays_patch_elements_and_shared_strings_are_locked() {
    let mut array = vec![0xac, 0xed, 0, 5, 0x75, 0x72, 0, 2, b'[', b'I'];
    array.extend_from_slice(&0_u64.to_be_bytes());
    array.extend_from_slice(&[2, 0, 0, 0x78, 0x70, 0, 0, 0, 2]);
    array.extend_from_slice(&5_i32.to_be_bytes());
    array.extend_from_slice(&9_i32.to_be_bytes());
    let mut decoded = decode(&array);
    let id = decoded.doc.resolve(&Path::parse("/0/1").unwrap()).unwrap();
    decoded
        .doc
        .set_value(
            id,
            Value::Int {
                v: 42,
                width: saveprism_core::IntWidth::I32,
            },
        )
        .unwrap();
    let changed = JavaSerializationCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap();
    assert_eq!(&changed[changed.len() - 4..], &42_i32.to_be_bytes());

    let shared = [0xac, 0xed, 0, 5, 0x74, 0, 1, b'x', 0x71, 0, 0x7e, 0, 0];
    let decoded = decode(&shared);
    let string = decoded.doc.resolve(&Path::parse("/0").unwrap()).unwrap();
    assert!(
        decoded
            .doc
            .get(string)
            .unwrap()
            .flags
            .contains(saveprism_core::NodeFlags::READ_ONLY)
    );
}

#[test]
fn modified_utf8_round_trips_nul_and_supplementary_text() {
    let bytes = [
        0xac, 0xed, 0, 5, 0x74, 0, 8, 0xc0, 0x80, 0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80,
    ];
    let decoded = decode(&bytes);
    assert_eq!(
        JavaSerializationCodec
            .encode(&decoded.doc, &decoded.params)
            .unwrap(),
        bytes
    );
    assert_eq!(
        decoded
            .doc
            .get(decoded.doc.resolve(&Path::parse("/0").unwrap()).unwrap())
            .unwrap()
            .value,
        Value::Str(format!("\0{}", char::from_u32(0x1f600).unwrap()))
    );
}

#[test]
fn jdk_stream_fixture_round_trips_without_instantiating_classes() {
    let bytes = include_bytes!("../../../../fixtures/synthetic/javaser/save-data.ser");
    let base = decode(bytes);
    assert_eq!(
        JavaSerializationCodec
            .encode(&base.doc, &base.params)
            .unwrap(),
        bytes
    );
    for (path, value) in [
        (
            "/0/gold",
            Value::Int {
                v: 300,
                width: saveprism_core::IntWidth::I32,
            },
        ),
        (
            "/0/playTime",
            Value::Int {
                v: 123456789,
                width: saveprism_core::IntWidth::I64,
            },
        ),
        (
            "/0/hp",
            Value::Float {
                v: 42.25,
                width: saveprism_core::FloatWidth::F32,
            },
        ),
        ("/0/flags", Value::Bool(false)),
        (
            "/0/inventory/1",
            Value::Int {
                v: 99,
                width: saveprism_core::IntWidth::I32,
            },
        ),
        ("/0/name", Value::Str("Changed name".into())),
    ] {
        let mut edited = base.clone();
        let id = edited.doc.resolve(&Path::parse(path).unwrap()).unwrap();
        edited.doc.set_value(id, value).unwrap();
        let output = JavaSerializationCodec
            .encode(&edited.doc, &edited.params)
            .unwrap();
        let again = decode(&output);
        assert_eq!(
            again
                .doc
                .get(again.doc.resolve(&Path::parse(path).unwrap()).unwrap())
                .unwrap()
                .value,
            edited.doc.get(id).unwrap().value,
            "{path}"
        );
    }
    for path in ["/0/party/elements", "/0/counts/entries", "/0/(customData)"] {
        assert!(
            base.doc.resolve(&Path::parse(path).unwrap()).is_some(),
            "missing {path}"
        );
    }
    let shared = base
        .doc
        .resolve(&Path::parse("/0/sharedA").unwrap())
        .unwrap();
    assert!(
        base.doc
            .get(shared)
            .unwrap()
            .flags
            .contains(saveprism_core::NodeFlags::READ_ONLY)
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn arbitrary_stream_suffixes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..128)) {
        let mut bytes = vec![0xac, 0xed, 0, 5];
        bytes.extend_from_slice(&data);
        if let Ok(decoded) = JavaSerializationCodec.decode(&bytes, &mut DecodeCtx::new(&Limits::default())) {
            let encoded = JavaSerializationCodec.encode(&decoded.doc, &decoded.params).expect("decoded stream must encode");
            let again = decode(&encoded);
            prop_assert!(decoded.doc.tree_eq(&again.doc));
        }
    }
}
