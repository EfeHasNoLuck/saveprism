# K-3a report

Not compiled or run (no Bash). CEO: `cargo fmt --all`, `bash .claude/araclar/kontrol.sh rust`, `pnpm -C apps/desktop typecheck`.

## Changed files
- `apps/desktop/src-tauri/src/session/game.rs` (new): `Field`, `Entry`, `Actor`, `RpgMakerView`, `ValueRow`, `GameView`; RPG Maker sections (follows `@a` wrapped arrays and `@r` refs via an `@c` index); names from `<save dir>/../data` then `<save dir>/../www/data` (serde_json, 32 MB cap, errors give no names); limits; Values list; `Session::game_view`, `field_info`, `check_bounds`.
- `src-tauri/src/session/mod.rs`: `mod game` + re-exports; `edit_value` calls `check_bounds` (OUT_OF_RANGE + hint "Type a number from X to Y.").
- `src-tauri/src/lib.rs`: commands `game_view`, `field_info`.
- `src-tauri/src/session/tests.rs`: 6 new tests (MZ, MV, names, ES3 values, gold edit/original, plain JSON).
- `src/api/commands.ts`: types + `gameView()`, `fieldInfo()`.

## Notes / uncertain
- Deviation: `ValueRow` has an extra `path` (needed to expand a group). Contract otherwise exact.
- RPG Maker view needs recipe layer `compression.lz-string` (mv) or `encoding.js-binary-string` (mz) AND `party._gold`; otherwise falls back to Values.
- "was ..." original = value before the first pending edit; after save it becomes the saved value.
- Variables/switches: index 0 skipped; unnamed 0/""/false entries and null entries are left out; named ones shown (null named cannot be edited, skipped).
- Whole numbers with no fixed width (JSON) get the i64 range in the Values list only; RPG Maker variables/bonuses have no range.
- Values list is capped at 10,000 rows. MV fixture content was not inspected (binary); MV test assertions are loose.
- No new dependencies (serde_json already in Cargo.toml).
