# HANDOFF

**State:** process simplified (CLAUDE.md "How we work", NOW.md is the single plan). Desktop UI is built and machine-checked but nobody has
looked at the running app yet. Last code commit `5922b85`.

**Works (with evidence)**
- `bash .claude/araclar/kontrol.sh rust` 3/3 (322 tests), `kontrol.sh web` 3/3 (60 tests), on 2026-10-22.
- Reviewer: no critical/high findings on the desktop backend and UI.

**Broken / not checked**
- The running app was never looked at: computer-use access to `saveprism-desktop.exe` was denied while the user was on the phone, and the
  browser pane could not reach `http://localhost:1420`. Next time ask for computer-use access while the user is at the desktop, and let a
  subagent take the screenshots.
- The context-warning hook `.claude/hooks/baglam_uyari.sh` still warns at 55% (locked file; only the user can change it to ~70%).

**Next step**
NOW.md todo 1: start `pnpm -C apps/desktop tauri dev`, check the app through a subagent with screenshots (dark + light), fix what looks
wrong, then show the user.
