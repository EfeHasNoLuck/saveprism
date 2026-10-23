# NOW

**Goal:** Windows v0.1 — a local, polished replacement for saveeditonline.com: everything it can do, nothing less (more is fine
when it adds function, not complexity), plus customization and a way for AI tools to use it. Installable `.msi`/`.exe`.
The app's job is input → good output; no history screens (user, 2026-10-23: no "recent files").

## Done (verified)
- Core + CLI: RPG Maker MV/MZ, Easy Save 3 (plain, GZip, AES), Unreal GVAS, Palworld PlZ; round-trip guard; backup + atomic write.
- Desktop backend: open, browse, search, edit with type limits, save with backup and verify; RPG Maker sections with names from the
  game's `data/` folder.
- Desktop UI: start screen, Open file / Save as dialogs + drag-drop, settings modal, theme, background pattern, RPG Maker
  sections, Values list. Checked in the running app (dark + light, EN + TR) on 2026-10-22 and fixed: English by default,
  translatable hints/stat labels, contrast, page gutter, ES3 table, JSON search, blue star lattice, one-click theme toggle,
  names from a `data` folder copied next to the save. Checks: rust 3/3, web 3/3 (63 tests).
- 2026-10-23, user approved the look (`c94dad9`): dense card layout (entries in as many columns as fit, search + pager per
  list, character picker), Stitch colors only (violet changed + `old → new`, red error, cyan accent), editable JSON with
  Apply/Export .json, true/false choice, close guard. Web 3/3 (76 tests).
- Visual checks without computer-use: start `tauri dev` with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  and drive the WebView over CDP (patch `window.fetch` for `plugin:dialog|open|save`; `__TAURI_INTERNALS__.invoke` is read-only).
- 65 real saves for local tests in `fixtures/external/` (git-ignored; sources and licenses in `fixtures/external/SOURCES.md`).

## Todo (in order; widened by the user on 2026-10-23)
1. Polish, rest: backups screen (list the backups of the open save, restore one; writes files → reviewer); bulk edit per list
   ("set all to …", "all switches true/false"). (The odd busy-looking cursor was only in the dev window started from a
   background shell and is gone now; not an app bug.)
   Done 2026-10-23 (`f6b9fc5`): undo/redo history + buttons, Ctrl+Z/Y/O/S/Shift+S/F, opening overlay, About.
2. Parity with saveeditonline.com: gap list done in `docs/research/parity.md`. Top gaps: Ren'Py, RPG Maker XP/VX/VX Ace (Ruby
   Marshal, parse as data only), Twine/SugarCube, paste a save code instead of a file, Godot, other Unity saves, Wolf RPG, add a
   value from the card view, pick the game's data folder by hand when names are not found.
3. Customization: settings for density, font size, save mode (overwrite / save as), number of backups; color settings
   (changed, accent, error); own names for unnamed variables/switches, kept per game; a doc on adding a theme in code.
4. AI: MCP server (open, read, search, propose and apply edits; saving still makes a backup); "Ask AI" copies the selected
   values with a short context; docs.
5. Installer: `tauri build` works (test build `0.0.1`, 2026-10-23); `THIRD_PARTY_NOTICES.md` generated and bundled
   (`scripts/third-party-notices/generate.sh`). Left: version 0.1.0, README install section (user: not before the work is
   done), then ask the user to publish.

## Known gaps
- RPG Maker saves re-compress to different bytes: editable, not byte-exact. ES3 dicts with number keys don't open. Palworld PlZ2 world
  files are slow.
- JSON view/export on huge saves: the 50 MB cap is checked only after the whole text is built, and `unique_key` in
  `session/export.rs` is quadratic on many repeated keys; both run under the session lock (review, 2026-10-22).
- JSON Apply (review of `9849caa`, no critical/high): many edits in one huge object are O(N·M) under the session lock
  (core `set` scans keys); documents nested ≥127 levels cannot be applied (serde_json limit), message says 128.
- Real saves were pushed once in `bed2ed9` (private repo); ask the user before any history cleanup.

## Later (not now)
Linux/macOS, game showcase, plugins, web version, code signing, hex editing, recent-files list (user: not wanted).
Old plans: `docs/ROADMAP.md`, `docs/archive/TASKS.md`.
