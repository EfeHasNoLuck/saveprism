# MAP — Where Things Live

> **TL;DR:** The fastest path from a symptom to the right file. Look here before searching the codebase. Every new module, error prefix or doc gets a line here in the same change (the reviewer checks it).
>
> **Next:** the "Code" table fills in as M1 lands; until then it shows the planned layout.

---

## 1. Symptom → where to look

| Symptom | First look | Then |
|---|---|---|
| File not recognized | Detector (`crates/saveprism-core/src/detect.rs`) | Diagnostic Report "detection trace" |
| Opens read-only ("round-trip mismatch") | Codec `encode` + saved `Params` for that format | Golden fixture for the format |
| Wrong password / key | `crates/saveprism-codecs/src/crypto/`, key vault (`crates/saveprism-keys/`) | `docs/research/02-format-ve-kutuphane-atlasi.md` (scheme) |
| Save broken after writing | Safe-write module (`crates/saveprism-core/src/io.rs`) | Backup ring in the app data dir |
| UI freezes on a big file | IPC paging (`apps/desktop/src-tauri/src/commands/`) | Virtual tree (`apps/desktop/src/tree/`) |
| Wrong text / missing translation | `apps/desktop/locales/<lang>/` | i18n setup (`apps/desktop/src/i18n/`) |
| Crash with error code `SP-…` | Error prefix table (§3) | `QUALITY.md` §1 |
| Auto-update fails | `tauri.conf.json` updater section | `.github/workflows/release.yml` |

## 2. Code (✅ = exists, others planned)

| Area | Path | Owns |
|---|---|---|
| Workspace ✅ | `Cargo.toml` · `package.json` · `pnpm-workspace.yaml` · `clippy.toml` · `rust-toolchain.toml` | Members, shared lints (no unwrap/expect/panic/unsafe), release profile, root scripts |
| License policy ✅ | `deny.toml` · `.github/workflows/deny.yml` | Allowed licenses, advisories, sources |
| Core model ✅ | `crates/saveprism-core/src/` | `doc` SaveDoc · `codec` Layer/Codec · `recipe` · `params` · `pipeline` · `detect` · `guard` (round-trip) · `io` (safe write, M1-8) · `limits` · `error` |
| Codecs ✅ | `crates/saveprism-codecs/src/` | `json/` (parse, write, profile) · `layers/` (base64, gzip, zlib, lzstring) · later es3, gvas, rpgmaker, godot… |
| Keys | `crates/saveprism-keys/` | Key vault, key finder |
| CLI | `crates/saveprism-cli/` | `saveprism` command |
| Desktop host ✅ | `apps/desktop/src-tauri/` | Tauri commands → core (`core_version`), window, CSP, capabilities |
| Desktop UI ✅ | `apps/desktop/src/` | React UI: `App.tsx` shell · `theme/` (M0-6) · `i18n/` (M0-7) |
| Community ✅ | `CONTRIBUTING.md` · `SECURITY.md` · `DISCLAIMER.md` · `CODE_OF_CONDUCT.md` · `.github/ISSUE_TEMPLATE/` | Contributor rules, reporting, scope |
| Profiles | `profiles/` | Game profiles (TOML) |
| Fixtures ✅ | `fixtures/samples/` (locked) · `fixtures/synthetic/` · `fixtures/expected/` | Golden corpus: `crates/saveprism-codecs/tests/golden.rs`; bless with `SAVEPRISM_BLESS=1` |
| Fuzzing ✅ | `fuzz/` (outside the workspace) · `.github/workflows/fuzz.yml` | Targets detect, json, layers; limit tests in `crates/saveprism-codecs/tests/limits.rs` |

## 3. Error code prefixes

| Prefix | Crate / area |
|---|---|
| `SP-CORE` · `SP-DET` · `SP-IO` | `saveprism-core` (model · detector · safe write) |
| `SP-LAY` · `SP-JSON` · `SP-ES3` · `SP-GVAS` · `SP-RPG` · `SP-PAL` · `SP-GODOT` · `SP-PREFS` · `SP-RENPY` | `saveprism-codecs` (`src/<format>/mod.rs`: `rpgmaker`, `es3`, `gvas`, `palworld`) |
| `SP-KEYS` | `saveprism-keys` |
| `SP-APP` · `SP-UI` | desktop host · UI (`SP-APP-001`: app failed to start → `apps/desktop/src-tauri/src/lib.rs`) |

## 4. Docs — which file answers what

| Question | File |
|---|---|
| Where are we, what's next? | `NOW.md` |
| What are we building, in what order? | `docs/ROADMAP.md` |
| How is it built? Why this way? | `docs/architecture/MIMARI.md` · `docs/architecture/kararlar.md` |
| How do we keep bugs out? | `docs/architecture/QUALITY.md` |
| What could go wrong? What won't we do? | `docs/RISKS.md` |
| How does a format work? | `docs/research/02-format-ve-kutuphane-atlasi.md` |
| How do we work day to day? | `docs/FABRIKA.md` · `TASKS.md` · `handoff/` |

For deeper questions once code exists, `/graphify` builds a queryable knowledge graph of the repo; this file stays the curated entry point.
