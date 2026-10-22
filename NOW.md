# NOW

**Goal:** Windows v0.1 — open an RPG Maker MV/MZ, Unity ES3 or Unreal GVAS save, change values, save with a backup. Installable `.msi`/`.exe`.

## Done (verified)
- Core + CLI: RPG Maker MV/MZ, Easy Save 3 (plain, GZip, AES), Unreal GVAS, Palworld PlZ; round-trip guard; backup + atomic write.
- Desktop backend: open, browse, search, edit with type limits, save with backup and verify; RPG Maker sections with names from the
  game's `data/` folder.
- Desktop UI: start screen, Open file / Save as dialogs + drag-drop, settings modal, theme, background pattern, RPG Maker
  sections, Values list. Checked in the running app (dark + light, EN + TR) on 2026-10-22 and fixed: English by default,
  translatable hints/stat labels, contrast, page gutter, ES3 table, JSON search, blue star lattice, one-click theme toggle,
  names from a `data` folder copied next to the save. Checks: rust 3/3, web 3/3 (63 tests).
- Visual checks without computer-use: start `tauri dev` with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`
  and drive the WebView over CDP (patch `window.fetch` for `plugin:dialog|open|save`; `__TAURI_INTERNALS__.invoke` is read-only).
- 65 real saves for local tests in `fixtures/external/` (git-ignored; sources and licenses in `fixtures/external/SOURCES.md`).

## Todo (in order)
1. User tries it once (open → change → save) and approves the look.
2. Windows installer: `pnpm -C apps/desktop tauri build` → `.msi`/`.exe`; README install section; `THIRD_PARTY_NOTICES`.
3. Ask the user to publish the release.

## Known gaps
- RPG Maker saves re-compress to different bytes: editable, not byte-exact. ES3 dicts with number keys don't open. Palworld PlZ2 world
  files are slow.
- JSON view/export on huge saves: the 50 MB cap is checked only after the whole text is built, and `unique_key` in
  `session/export.rs` is quadratic on many repeated keys; both run under the session lock (review, 2026-10-22).
- Real saves were pushed once in `bed2ed9` (private repo); ask the user before any history cleanup.

## Later (not now)
Linux/macOS, AI bridge, MCP server, game showcase, plugins, web version, code signing, hex and JSON editing, game profiles.
Old plans: `docs/ROADMAP.md`, `docs/archive/TASKS.md`.
