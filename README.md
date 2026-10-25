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
| Unity · Easy Save 3 | `.es3` | plain, GZip and AES (password needed) |
| Unity · PlayerPrefs | registry export `.reg` | ints, floats, strings |
| Unity / .NET BinaryFormatter | `.dat`, RAGS `.rsv` | values (read as data, never run) |
| Unreal Engine | `.sav` (GVAS) | values |
| Palworld | `.sav` (PlZ) | values (PlM/Oodle not supported) |
| Godot | ConfigFile text, `store_var` binary, JSON | values (encrypted files are refused) |
| Ren'Py | `.save`, `persistent` | store variables (read as data, never run) |
| Twine · SugarCube 2 | save files and save codes | story variables |
| TyranoScript | `.sav` | values |
| Naninovel | save slots | custom variables |
| QSP | `.sav` | values; Windows-1251 and KOI8-R legacy saves |
| Flash | `.sol` (AMF0 / AMF3) | values |
| Pokémon Gen 3 | `.sav` | money, coins, bag, party EXP / IVs / EVs / stats |
| The Witcher 3 | `.sav` | orens, item quantities |
| Java serialization | object streams | values (read as data, never run) |
| SQLite | database saves | table values |
| JSON and text | `.json`, zip- or gzip-packed JSON | values; add and remove entries |
| MessagePack | `.msgpack`, `.msgp` | values (extension payloads are read-only) |
| CBOR | `.cbor` | values (tags stay as read-only wrappers) |
| BSON | `.bson` | doubles, strings, ints and booleans (other types read-only) |
| Minecraft NBT | `.dat`, `.nbt` (Java big-endian, Bedrock little-endian, gzip) | values and numeric array elements |
| Compressed saves | bzip2, Zstandard, xz, LZMA-alone, LZ4 frame, Brotli (`.br`) | the same formats inside any of these layers |
| Tar archives | `.tar` | one payload entry, the rest of the archive kept byte-identical |
| Pasted hex / percent codes | text | decoded when the text spells a format SavePrism knows |
| XML | `.xml` | element text and attribute values (comments, CDATA and declarations stay read-only) |
| INI / CFG | `.ini`, `.cfg`, `.conf` | section values |
| Java properties | `.properties` | values (Java escapes and line continuations) |
| TOML | `.toml` | strings, integers, floats and booleans |
| YAML | `.yaml`, `.yml` | plain, quoted and literal-block scalars (anchors and tags stay read-only) |
| CSV | `.csv` | cells (quoting kept) |
| Text encodings | every text format above | UTF-8/16/32 (with or without BOM), Shift-JIS, GB18030, CP949, Windows-1252; an edit the encoding cannot hold is refused |
| Property lists | `.plist` (XML and binary) | XML strings, integers, reals and booleans; binary lists open read-only |

A file that does not survive the round-trip check opens read-only with a warning.

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
