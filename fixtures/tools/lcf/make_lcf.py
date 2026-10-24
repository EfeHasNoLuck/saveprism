#!/usr/bin/env python3
"""Write invented RPG Maker 2000 / 2003 saves and game databases (LCF) from the public format notes.

The output has no game code or real save data: every name and number below is made up.
Layout: BER numbers (7-bit groups, most significant first), chunks of id + length + data,
structs ended by a 0 byte (the top level has none), arrays of structs with a count and element ids.
"""
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
OUT_2K = ROOT / "synthetic" / "rpgmaker-2k"
OUT_2K3 = ROOT / "synthetic" / "rpgmaker-2k3"


def ber(value: int) -> bytes:
    value &= 0xFFFFFFFF
    groups = [value & 0x7F]
    value >>= 7
    while value:
        groups.append((value & 0x7F) | 0x80)
        value >>= 7
    return bytes(reversed(groups))


def chunk(chunk_id: int, data: bytes) -> bytes:
    return ber(chunk_id) + ber(len(data)) + data


def body(*chunks: bytes) -> bytes:
    """The chunks of a struct and its end mark."""
    return b"".join(chunks) + b"\x00"


def int_(chunk_id: int, value: int) -> bytes:
    return chunk(chunk_id, ber(value))


def text(chunk_id: int, value) -> bytes:
    return chunk(chunk_id, value if isinstance(value, bytes) else value.encode("ascii"))


def flag(chunk_id: int, value: bool) -> bytes:
    return chunk(chunk_id, b"\x01" if value else b"\x00")


def array(elements) -> bytes:
    """An array of structs: [(element id, [chunks...]), ...]."""
    out = ber(len(elements))
    for element_id, chunks in elements:
        out += ber(element_id) + body(*chunks)
    return out


def int16s(values) -> bytes:
    return b"".join(struct.pack("<h", value) for value in values)


def int32s(values) -> bytes:
    return b"".join(struct.pack("<i", value) for value in values)


def file(header: bytes, chunks, end_mark: bool = False) -> bytes:
    out = ber(len(header)) + header + b"".join(chunks)
    return out + (b"\x00" if end_mark else b"")


def music(name: str, volume: int) -> bytes:
    return body(text(0x01, name), int_(0x02, 0), int_(0x03, volume), int_(0x04, 100), int_(0x05, 50))


def sound(name: str) -> bytes:
    return body(text(0x01, name), int_(0x03, 100), int_(0x04, 100), int_(0x05, 50))


def title(hero: str, level: int, hp: int, stamp: float) -> bytes:
    return body(
        chunk(0x01, struct.pack("<d", stamp)),
        text(0x0B, hero),
        int_(0x0C, level),
        int_(0x0D, hp),
        text(0x15, "Hero1"),
        int_(0x16, 0),
        text(0x17, "Hero1"),
        int_(0x18, 1),
    )


def system(switches, variables, extra=()) -> bytes:
    return body(
        int_(0x01, 5),
        int_(0x0B, 123456),
        text(0x15, "System1"),
        int_(0x1F, len(switches)),
        chunk(0x20, bytes(switches)),
        int_(0x21, len(variables)),
        chunk(0x22, int32s(variables)),
        chunk(0x47, music("Opening1", 100)),
        chunk(0x4B, music("Field1", 80)),
        chunk(0x5B, sound("Cursor1")),
        chunk(0x6F, b"\x01"),
        chunk(0x70, b"\x02"),
        flag(0x79, True),
        flag(0x7A, True),
        flag(0x7B, True),
        flag(0x7C, False),
        int_(0x83, 3),
        int_(0x84, 1),
        *extra,
    )


def actor(name, level, exp, hp, sp, mods, skills, equipped, extra=()):
    hp_mod, sp_mod, attack, defense, spirit, agility = mods
    return [
        text(0x01, name),
        text(0x02, b"\x01"),
        text(0x0B, "Hero1"),
        int_(0x0C, 0),
        text(0x15, "Hero1"),
        int_(0x16, 0),
        int_(0x1F, level),
        int_(0x20, exp),
        int_(0x21, hp_mod),
        int_(0x22, sp_mod),
        int_(0x29, attack),
        int_(0x2A, defense),
        int_(0x2B, spirit),
        int_(0x2C, agility),
        int_(0x33, len(skills)),
        chunk(0x34, int16s(skills)),
        chunk(0x3D, int16s(equipped)),
        int_(0x47, hp),
        int_(0x48, sp),
        int_(0x51, 0),
        chunk(0x52, b""),
        flag(0x53, False),
        *extra,
    ]


def inventory(party, items, gold=None) -> bytes:
    ids = [item_id for item_id, _ in items]
    counts = [count for _, count in items]
    chunks = [
        int_(0x01, len(party)),
        chunk(0x02, int16s(party)),
        int_(0x0B, len(ids)),
        chunk(0x0C, int16s(ids)),
        chunk(0x0D, bytes(counts)),
        chunk(0x0E, bytes(len(ids))),
    ]
    if gold is not None:
        chunks.append(int_(0x15, gold))
    chunks += [int_(0x20, 10), int_(0x21, 0), int_(0x22, 1), int_(0x23, 9), int_(0x29, 57), int_(0x2A, 1234)]
    return body(*chunks)


def common_tail() -> list:
    return [
        chunk(0x6E, array([(1, [int_(0x01, 1), int_(0x02, 3), int_(0x03, 4)])])),
        chunk(0x6F, body(int_(0x01, 0), int_(0x02, 0))),
        chunk(0x70, body(int_(0x01, 0), int_(0x02, 0))),
        chunk(0x71, body()),
        chunk(0x72, array([])),
    ]


def save_2k() -> bytes:
    return file(b"LcfSaveData", [
        chunk(0x64, title("Alex", 12, 250, 45123.75)),
        chunk(0x65, system([1, 0, 1, 0, 0], [100, -7, 0, 20000000, -123456789, 2147483647])),
        chunk(0x66, body(int_(0x01, 100), int_(0x02, 100))),
        chunk(0x67, array([(1, [text(0x01, "Sun")])])),
        chunk(0x68, body(int_(0x01, 1), int_(0x02, 10), int_(0x03, 7))),
        chunk(0x69, body(int_(0x0B, 2))),
        chunk(0x6C, array([
            (1, actor(b"\x01", 12, 3400, 250, 40, (10, 5, 3, 2, 0, 1), [1, 3], [2, 5, 0, 0, 0])),
            (2, actor("Mira", 8, 1200, 140, 95, (0, 20, 0, 0, 4, 2), [4], [0, 0, 0, 0, 0])),
            (3, actor(b"Jos\xe9", 3, 150, 60, 0, (0, 0, 1, 1, 0, 3), [], [0, 0, 0, 0, 0])),
        ])),
        chunk(0x6D, inventory([1, 2], [(1, 3), (2, 1), (5, 2), (7, 99)], gold=5000)),
        *common_tail(),
    ])


def save_2k3() -> bytes:
    battle = [
        chunk(0x50, int32s([1, 2, 3, 4, 0, 0, 0])),
        int_(0x5A, 2),
        int_(0x5B, 0),
        flag(0x5C, False),
        flag(0x5D, False),
        flag(0x5E, False),
        flag(0x5F, False),
        int_(0x60, 1),
    ]
    return file(b"LcfSaveData", [
        chunk(0x64, title("Kai", 20, 480, 46001.5)),
        chunk(0x65, system([0, 1, 1, 0], [-1, 16777217, 0, 0, 7], extra=[int_(0x8C, 1)])),
        chunk(0x6C, array([
            (1, actor(b"\x01", 15, 9100, 300, 60, (0, 0, 2, 2, 2, 2), [1], [2, 0, 0, 0, 0], battle)),
            (3, actor("Kai", 20, 25000, 480, 120, (50, 10, 6, 4, 3, 5), [2, 6], [0, 0, 0, 0, 0], battle)),
        ])),
        chunk(0x6D, inventory([3, 1], [(2, 1), (7, 5)])),
        *common_tail(),
    ])


def db_item(item_id: int, name, kind: int, price: int, description: str = ""):
    return (item_id, [text(0x01, name), text(0x02, description), int_(0x03, kind), int_(0x05, price)])


def names(entries):
    return array([(entry_id, [text(0x01, name)]) for entry_id, name in entries])


def database_2k() -> bytes:
    return file(b"LcfDataBase", [
        chunk(0x0B, array([
            (1, [text(0x01, "Alex"), text(0x02, "Hero")]),
            (2, [text(0x01, "Mira"), text(0x02, "Mage")]),
            (3, [text(0x01, "Jose"), text(0x02, "Thief")]),
        ])),
        chunk(0x0C, names([(1, "Fire")])),
        chunk(0x0D, array([
            db_item(1, "Potion", 6, 50, "Heals 50 HP"),
            db_item(2, "Bronze Sword", 1, 200),
            db_item(3, "Iron Helm", 4, 150),
            db_item(5, "Leather Shield", 2, 120),
            db_item(7, b"\xc9lixir", 0, 0),
        ])),
        chunk(0x15, body(text(0x01, "Encounter"))),
        chunk(0x17, names([(1, "Door Open"), (2, "Lamp Lit"), (3, "Boss Beaten")])),
        chunk(0x18, names([(1, "Coins"), (2, "Day"), (3, "Zero Counter")])),
    ])


def database_2k3() -> bytes:
    return file(b"LcfDataBase", [
        chunk(0x0B, array([
            (1, [text(0x01, "Alex"), text(0x02, "Hero")]),
            (3, [text(0x01, "Kai"), text(0x02, "Knight")]),
        ])),
        chunk(0x0D, array([db_item(2, "Bronze Sword", 1, 200), db_item(7, "Ether", 6, 80)])),
        chunk(0x17, names([(2, "Gate Open")])),
        chunk(0x18, names([(1, "Mood"), (2, "Big Number")])),
        chunk(0x1D, body(int_(0x02, 1))),
        chunk(0x1E, names([(1, "Knight")])),
    ], end_mark=True)


def main() -> None:
    OUT_2K.mkdir(parents=True, exist_ok=True)
    OUT_2K3.mkdir(parents=True, exist_ok=True)
    (OUT_2K / "Save01.lsd").write_bytes(save_2k())
    (OUT_2K / "RPG_RT.ldb").write_bytes(database_2k())
    (OUT_2K3 / "Save01.lsd").write_bytes(save_2k3())
    (OUT_2K3 / "RPG_RT.ldb").write_bytes(database_2k3())


if __name__ == "__main__":
    main()
