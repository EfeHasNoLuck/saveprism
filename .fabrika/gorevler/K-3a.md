# K-3a — Game view backend (RPG Maker sections, data-folder names, field limits)

- **Dalga:** D9 · **Motor:** claude/kodcu·sonnet · **Effort:** medium · **Deneme:** 0/3
- **Amaç:** the screen does not show a tree. For RPG Maker MV/MZ it shows game sections (Gold, Characters, Items, Weapons, Armors,
  Variables, Switches) with real names; for every other format it shows a flat "Values" list. Every editable value carries its type and
  limits so the UI can say "Whole number · 0 to 99,999,999 · was 10,000" and block out-of-range input. Code and comments in English.
- **Bash'in yok, derleyemezsin.** CEO derler ve testleri koşar; hataları düzeltme turunda verir.

## Okur
`apps/desktop/src-tauri/src/**` (K-1: `session/mod.rs`, `lib.rs`) · `apps/desktop/src/api/commands.ts` ·
`crates/saveprism-codecs/src/rpgmaker/**` (how MV/MZ saves become a tree, incl. `@c`/`@a` references) ·
`crates/saveprism-codecs/src/es3/**` (`__type` field) · `crates/saveprism-core/src/{doc,path,edit}.rs` (public API) ·
`fixtures/synthetic/rpgmaker-mv/**`, `fixtures/synthetic/rpgmaker-mz/**`, `fixtures/synthetic/es3/**` (read-only)

## Sahip (yalnız bunlara yaz)
`apps/desktop/src-tauri/**` · `apps/desktop/src/api/**` · `.fabrika/raporlar/K-3a.md`

## Dokunma
`crates/**`, root `Cargo.toml`, `package.json`, `apps/desktop/src/**` outside `src/api/`, `fixtures/**`. No new third-party dependency.

## Contract (K-3b builds the screen against exactly this; serde camelCase)
```ts
type FieldKind = "int" | "float" | "bool" | "string";
type Field = {
  path: string;            // core path string, passed back to edit_value
  value: string;           // current value as text
  original: string;        // value when the file was opened (for "was …")
  kind: FieldKind;
  typeLabel: string;       // "Whole number", "Decimal number", "On/off", "Text", or the ES3 type e.g. "Int32"
  min: number | null;
  max: number | null;
  hint: string;            // e.g. "Whole number · 0 to 99,999,999"
  editable: boolean;
};
type Entry = { id: number; name: string | null; field: Field };          // name null → UI shows "Item #5"
type Actor = { id: number; name: string; level: Field; hp: Field; mp: Field; exp: Field | null;
               bonuses: { label: string; field: Field }[] };              // Max HP, Max MP, Attack, Defense, M.Attack, M.Defense, Agility, Luck
type RpgMakerView = { engine: "mv" | "mz"; gold: Field; actors: Actor[]; items: Entry[]; weapons: Entry[]; armors: Entry[];
                      variables: Entry[]; switches: Entry[]; namesFolder: string | null };
type ValueRow = { name: string; field: Field | null; children: number };  // Values list for other formats; field null for groups
```
Commands (add to `lib.rs`, wrappers + types to `src/api/commands.ts`):
- `game_view() -> { kind: "rpgmaker", view: RpgMakerView } | { kind: "values", rows: ValueRow[] }` for the open document.
- `field_info(path) -> Field` for any scalar node (used by the generic Values list when a group is expanded).
- `edit_value` keeps its signature but now enforces the same `min`/`max` as `Field` (error code + hint on violation).

## Yapılacaklar
1. **RPG Maker view** from the decoded tree: `party._gold`; party member ids `party._actors` → `actors._data[id]` (`_name`, `_level`,
   `_hp`, `_mp`, `_exp[<classId>]`, `_paramPlus[0..7]`); `party._items` / `_weapons` / `_armors` (id → count maps); `variables._data`
   (index = id, skip index 0 and null/0-unused entries only when no name exists); `switches._data` (bool). Resolve `@c`/`@a` references
   the way the codec represents them. If a part is missing, return that section empty instead of failing.
2. **Names from the game's data folder:** look next to the save: `<save dir>/../data/` (MZ) and `<save dir>/../www/data/` (MV layout
   `www/save/`), first match wins. Read `Items.json`, `Weapons.json`, `Armors.json` (arrays of `{id, name}`), `System.json`
   (`variables` and `switches` name arrays). Each file ≤ 32 MB, parse errors → just no names (never fail the view). `namesFolder` = the
   folder used. Only read files, never execute anything.
3. **Limits (RPG Maker defaults):** gold 0–99,999,999 · item/weapon/armor count 0–99 · level 1–99 · hp/mp/exp ≥ 0 · stat bonuses any
   whole number · variables: whole or decimal number (keep kind from the current value) · switches bool. Hints: "Whole number · 0 to
   99,999,999", "Whole number · 0 or higher", "Whole number", "Decimal number", "On/off".
4. **Values list (other formats):** top-level entries of the document; for ES3 use the entry name and read `__type`/`value`
   (`System.Int32` → int −2,147,483,648…2,147,483,647 "Int32", `System.Int64`, `System.Single`/`Double` → float, `System.Boolean`,
   `System.String`); GVAS/JSON: infer from the value (int → i64 range unless the codec keeps a width). Groups (objects/arrays) → `field: null`
   + child count.
5. No panics (workspace denies unwrap/expect), no new deps.

## Testler (desktop crate)
MZ and MV synthetic saves: `game_view` returns kind `rpgmaker`, gold value, ≥1 actor with level/hp, items present · copy an MZ save
into a temp `save/` folder with a sibling `data/` holding small hand-written `Items.json` and `System.json` → names appear;
without `data/` → names null · ES3 `progress.es3` → kind `values`, `coins` has typeLabel `Int32`, max 2147483647 · `edit_value` on gold
with 100000000 → range error; 500 → ok and `field.original` stays the old value.

## Kabul
CEO: `cargo fmt --all` + `bash .claude/araclar/kontrol.sh rust` exit 0 and `pnpm -C apps/desktop typecheck` exit 0.
Rapor ≤8 satır (`.fabrika/raporlar/K-3a.md`).
