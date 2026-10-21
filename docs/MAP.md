# MAP — Where Things Live

> **TL;DR:** The fastest path from a symptom to the right file. Look here before searching the codebase. Every new module, error prefix or doc gets a line here in the same change (the reviewer checks it).
>
> **Next:** the "Code" table fills in as M1 lands; until then it shows the planned layout.

---

## 1. Symptom → where to look

| Symptom | First look | Then |
|---|---|---|
| File not recognized | Detector (`crates/saveprism-core/src/detect/`) | Diagnostic Report "detection trace" |
| Opens read-only ("round-trip mismatch") | Codec `encode` + saved `Params` for that format | Golden fixture for the format |
| Wrong password / key | `crates/saveprism-codecs/src/crypto/`, key vault (`crates/saveprism-keys/`) | `docs/research/02-format-ve-kutuphane-atlasi.md` (scheme) |
| Save broken after writing | Safe-write module (`crates/saveprism-core/src/io/`) | Backup ring in the app data dir |
| UI freezes on a big file | IPC paging (`apps/desktop/src-tauri/src/commands/`) | Virtual tree (`apps/desktop/src/tree/`) |
| Wrong text / missing translation | `apps/desktop/locales/<lang>/` | i18n setup (`apps/desktop/src/i18n/`) |
| Crash with error code `SP-…` | Error prefix table (§3) | `QUALITY.md` §1 |
| Auto-update fails | `tauri.conf.json` updater section | `.github/workflows/release.yml` |

## 2. Code (planned layout — becomes real in M0-5 / M1)

| Area | Path | Owns |
|---|---|---|
| Core model | `crates/saveprism-core/` | SaveDoc, Codec trait, Recipe, Detector, patching, round-trip guard, safe write |
| Codecs | `crates/saveprism-codecs/` | json, layers, es3, gvas, rpgmaker, godot, playerprefs, renpy… |
| Keys | `crates/saveprism-keys/` | Key vault, key finder |
| CLI | `crates/saveprism-cli/` | `saveprism` command |
| Desktop host | `apps/desktop/src-tauri/` | Tauri commands → core |
| Desktop UI | `apps/desktop/src/` | React UI |
| Profiles | `profiles/` | Game profiles (TOML) |
| Fixtures | `fixtures/samples/` (locked) · `fixtures/expected/` | Golden corpus |

## 3. Error code prefixes

| Prefix | Crate / area |
|---|---|
| `SP-CORE` · `SP-DET` · `SP-IO` | `saveprism-core` (model · detector · safe write) |
| `SP-LAY` · `SP-JSON` · `SP-ES3` · `SP-GVAS` · `SP-RPGM` · `SP-GODOT` · `SP-PREFS` · `SP-RENPY` | `saveprism-codecs` |
| `SP-KEYS` | `saveprism-keys` |
| `SP-APP` · `SP-UI` | desktop host · UI |

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
