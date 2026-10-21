# D4-codex — Second-eye review of layers, JSON codec and round-trip guard (read-only)

The sandbox is read-only: do not try to create or modify any file. Your **final answer message is the review**; the CLI saves it.

## Context
SavePrism: local, MIT, universal save-game editor. Saves are **untrusted input**. Product rules (`AGENTS.md`): no panics, size/depth
limits, decompression-bomb guard, byte-exact round trip before editing is enabled. The core contract (`crates/saveprism-core`)
was reviewed by you already (`.fabrika/raporlar/M1-1-codex*.md`); accepted cost: layer output limits are cooperative (layers
must use `DecodeCtx::read_bounded` or an equivalent budget).

New code to review (tests pass, clippy clean):
- `crates/saveprism-codecs/src/layers/**` — base64, gzip, zlib, lz-string (own bounded decompressor; compression via `lz-str`)
- `crates/saveprism-codecs/src/json/**` — own iterative JSON parser/writer, format profile, lexemes
- `crates/saveprism-core/src/guard.rs` — per-step Exact / Semantic / Different verdicts
Cards (what was asked, Turkish): `.fabrika/gorevler/M1-3.md`, `M1-4.md`, `M1-7.md`.

## Questions
1. **Hostile input:** any panic path, unbounded allocation (before a budget check), quadratic/exponential time, deep recursion,
   integer overflow, or slice indexing that can fail? Pay special attention to the lz-string decompressor (dictionary growth,
   entry length), gzip/zlib header parsing (FEXTRA/FNAME lengths, trailing data), base64 wrapping detection, JSON string/number
   scanning and the explicit stacks.
2. **Byte-exactness:** cases where decode → encode silently changes bytes but the code claims exact (e.g. gzip level detection,
   zlib header, base64 wrap/newline, JSON profile inference, lexeme rules for numbers and strings, BOM/CRLF).
3. **Guard logic:** can `check_round_trip` report `Exact`/`Semantic` for a file that would not be written back correctly?
4. Anything that will break real files: RPG Maker MV `.rpgsave` (LZ-String `compressToBase64` of `JSON.stringify` output),
   RPG Maker MZ (zlib via pako, level 1), Unity/Newtonsoft indented JSON, Python `json.dumps`.

## Output format (Markdown, English, at most ~60 lines)
First line: verdict (ship / ship after fixes / rework). Then findings, most severe first: KRITIK / YUKSEK / ORTA / DUSUK ·
`file:line` · problem · concrete fix. Only findings you are confident about; mark guesses "unsure".
