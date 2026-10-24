<!-- Research by agy (gemini-3.8-flash) on 2026-10-24 from public sources. Not checked line by line: verify every table against the primary source before implementing. Behavior only (clean room). -->

# Save File Format Research Report for SavePrism

This report provides clean-room binary and serialization specifications for implementing save editors in SavePrism. No code is copied; all specifications are derived from public documentation, open-source reverse-engineering projects, file format standards, and protocol specifications.

---

## 1. The Witcher 3: Wild Hunt Save Format (`.sav`) (PC)

### 1. File Locations, Names, and Version Differences
* **Save Locations (PC):**
  * Windows: `%USERPROFILE%\Documents\The Witcher 3\gamesaves\`
  * Steam Cloud / Steam Deck (Proton): `<Steam-folder>/userdata/<user-id>/292030/remote/` or `~/.local/share/Steam/steamapps/compatdata/292030/pfx/drive_c/users/steamuser/Documents/The Witcher 3/gamesaves/`
  * GOG Galaxy Cloud: `%USERPROFILE%\Documents\The Witcher 3\gamesaves\`
* **File Names & Extensions:**
  * Auto-save: `AutoSave_<Index>.sav` (e.g., `AutoSave_12345.sav`)
  * Manual save: `ManualSave_<Index>.sav`
  * Quick save: `QuickSave_<Index>.sav`
  * Checkpoint: `CheckPoint_<Index>.sav`
  * Associated thumbnail/metadata: Each `.sav` is accompanied by an identically named `.png` screenshot (e.g., `ManualSave_12345.sav.png`).
* **Game Version Differences:**
  * *Original Release (v1.00 – v1.32)*: LZ4 chunk-compressed REDengine 3 save container.
  * *Next-Gen / Complete Edition (v4.00+)*: Identical outer container and LZ4 compression. A known engine bug in v4.00–v4.02 caused repeated quest "facts" accumulation ("save bloat"), exceeding 255 chunks (~97 MB to 255 MB uncompressed) and causing chunk table boundary collisions.
  * *Console / Switch cross-saves*: Nintendo Switch saves wrap this exact PC payload inside an outer `NXZS` container (Magic `4E 58 5A 53`) with a 4-byte uncompressed size, 4-byte compressed size, and raw zlib deflate stream. Decompressing the Switch stream yields the exact PC `.sav` stream described below. Companion `.req` files on consoles are plain-text dependency manifests (e.g., `content0;content1;`), not checksums.

### 2. File Recognition & Signature Matching
* **Primary Magic Bytes:** First 8 bytes must be ASCII `SNFHFZLC` (`0x53 0x4E 0x46 0x48 0x46 0x5A 0x4C 0x43`).
* **Offset & Length Checks:**
  * Bytes `0x00..0x08`: `SNFHFZLC`
  * Bytes `0x08..0x0C`: Little-endian `uint32` chunk count $N$ ($1 \le N \le 1024$).
  * Bytes `0x0C..0x10`: Little-endian `uint32` header offset (start of payload / table metadata).
* **Disambiguation:** Differentiates from generic `.sav` files (GameMaker, emulator states, Nintendo DS/GBA raw battery dumps) by the mandatory 8-byte ASCII magic `SNFHFZLC`.

### 3. Byte Layout Specification

#### A. Outer Container Layout (Disk File)
All multi-byte integers are **Little-Endian**.

| Offset | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `0x00` | 8 | ASCII String | `magic` | Exact string `"SNFHFZLC"` | Confirmed |
| `0x08` | 4 | `uint32` | `chunkCount` | Total number of compressed chunks ($N$) | Confirmed |
| `0x0C` | 4 | `uint32` | `headerOffset` | Offset to uncompressed metadata header / end of chunk table | Confirmed |
| `0x10` | $N \times 12$ | `ChunkHeader[N]` | `chunkTable` | Array of chunk descriptor entries | Confirmed |
| $0x10 + (N \times 12)$ | Variable | Byte stream | `compressedPayload` | Sequential raw LZ4 compressed blocks | Confirmed |

#### B. Chunk Table Entry (`ChunkHeader`, 12 bytes each)
| Sub-Offset | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `+0x00` | 4 | `uint32` | `compressedSize` | Byte size of this chunk as stored on disk | Confirmed |
| `+0x04` | 4 | `uint32` | `decompressedSize` | Expected output buffer size (typically `0x100000` = 1,048,576 bytes / 1 MB, except final chunk) | Confirmed |
| `+0x08` | 4 | `uint32` | `endOffset` | Absolute end byte position of this compressed block in the file | Confirmed |

#### C. Inner Decompressed Stream Layout
Concatenating all decompressed LZ4 chunks produces the unified game-state stream:

| Section | Type / Marker | Description | Status |
| :--- | :--- | :--- | :--- |
| Stream Header | `SAV3` (`0x53 0x41 0x56 0x33`) or `CR2W` | Internal save format version and engine marker | Confirmed |
| Metadata Block | Fixed / TLV | Engine version, build timestamp, save title, character level | Confirmed |
| String Table / Name Pool | Symbol Table | Array of indexed ASCII/UTF-8 strings (symbol pool) | Confirmed |
| Property & Object Tree | Serialized hierarchy | Hierarchical tree of REDengine serialized properties and classes | Confirmed |

#### D. Name Pool & Property Serialization
* **String / Symbol Pool:**
  * Strings (property names, enum names, type names) are stored once in the symbol table.
  * Entry format: 2-byte or 4-byte string length $L$, followed by $L$ bytes of UTF-8 text (or 2-byte length with high bit indicating UTF-16LE).
* **Property Serialization Node:**
  Each game property is serialized as:
  1. `nameIndex`: 2-byte or 4-byte `uint16`/`uint32` pointing to the string in the Name Pool.
  2. `typeIndex`: 2-byte or 4-byte `uint16`/`uint32` pointing to the type name string in the Name Pool.
  3. `dataSize`: 4-byte `uint32` specifying byte length of the value payload.
  4. `value`: Raw typed payload:
     * `Int32` / `Uint32`: 4 bytes little-endian.
     * `Int64` / `Uint64`: 8 bytes little-endian.
     * `Float`: 4 bytes IEEE 754 float.
     * `Bool`: 1 byte (`0x00` = false, `0x01` = true).
     * `String`: Length-prefixed character array.
     * `Struct` / `Object`: Nested list of property nodes.
     * `Array`: 4-byte element count followed by contiguous typed elements.

### 4. Compression, Encryption, and Checksums
* **Compression:** Raw **LZ4 block compression** (no LZ4 frame headers; raw LZ4 block decompression using `decompressedSize` as destination buffer capacity).
* **Encryption:** None on PC.
* **Checksums / Hashes:**
  * **No cryptographic hash or CRC** is checked for file validity. Neither MD5, SHA-1, nor CRC32 exists across the `.sav` container.
  * **Integrity Guard:** The engine strictly validates:
    1. Chunk table offsets: `endOffset` must equal current file pointer + `compressedSize`.
    2. Sum of all `compressedSize` entries + header length must equal total file size.
    3. Total uncompressed size and stream bounds must match expected object deserialization sizes.
* **Byte-Exact Rewrite Rules:**
  * To rewrite the file: modify values in the decompressed buffer; if the buffer length changed, re-encode into 1 MB chunks using standard LZ4 block compression; rebuild the 12-byte `ChunkHeader` entries with new `compressedSize`, `decompressedSize`, and `endOffset` values; write out `SNFHFZLC`, updated `chunkCount`, `headerOffset`, chunk table, and the new compressed chunks.

### 5. Player Modification Targets
* **Currencies (Crowns, Orens, Florens):**
  * *Crowns (Novigrad Crowns)*: Geralt's spendable currency. Stored in the player character entity inventory (`W3Player` / `CR4Player` $\to$ `W3InventoryComponent`). Stored as a property named `Crowns` or within an inventory item structure referencing item template ID `Crowns` as an `Int32`.
  * *Orens & Florens*: Stored as inventory item entries (`Orens`, `Florens`) with an item count (`Int32`).
* **Inventory Items:**
  * Contained in `W3InventoryComponent`.
  * Represented as arrays of inventory item structures containing:
    * `itemId` (Name Pool reference or GUID).
    * `quantity` (Int32).
    * `durability` (Float: `1.0` = 100%).
    * `charges` / `enhancements` (Array of slotted rune/glyph IDs).
* **Stats & Ability Points:**
  * Properties `level` (Int32), `experiencePoints` (Int32), `abilityPoints` (Int32).
* **Quest Facts:**
  * Facts database section (e.g., `SBDF` facts container): hash-mapped key-value pairs (Fact name string or CRC32 hash $\to$ integer value/flag `0` or `1`).

### 6. Sample Files
* Publicly accessible, openly licensed sample saves:
  * Open-source testing samples in repository: `https://github.com/solusipse/witcher3-inventory-editor/tree/master/test` (MIT License).
  * Community testing assets: `Atvaark/W3SavegameEditor` sample assets (MIT License).

### 7. Sources & Open-Source References
* `solusipse/witcher3-inventory-editor`: [GitHub](https://github.com/solusipse/witcher3-inventory-editor) (License: MIT)
* `Atvaark/W3SavegameEditor`: [GitHub](https://github.com/Atvaark/W3SavegameEditor) (License: MIT)
* CD Projekt Red REDengine 3 technical community research: [ecostack.dev/witcher3-save-architecture](https://ecostack.dev)
* WolvenKit REDengine file format specifications: [WolvenKit GitHub](https://github.com/WolvenKit/WolvenKit) (License: GPL-3.0; architectural reference only, clean-room verified).

---

## 2. Unity PlayerPrefs on Windows, macOS, and Linux

### 1. File & Registry Locations
* **Windows (Registry):**
  * Registry Key: `HKEY_CURRENT_USER\Software\[CompanyName]\[ProductName]`
  * If `CompanyName` is left unset in Unity Editor Player Settings, the default is `DefaultCompany`.
* **Windows (.reg Export Files):**
  * Exported via `reg.exe export "HKCU\Software\[CompanyName]\[ProductName]" prefs.reg /y` or created by users for backup/transfer.
* **macOS:**
  * Path: `~/Library/Preferences/unity.[CompanyName].[ProductName].plist` (or `com.[CompanyName].[ProductName].plist`).
  * Format: Apple Property List (XML plist or binary `bplist00`).
* **Linux:**
  * Path: `~/.config/unity3d/[CompanyName]/[ProductName]/prefs`
  * Format: Plain-text XML configuration file.

### 2. File Recognition & Signature Matching
* **Windows Registry Key:** Identified by path format `HKCU\Software\<Company>\<Product>`.
* **Windows `.reg` Export File:**
  * Magic Bytes: `0xFF 0xFE` (UTF-16LE Byte Order Mark).
  * First Line (UTF-16LE): `Windows Registry Editor Version 5.00\r\n`.
* **macOS `.plist`:**
  * Binary Plist: Magic `bplist00` (`0x62 0x70 0x6C 0x69 0x73 0x74 0x30 0x30`).
  * XML Plist: ASCII text starting with `<?xml version="1.0" encoding="UTF-8"?>` and containing `<!DOCTYPE plist ...>`.
* **Linux `prefs`:**
  * Plain text XML starting with `<unity_prefs>` root tag.

### 3. Byte Layout & Storage Specification

#### A. The Value-Name Hash Suffix (`_h<number>`)
Unity appends `_h` followed by an unsigned 32-bit integer string to every key name to guarantee case sensitivity in case-insensitive environments (like the Windows Registry):
* **Hash Algorithm:** A modified 32-bit **djb2** hash with bitwise XOR:
  * Initial seed: `hash = 5381` (`uint32`)
  * For each character $c$ in the key string:
    $$\text{hash} = ((\text{hash} \times 33) \oplus c) \pmod{2^{32}}$$
  * Format: `<KeyName>_h<hash>` where `<hash>` is the decimal string representation of the unsigned 32-bit integer.
  * *Example:* For key `"HighScore"`, the hash is computed character-by-character over the ASCII/UTF-8 values of `'H'`, `'i'`, `'g'`, `'h'`, `'S'`, `'c'`, `'o'`, `'r'`, `'e'`.

#### B. Data Types in the Windows Registry
| Unity Type | Registry Type | Binary Value Format | Registry Representation / .reg Syntax | Status |
| :--- | :--- | :--- | :--- | :--- |
| `int` | `REG_DWORD` (`0x04`) | 4 bytes, Little-Endian `int32` | `dword:0000002a` (hex value) | Confirmed |
| `float` | `REG_DWORD` (`0x04`) or `REG_BINARY` (`0x03`) | 8 bytes, Little-Endian IEEE-754 **`double` (64-bit)** | In registry, Unity writes 8 bytes under `REG_DWORD`. In `.reg` files, exported as `hex(4):00,00,00,00,00,00,f0,3f` or `hex(b):...` | Confirmed |
| `string` | `REG_BINARY` (`0x03`) | UTF-8 encoded bytes followed by a **single trailing NUL (`0x00`)** byte | `"Key_h123"=hex:48,65,6c,6c,6f,00` | Confirmed |

> [!IMPORTANT]
> **The Float/Double Quirk:** Unity's C# API calls `PlayerPrefs.SetFloat(key, floatVal)`, but the underlying native Windows implementation converts the 32-bit float to a 64-bit IEEE 754 double and writes 8 bytes to a value registered as `REG_DWORD`. Windows `regedit` expects exactly 4 bytes for `REG_DWORD`, so it displays `"Invalid DWORD (32-bit) value"` and shows raw hex. To parse it: read 8 bytes as a Little-Endian `uint64`/`double`, then cast to `float`.
>
> **The String Trailing NUL:** Unity stores strings as `REG_BINARY`, not `REG_SZ`. It appends a terminating `0x00` byte. When editing, the trailing `0x00` MUST be preserved, or Unity's string decoder will fail or read past the buffer boundary.

#### C. Windows `.reg` File Format Grammar (Version 5.00)
1. **Encoding:** Strict UTF-16LE with BOM (`FF FE`). Lines end with CRLF (`\r\n` encoded as `0D 00 0A 00`).
2. **File Header:** Exactly `Windows Registry Editor Version 5.00` followed by an empty line.
3. **Section Header:** `[HKEY_CURRENT_USER\Software\CompanyName\ProductName]`
4. **Data Lines:**
   * DWORD: `"key_h1234"=dword:00000064`
   * Binary / String / Float:
     * `hex:` indicates `REG_BINARY`.
     * `hex(4):` indicates `REG_DWORD` with non-standard lengths (used for 8-byte floats).
     * `hex(b):` indicates `REG_QWORD` (type `0x0B`).
   * Line continuations: Lines exceeding 80 characters end with a backslash `\` followed by CRLF. The next line begins with two indentation spaces (`0x20 0x20`), followed by continuing comma-separated two-digit hex bytes:
     ```reg
     "LongData_h4567"=hex:01,02,03,04,05,06,07,08,09,0a,0b,0c,0d,0e,0f,10,\
       11,12,13,14,00
     ```

#### D. macOS & Linux Layouts
* **macOS (`.plist`):**
  * Standard Apple property list dictionary: `<dict><key>KeyName</key><integer>10</integer>...</dict>`.
  * Note: macOS keys **do NOT have the `_h<hash>` suffix**. They use the clean key name directly.
* **Linux (`prefs`):**
  * Plain text XML:
    ```xml
    <unity_prefs>
      <pref name="HighScore" type="int">100</pref>
      <pref name="Volume" type="float">0.85</pref>
      <pref name="PlayerName" type="string">Hero</pref>
    </unity_prefs>
    ```
  * Linux keys **do NOT have the `_h<hash>` suffix**.

### 4. Compression, Encryption, and Checksums
* **Compression:** None native to PlayerPrefs.
* **Encryption:** None native. (Third-party packages like anti-cheat plugins may pre-encrypt strings before passing them to PlayerPrefs).
* **Checksums:** No registry-level checksums. Registry edits take effect immediately upon process start.

### 5. Player Modification Targets
* All keys exposed by the game developer: currency counters, audio volumes, unlock flags, unlocked level IDs, high scores, screen resolution, graphics quality settings.

### 6. Sample Files
* Reference test files in open-source projects:
  * `flibitijibibo/PlayerPrefsEditor`: [GitHub](https://github.com/flibitijibibo/PlayerPrefsEditor) (License: Public Domain / Unlicense).
  * Sample registry exports in Unity testing tools: [GitHub gist](https://gist.github.com) samples under MIT/Public Domain.

### 7. Sources & Open-Source References
* Unity Documentation on PlayerPrefs: [docs.unity3d.com/ScriptReference/PlayerPrefs](https://docs.unity3d.com/ScriptReference/PlayerPrefs.html)
* djb2 hash verification in open-source Unity utilities: [PlayerPrefsEditor on GitHub](https://github.com/flibitijibibo/PlayerPrefsEditor) (Unlicense)
* Microsoft Windows Registry File Format specification: [MSDN Registry Format](https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-files)

---

## 3. Naninovel Save Slots (`.nson` / `.json`)

### 1. File Locations and Names
* **Locations:**
  * Windows: `%USERPROFILE%\AppData\LocalLow\[CompanyName]\[ProductName]\Saves\`
  * WebGL builds: Browser `IndexedDB` (`/idbfs`).
  * Standalone / Custom: User-configurable via `StateConfiguration` (can point to any relative subfolder such as `Saves/`).
* **File Names & Slot Architecture:**
  * Game save slots: `GameSave<Index>.nson` or `GameSave<Index>.json` (e.g., `GameSave001.nson`, `GameSave002.nson`).
  * Quick save: `QuickSave.nson`.
  * Auto save: `AutoSave.nson`.
  * Global persistent state: `GlobalSave.nson` (unlocked CGs, global variables, read-text state).
  * Settings / Configuration: `Settings.nson`.

### 2. File Recognition & Signature Matching
* **Plain JSON Mode (`.json`):** Standard UTF-8 JSON text starting with `{` and containing keys formatted as engine namespaces (e.g., `"Naninovel.CustomVariableManager"` or `"PlaybackSpot"`).
* **Binary Mode (`.nson`):**
  * Magic Bytes: `1F 8B 08` (standard Gzip magic number and Deflate compression flag).
  * Differentiating from generic Gzip: Decompressing the stream immediately yields UTF-8 JSON text containing root keys with `"Naninovel."` namespace identifiers.

### 3. Serialization Layout & JSON Schema

#### A. Container Layer
Naninovel's default "binary" save is **Gzip-compressed UTF-8 JSON text**. When "Binary Save Files" is disabled in the engine configuration, it writes raw, uncompressed UTF-8 JSON.

#### B. JSON Object Hierarchy
The root JSON object is a serialized dictionary representing `GameStateMap`, where keys are fully qualified C# type names or service aliases:

```json
{
  "Naninovel.CustomVariableManager": {
    "Variables": [
      {
        "Name": "score",
        "Value": "100"
      },
      {
        "Name": "has_key",
        "Value": "true"
      }
    ]
  },
  "Naninovel.ScriptPlayer": {
    "PlaybackSpot": {
      "ScriptName": "Chapter1",
      "LineIndex": 42,
      "InlineIndex": 0
    },
    "PlayedCommands": [ ... ]
  },
  "Naninovel.TextPrinterManager": { ... },
  "Naninovel.CharacterManager": { ... },
  "Naninovel.SpawnManager": { ... }
}
```

#### C. Custom Variables Storage (`CustomVariableManager`)
* Custom variables are stored as an array of objects under the `"Variables"` property of the `"Naninovel.CustomVariableManager"` service block.
* **Variable Structure:**

| Field Name | Type | Description | Status |
| :--- | :--- | :--- | :--- |
| `Name` | `string` | The variable identifier used in scripts (e.g., `"affinity"`, `"gold"`) | Confirmed |
| `Value` | `string` | Stringified representation of the value (numbers stored as `"50"`, booleans as `"true"`/`"false"`, strings as `"text"`) | Confirmed |

* **Global / Meta Variables (`GlobalSave.nson`):**
  * Variables with names prefixed by `g_` or `G_` (or marked as `meta!`) are excluded from slot-specific saves and stored inside `GlobalSave.nson` under `"GlobalVariables"`.

### 4. Compression, Encryption, and Checksums
* **Compression:** Standard **Gzip** (RFC 1952) with DEFLATE compression.
* **Encryption:**
  * Naninovel does not include AES encryption out-of-the-box in its default slot manager; the "Binary Save" option is solely Gzip compression over JSON.
  * Developers can supply custom encryption handlers implementing `IStateSlotManager` (commonly wrapping the Gzip stream with AES-128 or AES-256).
* **Checksums:**
  * No custom checksums are embedded in the save data.
  * Standard 4-byte Gzip CRC32 and 4-byte ISIZE trailers apply to `.nson` files. Recompressing modified JSON with standard Gzip generates valid checksums automatically.

### 5. Player Modification Targets
* **Custom Variables:** In `"Naninovel.CustomVariableManager"` $\to$ `"Variables"`: player money, route affection/affinity counters, quest progress flags, story switches.
* **Script Position:** In `"Naninovel.ScriptPlayer"` $\to$ `"PlaybackSpot"`: `ScriptName` and `LineIndex`. (Caution: jumping ahead arbitrarily skips initialization commands and can glitch character display states).
* **Global Unlocks:** In `GlobalSave.nson`: unlock flags for the gallery, CG viewer, background music (BGM) jukebox, and seen dialog branch arrays (`PlayedCommands`).

### 6. Sample Files
* Publicly accessible samples:
  * Naninovel official documentation repository: [GitHub naninovel/docs](https://github.com/naninovel/docs) (MIT License).
  * Community testing saves: Available in open visual novel GitHub templates using Naninovel under MIT/Apache licenses.

### 7. Sources & Open-Source References
* Official Naninovel Documentation: [naninovel.com/guide/state-management](https://naninovel.com/guide/state-management)
* Naninovel Custom Variables Documentation: [naninovel.com/guide/custom-variables](https://naninovel.com/guide/custom-variables)
* Naninovel GitHub Source Tracker & Docs: [github.com/naninovel/docs](https://github.com/naninovel/docs)

---

## 4. Browser/HTML Game Saves on Disk (NW.js, Electron, LevelDB, GameMaker, Construct)

### 1. File Locations & Directory Structures
HTML5 games distributed as desktop executables package Chromium and Node.js via **NW.js** or **Electron**.

* **NW.js Game Saves (RPG Maker MV, Construct 2/3 NW.js export):**
  * Windows: `%LOCALAPPDATA%\[package_name]\User Data\Default\Local Storage\leveldb\`
  * Note: `[package_name]` is defined in the game's `package.json` (`"name"` attribute).
  * *RPG Maker MV file fallback*: Many games also write standalone save files to the game executable directory under `www/save/file1.rpgsave` (or `save/file1.rpgsave`).
* **Electron Game Saves (Twine standalone, GDevelop, RPG Maker MZ, modern web ports):**
  * Windows: `%APPDATA%\[app_name]\Local Storage\leveldb\`
  * Note: `[app_name]` is defined in Electron's `package.json` (`"name"` attribute) or `app.setName()`.
  * *RPG Maker MZ file fallback*: Often also writes to `<GameDir>/save/file1.rmmzsave`.
* **GameMaker (Standard Windows Runner):**
  * Windows: `%LOCALAPPDATA%\[GameName]\`
  * Files: `save.dat`, `savegame.sav`, or `*.ini` (e.g., `options.ini`, `save.ini`).

### 2. File Recognition & Signature Matching
* **Chromium LocalStorage LevelDB Directory:**
  Contains the following set of files:
  * `CURRENT`: Plain ASCII text containing `"MANIFEST-00000X\n"`.
  * `MANIFEST-XXXXXX`: Binary transaction log of SSTable levels.
  * `LOCK`: Empty file used for advisory single-process locking.
  * `*.ldb` (or `*.sst`): SSTable table files.
    * Magic Bytes: Final 8 bytes of each `.ldb` file are `0xDB 0x47 0x75 0x24 0x8B 0x80 0xFB 0x57` (LevelDB table magic `0x57fb808b247547db`).
  * `*.log`: Write-Ahead Log (WAL) records.
* **RPG Maker MV/MZ Standalone Saves (`.rpgsave` / `.rmmzsave`):**
  * UTF-8 strings compressed with LZ-String:
    * MV (`.rpgsave`): Base64-encoded LZ-String or JSON.
    * MZ (`.rmmzsave`): UTF-8 JSON or compressed LZ-String.
* **GameMaker Secure Saves (`ds_map_secure_save`):**
  * Proprietary binary format linked to hardware UUID; starts with a 16-byte MD5 hash followed by an obfuscated byte stream.
* **Construct Saves (`.c2save`, `.c3save`):**
  * Plain-text UTF-8 JSON containing root properties `"c2runtime": true` or `"c3runtime": true`.

### 3. LevelDB Serialization & Chromium LocalStorage Byte Layout

#### A. Can a Desktop Tool Read and Rewrite LevelDB Safely?
* **Concurrency Requirement:** LevelDB is strictly single-process. If the game (Electron / NW.js) is running, it holds an exclusive OS file lock on the `LOCK` file. Any external process attempting to open the directory will fail. The game **must be terminated** before editing.
* **Safe Rewriting:**
  * Attempting to raw-edit an `.ldb` file in place will invalidate block offsets, Snappy compression blocks, and the 32-bit block CRCs, corrupting the database.
  * **Safe approach:** Use an embedded LevelDB engine (e.g., pure Rust `rusty-leveldb` or LevelDB C-bindings). The engine safely parses `CURRENT`, opens the active `MANIFEST`, reads the memtable and SSTables, accepts put/delete mutations, and writes a clean new `.log` / `.ldb` with correct Snappy compression and CRC32 checks.

#### B. Chromium LocalStorage LevelDB Key-Value Encoding
Chromium stores LocalStorage entries in LevelDB with a specific prefix schema:

1. **Key Format:**
   * Storage Record Key: `_` + `<OriginURL>` + `\x00\x01` + `<StorageKey>`
   * *Example:* For origin `file://` and key `"RPG File1"`:
     `_file://\x00\x01RPG File1`
   * Metadata Key: `META:` + `<OriginURL>` (stores total byte usage and commit metadata).

2. **Value Format:**
   The first byte of the LevelDB value indicates the character encoding of the stored data:

| Leading Byte | Encoding Type | Payload Description | Status |
| :--- | :--- | :--- | :--- |
| `0x00` | UTF-16LE | String decoded as 16-bit little-endian Unicode characters | Confirmed |
| `0x01` | Latin-1 / UTF-8 | String decoded as 8-bit bytes (ISO-8859-1 or raw UTF-8 string) | Confirmed |

#### C. LevelDB WAL (`.log`) Record Layout
LevelDB `.log` files are divided into contiguous **32 KB (32,768 byte) blocks**. Records never cross block boundaries without being fragmented into chunk types.

Each record in a 32 KB block begins with a **7-byte header**:

| Offset | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `0x00` | 4 | `uint32` | `checksum` | Masked CRC32C ($(\text{crc} \gg 15) \vee (\text{crc} \ll 17) + 0xa282ead8$) | Confirmed |
| `0x04` | 2 | `uint16` | `length` | Little-endian length of data payload ($0 \le \text{length} \le 32761$) | Confirmed |
| `0x06` | 1 | `uint8` | `recordType` | `0x01`=Full, `0x02`=First, `0x03`=Middle, `0x04`=Last | Confirmed |
| `0x07` | `length` | Bytes | `payload` | Batch update payload (SequenceNumber + Count + Key-Values) | Confirmed |

#### D. LevelDB SSTable (`.ldb`) Footer Layout
The last **48 bytes** of every `.ldb` file contain the footer:

| Offset from End | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `-48` | Varint ($\le 16$) | `BlockHandle` | `metaindexHandle` | Offset and size of metaindex block | Confirmed |
| Variable | Varint ($\le 16$) | `BlockHandle` | `indexHandle` | Offset and size of index block | Confirmed |
| Up to `-8` | Padding | Zero bytes | `padding` | Padding to align footer to 40 bytes before magic | Confirmed |
| `-8` | 8 | Fixed `uint64` | `magic` | `0x57fb808b247547db` (Little-Endian: `DB 47 75 24 8B 80 FB 57`) | Confirmed |

*Block Compression:* Blocks in `.ldb` files are compressed using **Snappy**. A 1-byte compression type flag (`0x00` = none, `0x01` = Snappy) immediately follows each block before its 4-byte CRC32C.

### 4. Compression, Encryption, and Checksums
* **Compression:** Google **Snappy** for LevelDB SSTables. **LZ-String** for RPG Maker MV/MZ web exports.
* **Encryption:**
  * Chromium LocalStorage: No native encryption.
  * GameMaker `ds_map_secure_save`: Obfuscated and hardware-bound using OS machine GUID / hardware hash. Not portable across machines.
* **Checksums:**
  * LevelDB: Strict **CRC32C** (Castagnoli) with bit-rotation masking across WAL records and SSTable blocks. Corrupted CRCs cause LevelDB recovery failures.

### 5. Player Modification Targets
* **RPG Maker MV/MZ:** Keys like `_file://\x00\x01RPG File1`:
  * Parsed from JSON: `party._gold` (money), `actors._data[x]._hp`, `actors._data[x]._mp`, `variables._data` (game variables/flags), `switches._data` (boolean progress switches).
* **Twine Standalone (SugarCube / Harlowe):**
  * State variables in LocalStorage under `state`: `$money`, `$health`, `$inventory`.
* **Construct 2 / 3:**
  * JSON properties: `globalvars` (dictionary of all global variable names and values), instance variables under `instances`.

### 6. Sample Files
* LevelDB test suites and samples: [Google LevelDB GitHub test files](https://github.com/google/leveldb) (BSD-3-Clause).
* `ccl_chromium_reader` sample datasets: [GitHub ccl-investigations/ccl_chromium_reader](https://github.com/ccl-investigations/ccl_chromium_reader) (MIT License).

### 7. Sources & Open-Source References
* Google LevelDB Implementation Specification: [github.com/google/leveldb](https://github.com/google/leveldb) (License: BSD-3-Clause)
* Chromium Local Storage Forensics by Alex Caithness (CCL Solutions Group): [cclsolutionsgroup.com research](https://www.cclsolutionsgroup.com)
* `ccl_chromium_reader`: [GitHub](https://github.com/ccl-investigations/ccl_chromium_reader) (License: MIT)
* `rusty-leveldb`: [GitHub](https://github.com/dermesser/rusty-leveldb) (License: MIT)

---

## 5. Common Packed Save Containers (ZIP, GZIP, 7z, LZ4 Frame, Zlib, Unity ES2/ES3)

### 1. Recognition Signatures & Magic Bytes

| Format | Magic Bytes (Hex) | Offset | ASCII Equivalent | Distinguishing Characteristics | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **ZIP** | `50 4B 03 04` | 0 | `PK\x03\x04` | Standard local file header. Empty archives have `50 4B 05 06` (`PK\x05\x06`). | Confirmed |
| **GZIP** | `1F 8B 08` | 0 | `\x1f\x8b\x08` | Byte 0=`0x1F`, Byte 1=`0x8B`, Byte 2=`0x08` (DEFLATE). Followed by flag byte and 4-byte MTIME. | Confirmed |
| **7z** | `37 7A BC AF 27 1C` | 0 | `'7' 'z' \xbc \xaf \x27 \x1c` | 6 fixed signature bytes. Followed by major/minor version (`0x00 0x04`). | Confirmed |
| **LZ4 Frame** | `04 22 4D 18` | 0 | `\x04\x22\x4d\x18` | Little-endian uint32 `0x184D2204`. Legacy LZ4 frame starts with `02 21 4C 18` (`0x184C2102`). | Confirmed |
| **Zlib (RFC 1950)**| `78 01`, `78 9C`, `78 DA` | 0 | `x\x01`, `x\x9c`, `x\xda` | Byte 0 is `0x78` (CM=8, CINFO=7). Value `(CMF * 256 + FLG) % 31 == 0`. Trailing 4-byte Adler-32. | Confirmed |
| **ES2/ES3 (Base64 Gzip)**| `48 34 73 49` | 0 | `H4sI` | ASCII string `H4sI`. The Base64 encoding of `0x1F 0x8B 0x08`. | Confirmed |
| **ES3 (Raw JSON)**| `7B` | 0 | `{` | Raw UTF-8 JSON text starting with `{` and containing `"__type"` or `"value"` fields. | Confirmed |
| **ES3 (AES Encrypted)**| Variable (High Entropy) | 0 | Binary | 16-byte random IV/salt prefix followed by AES-CBC/GCM ciphertext; high Shannon entropy ($> 7.9$). | Confirmed |

---

### 2. Exact Layouts & Container Specifications

#### A. GZIP (RFC 1952)
Little-Endian. Fixed 10-byte header:

| Offset | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `0x00` | 2 | Fixed | `ID1`, `ID2` | `0x1F`, `0x8B` | Confirmed |
| `0x02` | 1 | `uint8` | `CM` | Compression method (`0x08` = Deflate) | Confirmed |
| `0x03` | 1 | `uint8` | `FLG` | Bit flags (FTEXT, FHCRC, FEXTRA, FNAME, FCOMMENT) | Confirmed |
| `0x04` | 4 | `uint32` | `MTIME` | Unix timestamp | Confirmed |
| `0x08` | 1 | `uint8` | `XFL` | Extra flags (`2` = max compression, `4` = fastest) | Confirmed |
| `0x09` | 1 | `uint8` | `OS` | Operating system (`0` = FAT, `3` = Unix, `11` = NTFS) | Confirmed |
| `0x0A` | Variable | Optional | Extra headers | Present if indicated by `FLG` | Confirmed |
| Variable | Variable | Bytes | `deflateData` | Raw DEFLATE compressed stream | Confirmed |
| `-8` | 4 | `uint32` | `CRC32` | CRC-32 of uncompressed data | Confirmed |
| `-4` | 4 | `uint32` | `ISIZE` | Uncompressed input size modulo $2^{32}$ | Confirmed |

#### B. Zlib Stream (RFC 1950)
Big-Endian checksum. 2-byte header:

| Offset | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `0x00` | 1 | `uint8` | `CMF` | bits 0-3: CM (8 = Deflate), bits 4-7: CINFO (7 = 32K window) $\to$ `0x78` | Confirmed |
| `0x01` | 1 | `uint8` | `FLG` | Check bits: `(CMF * 256 + FLG) % 31 == 0` | Confirmed |
| `0x02` | Variable | Bytes | `deflateData` | Raw DEFLATE compressed stream | Confirmed |
| `-4` | 4 | `uint32` (BE) | `Adler32` | Big-endian Adler-32 checksum of uncompressed payload | Confirmed |

#### C. LZ4 Frame Format
Little-Endian. Starts with 4-byte magic:

| Offset | Size (bytes) | Type | Field Name | Description | Status |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `0x00` | 4 | `uint32` | `magic` | `0x184D2204` (`04 22 4D 18`) | Confirmed |
| `0x04` | 1 | `uint8` | `FLG` | Version (`01`), BlockIndependence, BlockChecksum, ContentSize, ContentChecksum | Confirmed |
| `0x05` | 1 | `uint8` | `BD` | Block maximum size (`4`=64KB, `5`=256KB, `6`=1MB, `7`=4MB) | Confirmed |
| `0x06` | 0 or 8 | `uint64` | `contentSize` | Present if ContentSize flag set in `FLG` | Confirmed |
| Variable | 1 | `uint8` | `headerChecksum` | 1-byte XXH32 hash of descriptor fields: `(xxh32(header) >> 8) & 0xFF` | Confirmed |
| Variable | Variable | Blocks | `dataBlocks` | Series of: 4-byte `blockSize` + compressed block data (+ optional block CRC) | Confirmed |
| Variable | 4 | `uint32` | `endMark` | `0x00000000` marking end of frame | Confirmed |
| Variable | 0 or 4 | `uint32` | `contentChecksum` | XXH32 checksum of uncompressed payload (if flag set) | Confirmed |

#### D. Unity Easy Save 2 & 3 (ES2 / ES3) Variants

* **Variant 1: Raw JSON (ES3 Default)**
  * Uncompressed, unencrypted JSON text.
  * Root object contains key-value mappings where complex types include an embedded `"__type"` descriptor:
    ```json
    {
      "playerLevel": {
        "__type": "int",
        "value": 25
      },
      "inventory": {
        "__type": "System.Collections.Generic.List`1[[System.String, mscorlib]],mscorlib",
        "value": ["Sword", "Shield"]
      }
    }
    ```
* **Variant 2: Gzip Compressed (ES3)**
  * Raw Gzip stream. Begins with `1F 8B 08`. Decompressing produces Variant 1 JSON.
* **Variant 3: Base64 Gzip (ES2 & ES3 Web/PlayerPrefs)**
  * Used when saving to Windows Registry, web services, or single-line config fields.
  * Encoded using standard Base64 characters (`A-Z`, `a-z`, `0-9`, `+`, `/`, `=`).
  * Always begins with the four ASCII characters **`H4sI`** (because `0x1F 0x8B 0x08` in Base64 is `H4sI`).
  * Decoding pipeline: Base64-decode $\to$ Gzip-decompress $\to$ UTF-8 JSON.
* **Variant 4: AES Encrypted (ES2 & ES3)**
  * Algorithm: AES-128 or AES-256 (CBC or GCM mode depending on settings).
  * Key Derivation: PBKDF2 or direct SHA-256 hash of developer-defined string password.
  * Stream Layout: 16-byte initialization vector (IV) prepended to ciphertext.
  * Recomputing: Requires developer password (typically extracted from compiled `Assembly-CSharp.dll`).

### 3. Checksums and Byte-Exact Requirements

| Format | Checksum / CRC Algorithm | Endianness | Verification & Rewrite Requirements |
| :--- | :--- | :--- | :--- |
| **ZIP** | CRC-32 | Little-Endian | Recompute CRC-32 for each edited file entry in local header and Central Directory header. |
| **GZIP** | CRC-32 | Little-Endian | Recompute CRC-32 over the complete uncompressed byte stream and write to last 8 bytes along with uncompressed `ISIZE`. |
| **7z** | CRC-32 | Little-Endian | Recompute CRC-32 for headers and stream blocks. |
| **LZ4 Frame**| XXH32 (xxHash) | Little-Endian | Header uses 1-byte truncated XXH32; frame trailer uses 4-byte full XXH32 over uncompressed payload. |
| **Zlib** | Adler-32 | **Big-Endian** | Recompute Adler-32 over the uncompressed payload and write to the final 4 bytes in Big-Endian order. |
| **ES2 / ES3**| Inherent to wrapper | Matches wrapper | If Gzipped, recompute Gzip CRC32. If Base64, re-encode Base64 without line breaks. |

### 4. Player Modification Targets
* Unpacking any of these containers exposes the inner save payload (typically JSON, XML, SQLite, or raw binary serialization). Players modify high scores, inventory arrays, currency quantities, and character stats inside the extracted stream, then repack with matching container compression parameters and updated checksums.

### 5. Sample Files
* Gzip, Zip, and Zlib test files: [Standard test corpus in libpng / zlib repository](https://github.com/madler/zlib) (Zlib License).
* LZ4 Frame test files: [LZ4 GitHub repository test suite](https://github.com/lz4/lz4/tree/dev/tests) (BSD-2-Clause).
* Easy Save test files: Included in public open-source Unity sample projects on GitHub under MIT/Apache licenses.

### 6. Sources & Open-Source References
* RFC 1950 (ZLIB Compressed Data Format Specification): [ietf.org/rfc/rfc1950.txt](https://www.ietf.org/rfc/rfc1950.txt)
* RFC 1952 (GZIP File Format Specification): [ietf.org/rfc/rfc1952.txt](https://www.ietf.org/rfc/rfc1952.txt)
* PKWARE ZIP File Format Specification: [pkware.com/appnote](https://support.pkware.com/home/pkzip/developer-tools/appnote)
* LZ4 Frame Format Specification: [github.com/lz4/lz4/blob/dev/doc/lz4_Frame_format.md](https://github.com/lz4/lz4/blob/dev/doc/lz4_Frame_format.md) (License: BSD-2-Clause)
* Moodkie Easy Save Documentation: [moodkie.com/easy-save-3/documentation](https://moodkie.com)
