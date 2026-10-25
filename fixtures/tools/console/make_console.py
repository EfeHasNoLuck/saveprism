#!/usr/bin/env python3
"""Builds the synthetic console-container fixtures under fixtures/synthetic/console/.

All data is invented here; nothing comes from a real memory card or game. The layouts follow the
public PS1 (psx-spx memory-card chapter), PS2 (Ross Ridge's memory-card file-system document,
pypsu), EMS .psu and GameShark/SharkPort descriptions. Run from anywhere; paths are relative to
this file.
"""

from __future__ import annotations

import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "console"

FRAME = 128
BLOCK = 8192
CARD = 128 * 1024


def json_bytes(tag: str, target: int) -> bytes:
    prefix = (
        '{"game":"synthetic","tag":"%s","gold":1234,"name":"Test Player",'
        '"items":["potion",1,2,3],"padding":"' % tag
    ).encode()
    suffix = b'"}'
    room = target - len(prefix) - len(suffix)
    if room < 0:
        raise SystemExit(f"{tag}: target {target} is too small for the JSON")
    return prefix + b"A" * room + suffix


def xor_frame(frame: bytearray) -> None:
    frame[127] = 0
    check = 0
    for byte in frame[:127]:
        check ^= byte
    frame[127] = check


# ---------------------------------------------------------------- PS1 card


def ps1_frame(index: int) -> bytearray:
    assert 0 <= index < 16
    return bytearray(FRAME)


def write_ps1_card() -> bytes:
    card = bytearray(CARD)

    header = ps1_frame(0)
    header[0:2] = b"MC"
    xor_frame(header)
    card[0:FRAME] = header

    name = b"BASLUS-00000SAVE"
    entry = ps1_frame(1)
    struct.pack_into("<I", entry, 0, 0x51)
    struct.pack_into("<I", entry, 4, 2 * BLOCK)
    struct.pack_into("<H", entry, 8, 1)  # next block number minus one
    entry[10 : 10 + len(name)] = name
    xor_frame(entry)
    card[FRAME : 2 * FRAME] = entry

    last = ps1_frame(2)
    struct.pack_into("<I", last, 0, 0x53)
    struct.pack_into("<I", last, 4, 0)
    struct.pack_into("<H", last, 8, 0xFFFF)
    xor_frame(last)
    card[2 * FRAME : 3 * FRAME] = last

    title = bytearray(FRAME)
    title[0:2] = b"SC"
    title[2] = 0x11  # one icon frame
    name_text = b"SYNTHETIC CONSOLE"
    title[4 : 4 + len(name_text)] = name_text
    card[BLOCK : BLOCK + FRAME] = title

    payload = json_bytes("ps1", 62 * FRAME + BLOCK)
    start = BLOCK + 2 * FRAME
    card[start : start + len(payload)] = payload
    return bytes(card)


def write_gme(card: bytes) -> bytes:
    header = bytearray(0xF40)
    header[0:12] = b"123-456-STD\x00"
    header[0x10:0x15] = b"\x00\x00\x01\x00\x01"
    return bytes(header) + card


# ---------------------------------------------------------------- PS2 card


ECC_BASIS = (0x87, 0x96, 0xA5, 0xB4, 0xC3, 0xD2, 0xE1, 0xF0)


def ps2_byte_parity(byte: int) -> int:
    value = 0
    for bit in range(8):
        if byte & (1 << bit):
            value ^= ECC_BASIS[bit]
    return value


def ps2_ecc(chunk: bytes) -> bytes:
    column = 0
    lines_odd = 0
    lines_even = 0
    for index, byte in enumerate(chunk):
        value = ps2_byte_parity(byte)
        column ^= value
        if value & 0x80:
            lines_odd ^= (~index) & 0xFF
            lines_even ^= index
    return bytes((~column & 0x77, ~lines_odd & 0x7F, ~lines_even & 0x7F))


def ps2_page(data: bytes, spare: bool) -> bytes:
    assert len(data) == 512
    if not spare:
        return data
    code = b"".join(ps2_ecc(data[i : i + 128]) for i in range(0, 512, 128))
    return data + code + b"\x00\x00\x00\x00"


def ps2_time(day: int = 25, month: int = 10, year: int = 2026) -> bytes:
    return bytes((0, 0, 0, 12, day, month)) + struct.pack("<H", year)


def ps2_dir_entry(
    mode: int,
    length: int,
    cluster: int,
    name: bytes,
    parent_cluster: int = 0,
) -> bytes:
    entry = bytearray(512)
    struct.pack_into("<H", entry, 0x00, mode)
    struct.pack_into("<I", entry, 0x04, length)
    entry[0x08:0x10] = ps2_time()
    struct.pack_into("<I", entry, 0x10, cluster)
    struct.pack_into("<I", entry, 0x14, parent_cluster)
    entry[0x18:0x20] = ps2_time()
    struct.pack_into("<I", entry, 0x20, 0)
    entry[0x40 : 0x40 + len(name)] = name
    return bytes(entry)


def write_ps2_card(spare: bool) -> bytes:
    clusters = 512
    pages_per_cluster = 2
    page_len = 512
    total_pages = clusters * pages_per_cluster
    alloc_offset = 41
    data = bytearray(total_pages * page_len)

    def page(index: int) -> int:
        return index * page_len

    def cluster(index: int) -> int:
        return page(index * pages_per_cluster)

    def put_chain(first_cluster: int, bytes_: bytes) -> None:
        chunk = pages_per_cluster * page_len
        for offset in range(0, len(bytes_), chunk):
            absolute = first_cluster + offset // chunk
            start = cluster(absolute)
            piece = bytes_[offset : offset + chunk]
            data[start : start + len(piece)] = piece

    save_json = json_bytes("ps2", 1000)
    icon = b"SYNTHETIC ICON" + b"\x00" * 86

    superblock = bytearray(512)
    superblock[0:28] = b"Sony PS2 Memory Card Format "
    superblock[0x1C:0x28] = b"1.2.0.0\x00\x00\x00\x00\x00"
    struct.pack_into("<H", superblock, 0x28, page_len)
    struct.pack_into("<H", superblock, 0x2A, pages_per_cluster)
    struct.pack_into("<H", superblock, 0x2C, 16)
    struct.pack_into("<H", superblock, 0x2E, 0xFF00)
    struct.pack_into("<I", superblock, 0x30, clusters)
    struct.pack_into("<I", superblock, 0x34, alloc_offset)
    struct.pack_into("<I", superblock, 0x38, clusters - 1)
    struct.pack_into("<I", superblock, 0x3C, 0)
    struct.pack_into("<I", superblock, 0x40, 63)
    struct.pack_into("<I", superblock, 0x44, 62)
    struct.pack_into("<I", superblock, 0x50, 8)  # indirect FAT cluster
    for index in range(32):
        struct.pack_into("<I", superblock, 0xD0 + index * 4, 0xFFFFFFFF)
    superblock[0x150] = 2  # card_type
    superblock[0x151] = 0x01 if spare else 0x00
    data[page(0) : page(0) + 512] = superblock

    indirect = bytearray(page_len * pages_per_cluster)
    struct.pack_into("<I", indirect, 0, 9)  # FAT cluster
    put_chain(8, bytes(indirect))

    fat = bytearray(page_len * pages_per_cluster)
    entries = {
        0: 0x80000001,  # root first cluster -> next
        1: 0xFFFFFFFF,  # root last
        2: 0x80000003,  # folder first -> next
        3: 0xFFFFFFFF,
        4: 0xFFFFFFFF,  # save.json
        5: 0xFFFFFFFF,  # icon.sys
    }
    for index, value in entries.items():
        struct.pack_into("<I", fat, index * 4, value)
    put_chain(9, bytes(fat))

    mode_dir = 0x8423
    mode_file = 0x8413
    save_length = len(save_json)
    icon_length = len(icon)
    root = (
        ps2_dir_entry(mode_dir, 3, 0, b"BASLUS-00001")
        + ps2_dir_entry(mode_dir, 0, 0, b"..")
        + ps2_dir_entry(mode_dir, 4, 2, b"BASLUS-00001")
    )
    put_chain(41, root)
    folder = (
        ps2_dir_entry(mode_dir, 0, 0, b".", parent_cluster=0)
        + ps2_dir_entry(mode_dir, 0, 0, b"..", parent_cluster=0)
        + ps2_dir_entry(mode_file, save_length, 4, b"save.json")
        + ps2_dir_entry(mode_file, icon_length, 5, b"icon.sys")
    )
    put_chain(43, folder)
    put_chain(45, save_json + b"\x00" * (1024 - save_length))
    put_chain(46, icon + b"\x00" * (1024 - icon_length))

    out = bytearray()
    for index in range(total_pages):
        chunk = data[page(index) : page(index) + page_len]
        out += ps2_page(bytes(chunk), spare)
    return bytes(out)


# ---------------------------------------------------------------- PSU


def psu_header(kind: int, size: int, name: bytes) -> bytes:
    header = bytearray(512)
    struct.pack_into("<H", header, 0, kind)
    struct.pack_into("<H", header, 2, 0)
    struct.pack_into("<I", header, 4, size)
    header[8:16] = ps2_time()
    struct.pack_into("<H", header, 0x10, 0)
    struct.pack_into("<H", header, 0x12, 0)
    struct.pack_into("<I", header, 0x14, 0)
    header[0x18:0x20] = ps2_time()
    header[64 : 64 + len(name)] = name
    return bytes(header)


def psu_file(name: bytes, content: bytes) -> bytes:
    padding = (-len(content)) % 1024
    return psu_header(0x8497, len(content), name) + content + b"\x00" * padding


def write_psu() -> bytes:
    save_json = json_bytes("psu", 1000)
    icon = b"SYNTHETIC ICON" + b"\x00" * 86
    entries = [
        psu_header(0x8427, 4, b"BASLUS-00001"),
        psu_header(0x8427, 0, b"."),
        psu_header(0x8427, 0, b".."),
        psu_file(b"save.json", save_json),
        psu_file(b"icon.sys", icon),
    ]
    return b"".join(entries)


# ---------------------------------------------------------------- SharkPort


def sps_text(text: bytes) -> bytes:
    return struct.pack("<I", len(text)) + text


def sps_checksum(data: bytes) -> int:
    total = 0
    for byte in data:
        total = (total + (byte << (total % 18))) & 0xFFFFFFFF
    return total


def write_sharkport(title: bytes) -> bytes:
    raw = json_bytes("gba", 1024)
    second = bytearray(0x1C)
    second[0:16] = b"SYNTHSAVE\x00\x00\x00\x00\x00\x00\x00"
    struct.pack_into("<H", second, 16, 0x1234)
    second[18] = 0x56
    second[19] = 0x30
    second[20] = 1
    covered = bytes(second) + raw
    return (
        sps_text(b"SharkPortSave")
        + struct.pack("<I", 0x000F0000)
        + sps_text(title)
        + sps_text(b"2026-10-25")
        + sps_text(b"synthetic fixture")
        + struct.pack("<I", len(covered))
        + covered
        + struct.pack("<I", sps_checksum(covered))
    )


def main() -> None:
    (OUT / "ps1").mkdir(parents=True, exist_ok=True)
    (OUT / "ps2").mkdir(parents=True, exist_ok=True)
    (OUT / "gba").mkdir(parents=True, exist_ok=True)

    card = write_ps1_card()
    (OUT / "ps1" / "card.mcr").write_bytes(card)
    (OUT / "ps1" / "card.gme").write_bytes(write_gme(card))

    (OUT / "ps2" / "card.ps2").write_bytes(write_ps2_card(spare=True))
    (OUT / "ps2" / "card-raw.ps2").write_bytes(write_ps2_card(spare=False))
    (OUT / "ps2" / "save.psu").write_bytes(write_psu())

    sps = write_sharkport(b"Test Save")
    (OUT / "gba" / "save.sps").write_bytes(sps)
    (OUT / "gba" / "save.xps").write_bytes(sps)
    print(f"wrote fixtures under {OUT}")


if __name__ == "__main__":
    main()
