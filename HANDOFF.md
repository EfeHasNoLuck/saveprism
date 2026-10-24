# HANDOFF

**State (2026-10-24, evening):** NOW.md todos 1-4 are done; todo 5 waits only for the user's OK to publish.
Last code commit `910e293` (version 0.1.0). Working tree clean except git-ignored `fixtures/external/` and `.motor/`.

**Works (with evidence)**
- MCP server (`e553b88`): `SavePrism.exe mcp` (dev: `target/debug/saveprism-desktop.exe mcp`) speaks MCP over stdio,
  16 tools over the same `Session` the UI uses (`apps/desktop/src-tauri/src/mcp/`). Checked end to end on a copy of a
  real RPG Maker MV save: open, game view, propose edits, save (backup made, verified), reopen shows the values, a file
  changed on disk is refused (SP-APP-023). The release exe (window subsystem) answers over stdio too. A Claude subagent
  reviewed it; fixed: save_changes writes only the open file or a new file next to it, empty filter, 80,000-character
  result cap, `pendingCount`, panic closes the save, non-UTF-8 lines, annotations, `args_os`.
- Ask AI on every list card + Settings → AI tools → Copy setup (`f4fe817`), checked in the running app by a subagent
  (dark/light, EN/TR, clipboard text and toasts). Plain-JSON number limits left out of the note (`1c5fb17`).
- Docs: `docs/ai.md` (setup for Claude Desktop, Claude Code, Cursor, VS Code; tool table; safety). README rewritten.
- Installers: `target/release/bundle/nsis/SavePrism_0.1.0_x64-setup.exe`, `.../msi/SavePrism_0.1.0_x64_en-US.msi`;
  the release exe opens its window titled SavePrism.
- `bash .claude/araclar/kontrol.sh rust` 3/3 and `web` 3/3 after the MCP merge.

**Broken / not checked**
- Not tried with a real MCP client (Claude Desktop/Code); only hand-written JSON-RPC over pipes.
- The installers were built but not installed and run.
- Layout gaps found by the visual check are in NOW.md Known gaps (list header wraps at 1000 px; TR bottom bar at 900 px).

**Next step:** ask the user whether to publish v0.1.0 (draft GitHub release on the private repo with both installers,
or make the repo public first, which needs a history cleanup of the real saves in `bed2ed9` and their OK).

**Engines:** EVREN deepseek-v4.1-flash wrote the MCP server and Ask AI well; its fix run stopped midway (a leftover
`opencode.exe` from a stopped task kept editing; see memory `external-engines`). Stale branch `wt/mcp` is left (the hook
refuses `git branch -D`); it is superseded by `e553b88`. Weekly Claude quota was ~80% used on 2026-10-24.
