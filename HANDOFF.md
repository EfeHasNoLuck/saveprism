# HANDOFF

**State (2026-10-27, ~20:00 TR):** building the maintainer's polish list `docs/design/polish-v0.1.md` (todo 7 in
`NOW.md`). Mockups approved (`.motor/mockups-1027/`, local): Settings layout **B**, start screen and "Where is my
save?" as drawn, About says "MIT License" (no "free"). Everything is local on `main` (not pushed: GitHub Actions minutes
are out; last pushed `4214786`). Weekly Claude limit was 85% → EVREN builds, Claude only merges, checks and does the
final visual check.

**Running (EVREN, started ~19:55 TR, background Bash of the session that wrote this):**
- `w4-settings` in `../saveprism-wt/w4-settings` (brief `.motor/w4-settings.md`, log `.motor/w4-settings.log`, report
  `../saveprism-wt/w4-settings/.motor/w4-settings-report.md`): Settings page B with General, Appearance, Saving & backups.
- `w4-start` in `../saveprism-wt/w4-start` (brief `.motor/w4-start.md`, report `.../w4-start/.motor/w4-start-report.md`):
  header, start screen + FAQ + slogan, "Where is my save?" redesign, custom scrollbar.
- If a job stopped without a report: find its session id in `~/.local/share/opencode/log/opencode.log` by the worktree
  path and resume with `bash .motor/resume_job.sh <name> <session> now` (the script assumes `w3-<name>` file names;
  adjust) or `opencode run --session <id>`.
- Merge each: `python -I .motor/merge_wt.py w4-<name> "<subject>"`, `cargo fmt --all`, kontrol rust + web,
  `cargo deny check`, `cargo test -p saveprism-desktop -- --ignored`, commit. Both touch `App.tsx`/`Header.tsx`/locales:
  expect conflicts, resolve by hand, then re-run the checks.

**Watching the jobs:** both run as background Bash of the OLD session (the one that wrote this). Do not archive it
until both reports exist or no `opencode.exe` is left in `tasklist`; then archive it. Watch with a background until-loop
on the two report files / `tasklist`.

**Next parts (briefs to write, EVREN, at most 2 at once):**
1. AI connection tab (stepper, per-client setup from `docs/ai.md`).
2. About tab (logo, version, MIT License, in-app Third-party notices list, What's new popup from release notes built
   in at build time) + first-launch tour.
3. Keyboard shortcuts tab: rebindable, ≤3 keys, ≤2 per action, conflict warning, reset, Windows-key note. Recording
   look decided: a small centered box like VS Code ("Press the new keys for <action>", keys shown large, Enter saves,
   Esc cancels, conflicts explained in the box).
4. Opening logo CSS/SVG animation, Explorer right-click "SavePrism ile çöz" / "Decode with SavePrism" (installer
   registry, under "Show more options"), theme file export/import, Community link.
Then one Claude visual check of everything (screenshots, dark/light, EN/TR), show the maintainer.
After the polish (maintainer: fine to do later): MCP tools `compare_saves`, `save_locations`, `open_code` (+ check
`open_save` uses the automatic key finder) + `docs/ai.md`. Then a fresh MSI for the maintainer to install and try,
then ask about publishing.

**User decisions:** focus engines Unreal/Unity/RPG Maker/Ren'Py; backups min 1 (default 10, 30 days, newest never
deleted); no contrast warning (auto text color); Win key not offered (note in the tab); GitHub Discussions as community
(website/donations later). Never touch the user's original saves.
**Gotchas:** Claude worktree subagents start at origin/main — brief them to `git merge --ff-only main` first. Leftover
folders the hook won't delete (user can delete by hand): `../saveprism-wt/satisfactory`,
`.claude/worktrees/agent-a0b6a9b8ab82d0764`, `agent-ab7b66a783b5d7813`, `agent-a5b61daf70f35e62e`,
`agent-a945e990b19f4bff1`, `agent-a15308e8a9ffff604`.
