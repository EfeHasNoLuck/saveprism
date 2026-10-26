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
- 2026-10-24: every screen merged overnight checked in the running app over CDP (dark/light, EN/TR); found and fixed: cut-off
  big numbers, missing read-only values, duplicate React keys on Ren'Py aliases, untranslated count message, TR card
  header wrap, dimmed read-only toggles (`422a4bb` and the next commit). Re-checked after the fixes.
- 2026-10-24 real saves (fetched by EVREN, local only, listed in `fixtures/external/SOURCES.md`): TyranoScript (2),
  zip-packed JSON (3: Shapez 2, House Flipper 2), Pokémon Gen 3 (3; fixed: 16-bit Pokémon checksum, FRLG bag 999,
  unchanged data kept), The Witcher 3 (3; fixed: variable table may reach the footer), Ren'Py, RPG Maker XP, Godot
  JSON: all open with an exact round trip. None found for VX/VX Ace, 2000/2003, Wolf RPG, QSP, Naninovel, `.sol`,
  SugarCube.
- 2026-10-25: Open folder (`7df9b4b`), translated error toasts + Compare/Where-is-my-save polish (`9501734`) and the
  password field focus (`b04c4ac`) re-checked in the running app over CDP (dark/light, EN/TR): all pass.
- 2026-10-25: logo pack from the maintainer in `docs/brand/`; app icons regenerated from `SavePrism-icon-navy.svg`;
  README shows the logo.
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
     `escape()` and `encodeURIComponent` styles; real saves from 2 games open with an exact round trip; the low 0.35 score only ranks candidates and nothing else claims the file), .NET BinaryFormatter data (Unity `.dat` saves, RAGS `.rsv`;
     parsed as data only, never deserialized), Unity PlayerPrefs as a registry `.reg` export (ints,
     Unity's 8-byte `hex(4)` floats, UTF-8 strings; regedit line wrapping kept), Godot ConfigFile text and `store_var` binary (Godot 3 and 4 type
     tables checked against the docs; packed number arrays read-only; encrypted GDEC files refused), SQLite saves (`rusqlite` bundled, MIT; edits are parameterized
     UPDATEs on an in-memory copy; tables with triggers, views and WITHOUT ROWID tables read-only), saves packed in zip archives (real Shapez 2 and House Flipper 2 saves open exact; one payload entry
     rewritten, the rest kept byte-identical; gzip was already a layer), Wolf RPG Editor `.sav` (unencrypted; switches, variables and the
     Variable DB; layout from the MIT WolfSave project, save part order from one source only; encrypted saves refused),
     QSP `.sav` (UTF-16 and legacy single-byte saves, read as Windows-1251; KOI8-R saves are not told apart),
     Naninovel save slots with a custom-variables view (plain, gzip, deflate; compressed ones re-compress to other bytes).
   - Formats left: none. The Witcher 3 `.sav` done 2026-10-24 (orens and item quantities; LZ4 chunks via `lz4_flex`, MIT;
     layout from public MIT projects; undecoded record shapes read-only).
   - Done 2026-10-24 (`wt/cards`; checked in the running app 2026-10-24): add items/weapons/armors to RPG Maker lists
     (MV/MZ and XP/VX/VX Ace), add/remove values in JSON, ES3 and SugarCube lists, undo for added/removed entries,
     "Choose data folder…" for names. Its core/session part was written by a Claude subagent through Bash after the
     project hook refused Edit/Write outside the project; finished by EVREN; merge breaks fixed by Claude.
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
3. Customization. Done 2026-10-24 (`45d202f`, checked in the running app, dark/light, EN/TR): Text size Small/Normal/Large
   (also the density setting: cards and grids scale with it), color presets for changed/accent/error (full tonal sets,
   contrast-tested), backups to keep 1-50 (only the saved file's folder is trimmed; review fixed), save mode (was there).
   Own names for unnamed RPG Maker variables/switches, kept per game (`a900b54`, checked in the running app) and
   `docs/themes.md`. Todo 3 is done.
4. AI. Done 2026-10-24: `saveprism-desktop.exe mcp` is an MCP server over stdio (`e553b88`; 16 tools over the same Session:
   open, game view by section/filter/page, list, search, propose edits in memory, add/remove, undo, pending changes,
   save with backup + verify only to the open file or a new file next to it, backups; 80,000-character result cap;
   a panic closes the save unwritten). Saving over a file that changed on disk is refused (SP-APP-023) in the app too.
   Checked end to end on a real RPG Maker MV save copy (edit, save, backup, reopen, changed-file refusal); reviewed by a
   Claude subagent, findings fixed. The release exe (window subsystem) answers over stdio too. A real client works:
   Claude Code (`claude -p --mcp-config`, sonnet) opened a copy of a real MV save, set gold 200 → 4321 and an item
   1 → 9, saved with a backup; values re-checked. A byte order mark before a line (Windows PowerShell pipes) is ignored.
   "Ask AI" on every list card and Settings → AI tools → Copy setup (`f4fe817`, checked in the running app, dark/light,
   EN/TR); its toast tells chatbot users what to do (`304baf0`). Docs: `docs/ai.md`.
5. Installer. 2026-10-24: version 0.1.0, README rewritten (install, all formats, AI), notices regenerated (now with
   rusqlite and lz4_flex), `tauri build` → `SavePrism_0.1.0_x64-setup.exe` (4.0 MB) and `SavePrism_0.1.0_x64_en-US.msi`
   (6.5 MB) in `target/release/bundle/`. The MSI unpacked without installing (`msiexec /a`) holds
   `PFiles\SavePrism\saveprism-desktop.exe` (the exe name users see), the DLL and the notices; that exe opens its
   window and answers `mcp`. Not installed for real yet (no Windows Sandbox on the dev PC). Publishing waits until the
   app is proven; do not ask the user before that.
6. Full parity with saveeditonline.com's `/formats` page (about 70 formats; the home page we matched lists 13). REA
   analysis 2026-10-24: `docs/research/saveeditonline-behavior.md` (section 9 = ranked gaps). The site uploads every save
   to its server, keeps it a day, limits free use per IP, has no backup/undo/round-trip check.
   - Wave 1 (EVREN, started 2026-10-24, worktrees `../saveprism-wt/{renpysign,layers,text,binobj,ui}`): Ren'Py saves
     signed with the player's own key; BZip2/Zstd/XZ/LZMA/LZ4/Brotli/TAR layers + hex/URL-encoded codes;
     XML/INI/properties/TOML/YAML/CSV/plist kept byte-exact; pickle files, MessagePack, CBOR, BSON, NBT;
     "Where is my save?" help and "Compare with another save".
     All merged 2026-10-25, rust 3/3 + web 3/3 after each: Ren'Py signing with the player's key (`13505dc`; Ren'Py's
     own `ecsign.py` verified our signatures) and its review follow-ups (`12dfc08`: unchanged signed persistent keeps
     its signature, unsigned files never signed, key read only for signed Ren'Py files, 64 KiB cap, regular files only,
     redacted key text, Ren'Py's own key folder order; reviewed, no critical/high), MessagePack/CBOR/BSON/NBT
     (`095ce2e`), BZip2/Zstd/XZ/LZMA/LZ4/Brotli/TAR layers + hex/percent codes (`fe4f891`), "Where is my save?" and
     "Compare with another save" (`752ae91`), XML/INI/properties/TOML/YAML/CSV/plist byte-exact (`bede97c`; notices
     regenerated). Not yet: a real Ren'Py game test; pickle files outside Ren'Py (check whether wave 1 covered them).
     Review lows left (renpyfix): an unchanged signed *save* (not persistent) is re-signed on write; any `Ren'Py Data`
     ancestor folder is trusted for keys; key buffer not zeroized.
   - No value finder / memory-scanner for unknown binaries (user, 2026-10-25: Cheat Engine already does that; it would
     widen the scope).
   - Wave 2 running (EVREN, started 2026-10-25 ~03:50, worktrees `../saveprism-wt/{unlock,keys,unity,enc}`, briefs
     `.motor/w2-*.md`): content-based layers + XOR + LZF; key finder for ES3 passwords from the game's files; Odin + ES2;
     text encodings. Terraria, Adventure Creator, Pixel Crushers and the Gwent view come after.
     Merged 2026-10-25 (rust 3/3 + web 3/3): layers by content + XOR auto-break + LZF (`9d50e17`), Odin Serializer
     binary (`73ebf7a`; Easy Save 2 not done: no public byte layout, clean room), text encodings for text saves,
     RPG Maker 2000/2003 and QSP (`9ef817e`), ES3 password found in the game's own files with evidence (`60a36d2`;
     reviewed twice, no critical/high; password dialog checked in the running app 2026-10-25, dark/light, EN/TR:
     Pick the game folder, "No password found in that folder.", typed text kept, wrong/right password, Esc; the field
     now keeps focus after a wrong try or a folder pick, not re-checked in the app).
   - Merged 2026-10-25 afternoon (rust 3/3, web 3/3, cargo deny ok): visual-check polish + translated error messages
     (`9501734`; not re-checked in the app), KiriKiri TJS2 + GameMaker ds_map/ds_list (`fabf118`; Pixel Game Maker MV
     saves are plain JSON), PS1/PS2 memory cards, .psu, SharkPort (`033a67b`; unknown game data opens as read-only
     `format.raw`). Also fixed: two fuzz memory blowups (NRBF null runs, WOLF field-less types) and three dependency
     advisories (`1cabd55`).
   - Terraria merged 2026-10-25 (`e770de5`, review fixes `2d08633`; rust 3/3, web 3/3, cargo deny ok): `.plr` AES key
     found in the player's own `Terraria.exe` (8-character `#US` strings only, stored with evidence), player values and
     inventory; `.wld` header flags and seed editable, name/id/size read-only (footer and tiles depend on them);
     implausible values open read-only. Synthetic files only: no real `.plr`/`.wld` tested yet, item id cap 5600 is a
     guess above 1.4.4's count.
   - Merged 2026-10-26 (rust 3/3, web 3/3, cargo deny ok): BioWare GFF V3.2 (numbers in place, strings only at the
     same length), Dragon Age GFF V4 read-only, ERF/MOD/SAV/HAK containers (`29f6677`); Adventure Creator (parts
     split at `||`/`|ROOMDELIMITER|`, each opened as JSON/XML/Base64 NRBF) and Pixel Crushers (nested JSON and
     Dialogue System Lua as data; encrypted refused) (`4214786`). zstd 0.14.3 for three new advisories (`dcfa9c1`).
     AGS (header/description editable, state read-only), ONScripter (global.sav numbers and Shift-JIS strings,
     slot description; from a behavioral spec, GPL source not copied), Emuera binary saves (`ccd64b1`).
     FromSoftware BND4 (raw and DCX-zlib entries editable; Oodle/encrypted entries read-only, no key), Blizzard MPQ
     (stored entries editable; compressed read-only), Brickadia `.brz` + `.brs` (same-length stored blobs; `blake3`,
     CC0-1.0 OR Apache-2.0). Synthetic files only.
   - GitHub Actions minutes ran out 2026-10-26: commits stay local until next month (last pushed `4214786`).
   - Merged 2026-10-27 (rust 3/3, web 3/3, cargo deny ok): protobuf (schema-less, exact re-encode only), Bitser, SCS
     `.sii` text (binary/encrypted read-only), LZip, Base64URL, Snes9x snapshots (SRA editable), RetroArch states
     (read-only), `.srm` SRAM (`5e5a3e5`); LevelDB/RocksDB read-only, LiteDB v4/v5 fixed-size values in place with
     `_id` and indexed fields read-only (`dfc4903`; `snap`, BSD-3-Clause). Synthetic files only.
   - Merged 2026-10-27 (rust 3/3, web 3/3, cargo deny ok): the real Witcher 3 Gwent layout (`CR4GwintManager`,
     collections, decks, selected faction; only `numCopies` editable; checked on the 3 real saves) (`097915f`);
     AliceSoft System 3.x/4 editable, Artemis read-only, Construct 2/3 labels (`5a09adf`). Not possible (no public
     save layout, not claimed): Smile Game Builder `.sgs`, LiveMaker, Visionaire `.vsf`, Construct Classic, Aoi.
   - Merged 2026-10-27 (rust 3/3, web 3/3, cargo deny ok): Godot `.tres` text and `.res`/RSCC binary resources
     (`4a0ed89`; ConfigFile and resources share one Variant text parser); Gwent card list from the player's own
     game bundles (`78dfdc9`: `POTATO70` reader, `w3_cards` command with a pick-the-folder state; names from the XML
     `name` attribute because `.w3strings` is described only in GPL code). Synthetic files only.
   - ES3 dictionaries with bare number/bool keys (`3a2bc9a`): 2 real Supermarket Simulator saves now open; a sweep of
     all 88 local real saves showed no regression (3 ES3 files went from semantic to exact).
   - Gwent collection view merged in two layouts (`6092cc4`; checked in the running app on a copy of a real save:
     139 cards, 13 leaders, 5 decks, edit + undo, folder pick, TR, dark/light). Repeated Witcher 3 record names are
     now numbered (`Name#2`), so every card count (and repeated inventory record) is editable. Waiting for the
     user's A/B pick (`GWENT_LAYOUT` in `apps/desktop/src/editor/gwent.ts`, now "a"; screenshots in
     `.motor/gwent-ab/`), then delete the other layout.
   - Running: GVAS struct type hints found automatically (EVREN `gvashints`; a real UE 4.27 save still fails).
     Left from `/formats` after these: Easy Save 2 (no public layout), Oodle/Kraken (no permissive decoder),
     "SSSoftware Serialize" (unknown format).
   - Wave 2, first: auto-unlock (user idea, 2026-10-24): every layer tried by content whatever the extension; simple XOR
     obfuscation broken automatically from the expected start of the inner format (`{`, `<?xml`, `PK`...); a key finder
     that reads candidate passwords from the game's own files (Unity `Assembly-CSharp.dll` strings, IL2CPP
     `global-metadata.dat`, engine config files) and tries them on the save, with the evidence kept (AGENTS.md rule 7).
     It looks for the game folder itself (next to the save, Steam libraries); only when nothing is found does it ask the
     player to pick the game folder. No notice when it works: the file just opens (user, 2026-10-25: keep backend work
     invisible). Keys that are not on the PC (online, DRM) stay out of scope. Then Unity Easy Save 2, Odin, Adventure
     Creator, Pixel Crushers; Terraria `.plr`/`.wld`; text encodings (Shift-JIS, GB18030, CP949, UTF-16/32); Witcher 3
     Gwent view.
   - Wave 3: VN/indie engines (AGS, Artemis, Pixel Game Maker MV, Smile Game Builder, KiriKiri, NScripter, AliceSoft,
     LiveMaker, Emuera, GameMaker, Construct Classic, Visionaire); containers (BioWare ERF/GFF, BND4, MPQ, PS1/PS2 memory
     cards, emulator states, SharkPort); LevelDB/RocksDB/LiteDB; a local "unsupported file" report. Then the same REA
     pass on saveeditor.online.
   - Wave 3, 2026-10-25: local unsupported-file report done (Copy file report on the SP-DET-001 toast).

## Known gaps
- A zip save with several JSON entries opens only the largest (Shapez 2: `local-player.json`, not `research.json` or
  `savegame.json`; House Flipper 2: `parcel.json`, not `header.json`).
- RPG Maker saves re-compress to different bytes: editable, not byte-exact. ES3 dicts with number/bool keys (`3a2bc9a`) and object keys (`3a40520`) open; all 15 local real `.es3` files pass. Palworld PlZ2 world
  files are slow.
- JSON view/export on huge saves: the 50 MB cap is checked only after the whole text is built, and `unique_key` in
  `session/export.rs` is quadratic on many repeated keys; both run under the session lock (review, 2026-10-22).
- JSON Apply (review of `9849caa`, no critical/high): many edits in one huge object are O(N·M) under the session lock
  (core `set` scans keys); documents nested ≥127 levels cannot be applied (serde_json limit), message says 128.
- A save opened through a symbolic link: saving or restoring replaces the link with a plain file (the core atomic replace
  does not keep links), and its backups list is then empty (Codex review, 2026-10-23). Rare for game saves.
- Real saves were pushed once in `bed2ed9` (private repo); ask the user before any history cleanup.
- Layout (visual check, 2026-10-24): at 1000 px a list header with search, Add, Ask AI and Set all wraps to a second row
  (fits from 1100 px); at 900 px with pending edits in Turkish the bottom bar overflows sideways (945 px).
- Any unknown file whose size is a multiple of 16 bytes opens the "This file is encrypted" password dialog (it could be
  Easy Save 3 AES); a cut-off copy of a known format says "SavePrism does not know this file yet" (visual check,
  2026-10-25). After a wrong password the field is emptied.
- "Copy file report" (`585479e`) is tested only in Rust and vitest, not yet clicked in the running app.
- Light theme: the Compare "only here/there" chips barely differ from the row background (text contrast is fine).
- Ask AI on the Values card copies only the top-level values (as its search does), not values inside groups.
- An MCP edit does not reach a SavePrism window with the same file open; that window refuses to save over it
  (SP-APP-023) until the file is opened again.

## Later (not now)
Linux/macOS, game showcase, plugins, web version, code signing, hex editing, recent-files list (user: not wanted).
Old plans: `docs/ROADMAP.md`, `docs/archive/TASKS.md`.
