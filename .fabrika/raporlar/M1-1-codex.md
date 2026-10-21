Verdict: **redesign needed**

Findings, most severe first:

- **KRITIK** · [`doc.rs:220`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/doc.rs:220) · `SaveDoc::push` requires children before their container and exposes no placeholder/reservation or later node mutation. A decoder therefore cannot represent a container that references itself or an ancestor, blocking cyclic Python pickle/Ruby Marshal graphs. · Add a safe two-phase node builder (`reserve` then `finish`) or another validated graph-construction API.

- **YÜKSEK** · [`pipeline.rs:81`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/pipeline.rs:81) · Layer output is fully allocated before `check_output` runs; codec depth/node limits are also cooperative calls to `DecodeCtx`, not enforced around `Codec::decode`. A hostile input can therefore cause excessive allocation or deep recursion before the contract rejects it. · Give layers a bounded-output interface and enforce node/depth budgets in parser-facing builders; document and enforce limits for codec output too.

- **YÜKSEK** · [`doc.rs:126`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/doc.rs:126) · `ByteSpan` records only a node’s byte range; `Path` resolves `Map` entries to values, while `RenameKey` only targets text-keyed `Object`s ([`path.rs:3`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/path.rs:3), [`edit.rs:69`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/edit.rs:69)). Lossless patching cannot address or replace non-string map keys, and has no explicit key-span representation. · Add map-key addressing/edit operations and spans for key and value regions, or a codec-owned patch-location structure.

- **YÜKSEK** · [`doc.rs:327`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/doc.rs:327) · `tree_eq` treats every pair of `Value::Ref` nodes as equal without comparing targets. Distinct reference graphs can pass semantic round-trip comparison. · Compare reference targets with cycle-aware pair tracking, or return an explicit “reference topology not compared” result that cannot count as semantic equality.

- **ORTA** · [`codec.rs:169`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/codec.rs:169) · Default `Codec::patch` decodes with `Limits::default()` and does not accept the caller’s limits; callers cannot ensure patch decoding uses the same limits as the original decode. · Add a `DecodeCtx` or `&Limits` parameter to `patch` and pass it through the default implementation.

- **ORTA** · [`doc.rs:225`](/C:/Users/hsync/Desktop/Yazilim/Program/SaveEditor/crates/saveprism-core/src/doc.rs:225) · `SaveDoc::push` silently drops a node at the `u32` limit and returns `NodeId(u32::MAX)`, which may refer to an existing node instead of reporting failure. · Return `Result<NodeId>` and fail explicitly when the ID space or configured node budget is exhausted.

GVAS types are broadly representable (`TypeHint`, `FloatWidth`, `Map`); ES3 salt/key data fits `Params`; JsonEx tags and references fit the current value model except for cyclic graphs. `GuardReport.steps` can hold per-step verdicts, and `WriteOptions::max_age` already supports 30-day retention. The `safe_write` callback can verify read-back bytes, with restoration represented by IO error codes. An optional-backup mode conflicts with AGENTS.md’s mandatory-backup rule and should not be added unless that product rule changes.

Review was read-only; I did not run tests.