# HANDOFF

**State:** v0.1 was widened by the user on 2026-10-23 (NOW.md goal: a local saveeditonline.com with nothing less, plus polish,
customization and an AI bridge). The user approved the dense card layout with the Stitch colors. Last code commit `f6b9fc5`.

**Works (with evidence)**
- `bash .claude/araclar/kontrol.sh rust` 3/3 and `kontrol.sh web` 3/3 (80 tests) after `f6b9fc5`, 2026-10-23.
- Undo/redo, shortcuts, opening overlay and About checked in the running app over CDP by the coding subagent.
- `pnpm -C apps/desktop tauri build` produced `SavePrism_0.0.1_x64-setup.exe` and the `.msi` (2026-10-23, before `f6b9fc5`).
- `THIRD_PARTY_NOTICES.md`: 244 crates + 76 bundled npm packages, no GPL family (`d412a60`).
- External engines answer from this repo: Codex `gpt-6-luna`, agy, opencode `evren/deepseek-v4.1-flash`, `cursor-agent.cmd`.
  agy did the parity research well; EVREN failed a file-writing task twice (see memory `external-engines`).

**Broken / not checked**
- `CLAUDE.md` is changed but not committed (new goal line + "chores go to external engines first"): the auto-mode check blocks me
  from committing it; the user commits it.
- The odd mouse cursor the user saw was only in the dev window started from a background shell, and is gone now (user,
  2026-10-23). The app sets no CSS cursor. No action unless it shows in the installed app.
- The JSON Apply review left mediums in NOW.md "Known gaps".
- Visual checks: `scripts/cdp/README.md` (start `tauri dev` with the WebView2 debug port, drive it over CDP).

**Next step**
NOW.md todo 1: the backups screen (list + restore, reviewer for the restore write) and bulk edit
per list. Chores to external engines first (agy for research, EVREN only for repo-local read-mostly work), Claude subagents for
UI. The user hates being asked step by step and wants plain short Turkish.
