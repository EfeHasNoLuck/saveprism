#!/usr/bin/env python3
"""Build invented Pokémon Gen 3 battery saves from the public section layout."""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "gen3"
SECTION = 0x1000
BLOCK = 14 * SECTION
MAGIC = 0x08012025
ORDERS = (
    "GAEM", "GAME", "GEAM", "GEMA", "GMAE", "GMEA", "AGEM", "AGME",
    "AEGM", "AEMG", "AMGE", "AMEG", "EGAM", "EGMA", "EAGM", "EAMG",
    "EMGA", "EMAG", "MGAE", "MGEA", "MAGE", "MAEG", "MEGA", "MEAG",
)


def put16(data, offset, value):
    struct.pack_into("<H", data, offset, value)


def put32(data, offset, value):
    struct.pack_into("<I", data, offset, value)


def checksum(data):
    total = sum(struct.unpack_from("<I", data, at)[0] for at in range(0, len(data), 4)) & 0xFFFFFFFF
    return ((total & 0xFFFF) + (total >> 16)) & 0xFFFF


def pokemon(slot):
    pid = (slot + 1) * 24 + slot
    otid = 0x12340000 | (slot + 1)
    subs = {"G": bytearray(12), "A": bytearray(12), "E": bytearray(12), "M": bytearray(12)}
    put16(subs["G"], 0, 25 + slot)
    put16(subs["G"], 2, slot + 1)
    put32(subs["G"], 4, 1000 + slot * 250)
    subs["G"][9] = 70 + slot
    for move in range(4):
        put16(subs["A"], move * 2, move + 1)
        subs["A"][8 + move] = 10 + move
    subs["E"][:6] = bytes([1 + slot, 2, 3, 4, 5, 6])
    ivs = sum(((slot + stat) % 32) << (5 * stat) for stat in range(6))
    put32(subs["M"], 4, ivs)
    plain = b"".join(subs[tag] for tag in ORDERS[pid % 24])
    record = bytearray(100)
    put32(record, 0, pid)
    put32(record, 4, otid)
    record[8:18] = b"SYNTH\xff\0\0\0\0"
    record[0x1C:0x1E] = struct.pack("<H", sum(struct.unpack("<24H", plain)) & 0xFFFF)
    key = pid ^ otid
    for at in range(0, 48, 4):
        put32(record, 0x20 + at, struct.unpack_from("<I", plain, at)[0] ^ key)
    record[0x54] = 10 + slot
    for stat, value in enumerate((45, 30, 30, 25, 32, 29, 28)):
        put16(record, 0x56 + stat * 2, value + slot)
    return record


def make_block(game, index, key):
    block = bytearray(BLOCK)
    for section_id in range(14):
        start = section_id * SECTION
        put16(block, start + 0xFF4, section_id)
        put32(block, start + 0xFF8, MAGIC)
        put32(block, start + 0xFFC, index)
    s0, s1 = 0, SECTION
    block[s0:s0 + 7] = b"PLAYER\xff"
    block[s0 + 8] = 1
    put32(block, s0 + 0x0A, 0xBEEF1234)
    put16(block, s0 + 0x0E, 123)
    block[s0 + 0x10:s0 + 0x13] = bytes((45, 12, 0))
    if game == "emerald":
        put32(block, s0 + 0x0AC, key)
        put32(block, s0 + 0x1F4, key)
        team, money = 0x234, 0x490
        pockets = ((0x560, 30), (0x5D8, 30), (0x650, 16), (0x690, 64), (0x790, 46))
        pc_offset, pc_slots = 0x498, 50
    elif game == "frlg":
        put32(block, s0 + 0x0AC, 1)
        put32(block, s0 + 0xAF8, key)
        put32(block, s0 + 0xF20, key)
        team, money = 0x034, 0x290
        pockets = ((0x310, 42), (0x3B8, 30), (0x430, 13), (0x464, 58), (0x54C, 43))
        pc_offset, pc_slots = 0x298, 30
    else:
        team, money = 0x234, 0x490
        pockets = ((0x560, 20), (0x5B0, 20), (0x600, 16), (0x640, 64), (0x740, 46))
        pc_offset, pc_slots = 0x498, 50
    put32(block, s1 + team, 6)
    put32(block, s1 + money, (54321 ^ key) if game != "ruby" else 54321)
    put16(block, s1 + money + 4, (4321 ^ (key & 0xFFFF)) if game != "ruby" else 4321)
    for i in range(6):
        start = s1 + team + 4 + i * 100
        block[start:start + 100] = pokemon(i)
    for offset, slots in pockets:
        for slot in range(slots):
            put16(block, s1 + offset + slot * 4 + 2, key & 0xFFFF if game != "ruby" else 0)
    put16(block, s1 + pockets[0][0], 13)
    put16(block, s1 + pockets[0][0] + 2, (5 ^ (key & 0xFFFF)) if game != "ruby" else 5)
    put16(block, s1 + pc_offset, 2)
    put16(block, s1 + pc_offset + 2, 999)
    for section_id in range(14):
        start = section_id * SECTION
        size = {0: 3884, 4: 3848, 13: 2000}.get(section_id, 3968)
        put16(block, start + 0xFF6, checksum(block[start:start + size]))
    return block


def make_save(game, footer=False):
    key = 0x76543210 if game != "ruby" else 0
    data = bytearray(make_block(game, 40, key) + make_block(game, 41, key))
    data.extend(bytes([0xCC]) * 0x4000)
    if footer:
        data.extend(b"RTC-SYNTHETIC!!!")
    return data


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for game, name, footer in (
        ("emerald", "emerald.sav", False),
        ("frlg", "firered.sav", False),
        ("ruby", "ruby.sav", False),
        ("emerald", "emerald-footer.srm", True),
    ):
        (OUT / name).write_bytes(make_save(game, footer))


if __name__ == "__main__":
    main()
