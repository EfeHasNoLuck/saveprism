<!-- Research by agy (gemini-3.8-flash) on 2026-10-23 from public saveeditonline.com pages; checked by Claude. Behavior only (clean room). -->

# Parity Report: saveeditonline.com vs SavePrism

## Table 1: Formats

| Engine | Extensions | saveeditonline notes | SavePrism | Rough effort | Safety note |
|---|---|---|---|---|---|
| **Ren'Py** | `.save` | Python pickle format; provides auto-re-signing for HMAC tokens | No | M | Pickle must be parsed strictly as data/AST; never execute Python bytecode |
| **RPG Maker XP** | `.rxdata` | Ruby Marshal format | No | M | Parse strictly as a static data tree; never eval or instantiate arbitrary objects |
| **RPG Maker VX / VX Ace** | `.rvdata`, `.rvdata2` | Ruby Marshal 4.8 format | No | M | Data-only deserialization; forbid object execution or symbol table poisoning |
| **RPG Maker MV / MZ** | `.rpgsave`, `.rmmzsave` | LZString-compressed JSON | Yes | Done | LZString decompression bomb guard; strict JSON recursion depth limits |
| **Unity (Easy Save 3)** | `.es3` | Plain JSON, GZip, AES-128 | Yes | Done | Secure key handling; enforce decompression limits |
| **Unity (Generic / PlayerPrefs)** | `.dat`, `.data` | Generic binary, JSON, or PlayerPrefs key-value data | Partly | M | Never invoke `BinaryFormatter`; parse byte streams as structured data |
| **Unreal Engine** | `.sav` | Unreal GVAS binary with property tags | Yes | Done | Enforce string/array length bounds to prevent memory allocation panics |
| **Palworld** | `.sav` | Unreal GVAS with PlZ (zlib) compression | Yes | Done | Bound uncompressed stream size against PlZ decompression bombs |
| **Twine / SugarCube** | `.save`, `.savesbundle`, `.txt` | LZString Base64-encoded story state | No | S | Check buffer sizes before decoding Base64 and decompressing LZString |
| **Wolf RPG Editor** | `.sav` | Binary variable tables; unencrypted only | No | M | Strict array and string byte bounds checking on binary stream reads |
| **TyranoScript** | `.sav` | Base64-encoded JSON state | No | S | Enforce max payload limit prior to Base64 decoding |
| **Naninovel** | `.nson` | Unity VN structured JSON script state | Partly | S | Standard JSON recursion depth and token limit (opens as plain JSON today) |
| **QSP (Quest Soft Player)** | `.sav` | Binary state with simple character encoding | No | M | Static buffer validation during sequential binary reads |
| **Flash / ActionScript** | `.sol` | Flash Local Shared Object (AMF0 / AMF3 binary) | No | M | Parse AMF as static value tree; guard against circular reference recursion |
| **Pokémon Gen 3 (GBA)** | `.sav`, `.srm`, `.sa1` | 128 KB raw flash blocks with 16-bit block checksums | No | M | Enforce fixed cartridge RAM size limits; recalculate CRC checksums safely |
| **Godot Engine** | `.save`, `.dat`, `.cfg` | ConfigFile (INI-like), JSON, or binary Variant data | Partly | M | Static parsing only; never evaluate GDScript expressions or dynamic resources |
| **SQLite Databases** | `.sqlite`, `.db`, `.sqlite3` | Raw relational SQLite database file | No | L | Transaction safety; read-only inspection until explicit atomic replace |
| **Generic JSON / Text** | `.json`, `.txt` | Plain JSON structure or raw text variables | Yes | Done | Generic size limit and nested level bounds |

---

## Table 2: Features

| Feature | saveeditonline | SavePrism | Gap |
|---|---|---|---|
| **File Upload / Open** | Browser drag-and-drop or file picker | Native file dialog + OS drag-and-drop | Parity achieved |
| **Paste Save Code** | Text box to paste Base64, LZString, or JSON strings directly | Requires saving code to a file on disk first | SavePrism cannot paste raw save code from clipboard |
| **Variable Search & Filter** | Instant filter across keys and values | Filter per section + pagination + JSON search | Parity achieved (SavePrism includes pagination) |
| **In-place Value Editing** | Inline edit prompt per variable | Dense cards with type validation & `old → new` diffs | Parity achieved (SavePrism has stronger type safety) |
| **Add / Delete Variables** | Can add arbitrary keys or remove entries (unconfirmed across all formats) | Keys can be added or deleted only in the raw JSON editor | SavePrism cannot add/delete keys directly in card view |
| **Game Names Resolution** | User uploads the game's `Items.json` / `System.json` to get names (seen 2026-10-23) | Automatically reads the `data/` folder next to the save | Gap: no manual "pick the data folder / names file" when the save is not next to the game |
| **Ren'Py HMAC Re-signing** | Automatically recalculates security hashes/tokens on save | No Ren'Py support | SavePrism lacks HMAC signature calculation |
| **Save / Download** | Browser downloads modified file; no backup | Atomic replace with automatic backup in app data | SavePrism exceeds saveeditonline (safer) |
| **Round-Trip Verification** | None (risk of silent corruption on unsupported structures) | Strict decode → encode check; locks if mismatch | SavePrism exceeds saveeditonline |
| **Full JSON / Tree View** | "Edit JSON (full data)" button (seen 2026-10-23) | Editable JSON dialog with search, Apply and Export | Parity (ours validates types and applies all-or-nothing) |
| **Local Privacy / Offline** | Web application (browser cache / remote processing) | 100% offline desktop binary; zero telemetry | SavePrism exceeds saveeditonline |

---

## Top 10 Gaps That Matter Most for Players

1. **Ren'Py (`.save`)**: Essential for the large visual novel catalog; requires safe AST-only pickle parsing and HMAC signature regeneration.
2. **RPG Maker XP / VX / VX Ace (`.rxdata`, `.rvdata`, `.rvdata2`)**: Critical for classic RPGs; requires safe Ruby Marshal deserialization without code execution.
3. **Twine / SugarCube (`.save`, `.savesbundle`, `.txt`)**: Dominant engine for browser text/choice games; requires LZString Base64 unpacking and repacking.
4. **Paste save code from clipboard**: Essential for web/HTML5 games that export save codes as text strings instead of files.
5. **Godot Engine (`.save`, `.dat`, `.cfg`)**: High-growth indie engine; needs ConfigFile and binary Variant serialization support.
6. **Unity generic binary & PlayerPrefs (`.dat`, `.data`)**: Non-ES3 Unity games storing saves in PlayerPrefs or binary blobs.
7. **Wolf RPG Editor (`.sav`)**: Popular engine for indie Japanese RPGs; requires binary variable table parsing.
8. **Add new variable in UI**: Ability to add missing items or flags directly from the card view without editing raw JSON.
9. **Flash Local Shared Objects (`.sol`)**: Key for preserved Flash games (Flashpoint/Ruffle); requires AMF0/AMF3 parser.
10. **TyranoScript / TyranoBuilder (`.sav`)**: Common visual novel framework using Base64-encoded JSON saves.
