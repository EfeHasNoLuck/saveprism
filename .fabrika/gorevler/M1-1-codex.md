# M1-1-codex — Second-eye review of the core contract (read-only)

You are reviewing, not writing. The sandbox is read-only: do not try to create or modify any file. Your **final answer message is
the review**; the CLI saves it to the report file automatically. So put the full review (format below) in your final answer.

## Context
SavePrism is a local, MIT, universal desktop save-game editor (Tauri 2 + Rust core + React). `crates/saveprism-core` now holds the
shared contract that every later codec will be written against: `SaveDoc` arena, `Value`/`Node`, `Path`, `Edit`, `Params`,
`Recipe` (TOML), `Layer` and `Codec` traits, `Registry`, `Limits`, `pipeline`, plus empty-bodied but final signatures for
`detect`, `guard` (round-trip check) and `io` (safe write). Changing these signatures later is expensive, which is why you review now.

Read: `docs/architecture/MIMARI.md` §3.1–3.5, `docs/architecture/kararlar.md` ADR-016, `AGENTS.md` (product rules),
the task card `.fabrika/gorevler/M1-1.md`, and the code in `crates/saveprism-core/src/**` and `crates/saveprism-codecs/src/**`.

## Questions to answer
1. **Fitness for the next codecs.** Will these types carry, without signature changes: Unreal GVAS (typed properties, maps with
   struct keys, f32/f64, byte-exact rewrite), Easy Save 3 (AES-CBC + PBKDF2 with salt, key supplied by the user, GZip inside),
   RPG Maker MV/MZ JsonEx (`@c` class tags, `@a` arrays, `@r` references), Python pickle / Ruby Marshal (references, cycles,
   non-string dict keys, lossless patch via `span`)? Name the exact type or signature that would break and why.
2. **Untrusted input.** Can a hostile file cause a panic, unbounded allocation, deep recursion or a hang through the contract or
   `pipeline`? Check `Limits`, `DecodeCtx` budgets, `tree_eq`, `apply_edits`, `Params` deserialization, `Recipe::from_toml`.
3. **Round-trip and safe write signatures.** Are `guard::check_round_trip` and `io::safe_write` (+ `WriteOptions`) shaped right
   for: per-layer verdicts, post-write verification, restoring from backup, a future optional-backup switch, and a 30-day retention?
4. **API sharp edges.** Ownership/borrowing that will hurt the Tauri host (documents living across IPC calls), thread-safety
   (`Send + Sync`), error type ergonomics (codes, context chain, i18n key), anything that leaks `toml`/`base64` types.
5. **Correctness bugs** you can see by reading (logic errors, wrong error codes, broken invariants).

## Output format (Markdown, English, at most ~60 lines)
- Verdict: one line (ship as is / ship after fixes / redesign needed).
- Findings, most severe first. Each: severity (KRITIK / YUKSEK / ORTA / DUSUK) · `file:line` · problem · concrete fix (signature or code sketch).
- Keep only findings you are confident about; mark guesses as "unsure".
