<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/brand/SavePrism-logo-on-dark.png">
  <img alt="SavePrism" src="docs/brand/SavePrism-logo-on-light.png" width="320">
</picture>

# SavePrism

A free, local, MIT-licensed desktop app for editing game save files. Open a save from RPG Maker, Unity, Unreal Engine,
Ren'Py, Twine and more, change values like gold, stats, items, variables and switches, and save it back **in its original
format**. Your files never leave your computer.

**Status:** v0.1 for Windows.

---

## Install (Windows 10 and 11)

1. Download `SavePrism_0.1.0_x64-setup.exe` (or `SavePrism_0.1.0_x64_en-US.msi`) from the Releases page.
2. Run it. The installer is not code-signed yet, so Windows SmartScreen may say "Windows protected your PC":
   choose **More info → Run anyway**.
3. Open SavePrism, then drop a save file on the window or press **Open file**. You can also paste a save code.

SavePrism needs Microsoft Edge WebView2, which Windows 10 and 11 already have; the installer fetches it if it is missing.

## Why

1. **Online editors have limits:** upload caps, waiting times, paywalls, and your file goes to someone else's server.
2. **Open-source tools are scattered:** one CLI per engine, and the best desktop editors cover only a few engines.
3. **Nothing round-trips safely:** SavePrism decodes a save, re-encodes it unchanged and checks the result before it lets
   you edit. Every write makes a backup first (kept in `%LOCALAPPDATA%\SavePrism\backups`, never next to the save) and is
   read back and checked afterwards.

## Supported formats

| Engine or format | Files | What you can edit |
|---|---|---|
| RPG Maker MV / MZ | `.rpgsave`, `.rmmzsave` | gold, characters, items, weapons, armors, variables, switches, with names from the game's `data` folder |
| RPG Maker XP / VX / VX Ace | `.rxdata`, `.rvdata`, `.rvdata2` | the same sections, names from the `Data` folder |
| RPG Maker 2000 / 2003 | `.lsd` | the same sections, names from `RPG_RT.ldb`; Shift-JIS or Windows-1252 text (also from `RPG_RT.ini`) |
| Wolf RPG Editor | `.sav` (not encrypted) | switches, variables, the variable database |
| Adventure Game Studio | `agssave.000` ... `agssave.999` | description, engine and game title; 3.x script globals read-only |
| NScripter / ONScripter | `save*.dat`, `global.sav` / `gloval.sav` | global numbers and Shift-JIS strings; the slot description (the slot body stays read-only) |
| AliceSoft System 3.x / 4 | `.asd`, `.as2`, `.asdbak` | System 3.x numbers and fixed-length Shift-JIS strings; System 4 globals (numbers, booleans, floats, fixed-length strings); encrypted System 4 saves are read-only |
| Artemis Engine | `save/*.dat`, `saveg.dat` | detected (magic, version and the decompressed body); the body is read-only because its inner layout has no public description |
| Emuera / eraMaker | binary `saveNN.sav`, `global.sav`, `var_*.dat`, `chara_*.dat` | variables and character arrays by name; text-format saves are not opened |
| Unity · Easy Save 3 | `.es3` | plain, GZip and AES (password needed) |
| Unity · Odin Serializer | Odin binary (`BinaryDataWriter`) saves | primitives, strings and arrays (type tables and references read-only) |
| Unity · PlayerPrefs | registry export `.reg` | ints, floats, strings |
| Unity · Adventure Creator | save files in the persistent data folder (JSON, XML and Base64 BinaryFormatter parts joined by `||`) | values in every part; one part is rewritten at a time |
| Unity · Pixel Crushers Save System | `save<N>.dat` (JSON `SavedGameData`) | saver data as nested JSON; Dialogue System Lua variables and quests (read as data, never run) |
| Unity / .NET BinaryFormatter | `.dat`, RAGS `.rsv` | values (read as data, never run) |
| Unreal Engine | `.sav` (GVAS) | values |
| Palworld | `.sav` (PlZ) | values (PlM/Oodle not supported) |
| Godot | ConfigFile text, `store_var` binary, JSON | values (encrypted files are refused) |
| Ren'Py | `.save`, `persistent` | store variables (read as data, never run) |
| Twine · SugarCube 2 | save files and save codes | story variables |
| TyranoScript | `.sav` | values |
| Naninovel | save slots | custom variables |
| QSP | `.sav` | values; Windows-1251 and KOI8-R legacy saves |
| KiriKiri / KAG | `.ksd`, `.kdt` | dictionary and array values; plain, scrambled (`FE FE` mode 0/1) and zlib-compressed text |
| GameMaker | INI, JSON, `ds_map_write` hex dumps | INI/JSON values; `ds_map` values (other `ds_map` versions and `ds_list` dumps are not opened) |
| Pixel Game Maker MV | `play_LOADFILE*.json` | values (plain JSON) |
| Flash | `.sol` (AMF0 / AMF3) | values |
| Pokémon Gen 3 | `.sav` | money, coins, bag, party EXP / IVs / EVs / stats |
| The Witcher 3 | `.sav` | orens, item quantities |
| Terraria | player `.plr` (key from the game's own `Terraria.exe`), world `.wld` | name, difficulty, play time, life/mana, inventory; world name, seed, size, ids and flags (tiles and the rest read-only) |
| BioWare GFF V3.2 | `.bic`, `.ifo`, `.git`, `.are`, `.uti`, `.utc`, `.dlg`, `.jrl` (Neverwinter Nights, KOTOR, Jade Empire, The Witcher) | numbers and same-length strings; structs and lists stay read-only |
| BioWare ERF containers | `.erf`, `.mod`, `.hak`, `.sav` (Neverwinter Nights, KOTOR `SAVEGAME.sav`) | one resource inside; the others stay byte-identical |
| Dragon Age: Origins / II GFF V4 | `.das` | values shown read-only (the V4 layout is not rewritten) |
| Java serialization | object streams | values (read as data, never run) |
| SQLite | database saves | table values |
| LevelDB | `.ldb`, `.sst` (tables) and `.log` (write-ahead log) | keys and values as text or hex; read-only |
| RocksDB | `.sst` (block-based tables) | keys and values; read-only |
| LiteDB | `.db` (LiteDB 4 and 5 datafiles) | collection documents; fixed-size integers, doubles and booleans are editable |
| JSON and text | `.json`, zip- or gzip-packed JSON | values; add and remove entries |
| MessagePack | `.msgpack`, `.msgp` | values (extension payloads are read-only) |
| Protocol Buffers | `.pb`, `.pbf`, `.protobuf`, `.protobin`, `.bytes` | field numbers, varints, fixed values and strings; nested message lengths are recomputed (groups read-only) |
| Bitser (LÖVE) | `.dat`, `.save`, `.bin` by content | numbers, booleans and strings; tables, references, resources and class data stay read-only |
| SCS `.sii` (ETS2 / ATS) | `.sii`, `game.sii`, `info.sii`, `profile.sii` | unit attributes as text; binary `BSII` is read-only; encrypted `ScsC` opens with the `g_save_format` hint |
| CBOR | `.cbor` | values (tags stay as read-only wrappers) |
| BSON | `.bson` | doubles, strings, ints and booleans (other types read-only) |
| Minecraft NBT | `.dat`, `.nbt` (Java big-endian, Bedrock little-endian, gzip) | values and numeric array elements |
| Compressed saves | bzip2, Zstandard, xz, LZMA-alone, LZip, LZ4 frame, Brotli, raw deflate, LZF (raw and `ZV` blocks) — found by content, any extension | the same formats inside any of these layers |
| XOR-obfuscated saves | single-byte or repeating key up to 32 bytes | the format inside; the key is recovered from the inner format automatically |
| Tar archives | `.tar` | one payload entry, the rest of the archive kept byte-identical |
| PS1 memory cards | `.mcr`, `.mcd`, `.mem`, `.gme` | one save file from the card; XOR checksums recomputed and title/icon frames kept |
| PS2 memory cards | `.ps2` | one save file from the card's FAT tree; ECC recomputed on the pages that change |
| PS2 save exports | `.psu` | one file from the export; `.max` (LZARI) is not opened |
| SharkPort GBA snapshots | `.sps`, `.xps` | the raw GBA save; title, date, comment and checksum kept |
| FromSoftware BND4 containers | `.sl2`, `.bnd` (Dark Souls II/III, Sekiro, Elden Ring) | one entry inside (raw or DCX zlib); the other entries stay byte-identical; encrypted files are not opened |
| Blizzard MPQ archives | `.mpq`, `.SC2Save`, `.w3z` | one entry (zlib/bzip2 sectors are decompressed); uncompressed entries are editable, compressed or encrypted ones read-only |
| Brickadia `.brz` archives | `.brz` | one file from the archive; stored files are editable when no other file shares them |
| Brickadia saves | `.brs` | the map name, author and description; brick data stays read-only |
| Pasted hex / percent / Base64 codes | text | decoded when the text spells a format SavePrism knows; the Base64URL alphabet and optional padding are kept |
| XML | `.xml` | element text and attribute values (comments, CDATA and declarations stay read-only) |
| INI / CFG | `.ini`, `.cfg`, `.conf` | section values |
| Java properties | `.properties` | values (Java escapes and line continuations) |
| TOML | `.toml` | strings, integers, floats and booleans |
| YAML | `.yaml`, `.yml` | plain, quoted and literal-block scalars (anchors and tags stay read-only) |
| CSV | `.csv` | cells (quoting kept) |
| Text encodings | every text format above | UTF-8/16/32 (with or without BOM), Shift-JIS, GB18030, CP949, Windows-1252; an edit the encoding cannot hold is refused |
| Property lists | `.plist` (XML and binary) | XML strings, integers, reals and booleans; binary lists open read-only |
| Snes9x snapshots | `.000`-`.009`, `.frz` (plain or gzip) | the save-RAM (`SRA`) block as raw bytes; every other block is read-only |
| RetroArch save states | `.state`, `.state1` ... (uncompressed `RASTATE`) | blocks shown read-only; RZIP-compressed states open with a hint to turn off state compression |
| RetroArch battery saves | `.srm` | raw save RAM (fixed size) |

A file that does not survive the round-trip check opens read-only with a warning. Easy Save 2 files are not opened: Moodkie
publishes no byte-level layout for them, so SavePrism does not guess.

If SavePrism does not know a file, press **Copy file report** on the message and paste it into a new issue with the game's
name (extension, size, first bytes and a hash only; never the file or its path, and nothing is uploaded).

## Use it with AI

- **Ask AI** on every list copies the values with a short note, ready to paste into any AI chat.
- **MCP server:** `saveprism-desktop.exe mcp` lets AI apps such as Claude Desktop, Claude Code or Cursor open and edit saves through
  SavePrism, with the same checks and backups. **Settings → AI tools → Copy setup** gives you the entry to paste.

See [docs/ai.md](docs/ai.md).

## Platforms

Windows first. Linux and macOS builds come later.

## Command line

The `saveprism` command line tool (built from `crates/saveprism-cli`) detects every format above; `decode` and `encode`
work for JSON, RPG Maker MV/MZ, Easy Save 3 and GVAS.

```
saveprism inspect <save>                                   # detected format and round-trip result
saveprism decode <save> -o save.json                       # save → JSON + save.json.recipe.toml
saveprism encode save.json --recipe <recipe> -o <save>     # JSON → save (backup first)
saveprism keys try <save> --candidates <list.txt>          # find the password of an encrypted ES3 save
```

## Documentation

| File | Contents |
|---|---|
| [NOW.md](NOW.md) | Current state and the next step |
| [docs/ai.md](docs/ai.md) | Ask AI and the MCP server |
| [docs/themes.md](docs/themes.md) | Colors and themes |
| [docs/architecture/MIMARI.md](docs/architecture/MIMARI.md) | Architecture (Turkish) |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to build and contribute |

## License

[MIT](LICENSE). Third-party licenses: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Made for single-player saves of
games you own. Online games, anti-cheat and DRM are out of scope.
