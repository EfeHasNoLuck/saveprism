# Project: SavePrism — free, local, MIT desktop save editor (Tauri 2 + Rust core + React/TS)

@AGENTS.md

## The one goal right now
Windows v0.1 that a normal player can install: open an RPG Maker MV/MZ, Unity ES3 or Unreal GVAS save, change values, save with a
backup. Everything else waits in `NOW.md` "Later". Do not plan or build it.

## Files
- `NOW.md`: goal, what is done, short todo list, Later. The only planning file kept up to date.
- `HANDOFF.md`: one page, overwritten only when switching sessions.
- Reference, read only the part you need: `docs/architecture/MIMARI.md`, `docs/research/`.
- History, do not update: `docs/archive/`, `handoff/`, `.fabrika/`.

## How we work (user, 2026-10-22: "İş yükünü azalt, rahatlat, hedefe odaklan ve BAŞAR!")
- The main session writes small changes itself (about 20 lines or one file). Bigger pieces go to one coding subagent with a short brief
  in the prompt (no card files, no task lists).
- Subagents do the chores: research, drawing or writing inside a given frame, bulk/mechanical edits, and visual checks with screenshots
  (screenshots stay out of the main context; the subagent reports in text).
- Reviewer subagent only for code that writes files or handles passwords/keys.
- Evidence before "done": `bash .claude/araclar/kontrol.sh [rust|web]` output or what the user saw. Never claim unverified work.
- Ask the user only two things: how a screen looks (A/B with screenshots) and publishing a release / making the repo public.
  Everything else: decide, do, tell in one line.
- New dependency: fine with a permissive license (MIT, Apache-2.0, BSD, MPL-2.0 with NOTICE); mention it in one line. GPL family is banned.
- Git: commit straight to `main` and push to the private repo `huseyincancalti/saveprism` without asking. Public repo, releases and
  force-push only with the user's explicit OK.
- Chat with the user in short, plain Turkish, one question at a time. Everything in the repo is English (code, docs, UI, commits).

## Context
- Keep the main context lean: let a subagent take screenshots, read files partly, keep reports short.
- At a feature end or about 70% context: write `HANDOFF.md`, commit, push. Then:
  - user at the desktop → start the new session yourself ("Read HANDOFF.md and continue."), detach it, archive the old one.
  - user on Remote Control (the app refuses new sessions and clear there) → tell the user in one line:
    "/clear yaz, sonra şunu gönder: HANDOFF.md'yi oku ve devam et."
- Measure with `mcp__ccd_session_mgmt__get_usage`.

## Locked paths (agents never write; the hook enforces it)
`fixtures/samples/**`, `LICENSE`, `.claude/settings*.json`, `.claude/hooks/`, `.claude/araclar/`.
Real downloaded saves live in `fixtures/external/` (git-ignored, never pushed).

## Project facts
Node 22 · pnpm 11 · Python 3.11 · Rust 1.99 (MSVC) · WebView2. Windows only until v0.1 ships; Linux/macOS at the end.
