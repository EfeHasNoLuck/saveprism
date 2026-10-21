# AGENTS.md

Project: **SavePrism** — a universal, local, MIT-licensed desktop save editor (Tauri 2 + Rust core + React/TS).
Planning docs are in Turkish; code, identifiers and commit messages are in English.

## Start every session

1. Read `NOW.md` (state + the single next action) and the newest `handoff/HANDOFF-*.md` if one exists.
2. Read the matching milestone in `docs/ROADMAP.md`.
3. Work on ONE micro-task. Multi-task work runs through the factory workflow (`fabrika` skill, guide: `docs/FABRIKA.md`):
   plan → KAPI-1 (user approves the wave) → code → review → verify → KAPI-2 (user checks it in the app/game).
4. When done: update `NOW.md` ("Son kazanım", "Sıradaki tek adım"), then commit.

Locked paths (agents never write; the hook enforces it): `fixtures/samples/**`, `LICENSE`, `.claude/settings*.json`, `.claude/hooks/`, `.claude/araclar/`.
Evidence command: `bash .claude/araclar/kontrol.sh [rust|web]` (exit 0 pass · 1 fail · 3 nothing to run).

## Output style (i-have-adhd rules)

- Lead with the next action. Number multi-step work. End with one concrete next step.
- Restate where we are (milestone/task). Give concrete time estimates.
- Max 5 items per list group. No preamble, no recap, no closing pleasantries.
- Tangents go to the "Park alanı" in `NOW.md`, not into the current task.

## Non-negotiable product rules

1. Never write a save file without a backup and an atomic replace (temp file → replace). Backups live in the app data dir, never next to the save.
2. Round-trip guard: decode → encode must reproduce the original bytes before editing is enabled; otherwise the file is read-only with a warning.
3. Never execute content from save files: no Python unpickling, no Ruby eval, no BinaryFormatter deserialization. Parse to a tree only.
4. Parsers handle untrusted input: size/depth limits, decompression-bomb guard, no panics (fuzzed).
5. License hygiene: no GPL/AGPL/LGPL/SSPL code or dependencies in shipped binaries (cargo-deny). Apache-2.0 / MPL-2.0 allowed with NOTICE.
6. Clean room: never copy code, regexes, tables or key lists from closed-source sites or bundles. Write a behavioral spec; implement from the spec plus public docs.
7. Keys are derived only from game files the user owns, and stored with evidence (source file, method, date).

## Conventions

- Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`) — feeds release-please.
- Feature branches `feat/<topic>`; `main` stays releasable.
- Push, create the GitHub repo, make it public or publish releases **only with explicit maintainer approval**.
- Rust: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`. TS: typecheck, lint, `vitest`.
