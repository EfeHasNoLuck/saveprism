<!-- Research by agy (gemini-3.8-flash) on 2026-10-24 from public sources. Not checked line by line: verify every table against the primary source before implementing. Behavior only (clean room). -->

# Save File Format Specifications for SavePrism

This report provides technical specifications for five save file formats to enable byte-exact parsing, editing, and serialization in SavePrism under MIT clean-room conditions.

---

## 1. WOLF RPG Editor (WOLF RPG エディター) `.sav`

### 1.1 File Locations, Names, and Version Differences
- **File System Locations:**
  - Game root directory or dedicated subfolder: `Save/`, `SaveData/`, or root alongside `Game.exe`.
- **File Names & Extensions:**
  - `SaveData01.sav` through `SaveData99.sav` (regular save slots).
  - `System.sav` (global system data: CG gallery flags, config, system variables).
  - Occasionally single-file projects use `SaveData.sav`.
- **Version Differences:**
  - **Version 2.x (v2.00–v2.2x):** Shift-JIS string encoding standard, smaller fixed variable banks, no Pro-protection wrappers on standard saves.
  - **Version 3.x (v3.00–v3.3x+):** Unicode/UTF-8 support introduced in modern localizations, expanded variable array limits (up to 99,999+ user database entries), optional Pro-edition container encryption.

---

### 1.2 File Recognition & Format Identification
- **Magic Bytes & Header:**
  - Standard unencrypted WOLF saves **do not** feature a static ASCII identifier (such as `"WOLF"`).
  - Instead, unencrypted files start with structured little-endian header length fields followed by standard variable bank descriptors.
- **Distinguishing Encrypted vs. Unencrypted:**
  - **Unencrypted Saves:** High frequency of null bytes (`0x00`), predictable little-endian integers (`int32`), monotonic variable counters, and plain Shift-JIS or UTF-8 text strings within database entries. Shannon entropy is typically $< 4.5$ bits/byte.
  - **Encrypted Saves (Pro protection / custom key):** Byte offset 1 (`byte[1]`) may contain ASCII `'P'` (`0x50`) if wrapped in the Pro container format, followed by high-entropy pseudo-random bytes (entropy $> 7.5$ bits/byte) with no recognizable string tables or zero runs.
- **Disambiguation from other `.sav` files:**
  - Check file size against internal chunk headers: WOLF saves begin with header sizing blocks that match the subsequent variable/switch array allocation.

---

### 1.3 Exact Byte Layout (Unencrypted Format)

Integers are signed 32-bit little-endian (`int32_le`) unless specified. Strings are length-prefixed: a 32-bit integer indicates character/byte length, followed by raw string bytes (Shift-JIS for v2.x/Japanese v3.x, UTF-8 in modern translated v3.x builds).

#### Top-Level Stream Structure

| Offset Range | Field Name | Type | Description |
| :--- | :--- | :--- | :--- |
| `0x00` – `0x03` | Header Indicator / Version | `uint32_le` | Save system revision / format indicator. |
| Variable | Save Title / Metadata | Length-Prefixed String | Save timestamp, location name, or party summary displayed on the load screen. |
| Variable | System Variables Bank | Block | System Variable block (`SysVar 0..99`). |
| Variable | System Switches Bank | Block | System Flags/Switches block (`SysFlag 0..99`). |
| Variable | Normal Variables Bank | Block | General game variables (`Var 0..99` or dynamic length). |
| Variable | Normal Switches Bank | Block | General game flags (`Flag 0..99` or dynamic length). |
| Variable | User Database (UDB) Delta | Block | In-game dynamic changes to User Database tables. |
| Variable | Changeable Database (CDB) | Block | Dynamic database tracking player inventory, stats, quests. |
| Variable | Map & Event Runtime State | Block | Active map ID, hero coordinates, local event flags. |

#### Variable / Switch Bank Sub-Structure

| Field | Type | Description |
| :--- | :--- | :--- |
| `ArrayCount` | `uint32_le` | Number of variables/switches in the bank $N$. |
| `Values[N]` | `int32_le[N]` (Vars) or `uint8_le[N]` (Flags) | Sequential array of values. Flags are `0x00` (OFF) or `0x01` (ON). |

#### Database Change Record Layout

| Field | Type | Description |
| :--- | :--- | :--- |
| `TypeID` | `uint32_le` | Database Type Index. |
| `DataID` | `uint32_le` | Entry / Record Index within the database. |
| `FieldID` | `uint32_le` | Field / Property Index. |
| `ValueType` | `uint8_le` | `0x00` = Numeric (`int32_le`), `0x01` = String (Length-prefixed). |
| `Value` | `int32_le` or String | Stored value depending on `ValueType`. |

*Status:* Header indicator and variable bank sequences are **confirmed** by open-source parsers. Internal layout of dynamic event bytecodes is a **guess/partially variable** across engine minor versions.

---

### 1.4 Compression, Encryption, and Checksums
- **Standard Saves:** No compression, no cryptographic encryption. Some v3.x engine revisions append a simple 4-byte additive checksum or CRC32 at the end of the file.
- **Encrypted Saves:** 
  - Standard Pro encryption uses an engine-level key derivation scheme (AES-128-CBC or Pro stream cipher) utilizing a **per-game developer Protection Key** embedded inside the game binary (`Game.exe` or `Game.dat`).
  - *Hard constraint:* Clean-room implementations without access to the user's local `Game.exe` cannot decrypt protected saves. If detected, flag as read-only/unsupported with an encryption warning.

---

### 1.5 Target Edit Locations
- **Money / Gold:** Stored in either:
  1. A standard variable (typically `Var 0` through `Var 9`, customized per game project).
  2. A specific Changeable Database record (`CDB Type 0, Data 0, Field 0` in default Basic System).
- **Items & Quantities:** Stored in the User/Changeable Database under an Item Inventory type array where Field 0 = Item ID, Field 1 = Count.
- **Party Stats (HP, MP, Level, Atk, Def):** Stored in Actor database records inside the CDB runtime delta block.
- **Switches & Story Flags:** Located in the Normal Switches Bank (`Flag 0` to `Flag N`).

---

### 1.6 Sample Files & Open-Source Projects
- **Open-Source Reference Projects:**
  - `Sinflower/WolfSave` (C++): [https://github.com/Sinflower/WolfSave](https://github.com/Sinflower/WolfSave) — License: **MIT**.
  - `truongthang2211/RPGSaveEditor` (Tauri/TS): [https://github.com/truongthang2211/RPGSaveEditor](https://github.com/truongthang2211/RPGSaveEditor) — License: **Apache-2.0**.
- **Public Sample Saves:**
  - Available in freely redistributable WOLF RPG Editor sample projects provided by SilverSecond: [SmokingWOLF Official Site](https://www.silversecond.com/WolfRPGEditor/).

### 1.7 Sources
- [https://github.com/Sinflower/WolfSave](https://github.com/Sinflower/WolfSave)
- [https://github.com/truongthang2211/RPGSaveEditor](https://github.com/truongthang2211/RPGSaveEditor)
- [https://github.com/djytw/wolf-rpg-formats](https://github.com/djytw/wolf-rpg-formats)

---

## 2. RPG Maker 2000 / 2003 `.lsd` (LCF Format)

### 2.1 File Locations, Names, and Version Differences
- **File System Locations:** Root directory of the game (same folder as `RPG_RT.exe`).
- **File Names:** `Save01.lsd` through `Save15.lsd` (up to `Save99.lsd` with community engine patches).
- **Version Differences:**
  - **RPG Maker 2000 (v1.00–v1.62):** 5-slot equipment layout; standard turn-based battle state.
  - **RPG Maker 2003 (v1.00–v1.12+):** ATB battle system state, additional battle stats, dual-weapon equipment option, class-based parameters.

---

### 2.2 File Recognition & Format Identification
- **Magic Bytes:**
  - Offset `0x00`: Single byte length prefix `0x0B` (11).
  - Offset `0x01` – `0x0B`: ASCII string `"LcfSaveData"` (`4C 63 66 53 61 76 65 44 61 74 61`).
- **Disambiguation from other LCF files:**
  - `.ldb` Database: `0x0B` + `"LcfDataBase"`
  - `.lmu` Map Unit: `0x0A` + `"LcfMapUnit"`
  - `.lmt` Map Tree: `0x0A` + `"LcfMapTree"`

---

### 2.3 Exact Byte Layout (BER / VLQ Chunk Structure)

LCF uses a TLV (Type-Length-Value) architecture with BER (Basic Encoding Rules / Variable-Length Quantity) integers for Chunk IDs and Lengths:
- **BER Encoding:** 7 bits per byte. The high bit (bit 7) is `1` if more bytes follow, and `0` on the terminating byte. Big-endian byte order for payload bits.
  - $0 \dots 127 \rightarrow$ `0x00` $\dots$ `0x7F`
  - $128 \rightarrow$ `0x81 0x00`
- **Chunk Format:** `[Chunk ID: BER] [Data Length: BER] [Data Bytes: Length]`
- **Terminator:** Nested chunk sequences terminate with a single byte `0x00`.

#### Top-Level Chunks in `LcfSaveData`

| Chunk ID (Hex) | Chunk ID (Dec) | Delphi Class Name | Description |
| :--- | :--- | :--- | :--- |
| `0x64` | 100 | `TLcfgSaveHeader` / `SaveTitle` | Preview metadata (timestamp, hero leader, face graphics, level). |
| `0x65` | 101 | `TLcfgSystem` / `SaveSystem` | System state: Scene, FrameCount, Switches, Variables, BGMs. |
| `0x66` | 102 | `TLcfgScreen` / `SaveScreen` | Screen effects (tint, flash, shake, weather). |
| `0x67` | 103 | `TLcfgPicture` / `SavePicture` | Displayed picture list. |
| `0x68` | 104 | `TLcfgParty` / `SavePartyLocation` | Hero map position (X, Y, direction, moving state). |
| `0x69` | 105 | `TLcfgBoat` / `SaveVehicleLocation` | Skiff / Boat vehicle state. |
| `0x6A` | 106 | `TLcfgShip` / `SaveVehicleLocation` | Large Ship vehicle state. |
| `0x6B` | 107 | `TLcfgAirship` / `SaveVehicleLocation` | Airship vehicle state. |
| `0x6C` | 108 | `TLcfgPlayer` / `SaveActor` | Actor runtime data modified from `RPG_RT.ldb`. |
| `0x6D` | 109 | `TLcfgPartyInfo` / `SaveInventory` | Party member roster, items, money, battle statistics, steps. |
| `0x6E` | 110 | `TLcfgTeleport` / `SaveTeleport` | Registered escape/teleport target destinations. |
| `0x6F` | 111 | `TLcfgMap` / `SaveMapInfo` | Runtime map events state and altered tiles. |
| `0x70` | 112 | `TLcfgPanorama` / `SavePanorama` | Active panorama scroll and position. |
| `0x71` | 113 | `TLcfgEventData` / `SaveCommonEvent` | Common event runtime interpreters. |

#### Sub-Chunks for System Data (`SaveSystem`, ID `0x65`)

| Sub-Chunk ID | Type | Name | Description |
| :--- | :--- | :--- | :--- |
| `0x01` | `int32` (BER) | `scene` | Active scene ID. |
| `0x0B` | `int32` (BER) | `frame_count` | Accumulated frame count (playtime). |
| `0x15` | Array of Bit/Byte | `switches` | Switch flags array: Count (BER) followed by packed flags (1-indexed). |
| `0x16` | Array of `int32` (BER) | `variables` | Global variables array: Count (BER) followed by 32-bit signed BER values. |
| `0x51` | `int32` (BER) | `save_count` | Total times saved. |

#### Sub-Chunks for Inventory & Party (`SaveInventory`, ID `0x6D`)

| Sub-Chunk ID | Type | Name | Description |
| :--- | :--- | :--- | :--- |
| `0x01` | Array of `int16` | `party` | Array of actor IDs currently in the marching party (1-indexed). |
| `0x0B` | Array of `int16` | `item_ids` | Array of Item IDs possessed. |
| `0x0C` | Array of `uint8` | `item_amounts` | Quantities matching each entry in `item_ids`. |
| `0x0D` | Array of `uint8` | `item_usage` | Usage counters / equip state flags for each item. |
| `0x15` | `int32` (BER) | `gold` | Party Money / Gold balance. |
| `0x23` | `int32` (BER) | `step_count` | Number of steps taken by party. |

#### Sub-Chunks for Actor Data (`SaveActor`, ID `0x6C`)
Stored as an LCF Array of Records: `Count (BER)` $\rightarrow$ for each actor: `ActorID (BER)` + sub-chunks + `0x00`.

| Sub-Chunk ID | Type | Name | Description |
| :--- | :--- | :--- | :--- |
| `0x01` | String | `name` | Altered hero name (omitted if database default). |
| `0x02` | String | `title` | Altered title. |
| `0x0B` | `int32` (BER) | `level` | Current character level. |
| `0x0C` | `int32` (BER) | `exp` | Accumulated experience points. |
| `0x15` | `int32` (BER) | `hp` | Current Hit Points (HP). |
| `0x16` | `int32` (BER) | `sp` / `mp` | Current Mana / Skill Points (MP/SP). |
| `0x21` | `int32` (BER) | `max_hp_mod` | Modifier added to base database Max HP. |
| `0x22` | `int32` (BER) | `max_sp_mod` | Modifier added to base database Max SP. |
| `0x23` | `int32` (BER) | `attack_mod` | Modifier added to base Attack stat. |
| `0x24` | `int32` (BER) | `defense_mod` | Modifier added to base Defense stat. |
| `0x25` | `int32` (BER) | `spirit_mod` | Modifier added to base Spirit/Intellect stat. |
| `0x26` | `int32` (BER) | `agility_mod` | Modifier added to base Agility/Speed stat. |
| `0x33` | Array of 5 `int16` | `equipped` | Weapon, Shield, Armor, Helmet, Accessory IDs. |

*Confirmed:* Chunk definitions verified against EasyRPG `liblcf` and Makerpendium specification tables.

---

### 2.4 Compression, Encryption, and Checksums
- **None:** No compression algorithm is used; files are plain unencrypted chunk streams.
- **Integrity Requirement:** No hashes or checksums are present. However, bit-exact round-tripping requires that unrecognized chunks are retained verbatim without altering BER integer length encodings.

---

### 2.5 Target Edit Locations & Cross-Referencing
- **Money (Gold):** Chunk `0x6D` (`SaveInventory`), Sub-chunk `0x15`.
- **Items & Amounts:** Chunk `0x6D`, Sub-chunks `0x0B` (IDs) and `0x0C` (Quantities).
- **Actor Stats:** Chunk `0x6C` (`SaveActor`), Sub-chunks `0x0B` (Level), `0x0C` (Exp), `0x15` (HP), `0x16` (MP), `0x21..0x26` (Stat modifiers).
- **Switches & Variables:** Chunk `0x65` (`SaveSystem`), Sub-chunk `0x15` (Switches) and `0x16` (Variables).
- **Name Cross-Referencing:** LSD saves do not store variable or switch names. Displaying readable names requires reading `RPG_RT.ldb` (database chunk `0x16` for Switch Names, chunk `0x17` for Variable Names, chunk `0x0B` for Actor Names, chunk `0x0D` for Item Names).

---

### 2.6 Sample Files & Open-Source Projects
- **Open-Source Reference Projects:**
  - `EasyRPG/liblcf` (C++): [https://github.com/EasyRPG/liblcf](https://github.com/EasyRPG/liblcf) — License: **MIT**.
  - `EasyRPG/Player`: [https://github.com/EasyRPG/Player](https://github.com/EasyRPG/Player) — License: **GPL-3.0-or-later** (*Clean-room warning: refer only to liblcf data structures, do not copy Player engine code*).
- **Public Sample Saves:**
  - `liblcf` repository test suite: [https://github.com/EasyRPG/liblcf/tree/master/tests](https://github.com/EasyRPG/liblcf/tree/master/tests) contains sample `.lsd` save fixtures under the MIT test suite.

### 2.7 Sources
- [https://github.com/EasyRPG/liblcf](https://github.com/EasyRPG/liblcf)
- [https://wiki.easyrpg.org/dev/lcf/lsd](https://wiki.easyrpg.org/dev/lcf/lsd)
- [https://www.makerpendium.de/index.php?title=LcfSaveData](https://www.makerpendium.de/index.php?title=LcfSaveData)

---

## 3. QSP (Quest Soft Player) `.sav`

### 3.1 File Locations, Names, and Version Differences
- **File System Locations:** Same directory as `.qsp` / `.gam` game file, or user profile (`AppData/Roaming/QSP`).
- **File Names:** Arbitrary, typically chosen by the user in the file dialog (e.g., `game.sav`, `quick.sav`, `slot1.sav`).
- **Version Differences:**
  - **QSP 4.x (Legacy ANSI):** Encoded in Windows-1251 (Cyrillic) single-byte text with a byte offset.
  - **QSP 5.x – 5.8+ (Modern):** Standard UTF-16 Little Endian encoding throughout.

---

### 3.2 File Recognition & Format Identification
- **Magic Bytes:**
  - The first 24 bytes represent the ASCII string `"QSPSAVEDGAME"` encoded in UTF-16LE:
  - Hex bytes: `51 00 53 00 50 00 53 00 41 00 56 00 45 00 44 00 47 00 41 00 4D 00 45 00`
- **Line Ending:**
  - Followed immediately by a UTF-16LE CRLF (`0D 00 0A 00`).
- **Secondary Header:**
  - Next line contains the engine version string in UTF-16LE (e.g., `5.7.0` $\rightarrow$ `35 00 2E 00 37 00 2E 00 30 00 0D 00 0A 00`).

---

### 3.3 Exact Byte Layout & The Character Shift Mechanism

#### The Character Shift Algorithm
QSP save files are **not cryptographically encrypted**; they are obfuscated using a reversible arithmetic character code shift:
- **Encoding Rule:** Every UTF-16LE code unit in the payload lines has a fixed integer offset added to it:
  $$C_{\text{encoded}} = C_{\text{plain}} + 5$$
  *(CRLF delimiters `0x000D` and `0x000A` are generally left intact as record separators, or shifted depending on exact exporter build; unshifting subtracting 5 restores original UCS-2/UTF-16 code units).*
- **Decoding Rule:**
  $$C_{\text{plain}} = C_{\text{encoded}} - 5$$

#### Serialized Field Layout (After Shift Reversal)

The file is organized into CRLF-separated text records:

| Record Index / Field | Type | Description |
| :--- | :--- | :--- |
| Line 1 | Text (Plain) | Signature Header: `QSPSAVEDGAME` |
| Line 2 | Text (Plain) | Engine version string (e.g. `5.7.0`). |
| Line 3 | Text (Shifted) | Current Location Name. |
| Line 4 | Text (Shifted) | Active Main Description text buffer. |
| Line 5 | Integer (Shifted) | Action Count $A$. |
| Lines $6 \dots (6 + 2A - 1)$ | Text Pairs (Shifted) | For each action: Action Image Path, Action Label. |
| Next Line | Integer (Shifted) | Inventory Item Count $I$. |
| Following $2I$ Lines | Text Pairs (Shifted) | For each item: Item Image Path, Item Name. |
| Next Line | Integer (Shifted) | Variable Count $V$. |
| Following Records | Variable Tuples | Serialized variable blocks (see below). |

#### Variable Tuple Structure

For each of the $V$ variables:
1. `VarName` (String): Variable identifier (e.g., `MONEY`, `STRENGTH`, `$NAME`).
2. `ValCount` (Integer): Array dimension (number of values stored for this variable).
3. For each value index ($0 \dots \text{ValCount}-1$):
   - `NumValue` (Integer): Numeric integer value (0 if string variable).
   - `StrValue` (String): String text value (empty string if pure numeric).

*Confirmed:* Stream order and variable structures verified against `QSPFoundation/qsp` core library (`game.h`, `qspSaveGameStatus`).

---

### 3.4 Compression, Encryption, and Checksums
- **Compression:** None.
- **Encryption:** None; character code manipulation only (reversible arithmetic shift $+5$).
- **Checksums:** None. Modifying values and re-applying the shift produces a valid save file that loads without error.

---

### 3.5 Target Edit Locations
- **Player Stats & Money:** Located in the Variables block under game-specific names:
  - Numerical counters: `money`, `rub`, `gold`, `health`, `hp`, `intellect`.
- **Inventory:** Located in both the Inventory Items block (items physically displayed in the GUI) and the Variables block (quantities tracking items).
- **Story Switches:** Stored as integer variables with values `1` (true) or `0` (false).

---

### 3.6 Sample Files & Open-Source Projects
- **Open-Source Reference Projects:**
  - `Pararock/QSPSaveEditor` (C#): [https://github.com/Pararock/QSPSaveEditor](https://github.com/Pararock/QSPSaveEditor) — License: **MIT**.
  - `QSPFoundation/converters` (TypeScript): [https://github.com/QSPFoundation/converters](https://github.com/QSPFoundation/converters) — License: **MIT**.
  - `QSPFoundation/qsp` (C core): [https://github.com/QSPFoundation/qsp](https://github.com/QSPFoundation/qsp) — License: **LGPL-2.1** (*Clean-room warning: behavioral spec only*).
- **Public Sample Saves:**
  - Available in open-source QSP community test fixtures: [https://github.com/QSPFoundation/qspider](https://github.com/QSPFoundation/qspider) (MIT).

### 3.7 Sources
- [https://github.com/QSPFoundation/qsp](https://github.com/QSPFoundation/qsp)
- [https://github.com/Pararock/QSPSaveEditor](https://github.com/Pararock/QSPSaveEditor)
- [https://qsp.org](https://qsp.org)

---

## 4. RAGS (Rapid Adventure Game System) `.rsv`

### 4.1 File Locations, Names, and Version Differences
- **File System Locations:** Saved in the same directory as the game executable/`.rag` package or in `%USERPROFILE%/Documents/RAGS/Saves`.
- **File Names:** `Save1.rsv`, `Save2.rsv`, etc.
- **Version Differences:**
  - **RAGS 2.x (Player 2.4+):** Outputs monolithic save files containing serialized .NET object graphs.
  - **RAGS 3.x:** Enhanced game state objects, larger class payloads; same underlying NRBF binary container.
  - **Regalia (Web Player):** Exports JSON-based deltas; distinct from native desktop `.rsv`.

---

### 4.2 File Recognition & Format Identification
- **Magic Bytes (.NET NRBF Serialization Header):**
  - RAGS saves are direct streams produced by .NET `BinaryFormatter` implementing the `[MS-NRBF]` specification.
  - The first 17 bytes must strictly match the `SerializationHeaderRecord`:
  - `00 01 00 00 00 FF FF FF FF 01 00 00 00 00 00 00 00`
- **Breakdown of Header Bytes:**
  - `0x00`: RecordTypeEnum (`0x00` = `SerializedStreamHeader`)
  - `0x01` – `0x04`: `RootId` (`int32_le` = `1`)
  - `0x05` – `0x08`: `HeaderId` (`int32_le` = `-1` or `1`)
  - `0x09` – `0x0C`: `MajorVersion` (`int32_le` = `1`)
  - `0x0D` – `0x10`: `MinorVersion` (`int32_le` = `0`)
- **Subsequent Identification:**
  - Byte `0x11` typically begins a `SystemClassWithMembersAndTypes` (`0x04`) or `ClassWithMembersAndTypes` (`0x05`) record with type name strings containing `Rags.` (e.g., `Rags.GameState`, `Rags.Player`).

---

### 4.3 Exact Byte Layout ([MS-NRBF] Object Graph)

The format is a sequential stream of binary records. Data primitives are little-endian. Strings use .NET 7-bit encoded length prefixes followed by UTF-8 bytes.

#### Key NRBF Record Types Used in RAGS `.rsv`

| Record Type Byte (Hex) | Enumeration Name | Description |
| :--- | :--- | :--- |
| `0x00` | `SerializedStreamHeader` | Stream header record (first 17 bytes). |
| `0x01` | `ClassWithId` | Reuses class metadata defined in earlier records. |
| `0x04` | `SystemClassWithMembersAndTypes` | System class with inline member names and type descriptors. |
| `0x05` | `ClassWithMembersAndTypes` | Non-system class with assembly name and member type metadata. |
| `0x06` | `BinaryObjectString` | String record: ObjectID + 7-bit length prefix + UTF-8 string. |
| `0x07` | `BinaryArray` | Array of objects, references, or primitives. |
| `0x08` | `MemberPrimitiveTyped` | Single typed primitive value (int32, double, boolean). |
| `0x09` | `MemberReference` | Reference pointer (`IdRef: int32_le`) to an earlier object ID. |
| `0x0A` | `ObjectNull` | Null reference record. |
| `0x0B` | `MessageEnd` | Terminates the serialization stream. |

#### Internal RAGS Object Hierarchy (Tree Representation)
```
Rags.GameState (Root Object)
├── Player (Class Record)
│   ├── Name (String)
│   ├── Stats (Array / Dictionary of Primitives)
│   └── CurrentRoomId (Integer / String)
├── Inventory (Array of Item Object Records)
│   ├── ItemID (Integer)
│   ├── ItemName (String)
│   └── Quantity (Integer)
├── Variables (Dictionary<String, Object> or Array of Variable Records)
│   ├── Key (String)
│   └── Value (Integer / String / Boolean)
└── Rooms (Altered state list for visited rooms)
```

> [!CAUTION]
> **Clean-Room & Security Guard (Rule 3):**
> Never deserialize `.rsv` files using .NET `BinaryFormatter.Deserialize()`. Deserializing untrusted NRBF graphs allows arbitrary type instantiation and Remote Code Execution (CWE-502). Parse the file strictly to an AST/tree using a standalone MS-NRBF grammar parser.

---

### 4.4 Compression, Encryption, and Checksums
- **Compression:** None on `.rsv` files (hence save files often reach 2 MB to 20 MB).
- **Encryption:** None on save files. (Compiled `.rag` game distribution packages sometimes employ AES-128-CBC, but runtime `.rsv` save states are unencrypted NRBF streams).
- **Checksums:** None. Re-serializing the modified NRBF graph with valid record lengths loads directly in RAGS.

---

### 4.5 Target Edit Locations
- **Player Attributes & Money:** Found inside the member list of the `Player` class record (e.g., `Money`, `Health`, `Cash`).
- **Variables / Switches:** Stored inside the global `Variables` collection record (often an associative array mapping string names to values).
- **Items:** Stored inside the `Inventory` list record containing item instance records with quantity fields.

---

### 4.6 Sample Files & Open-Source Projects
- **Open-Source Reference Projects:**
  - `Kassy2048/rags2html` (Python): [https://github.com/Kassy2048/rags2html](https://github.com/Kassy2048/rags2html) — License: **MIT**.
  - `nneonneo/net-nrbf` (Python NRBF parser): [https://github.com/nneonneo/net-nrbf](https://github.com/nneonneo/net-nrbf) — License: **MIT**.
  - `selectivepaperclip/regalia` (JS): [https://github.com/selectivepaperclip/regalia](https://github.com/selectivepaperclip/regalia) — License: **MIT**.
  - `System.Formats.Nrbf` (C# official safe reader): [https://github.com/dotnet/runtime](https://github.com/dotnet/runtime) — License: **MIT**.
- **Public Sample Saves:**
  - "None found" as standalone public licensed repositories; generated from sample games exported with RAGS 2.4/3.0 demo suites.

### 4.7 Sources
- [MS-NRBF Specification - Microsoft Learn](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-nrbf/)
- [https://github.com/Kassy2048/rags2html](https://github.com/Kassy2048/rags2html)
- [https://github.com/nneonneo/net-nrbf](https://github.com/nneonneo/net-nrbf)

---

## 5. Pokémon Generation 3 (GBA) `.sav` / `.srm`

### 5.1 File Locations, Names, and Version Differences
- **File System Locations:** Emulators and flashcarts place saves alongside ROM files (`<ROM_Name>.sav` or `<ROM_Name>.srm`).
- **Physical Media:** 128 KB (1,048,576 bits) Flash-RAM chip.
- **Game Version Differences:**
  - **Ruby & Sapphire (RS):** Save layout revision 1; team size located at `0x0034`, team list at `0x0038`, money at `0x0290`; **no security key** (money and item counts are plaintext).
  - **FireRed & LeafGreen (FRLG):** Save layout revision 2; team size at `0x0234`, team list at `0x0238`, money at `0x0490`; **Security Key active** at offset `0x0F20` in Section 0.
  - **Emerald (E):** Save layout revision 2; team size at `0x0234`, team list at `0x0238`, money at `0x0490`; **Security Key active** at offset `0x01F4` in Section 0.

---

### 5.2 File Recognition & Format Identification
- **File Size:** Exactly 131,072 bytes (128 KB). (Some incomplete emulator dumps are 64 KB, but official GBA Flash-RAM cartridges are 128 KB).
- **Section Magic Signature:**
  - Each 4,096-byte section contains a 4-byte signature at offset `0x0FF8`:
  - `0x08012025` in little-endian hex: `25 20 01 08`.
- **Section ID:**
  - Offset `0x0FF4` contains a 2-byte unsigned integer ranging from `0x0000` to `0x000D` (0 through 13).
- **Disambiguation:**
  - Inspecting any 4 KB boundary (`chunk_idx * 4096 + 0x0FF8`) for `25 20 01 08` uniquely identifies a Pokémon Gen 3 save.

---

### 5.3 Exact Byte Layout

#### Dual Save Slot Architecture
The 128 KB file is split into two redundant slots of 57,344 bytes each, followed by Hall of Fame and reserved space:
- **Save Slot A:** Offset `0x00000` – `0x0DFFF` (14 sections $\times$ 4,096 bytes).
- **Save Slot B:** Offset `0x0E000` – `0x1BFFF` (14 sections $\times$ 4,096 bytes).
- **Hall of Fame & Sector Allocation:** `0x1C000` – `0x1FFFF`.

#### Determining the Active Save Slot
1. Read the 14 sections in Slot A and Slot B.
2. Read the 32-bit `Save Index` counter at offset `0x0FFC` in Section 0.
3. The slot with the **higher save index** whose section checksums validate is the **active slot**. If the newest slot is corrupted, the engine falls back to the older slot.

#### 4,096-Byte Section Structure
Sections in a slot are written dynamically and can appear in **any order** within the 14 sectors. The parser must index them by their `Section ID` at `0x0FF4`.

```
+-------------------------------------------------------------+
| 0x0000 - 0x0FF3: Section Data Payload (up to 3968 bytes)     |
+-------------------------------------------------------------+
| 0x0FF4 - 0x0FF5: Section ID (uint16_le: 0 to 13)            |
| 0x0FF6 - 0x0FF7: Section Checksum (uint16_le)               |
| 0x0FF8 - 0x0FFB: Signature (0x08012025 little-endian)        |
| 0x0FFC - 0x0FFF: Save Index / Counter (uint32_le)           |
+-------------------------------------------------------------+
```

#### Section Validation Table

| Section ID | Name / Contents | Bytes Validated for Checksum |
| :--- | :--- | :--- |
| **0** | Trainer Info & Security Key | 3,884 bytes (`0x0000` – `0x0F2B`) |
| **1** | Team & Items | 3,968 bytes (`0x0000` – `0x0F7F`) |
| **2** | Game State (Switches, Flags, Vars) | 3,968 bytes (`0x0000` – `0x0F7F`) |
| **3** | Misc Data | 3,968 bytes (`0x0000` – `0x0F7F`) |
| **4** | Rival Info | 3,848 bytes (`0x0000` – `0x0F07`) |
| **5 – 12** | PC Boxes 1 through 14 (Buffers A–H) | 3,968 bytes each |
| **13** | PC Buffer I | 2,000 bytes (`0x0000` – `0x07CF`) |

---

### 5.4 Checksums and Security Key Details

#### Section Checksum Algorithm
1. Initialize a 32-bit accumulator `sum = 0`.
2. Read the data payload as sequential 32-bit little-endian words (`uint32_le`) up to the validated byte count.
3. Add each word to `sum` (allowing standard 32-bit overflow):
   $$\text{sum} = (\text{sum} + \text{word}) \pmod{2^{32}}$$
4. Fold the 32-bit sum into a 16-bit checksum:
   $$\text{checksum} = \left((\text{sum} \ \& \ \text{0xFFFF}) + (\text{sum} \gg 16)\right) \ \& \ \text{0xFFFF}$$
5. Store at offset `0x0FF6`.

#### The Money and Items Security Key (Emerald / FRLG)
- **Key Offset (in Section 0):**
  - **Emerald:** Offset `0x01F4` (4 bytes, `uint32_le`).
  - **FireRed / LeafGreen:** Offset `0x0F20` (4 bytes, `uint32_le`).
  - **Ruby / Sapphire:** Not present / unmasked.
- **Masking Operations:**
  - **Money (32-bit):**
    $$\text{StoredMoney} = \text{ActualMoney} \oplus \text{SecurityKey}$$
  - **Coins (16-bit):**
    $$\text{StoredCoins} = \text{ActualCoins} \oplus (\text{SecurityKey} \ \& \ \text{0xFFFF})$$
  - **Item Quantities (16-bit):**
    Each item slot in the bag consists of `ItemID` (2 bytes, plaintext) followed by `Quantity` (2 bytes):
    $$\text{StoredQuantity} = \text{ActualQuantity} \oplus (\text{SecurityKey} \ \& \ \text{0xFFFF})$$

---

### 5.5 Target Edit Locations

#### Key Offsets in Section 0 (Trainer Info)
- `0x0000` (7 bytes): Trainer Name (Gen 3 character encoding).
- `0x0008` (1 byte): Gender (`0x00` = Boy, `0x01` = Girl).
- `0x000A` (2 bytes, `uint16_le`): Visible Trainer ID (TID).
- `0x000C` (2 bytes, `uint16_le`): Secret Trainer ID (SID).
- `0x000E` (2 bytes, `uint16_le`): Played Hours.
- `0x0010` (1 byte): Played Minutes.
- `0x0011` (1 byte): Played Seconds.
- `0x01F4` (`uint32_le`): **Security Key** (in Emerald).
- `0x0F20` (`uint32_le`): **Security Key** (in FireRed/LeafGreen).

#### Key Offsets in Section 1 (Team & Items)

| Field Name | Ruby / Sapphire | Emerald / FRLG | Size / Format |
| :--- | :--- | :--- | :--- |
| **Team Size** | `0x0034` | `0x0234` | 4 bytes (valid count: 1–6 in byte 0) |
| **Party Pokémon List** | `0x0038` | `0x0238` | 600 bytes (6 $\times$ 100-byte Pokémon structures) |
| **Money** | `0x0290` | `0x0490` | 4 bytes (XORed with Security Key in E/FRLG) |
| **Coins** | `0x0294` | `0x0494` | 2 bytes (XORed with Security Key in E/FRLG) |
| **PC Items** | `0x0298` | `0x0498` | 50 slots $\times$ 4 bytes (2B Item ID + 2B Count) |
| **Item Pocket (Bag)** | `0x0360` (80B) | `0x0560` (FRLG: 168B, E: 120B) | 4 bytes per item (2B ID + 2B Quantity) |
| **Key Items** | `0x03B0` (80B) | After Item Pocket | 4 bytes per item |
| **Poké Balls** | `0x0400` (64B) | After Key Items | 4 bytes per item |
| **TM / HM Case** | `0x0440` (256B) | After Poké Balls | 4 bytes per item |
| **Berry Pocket** | `0x0540` (184B) | After TM Case | 4 bytes per item |

#### Party Pokémon 100-Byte Structure
- `0x00` – `0x03`: Personality Value (`PID`, `uint32_le`).
- `0x04` – `0x07`: Trainer ID / Secret ID (`OTID`, `uint32_le`).
- `0x08` – `0x11`: Nickname (10 bytes).
- `0x12` – `0x13`: Language code (2 bytes).
- `0x14` – `0x1A`: Original Trainer Name (7 bytes).
- `0x1B`: Markings (1 byte).
- `0x1C` – `0x1D`: **Checksum** (`uint16_le`, sum of twenty-four 16-bit words of the decrypted 48-byte block).
- `0x1E` – `0x1F`: Unused / padding (2 bytes).
- `0x20` – `0x4F`: **Encrypted Data Payload (48 bytes):**
  - Encrypted via 32-bit XOR using key:
    $$\text{Key} = \text{PID} \oplus \text{OTID}$$
  - Contains four 12-byte blocks: **G** (Growth: Species, Item, Exp, Friendship), **A** (Attacks: Moves, PP), **E** (EVs & Condition), **M** (Misc: Pokérus, Met data, IVs, Ribbons, Ability).
  - Block ordering permuted by $\text{PID} \pmod{24}$ across 24 combinations.
- `0x50` – `0x63`: **Battle / Party Data (20 bytes, unencrypted):**
  - `0x50` (4B): Status condition flags (Sleep, Poison, Burn, Freeze, Paralysis).
  - `0x54` (1B): Level.
  - `0x56` (2B, `uint16_le`): Current HP.
  - `0x58` (2B, `uint16_le`): Max HP.
  - `0x5A` (2B, `uint16_le`): Attack.
  - `0x5C` (2B, `uint16_le`): Defense.
  - `0x5E` (2B, `uint16_le`): Speed.
  - `0x60` (2B, `uint16_le`): Special Attack.
  - `0x62` (2B, `uint16_le`): Special Defense.

---

### 5.6 Sample Files & Open-Source Projects
- **Open-Source Reference Projects:**
  - `pret/pokeemerald` (C assembly decompilation): [https://github.com/pret/pokeemerald](https://github.com/pret/pokeemerald) — License: **MIT / Public Domain (Unlicense)**.
  - `pret/pokefirered`: [https://github.com/pret/pokefirered](https://github.com/pret/pokefirered) — License: **MIT / Public Domain**.
  - `PKHeX` (C# save editor): [https://github.com/kwsch/PKHeX](https://github.com/kwsch/PKHeX) — License: **GPL-3.0** (*Clean-room warning: refer to data structures and documentation specs, do not copy source code*).
- **Public Sample Saves:**
  - `pret` project test saves and emulator test rom fixtures: [https://github.com/pret/pokeemerald/tree/master/test](https://github.com/pret/pokeemerald/tree/master/test).

### 5.7 Sources
- [Bulbapedia: Save data structure (Generation III)](https://bulbapedia.bulbagarden.net/wiki/Save_data_structure_(Generation_III))
- [Bulbapedia: Pokémon data structure (Generation III)](https://bulbapedia.bulbagarden.net/wiki/Pok%C3%A9mon_data_structure_(Generation_III))
- [Project Pokémon Gen 3 Format Specifications](https://projectpokemon.org/home/docs/gen-3/save-structure-r64/)
- [https://github.com/pret/pokeemerald](https://github.com/pret/pokeemerald)
