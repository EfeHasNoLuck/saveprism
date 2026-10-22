# AGENTS.md

Project: **SavePrism** — a universal, local, MIT-licensed desktop save editor (Tauri 2 + Rust core + React/TS).
Everything in the repo is English: code, docs, UI text, commit messages.

## Start every session

1. Read `NOW.md` (goal + todo) and `HANDOFF.md` if it exists.
2. Take the first open todo. Small steps: change, verify, commit, update `NOW.md`.

Locked paths (agents never write; the hook enforces it): `fixtures/samples/**`, `LICENSE`, `.claude/settings*.json`, `.claude/hooks/`, `.claude/araclar/`.
Evidence command: `bash .claude/araclar/kontrol.sh [rust|web]` (exit 0 pass · 1 fail · 3 nothing to run).

## Non-negotiable product rules

1. Never write a save file without a backup and an atomic replace (temp file → replace). Backups live in the app data dir, never next to the save.
2. Round-trip guard: decode → encode must reproduce the original bytes before editing is enabled; otherwise the file is read-only with a warning.
3. Never execute content from save files: no Python unpickling, no Ruby eval, no BinaryFormatter deserialization. Parse to a tree only.
4. Parsers handle untrusted input: size/depth limits, decompression-bomb guard, no panics (fuzzed).
5. License hygiene: no GPL/AGPL/LGPL/SSPL code or dependencies in shipped binaries (cargo-deny). Apache-2.0 / MPL-2.0 allowed with NOTICE.
6. Clean room: never copy code, regexes, tables or key lists from closed-source sites or bundles. Write a behavioral spec; implement from the spec plus public docs.
7. Keys are derived only from game files the user owns, and stored with evidence (source file, method, date).

## Conventions

- Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`).
- Commit to `main` and push to the private repo freely. Making the repo public, publishing a release or force-pushing needs the maintainer's explicit approval.
- Rust: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`. TS: typecheck, lint, `vitest`.
- Real downloaded saves stay in `fixtures/external/` (git-ignored). Never commit them.
