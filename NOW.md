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
1. Polish: done 2026-10-23. Undo/redo, shortcuts, opening overlay, About (`f6b9fc5`); Backups dialog in the bottom bar (list
   the backups of the open save, restore one: the current file is backed up first, the backup must open as a save, written
   bytes are checked; Codex review: no critical/high, symlink backup fixed); "Set all" per RPG Maker list (number box, switches
   all on/off; filtered entries; one undo step; all or nothing). Checked in the running app over CDP (dark/light, EN/TR).
2. Parity with saveeditonline.com: everything the site opens must open and edit here. Full list (site home page checked
   2026-10-24; `docs/research/parity.md` missed the rows marked *new*). A row is done only when a real or synthetic save opens,
   edits and saves with the round-trip guard passing.
   - Done: RPG Maker MV/MZ, Easy Save 3, Unreal GVAS, Palworld, JSON/text, SugarCube, paste/copy code, Ruby Marshal codec,
     RPG Maker XP/VX/VX Ace sections (gold, items, weapons, armors, actors, variables, switches, names from a plain `Data`
     folder; 2026-10-24, synthetic saves only; XP shows SP as "MP"), Flash `.sol` (AMF0/AMF3), Pokémon Gen 3 (money, coins, bag, party EXP/IV/EV/stats; 2026-10-24, synthetic only), Java serialization streams (data only; fixtures
     from a real JDK writer), RPG Maker 2000/2003 `.lsd` with the sections view and names from
     `RPG_RT.ldb` (ids checked against liblcf; non-ASCII text edits are refused only at save), Ren'Py `.save`
     and `persistent` (pickle parsed as data only; an edited save gets an empty `signatures` entry so the game asks the
     player to trust it; a signed persistent opens read-only; persistent re-compresses to different bytes), TyranoScript `.sav` (URL-escaped JSON, both
     `escape()` and `encodeURIComponent` styles; detection confidence only 0.35), .NET BinaryFormatter data (Unity `.dat` saves, RAGS `.rsv`;
     parsed as data only, never deserialized), Unity PlayerPrefs as a registry `.reg` export (ints,
     Unity's 8-byte `hex(4)` floats, UTF-8 strings; regedit line wrapping kept), Godot ConfigFile text and `store_var` binary (Godot 3 and 4 type
     tables checked against the docs; packed number arrays read-only; encrypted GDEC files refused), SQLite saves (`rusqlite` bundled, MIT; edits are parameterized
     UPDATEs on an in-memory copy; tables with triggers, views and WITHOUT ROWID tables read-only), saves packed in zip archives (one payload entry
     rewritten, the rest kept byte-identical; gzip was already a layer).
   - Formats left: Wolf RPG `.sav`, QSP,
     Naninovel view, The Witcher 3 `.sav` (*new*).
   - Implemented 2026-10-24 (uncommitted): add/remove values from the card view (RPG Maker inventory
     add/remove, JSON/ES3 add/remove, SugarCube variables), pick the game's data folder by hand.
     Rust and web checks pass; a visual check in the running app is still pending.
   Done 2026-10-23: Twine SugarCube 2 story view (current-moment variables edited at their last writer in `state.delta`;
   partly changed values and revive tuples read-only; synthetic fixtures in `fixtures/synthetic/sugarcube/`). Not yet:
   2.37 saves with `state.history` (no deltas) open as the plain Values list; no real game save tested.
   Format notes for SugarCube/TyranoScript/Ren'Py: `docs/research/formats-sugarcube-tyrano-renpy.md`.
   Done 2026-10-23: Paste code (start screen, header, Ctrl+V on the start screen; the code is kept as
   `%LOCALAPPDATA%\SavePrism\pasted\code-<ms>.txt`, newest 10) and Copy code (pasted sessions' main action; "Copy as save
   code" for text formats; edited values checked first). Codex review: one finding (link in the pasted folder) fixed.
   Done 2026-10-23: Ruby Marshal codec `format.ruby-marshal` (RPG Maker XP `.rxdata`, VX `.rvdata`, VX Ace `.rvdata2`):
   data tree only, links never expanded, exact round trip, scalar edits; fuzz target added (not run: no cargo-fuzz here).
   Codex review: 2 mediums fixed (composite hash keys now read-only inside; deep wrappers). Synthetic fixtures only (no
   openly licensed real saves found). Next: RPG Maker XP/VX/VX Ace sections (gold, items, actors, variables, switches) on
   this tree; Map keys show as indices in the generic Values list.
   On the dev PC something outside SavePrism alters clipboard text longer than ~1,400 characters (PowerShell
   `Set-Clipboard` too), so a long copied code pasted back there fails; not an app bug.
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
- A save opened through a symbolic link: saving or restoring replaces the link with a plain file (the core atomic replace
  does not keep links), and its backups list is then empty (Codex review, 2026-10-23). Rare for game saves.
- Real saves were pushed once in `bed2ed9` (private repo); ask the user before any history cleanup.

## Later (not now)
Linux/macOS, game showcase, plugins, web version, code signing, hex editing, recent-files list (user: not wanted).
Old plans: `docs/ROADMAP.md`, `docs/archive/TASKS.md`.
