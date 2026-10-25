#!/usr/bin/env python3
"""Generate an invented Terraria player (decrypted) and a tiny world save.

The layout follows the public MS-PL TEdit sources (`Player/PlayerFile.cs`,
`World.FileV2.cs`, `WorldConfiguration.cs`, `FileType.cs`) and the Terraria wiki save
folders. All names, ids, tiles and flags are invented. The player file here is the
*decrypted* plaintext so the golden corpus can read it without a key; the codec test
encrypts the same bytes with a made-up key provided by a fake `Terraria.exe` metadata
blob, never with the real game constant.
"""

from pathlib import Path
import struct

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "terraria"
RELOGIC = 0x006369676F6C6572
FILE_TYPE_WORLD = 2
FILE_TYPE_PLAYER = 3


def u8(value: int) -> bytes:
    return struct.pack("<B", value)


def i16(value: int) -> bytes:
    return struct.pack("<h", value)


def i32(value: int) -> bytes:
    return struct.pack("<i", value)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def i64(value: int) -> bytes:
    return struct.pack("<q", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def f64(value: float) -> bytes:
    return struct.pack("<d", value)


def dotnet_string(text: str) -> bytes:
    raw = text.encode("utf-8")
    length = len(raw)
    out = bytearray()
    while True:
        byte = length & 0x7F
        length >>= 7
        if length:
            byte |= 0x80
        out.append(byte)
        if not length:
            break
    out += raw
    return bytes(out)


def item(item_id: int, stack: int, prefix: int, favorite: bool) -> bytes:
    return i32(item_id) + i32(stack) + u8(prefix) + u8(1 if favorite else 0)


def player(version: int = 317) -> bytes:
    out = bytearray()
    out += i32(version)
    out += u64(RELOGIC | (FILE_TYPE_PLAYER << 56))
    out += u32(7)  # revision
    out += u64(1)  # favorite
    out += dotnet_string("Mira")
    out += u8(0)  # difficulty: classic
    out += i64(123_456_789)  # play time ticks
    out += i32(5)  # hair
    out += u8(0)  # hair dye
    out += u8(0)  # team
    out += b"\x00\x00"  # hide visible accessory
    out += u8(0)  # hide misc
    out += u8(0)  # skin variant
    out += i32(140)  # life
    out += i32(140)  # max life
    out += i32(60)  # mana
    out += i32(60)  # max mana
    out += u8(0)  # extra accessory
    out += b"\x00\x00"  # biome torch flags
    out += u8(0)  # artisan bread
    out += b"\x00" * 6  # aegis / arcane / galaxy / gummy / ambrosia
    out += u8(0)  # downed DD2
    out += i32(0)  # tax money
    out += i32(0) + i32(0)  # deaths PvE / PvP
    for color in [(215, 90, 55), (255, 125, 90), (105, 90, 75), (175, 165, 140),
                  (160, 180, 215), (255, 230, 175), (160, 105, 60)]:
        out += bytes(color)
    armor = [(0, 0)] * 20
    armor[0] = (1, 0)   # copper helmet
    armor[1] = (2, 0)   # copper chainmail
    armor[2] = (3, 0)   # copper greaves
    for item_id, prefix in armor:
        out += i32(item_id) + u8(prefix)
    for index in range(10):
        out += i32(0) + u8(0)  # dyes
    slots = [item(0, 0, 0, False) for _ in range(58)]
    slots[0] = item(1, 1, 0, True)     # copper shortsword
    slots[1] = item(2, 1, 0, False)    # copper pickaxe
    slots[2] = item(73, 99, 4, False)  # gold coin stack with a prefix
    slots[57] = item(75, 1, 0, False)  # fallen star
    out += b"".join(slots)
    # Tail: 44 buff slots (type, time), the spawn-point sentinel, then zeroed fields.
    out += b"".join(i32(0) + i32(0) for _ in range(44))
    out += i32(-1)
    out += bytes(64)
    padding = (-len(out)) % 16
    out += bytes(padding)
    return bytes(out)


def world(version: int = 326) -> bytes:
    name = "Prism Hollow"
    seed = "4829103"
    world_id = 917_263
    header = bytearray()
    header += dotnet_string(name)
    header += dotnet_string(seed)
    header += u64(9)  # world generation version
    header += bytes(range(16))  # invented GUID
    header += i32(world_id)
    header += i32(0)       # left
    header += i32(8400)    # right
    header += i32(0)       # top
    header += i32(1200)    # bottom
    header += i32(1200)    # height
    header += i32(8400)    # width
    header += i32(2)       # game mode: master
    for flag in [False, False, False, False, False, False, False, True, False]:
        header += u8(1 if flag else 0)  # drunk, good, tenth, dontstarve, bees, remix, traps, zenith, skyblock
    header += i64(637_000_000_000_000_000)  # creation time
    header += i64(637_100_000_000_000_000)  # last played
    header += u8(0)  # moon type
    header += i32(1200) + i32(3600) + i32(7000)  # tree x
    header += i32(1) + i32(2) + i32(3) + i32(4)  # tree styles
    header += i32(1100) + i32(3500) + i32(6900)  # cave back x
    header += i32(1) + i32(2) + i32(3) + i32(4)  # cave back styles
    header += i32(1) + i32(2) + i32(3)           # ice / jungle / hell
    header += i32(4200) + i32(320)               # spawn x / y
    header += f64(600.0) + f64(900.0) + f64(13500.0)  # ground / rock / time
    header += u8(1)  # day time
    header += i32(0)  # moon phase
    header += u8(0) + u8(0)  # blood moon / eclipse
    header += i32(1200) + i32(800)  # dungeon x / y
    header += u8(1)  # crimson
    boss_flags = [True, False, True, False, False, False, False, False, False, False,
                  True, False, False, False, False, False, False, False, False, False]
    for flag in boss_flags:
        header += u8(1 if flag else 0)
    header += u8(3)            # shadow orb count
    header += i32(2)           # altar count
    header += u8(1)            # hard mode
    header += u8(0)            # party of doom
    header += i32(0)           # invasion delay
    header += i32(0)           # invasion size
    header += i32(0)           # invasion type
    header += f64(0.0)         # invasion x
    header += f64(0.0)         # slime rain time
    header += u8(0)            # sundial cooldown
    header += u8(0)            # raining
    header += i32(0)  # anglers (tail starts here)
    header += bytes(12)

    section0 = bytearray()
    section0 += u32(version)
    section0 += b"relogic"
    section0 += u8(FILE_TYPE_WORLD)
    section0 += u32(3)
    section0 += u64(1)
    section0 += i16(11)
    pointers_at = len(section0)
    section0 += b"\x00" * (11 * 4)
    section0 += i16(4) + b"\x00"  # four false tile-frame bits
    section0_len = len(section0)

    section_bytes = [
        bytes(header),
        b"\x00\x01\x02\x03",  # tiles (invented)
        b"\x00\x00",          # chests
        b"\x00\x00",          # signs
        b"\x00\x00",          # NPCs
        b"\x00\x00\x00\x00",  # tile entities
        b"\x00\x00\x00\x00",  # pressure plates
        b"\x00\x00\x00\x00",  # town manager
        b"\x00\x00\x00\x00",  # bestiary
        b"\x00\x00\x00\x00",  # creative powers
    ]
    pointers = [section0_len]
    for chunk in section_bytes:
        pointers.append(pointers[-1] + len(chunk))
    out = bytearray(section0)
    for chunk in section_bytes:
        out += chunk
    out += b"\x01" + dotnet_string(name) + i32(world_id)  # footer
    for index, pointer in enumerate(pointers):
        out[pointers_at + index * 4: pointers_at + index * 4 + 4] = i32(pointer)
    return bytes(out)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "player-decrypted.plr").write_bytes(player())
    (OUT / "world.wld").write_bytes(world())
    print(f"wrote {OUT/'player-decrypted.plr'} and {OUT/'world.wld'}")


if __name__ == "__main__":
    main()
