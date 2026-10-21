//! Runs a [`Recipe`]: layers peel the file bytes, the codec builds the document, and back.
//!
//! Every error gets a context line such as `while decoding step 2/3 (compression.gzip)`.
//! Tracing spans carry only the step index, kind and byte size, never file content.

use crate::codec::{DecodeCtx, verify_decoded};
use crate::doc::SaveDoc;
use crate::error::{Error, Result, codes};
use crate::limits::{Limits, len_u64};
use crate::recipe::{Recipe, Step};
use crate::registry::Registry;

/// Result of [`decode`].
#[derive(Clone, Debug)]
pub struct DecodeOutput {
    /// The decoded document.
    pub doc: SaveDoc,
    /// The recipe with the parameters every step returned (salt, level, header, ...).
    pub recipe: Recipe,
    /// `frames[0]` is the input, `frames[i]` is the output of layer `i - 1`
    /// (so the last frame is the codec's input).
    pub frames: Vec<Vec<u8>>,
}

fn unknown_step(kind: &str) -> Error {
    Error::new(
        codes::CORE_UNKNOWN_STEP,
        format!("no layer or codec is registered as \"{kind}\""),
    )
}

fn no_steps() -> Error {
    Error::new(codes::CORE_INVALID_RECIPE, "the recipe has no steps")
}

fn decode_context(index: usize, total: usize, step: &Step) -> String {
    format!("while decoding step {}/{total} ({})", index + 1, step.kind)
}

fn encode_context(index: usize, total: usize, step: &Step) -> String {
    format!("while encoding step {}/{total} ({})", index + 1, step.kind)
}

/// Decodes `input` with `recipe`.
///
/// Checks the input size and the recipe first, then runs each layer (checking the output size and
/// ratio after every layer) and finally the codec. The total size of all kept frames is also
/// bounded by `Limits::max_decoded_bytes`. The codec's document must respect `Limits::max_nodes`
/// and have no unfinished reserved node, otherwise decoding fails.
///
/// Layers should read through `DecodeCtx::read_bounded` so oversized output is stopped while it is
/// produced; the check here is the back-stop.
pub fn decode(
    input: &[u8],
    recipe: &Recipe,
    reg: &Registry,
    limits: &Limits,
) -> Result<DecodeOutput> {
    limits.check_input(len_u64(input.len()))?;
    recipe.validate(reg, limits)?;
    let total = recipe.steps.len();
    let (codec_step, layer_steps) = recipe.steps.split_last().ok_or_else(no_steps)?;

    let mut ctx = DecodeCtx::new(limits);
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total);
    frames.push(input.to_vec());
    let mut steps_out: Vec<Step> = Vec::with_capacity(total);
    let mut kept_bytes = len_u64(input.len());

    for (index, step) in layer_steps.iter().enumerate() {
        let current = frames.last().ok_or_else(no_steps)?;
        let span = tracing::debug_span!(
            "decode_step",
            index,
            kind = %step.kind,
            bytes = current.len()
        );
        let _entered = span.enter();

        let layer = reg
            .layer(&step.kind)
            .ok_or_else(|| unknown_step(&step.kind))
            .map_err(|e| e.context(decode_context(index, total, step)))?;
        ctx = ctx.with_step(index, step.params.clone());
        let (inner, params) = layer
            .decode(current, &mut ctx)
            .map_err(|e| e.context(decode_context(index, total, step)))?;
        limits
            .check_output(len_u64(current.len()), len_u64(inner.len()))
            .map_err(|e| e.context(decode_context(index, total, step)))?;
        kept_bytes = kept_bytes.saturating_add(len_u64(inner.len()));
        if kept_bytes > limits.max_decoded_bytes {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "the intermediate results add up to {kept_bytes} bytes, the limit is {} bytes",
                    limits.max_decoded_bytes
                ),
            )
            .context(decode_context(index, total, step)));
        }
        steps_out.push(Step::new(step.kind.clone(), params));
        frames.push(inner);
    }

    let index = layer_steps.len();
    let current = frames.last().ok_or_else(no_steps)?;
    let span = tracing::debug_span!(
        "decode_step",
        index,
        kind = %codec_step.kind,
        bytes = current.len()
    );
    let _entered = span.enter();
    let codec = reg
        .codec(&codec_step.kind)
        .ok_or_else(|| unknown_step(&codec_step.kind))
        .map_err(|e| e.context(decode_context(index, total, codec_step)))?;
    ctx = ctx.with_step(index, codec_step.params.clone());
    let decoded = codec
        .decode(current, &mut ctx)
        .map_err(|e| e.context(decode_context(index, total, codec_step)))?;
    // Back-stop for codecs that skipped `ctx.new_doc()` or left a reserved node unfinished.
    verify_decoded(&decoded.doc, limits)
        .map_err(|e| e.context(decode_context(index, total, codec_step)))?;
    steps_out.push(Step::new(codec_step.kind.clone(), decoded.params));

    Ok(DecodeOutput {
        doc: decoded.doc,
        recipe: Recipe {
            version: recipe.version,
            steps: steps_out,
        },
        frames,
    })
}

/// Encodes `doc` with `recipe`: the codec writes the inner bytes, then the layers wrap them from the
/// innermost to the outermost. With a recipe returned by [`decode`] and an unchanged document the
/// result is the original input.
pub fn encode(doc: &SaveDoc, recipe: &Recipe, reg: &Registry) -> Result<Vec<u8>> {
    let total = recipe.steps.len();
    let (codec_step, layer_steps) = recipe.steps.split_last().ok_or_else(no_steps)?;

    let index = layer_steps.len();
    let span = tracing::debug_span!("encode_step", index, kind = %codec_step.kind);
    let inner = {
        let _entered = span.enter();
        let codec = reg
            .codec(&codec_step.kind)
            .ok_or_else(|| unknown_step(&codec_step.kind))
            .map_err(|e| e.context(encode_context(index, total, codec_step)))?;
        codec
            .encode(doc, &codec_step.params)
            .map_err(|e| e.context(encode_context(index, total, codec_step)))?
    };
    encode_layers(inner, layer_steps, total, reg)
}

/// Encodes already-decoded inner bytes outwards, starting at layer `from_layer` (0-based):
/// `inner` is the content of that layer, which is wrapped by layers `from_layer`, `from_layer - 1`,
/// ..., `0`. Passing the index of the last layer wraps the output of the codec completely.
pub fn encode_from(
    inner: &[u8],
    recipe: &Recipe,
    from_layer: usize,
    reg: &Registry,
) -> Result<Vec<u8>> {
    let total = recipe.steps.len();
    let (_, layer_steps) = recipe.steps.split_last().ok_or_else(no_steps)?;
    let wrapping = layer_steps.get(..=from_layer).ok_or_else(|| {
        Error::new(
            codes::CORE_INVALID_RECIPE,
            format!(
                "layer index {from_layer} is out of range, the recipe has {} layers",
                layer_steps.len()
            ),
        )
    })?;
    encode_layers(inner.to_vec(), wrapping, total, reg)
}

/// Wraps `bytes` with `layers`, last layer first.
fn encode_layers(
    mut bytes: Vec<u8>,
    layers: &[Step],
    total: usize,
    reg: &Registry,
) -> Result<Vec<u8>> {
    for (index, step) in layers.iter().enumerate().rev() {
        let span = tracing::debug_span!(
            "encode_step",
            index,
            kind = %step.kind,
            bytes = bytes.len()
        );
        let _entered = span.enter();
        let layer = reg
            .layer(&step.kind)
            .ok_or_else(|| unknown_step(&step.kind))
            .map_err(|e| e.context(encode_context(index, total, step)))?;
        bytes = layer
            .encode(&bytes, &step.params)
            .map_err(|e| e.context(encode_context(index, total, step)))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::codec::{Codec, Confidence, Decoded, Layer, SniffCtx};
    use crate::doc::{Node, Value};
    use crate::edit::{Edit, OwnedValue};
    use crate::params::Params;
    use crate::path::Path;

    /// XOR with the `key` parameter (a byte string, repeated).
    struct XorLayer;

    fn xor(data: &[u8], key: &[u8]) -> Vec<u8> {
        data.iter()
            .zip(key.iter().cycle())
            .map(|(byte, key_byte)| byte ^ key_byte)
            .collect()
    }

    impl Layer for XorLayer {
        fn id(&self) -> &'static str {
            "test.xor"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::LOW
        }
        fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            let key = ctx.hints().bytes("key")?.to_vec();
            if key.is_empty() {
                return Err(Error::new(codes::CORE_INVALID_PARAM, "empty xor key"));
            }
            let mut params = Params::new();
            params.set("key", key.clone());
            Ok((xor(input, &key), params))
        }
        fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
            Ok(xor(inner, params.bytes("key")?))
        }
    }

    /// Reverses the bytes.
    struct ReverseLayer;

    impl Layer for ReverseLayer {
        fn id(&self) -> &'static str {
            "test.reverse"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.iter().rev().copied().collect(), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.iter().rev().copied().collect())
        }
    }

    /// Always fails to decode.
    struct FailLayer;

    impl Layer for FailLayer {
        fn id(&self) -> &'static str {
            "test.fail"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, _input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Err(Error::new(codes::CORE_INVALID_PARAM, "boom").with_offset(7))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.to_vec())
        }
    }

    /// Expands every byte to `factor` bytes.
    struct GrowLayer;

    impl Layer for GrowLayer {
        fn id(&self) -> &'static str {
            "test.grow"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.repeat(1000), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.to_vec())
        }
    }

    /// Splits UTF-8 text on `\n` into a root array of strings; joining restores the bytes exactly.
    struct LinesCodec;

    impl Codec for LinesCodec {
        fn id(&self) -> &'static str {
            "test.lines"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::LOW
        }
        fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            let text = std::str::from_utf8(input)
                .map_err(|e| Error::new(codes::CORE_INVALID_PARAM, "not UTF-8").with_source(e))?;
            let mut doc = ctx.new_doc();
            let mut ids = Vec::new();
            for line in text.split('\n') {
                ctx.charge_nodes(1)?;
                ids.push(doc.push(Node::new(Value::Str(line.to_string())))?);
            }
            ctx.charge_nodes(1)?;
            let root = doc.push(Node::new(Value::Array(ids)))?;
            doc.set_root(root);
            Ok(Decoded {
                doc,
                params: Params::new(),
            })
        }
        fn encode(&self, doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            let Some(Value::Array(ids)) = doc.get(doc.root()).map(|n| &n.value) else {
                return Err(Error::new(
                    codes::CORE_INVALID_PARAM,
                    "root is not an array",
                ));
            };
            let mut lines = Vec::new();
            for id in ids {
                match doc.get(*id).map(|n| &n.value) {
                    Some(Value::Str(line)) => lines.push(line.as_str()),
                    _ => return Err(Error::new(codes::CORE_INVALID_PARAM, "not a string")),
                }
            }
            Ok(lines.join("\n").into_bytes())
        }
    }

    /// A codec that breaks the document rules in two ways, chosen by its id:
    /// `test.leaky` leaves a reserved node unfinished, `test.big` ignores the node limit by
    /// opening its document with `SaveDoc::new()` instead of `ctx.new_doc()`.
    struct BadDocCodec(&'static str);

    impl Codec for BadDocCodec {
        fn id(&self) -> &'static str {
            self.0
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, _input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            let mut doc = SaveDoc::new();
            if self.0 == "test.leaky" {
                doc.reserve()?;
            } else {
                for _ in 0..5 {
                    doc.push(Node::new(Value::Null))?;
                }
            }
            Ok(Decoded {
                doc,
                params: Params::new(),
            })
        }
        fn encode(&self, _doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    fn registry() -> Registry {
        let mut reg = Registry::default();
        reg.add_codec(Arc::new(BadDocCodec("test.leaky")));
        reg.add_codec(Arc::new(BadDocCodec("test.big")));
        reg.add_layer(Arc::new(XorLayer));
        reg.add_layer(Arc::new(ReverseLayer));
        reg.add_layer(Arc::new(FailLayer));
        reg.add_layer(Arc::new(GrowLayer));
        reg.add_codec(Arc::new(LinesCodec));
        reg
    }

    const KEY: [u8; 3] = [0x5A, 0x13, 0xC4];
    const PLAIN: &str = "hello\nworld\nthird line\n";

    fn xor_step() -> Step {
        let mut params = Params::new();
        params.set("key", KEY.to_vec());
        Step::new("test.xor", params)
    }

    /// recipe: xor -> reverse -> lines
    fn recipe() -> Recipe {
        Recipe::new(vec![
            xor_step(),
            Step::bare("test.reverse"),
            Step::bare("test.lines"),
        ])
    }

    /// The file bytes for `PLAIN`: undo the layers in reverse order.
    fn file_bytes() -> Vec<u8> {
        let reversed: Vec<u8> = PLAIN.bytes().rev().collect();
        xor(&reversed, &KEY)
    }

    #[test]
    fn decode_then_encode_is_byte_identical() {
        let reg = registry();
        let input = file_bytes();
        let out = decode(&input, &recipe(), &reg, &Limits::default()).unwrap();

        // The document holds the lines.
        let root = out.doc.get(out.doc.root()).unwrap();
        let Value::Array(ids) = &root.value else {
            panic!("root must be an array");
        };
        assert_eq!(ids.len(), 4);
        let first = out.doc.get(ids[0]).unwrap();
        assert_eq!(first.value, Value::Str("hello".into()));

        // Frames: input, after xor, after reverse.
        assert_eq!(out.frames.len(), 3);
        assert_eq!(out.frames[0], input);
        assert_eq!(out.frames[2], PLAIN.as_bytes());

        // The returned recipe carries the layer parameters.
        assert_eq!(
            out.recipe.kinds(),
            ["test.xor", "test.reverse", "test.lines"]
        );
        assert_eq!(out.recipe.steps[0].params.bytes("key").unwrap(), KEY);

        let again = encode(&out.doc, &out.recipe, &reg).unwrap();
        assert_eq!(again, input);
    }

    #[test]
    fn recipe_survives_a_toml_round_trip_before_encoding() {
        let reg = registry();
        let input = file_bytes();
        let out = decode(&input, &recipe(), &reg, &Limits::default()).unwrap();
        let text = out.recipe.to_toml().unwrap();
        let reloaded = Recipe::from_toml(&text).unwrap();
        assert_eq!(encode(&out.doc, &reloaded, &reg).unwrap(), input);
    }

    #[test]
    fn encode_from_wraps_from_the_given_layer_outwards() {
        let reg = registry();
        let input = file_bytes();
        let out = decode(&input, &recipe(), &reg, &Limits::default()).unwrap();
        // The codec input is the content of layer index 1 (the last layer).
        let wrapped = encode_from(&out.frames[2], &out.recipe, 1, &reg).unwrap();
        assert_eq!(wrapped, input);
        // Content of layer 0 is frames[1]; only the xor layer wraps it.
        let wrapped = encode_from(&out.frames[1], &out.recipe, 0, &reg).unwrap();
        assert_eq!(wrapped, input);

        let error = encode_from(&out.frames[2], &out.recipe, 2, &reg).unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_RECIPE);
    }

    #[test]
    fn unknown_kind_is_reported() {
        let reg = registry();
        let bad = Recipe::new(vec![Step::bare("test.nope"), Step::bare("test.lines")]);
        let error = decode(b"x", &bad, &reg, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_UNKNOWN_STEP);
    }

    #[test]
    fn too_many_layers_are_rejected() {
        let reg = registry();
        let mut steps: Vec<Step> = (0..9).map(|_| Step::bare("test.reverse")).collect();
        steps.push(Step::bare("test.lines"));
        let error = decode(b"x", &Recipe::new(steps), &reg, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_LAYERS);
    }

    #[test]
    fn large_input_is_rejected_before_any_work() {
        let reg = registry();
        let limits = Limits {
            max_input_bytes: 8,
            ..Limits::default()
        };
        let error = decode(&[0_u8; 9], &recipe(), &reg, &limits).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);
    }

    #[test]
    fn errors_carry_the_step_context() {
        let reg = registry();
        let failing = Recipe::new(vec![
            xor_step(),
            Step::bare("test.fail"),
            Step::bare("test.lines"),
        ]);
        let error = decode(&file_bytes(), &failing, &reg, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
        assert_eq!(error.offset(), Some(7));
        let report = error.report();
        assert!(report.contains("step 2/3"), "{report}");
        assert!(report.contains("test.fail"), "{report}");
    }

    #[test]
    fn missing_layer_parameter_fails_with_context() {
        let reg = registry();
        let no_key = Recipe::new(vec![Step::bare("test.xor"), Step::bare("test.lines")]);
        let error = decode(b"abc", &no_key, &reg, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
        assert!(error.report().contains("step 1/2"));
    }

    #[test]
    fn codec_errors_carry_the_codec_step_context() {
        let reg = registry();
        let single = Recipe::new(vec![Step::bare("test.lines")]);
        let error = decode(&[0xFF, 0xFE], &single, &reg, &Limits::default()).unwrap_err();
        assert!(error.report().contains("step 1/1 (test.lines)"));
    }

    #[test]
    fn decompression_bombs_are_stopped_after_the_layer() {
        let reg = registry();
        let limits = Limits {
            ratio_floor_bytes: 100,
            max_ratio: 10,
            ..Limits::default()
        };
        let bomb = Recipe::new(vec![Step::bare("test.grow"), Step::bare("test.lines")]);
        let error = decode(&[b'a'; 50], &bomb, &reg, &limits).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
        assert!(error.report().contains("step 1/2"));
    }

    #[test]
    fn node_limit_applies_to_the_codec() {
        let reg = registry();
        let limits = Limits {
            max_nodes: 2,
            ..Limits::default()
        };
        let error = decode(&file_bytes(), &recipe(), &reg, &limits).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
    }

    #[test]
    fn decode_rejects_a_document_with_unfinished_nodes() {
        let reg = registry();
        let leaky = Recipe::new(vec![Step::bare("test.leaky")]);
        let error = decode(b"x", &leaky, &reg, &Limits::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_DOC_BUILD);
        assert!(error.report().contains("step 1/1 (test.leaky)"));
    }

    #[test]
    fn decode_rejects_a_document_over_the_node_limit_even_if_the_codec_ignored_it() {
        let reg = registry();
        let big = Recipe::new(vec![Step::bare("test.big")]);
        // 6 nodes (root + 5) against a limit of 4.
        let limits = Limits {
            max_nodes: 4,
            ..Limits::default()
        };
        let error = decode(b"x", &big, &reg, &limits).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
        assert!(error.report().contains("step 1/1 (test.big)"));
        // With the default limit the same codec is fine.
        assert!(decode(b"x", &big, &reg, &Limits::default()).is_ok());
    }

    #[test]
    fn encode_reports_the_failing_step() {
        let reg = registry();
        let mut broken = recipe();
        // Remove the key so the xor layer cannot encode.
        broken.steps[0].params = Params::new();
        let doc = decode(&file_bytes(), &recipe(), &reg, &Limits::default())
            .unwrap()
            .doc;
        let error = encode(&doc, &broken, &reg).unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
        assert!(error.report().contains("while encoding step 1/3"));
    }

    #[test]
    fn default_patch_decodes_edits_and_encodes() {
        let codec = LinesCodec;
        let edits = [Edit::Set {
            path: Path::parse("/1").unwrap(),
            value: OwnedValue::Str("WORLD".into()),
        }];
        let limits = Limits::default();
        let patched = codec
            .patch(PLAIN.as_bytes(), &edits, &Params::new(), &limits)
            .unwrap();
        assert_eq!(patched, b"hello\nWORLD\nthird line\n");

        let bad = [Edit::Set {
            path: Path::parse("/9").unwrap(),
            value: OwnedValue::Null,
        }];
        let error = codec
            .patch(PLAIN.as_bytes(), &bad, &Params::new(), &limits)
            .unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PATH);
    }

    #[test]
    fn default_patch_uses_the_callers_limits() {
        let codec = LinesCodec;
        let params = Params::new();

        let small_input = Limits {
            max_input_bytes: 3,
            ..Limits::default()
        };
        let error = codec
            .patch(PLAIN.as_bytes(), &[], &params, &small_input)
            .unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);

        let few_nodes = Limits {
            max_nodes: 2,
            ..Limits::default()
        };
        let error = codec
            .patch(PLAIN.as_bytes(), &[], &params, &few_nodes)
            .unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_NODES);

        // The edit value is limited by the same `max_depth`.
        let shallow = Limits {
            max_depth: 1,
            ..Limits::default()
        };
        let nested = OwnedValue::Array(vec![OwnedValue::Array(vec![OwnedValue::Null])]);
        let edits = [Edit::Set {
            path: Path::parse("/0").unwrap(),
            value: nested,
        }];
        let error = codec
            .patch(PLAIN.as_bytes(), &edits, &params, &shallow)
            .unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DEPTH);
    }
}
