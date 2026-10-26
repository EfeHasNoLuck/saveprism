#!/usr/bin/env python3
"""Builds the synthetic Witcher 3 content bundles under fixtures/synthetic/witcher3-bundle/.

Every byte follows the public bundle notes in Systemcluster/w3edit (`w3edit-formats.md`,
BSD-2-Clause), written independently here. All card titles, names and factions are invented; no
text or table from the real game is copied. Python's standard library has no zlib-wrapper-free LZ4
or Snappy encoder, so this generator writes valid literal-only blocks for both.
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "witcher3-bundle"
MAGIC = b"POTATO70"
PAGE = 4096


def u16(value: int) -> bytes:
    return struct.pack("<H", value)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def align_page(value: int) -> int:
    return (value + PAGE - 1) // PAGE * PAGE


def lz4_literal(data: bytes) -> bytes:
    """A valid raw LZ4 block with one literal sequence and no match."""
    out = bytearray()
    length = len(data)
    if length < 15:
        out.append(length << 4)
    else:
        out.append(0xF0)
        rest = length - 15
        while rest >= 255:
            out.append(255)
            rest -= 255
        out.append(rest)
    out.extend(data)
    return bytes(out)


def snappy_literal(data: bytes) -> bytes:
    """A valid raw Snappy block made of literal elements only."""
    out = bytearray()
    value = len(data)
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            break
    position = 0
    while position < len(data):
        length = min(len(data) - position, 1 << 20)
        encoded = length - 1
        if encoded < 60:
            out.append(encoded << 2)
        else:
            width = max(1, (encoded.bit_length() + 7) // 8)
            out.append((59 + width) << 2)
            out.extend(encoded.to_bytes(width, "little"))
        out.extend(data[position : position + length])
        position += length
    return bytes(out)


def compress(data: bytes, compression: int) -> bytes:
    if compression == 0:
        return data
    if compression == 1:
        return zlib.compress(data)
    if compression == 2:
        return snappy_literal(data)
    if compression in (4, 5):
        return lz4_literal(data)
    raise ValueError(f"unsupported compression {compression}")


def entry_name(name: str) -> bytes:
    raw = name.encode("latin-1")
    if not raw or len(raw) >= 0x100:
        raise ValueError(f"bundle name must be 1..255 bytes: {name}")
    return raw + b"\0" * (0x100 - len(raw))


def bundle(version: int, entries: list[tuple[str, bytes, int]]) -> bytes:
    """Writes one bundle: page-aligned payloads, CRC over the uncompressed bytes."""
    entry_size = 0x140 if version == 3 else 0x130
    toc_size = len(entries) * entry_size
    toc_end = 0x20 + toc_size
    payloads: list[tuple[str, bytes, int, int, int, bytes]] = []
    position = align_page(toc_end)
    for name, data, compression in entries:
        payload = compress(data, compression)
        payloads.append((name, payload, len(data), compression, position, data))
        position = align_page(position + len(payload))
    total = position

    header = bytearray(MAGIC)
    if version == 3:
        header += u32(total) + u32(0) + u32(toc_size)
        header += bytes([0x03, 0x00, 0x01, 0x00, 0x00]) + b"\x13" * 7
    else:
        header += u64(total) + u32(toc_size) + u16(5) + u64(toc_end) + b"\0\0"
    assert len(header) == 0x20

    out = bytearray(header)
    for name, payload, size, compression, offset, data in payloads:
        toc = bytearray(entry_name(name)) + b"\0" * 16
        crc = zlib.crc32(data) & 0xFFFFFFFF
        if version == 3:
            toc += u32(0) + u32(size) + u32(len(payload)) + u32(offset)
            toc += u32(0) + u32(0) + b"\0" * 16 + u32(crc) + u32(compression)
        else:
            toc += u64(offset) + u32(size) + u32(len(payload)) + u32(crc) + u32(compression)
            toc += b"\0" * 8
        out += toc
    if payloads:
        out += b"\0" * (payloads[0][4] - len(out))
    for _, payload, _, _, offset, _ in payloads:
        if len(out) < offset:
            out += b"\0" * (offset - len(out))
        out += payload
    return bytes(out)


def card_xml(cards: list[tuple[int, str, str, bool]]) -> bytes:
    lines = ['<?xml version="1.0" encoding="UTF-8"?>', "<redxml>", "  <definitions>"]
    for index, name, faction, leader in cards:
        lines.append(f'    <card index="{index}" title="invented_{name}" name="{name}"')
        lines.append(f'          faction_index="{faction}">')
        lines.append("      <type_flags>")
        if leader:
            lines.append('        <flag name="TYPE_KING" />')
        lines.append('        <flag name="TYPE_CREATURE" />')
        lines.append("      </type_flags>")
        lines.append("    </card>")
    lines += ["  </definitions>", "</redxml>", ""]
    return "\n".join(lines).encode("utf-8")


def card_xml_utf16(cards: list[tuple[int, str, str, bool]]) -> bytes:
    text = card_xml(cards).decode("utf-8")
    return b"\xff\xfe" + text.encode("utf-16-le")


REGULAR = [
    (1, "alpha_test", "F_NEUTRAL", False),
    (2, "beta_test", "F_NORTHERN_KINGDOM", False),
    (3, "gamma_test", "F_NILFGAARD", False),
]
LEADERS = [
    (1001, "leader_one", "F_NORTHERN_KINGDOM", True),
    (1002, "leader_two", "F_SCOIATAEL", True),
]
REGULAR_PATH = "gameplay\\items\\def_gwint_cards_final.xml"
PLUS_PATH = "gameplay\\items_plus\\def_gwint_cards_final.xml"
LEADER_PATH = "gameplay\\items\\def_gwint_battle_king_cards.xml"


def game_files() -> dict[str, bytes]:
    base = bundle(
        3,
        [
            (REGULAR_PATH, card_xml(REGULAR), 1),
            (LEADER_PATH, card_xml_utf16(LEADERS), 1),
        ],
    )
    patch1 = bundle(
        5,
        [
            (REGULAR_PATH, card_xml([(2, "beta_patched", "F_NORTHERN_KINGDOM", False)]), 4),
            (PLUS_PATH, card_xml([(1, "alpha_plus", "F_NEUTRAL", False)]), 4),
        ],
    )
    patch2 = bundle(
        5,
        [(REGULAR_PATH, card_xml([(3, "gamma_patch2", "F_NILFGAARD", False)]), 2)],
    )
    dlc = bundle(
        5,
        [(LEADER_PATH, card_xml([(1001, "leader_one_dlc", "F_NORTHERN_KINGDOM", True)]), 5)],
    )
    return {
        "content/content0/bundles/xml.bundle": base,
        "content/content0/bundles/patch1.bundle": patch1,
        "content/content0/bundles/patch2.bundle": patch2,
        "dlc/ep2/content/bundles/blob0.bundle": dlc,
    }


def write_game(root: Path) -> None:
    for relative, data in game_files().items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


def main() -> None:
    write_game(OUT / "game")
    write_game(OUT / "steam" / "steamapps" / "common" / "The Witcher 3")
    write_game(OUT / "gog" / "The Witcher 3 Wild Hunt GOTY")

    standalone = OUT / "standalone"
    standalone.mkdir(parents=True, exist_ok=True)
    base_entries = [
        (REGULAR_PATH, card_xml(REGULAR), 0),
        (LEADER_PATH, card_xml(LEADERS), 0),
    ]
    (standalone / "v3-none.bundle").write_bytes(bundle(3, base_entries))
    (standalone / "v5-none.bundle").write_bytes(bundle(5, base_entries))
    (standalone / "v3-zlib.bundle").write_bytes(
        bundle(
            3,
            [
                (REGULAR_PATH, card_xml(REGULAR), 1),
                (LEADER_PATH, card_xml_utf16(LEADERS), 1),
            ],
        )
    )
    (standalone / "v5-lz4.bundle").write_bytes(
        bundle(
            5,
            [
                (REGULAR_PATH, card_xml(REGULAR), 4),
                (LEADER_PATH, card_xml_utf16(LEADERS), 5),
            ],
        )
    )
    (standalone / "v5-snappy.bundle").write_bytes(
        bundle(
            5,
            [
                (REGULAR_PATH, card_xml(REGULAR), 2),
                (LEADER_PATH, card_xml(LEADERS), 2),
            ],
        )
    )


if __name__ == "__main__":
    main()
