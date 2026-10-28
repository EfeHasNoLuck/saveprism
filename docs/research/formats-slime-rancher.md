<!-- Behavioral spec written on 2026-10-28 from the bytes of the maintainer's own Slime Rancher 2 saves (game 1.3.0, five slots of one game, plus slimerancher.cfg and slimerancher.prf). Clean room: no game code was decompiled and no third-party editor or table was used. -->

# Slime Rancher 2 saves

Implemented by `format.slime-rancher-2` (`crates/saveprism-codecs/src/slime_rancher/`).

## Sources

- The maintainer's own saves, copied to `fixtures/external/slime-rancher-2/` (git-ignored, never committed):
  `<yyyyMMddHHmmss>_<game>_<slot>.sav` (about 350 KB each), `slimerancher.cfg`, `slimerancher.prf`.
  They live in `%USERPROFILE%\AppData\LocalLow\MonomiPark\SlimeRancher2\` (Unity `persistentDataPath`).
- Public descriptions: none found. Web searches on 2026-10-28 for a Slime Rancher 2 save editor or a description
  of the format under any license returned nothing usable, so every statement below comes from the bytes.
- Comparing the five slots of one game (played 45 minutes apart) told which values change: money went
  745 → 785 → 835 → 835 → 846, the vacpack emptied between slot 2 and 3.

## Encoding

Not encrypted, not compressed. Little-endian. Strings are .NET `BinaryWriter` strings: a 7-bit encoded byte length
(low 7 bits first, high bit = more bytes follow), then UTF-8 bytes. Unity games use .NET, so this is the expected
`BinaryWriter.Write(string)` layout.

## Records

Everything is a record:

```
tag      string            ASCII upper-case, may be empty
version  int32
begin    00 30
body     ...               depends on tag and version
end      00 40
```

A body mixes plain values (`int32`, `float32`, `float64`, `int64`, strings) with nested records. There is no length
field anywhere, so a body can only be read with its schema, and the `00 30` / `00 40` marks cannot be matched like
brackets (a `float32` 2.0 is `00 00 00 40`). A reader that does not know a record cannot skip it. The same tag can
mean different records by version (`SRAD` version 2 is an ammo slot, version 4 a world actor).

Counted lists are an `int32` count followed by the items.

## File layout (`.sav`)

The root record has an empty tag (version 11 in game 1.3.0). Its body starts with:

1. `INDXTBLLKUP` v1: `int32` table count; per table a string (the table name) and an `INDXTBL` v1 record
   (`int32` count, then that many strings). The first lookup has `IdentifiableType` (789 names such as
   `IdentifiableType.BriarHen`, `SlimeDefinition.Tabby`), `ResourceGrowerDefinition`, `[1].GameIconDefinition`,
   `[1].World.ZoneDefinition`. Other records store an index into these tables instead of a name.
2. The load-screen summary: the save name (`20261028110957_1`), a string (`"1"`), an `int32` (0), then an
   empty-tag record v3 holding the game version (`1.3.0 [20260704030000-113492]`), an `int32` (1), a currency
   record, and more values (an `int32`, the world time as `float64`, the save time as .NET ticks `int64`, flags and
   the game icon index).
3. `SRGAMESETTINGS` v1: `int32` count of empty-tag v0 records `{string key, string value}`
   (`setting.TutorialsEnabled` = `on` ...), then an `int32`.
4. A second `INDXTBLLKUP` (scene groups, upgrade components, weather, palettes ...).
5. The world (`SRW` v9) and the player (an empty-tag record v12) and everything else: `SRV3` (3 × `float32`),
   `SRPU`, `SRUPGRADECOMPONENTS`, `SRLP` (land plots), `SRRESNODE`, `SREVENTENTRY`, `SRWEATHER`, `PALINV`,
   `SRRANCH`, `SRSHOP`, `SRMETADATA` ... These are not decoded.

The file ends with the root record's `00 40`.

### Currency: `SRCRNCY` v1

```
int32  count            1 in every sample
count × { int32 amount; int32 other }    other = 250 in every sample (meaning unknown)
```

There are two: one in the summary (step 2 above) and one in the player record, right after four `int32`s
(100, current energy, 0, 1). Both hold the same values. The player's copy is the game state; the summary copy is
what the load screen shows, so an editor keeps the two in step.

### Ammo slot: `SRAD` v2

```
int32   item            index into IdentifiableType, -1 = empty slot
int32   count           0 for an empty slot
int32   unknown         0 in every sample
SRSED   v1              int32 count (4), then count × { int32 key; float32 value }  (slime emotions)
byte    unknown         0 in every sample
```

Slots come in groups: an `int32` slot count immediately followed by that many slot records. The player's vacpack is
the first group after the player's currency record (7 slots in the samples: e.g. `PhosphorPlort` 3,
`SlimeDefinition.Rock` 8, `SlimeDefinition.Tabby` 18, four empty). Other groups belong to plots (`SRLP`) and similar.

## Settings and profile files

`slimerancher.cfg` and `slimerancher.prf` use the same records with a tagged root:
`SRSETTINGS` v1 → `SROPTIONS` v2 → `int32` count of `{string key, string value}` empty-tag records
(`setting.Resolution` = `1920x1080` ...). The `.prf` is an empty-tag v3 record with the current save name and a
GUID. SavePrism does not open them yet (the codec's sniff wants the `INDXTBLLKUP` that only `.sav` files start with).

## Slime Rancher 1

Not checked: no Slime Rancher 1 save was available and no public description was found. Monomi Park made both
games in Unity, so a similar tagged-record writer is plausible, but SavePrism must not assume it; a Slime Rancher 1
save needs its own look at real bytes.

## What the codec shows and edits

- Read-only: root version, save name, game version, item names.
- Editable: the player's money (the summary copy follows when it was equal), the count of every filled ammo slot.
- Only records found by their exact header bytes and parsed to their end mark are used; everything else is copied
  byte for byte. An unchanged document writes the original file exactly.
