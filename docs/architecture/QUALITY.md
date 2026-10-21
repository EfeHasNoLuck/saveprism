# Quality, Errors and Debuggability

> **TL;DR:** Bugs are caught in this order: compiler and lints → unit tests → golden round-trip corpus → property tests → fuzzing → reviewer → you. Every error that does reach a user carries a stable code, a "cause + fix" message and a Diagnostic Report, so it can be traced to one file in minutes.
>
> **Next:** M1-1 creates the error type and the lint configuration first; everything else builds on them.

---

## 1. Error model

| Rule | Detail |
|---|---|
| Typed errors per crate | `thiserror` enums in library crates; no stringly-typed errors |
| Stable error codes | `SR-<AREA>-<NNN>`, e.g. `SR-ES3-003` (wrong password). Codes never change meaning once released |
| Two messages per error | User text (i18n key, "Cause + Fix") and technical context (offsets, layer, recipe step) |
| Context chain | Each layer adds context (`while decoding layer 2/4 (compression.gzip) at offset 0x1A0`) |
| No panics in libraries | Parser crates deny `unwrap`, `expect`, `panic!`, unchecked indexing via clippy lints; tests may use them |
| Panic safety net | App-level panic hook and React ErrorBoundary turn any crash into a Diagnostic Report |
| Fail at the boundary | Inputs are validated where they enter a codec; internal code may assume invariants |

Area prefixes (registered in [MAP.md](../MAP.md)): `CORE` · `DET` (detector) · `LAY` (layers) · `JSON` · `ES3` · `GVAS` · `RPGM` · `GODOT` · `PREFS` · `RENPY` · `KEYS` · `IO` (safe write) · `APP` · `UI`.

## 2. Test pyramid

| Level | Tool | What it proves | When it runs |
|---|---|---|---|
| Unit | `cargo test`, `vitest` | Functions behave as specified | Every change |
| Golden round-trip | Fixture corpus (`fixtures/samples` + `fixtures/expected`) | decode → encode reproduces original bytes; decoded tree matches the expected snapshot | Every change |
| Snapshot | `insta` | Tree output, recipes and Diagnostic Reports don't change unnoticed | Every change |
| Property | `proptest` | `decode(encode(doc)) == doc` for generated documents; limits hold for random input | Every change |
| Fuzz | `cargo-fuzz` (one target per decoder) | No panic, hang or unbounded allocation on hostile bytes | Short run per PR, long run weekly |
| UI / E2E | `vitest` + Playwright (later `tauri-driver`) | Open → edit → save flow works | Per PR (smoke), nightly (full) |
| Mutation | `cargo-mutants` | Tests actually catch broken logic | Per milestone on `core` |

**Regression rule:** a bug is fixed only after a failing test reproduces it. The test stays forever.

## 3. Observability

1. `tracing` with one span per pipeline layer (file id hash, layer kind, byte range). No save content in logs.
2. In-memory log ring buffer (last 200 lines) feeds the Diagnostic Report ([MIMARI §3.8](MIMARI.md)).
3. "Copy debug bundle" = Diagnostic Report + recipe + masked header bytes. Enough to reproduce without the save itself in most cases.

## 4. Gates

| Gate | Blocks merge when |
|---|---|
| `kontrol.sh rust` | fmt, clippy (`-D warnings`) or tests fail |
| `kontrol.sh web` | typecheck, lint or tests fail |
| `cargo-deny` | GPL-family license or known advisory |
| Fuzz (PR run) | any crash |
| Reviewer (denetçi) | unresolved KRITIK/YUKSEK finding |

## 5. Dependencies this strategy needs (all permissive)

`thiserror` · `tracing` · `tracing-subscriber` · `proptest` · `insta` · `cargo-fuzz` · `cargo-mutants` · `cargo-deny` — MIT and/or Apache-2.0. Added in M1 after approval (ADR-013).
