# NOW

**Goal:** Windows v0.1 — open an RPG Maker MV/MZ, Unity ES3 or Unreal GVAS save, change values, save with a backup. Installable `.msi`/`.exe`.

## Done (verified)
- Core + CLI: RPG Maker MV/MZ, Easy Save 3 (plain, GZip, AES), Unreal GVAS, Palworld PlZ; round-trip guard; backup + atomic write.
- Desktop backend: open, browse, search, edit with type limits, save with backup and verify; RPG Maker sections with names from the
  game's `data/` folder.
- Desktop UI (machine-checked, **not yet looked at in the running app**): start screen, Open file / Save as dialogs + drag-drop,
  settings modal, theme, background pattern, RPG Maker sections, Values list. Checks: rust 3/3 (322 tests), web 3/3 (60 tests).
- 65 real saves for local tests in `fixtures/external/` (git-ignored; sources and licenses in `fixtures/external/SOURCES.md`).

## Todo (in order)
1. Look at the running app (`pnpm -C apps/desktop tauri dev`) through a subagent with screenshots, dark + light: open
   `fixtures/external/rpgmaker-mz/journey-of-elaina/file0.rmmzsave` (names should show), set Gold to 100000000 (red hint), Save as,
   open `fixtures/synthetic/es3/progress.es3` (Values table). Fix what looks wrong.
2. User tries it once (open → change → save) and approves the look.
3. Windows installer: `pnpm -C apps/desktop tauri build` → `.msi`/`.exe`; README install section; `THIRD_PARTY_NOTICES`.
4. Ask the user to publish the release.

## Known gaps
- RPG Maker saves re-compress to different bytes: editable, not byte-exact. ES3 dicts with number keys don't open. Palworld PlZ2 world
  files are slow.
- Real saves were pushed once in `bed2ed9` (private repo); ask the user before any history cleanup.
- Small UI leftovers: duplicate switch/info-tip components in `src/editor/ui/` vs `src/components/ui/`.

## Later (not now)
Linux/macOS, AI bridge, MCP server, game showcase, plugins, web version, code signing, hex and JSON editing, game profiles.
Old plans: `docs/ROADMAP.md`, `docs/archive/TASKS.md`.
