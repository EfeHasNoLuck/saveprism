#!/usr/bin/env python3
"""Builds the synthetic Witcher 3 (PC) save under fixtures/synthetic/witcher3/.

The bytes follow the public descriptions of the `SNFHFZLC` container and the REDengine record
stream (see the codec module doc). All names, item counts and text are invented. Python's
standard library has no LZ4, so each chunk is a literal-only LZ4 block, which is valid LZ4 and
decodes to the original bytes.
"""

from __future__ import annotations

import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "witcher3" / "ManualSave_invented.sav"

HEADER_SIZE = 40  # 16 + 12 * 2


def u16(value: int) -> bytes:
    return struct.pack("<H", value)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def lz4_literal(data: bytes) -> bytes:
    """A valid LZ4 block with one literal sequence and no match."""
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


def red_string(text: str, narrow: bool = True) -> bytes:
    """REDengine length-prefixed string: narrow bytes (0x80) or UTF-16LE characters."""
    if narrow:
        data = text.encode("latin-1")
        if len(data) < 0x3F:
            return bytes([0x80 | len(data)]) + data
        return bytes([0x80 | 0x40]) + bytes([len(data) >> 6]) + data
    data = text.encode("utf-16-le")
    count = len(data) // 2
    if count < 0x3F:
        return bytes([count]) + data
    return bytes([0x40]) + bytes([count >> 6]) + data


def red_string_ansi(text: str) -> bytes:
    data = text.encode("latin-1") + b"\0"
    return bytes([len(data)]) + data


NAMES = [
    "W3Player",  # 1
    "Orens",  # 2
    "Int32",  # 3
    "level",  # 4
    "name",  # 5
    "String",  # 6
    "weight",  # 7
    "Float",  # 8
    "CInventoryComponent",  # 9
    "SItem",  # 10
    "quantity",  # 11
    "itemName",  # 12
    "CName",  # 13
    "quantities",  # 14
    "array:2,0,Int32",  # 15
    "saveInfo",  # 16
    "playthroughId",  # 17
    "facts",  # 18
    "GwintManager",  # 19
    "cardCount",  # 20
    "collection",  # 21
    "array:2,0,Uint8",  # 22
    "InitialBuildIDBlock",  # 23
    "InitialBuildID",  # 24
    "StringAnsi",  # 25
    "Bool",  # 26
    "isNewGamePlus",  # 27
    "Double",  # 28
    "playtime",  # 29
    "Uint64",  # 30
    "seed",  # 31
    "latinText",  # 32
    "bonus",  # 33
    "OP",  # 34 (unused placeholder, keeps indices explicit)
]


def name(text: str) -> int:
    return NAMES.index(text) + 1


def vl(prop: str, type_name: str, value: bytes):
    return ("VL", prop, type_name, value)


def op(prop: str, type_name: str, value: bytes):
    return ("OP", prop, type_name, value)


def bs(block: str, children):
    return ("BS", block, children)


def blck(block: str, children):
    return ("BLCK", block, children)


def ss(block: str, payload: bytes):
    return ("SS", block, payload)


TREE = [
    bs(
        "W3Player",
        [
            vl("Orens", "Int32", struct.pack("<i", 1250)),
            vl("level", "Int32", struct.pack("<i", 12)),
            vl("name", "String", red_string("Geralt")),
            vl("weight", "Float", struct.pack("<f", 71.5)),
            vl("latinText", "String", red_string("caf\u00e9")),
            bs(
                "CInventoryComponent",
                [
                    vl("quantity", "Int32", struct.pack("<i", 3)),
                    vl(
                        "quantities",
                        "array:2,0,Int32",
                        u32(3) + struct.pack("<iii", 1, 2, 3),
                    ),
                    bs(
                        "SItem",
                        [
                            vl("quantity", "Int32", struct.pack("<i", 7)),
                            vl("itemName", "CName", u16(name("Orens"))),
                        ],
                    ),
                    vl("isNewGamePlus", "Bool", b"\x01"),
                ],
            ),
        ],
    ),
    bs(
        "saveInfo",
        [
            vl("playthroughId", "String", red_string("invented-playthrough", narrow=False)),
            op("bonus", "Int32", struct.pack("<i", 5)),
        ],
    ),
    bs("facts", [ss("facts", b"SXAP" + u32(66) + u32(29) + u32(163) + b"invented-facts")]),
    blck(
        "GwintManager",
        [
            vl("cardCount", "Int32", struct.pack("<i", 2)),
            vl("collection", "array:2,0,Uint8", u32(4) + bytes([1, 2, 3, 4])),
        ],
    ),
    bs(
        "InitialBuildIDBlock",
        [
            vl("InitialBuildID", "StringAnsi", red_string_ansi("invented-build-1")),
            vl("playtime", "Double", struct.pack("<d", 1234.5)),
            vl("seed", "Uint64", struct.pack("<Q", 18446744073709551615)),
        ],
    ),
]


def build_stream() -> bytes:
    stream = bytearray(b"SAV3" + u32(66) + u32(29) + u32(163))
    entries: list[tuple[int, int, int]] = []
    # Explicit pre-order walk so parents are recorded before their children.
    def emit(node):
        start = len(stream)
        kind = node[0]
        if kind == "BS":
            stream.extend(b"BS" + u16(name(node[1])))
            for child in node[2]:
                emit(child)
        elif kind == "BLCK":
            stream.extend(b"BLCK" + u16(name(node[1])) + b"\0\0" + b"\0\0")
            body = len(stream)
            for child in node[2]:
                emit(child)
            struct.pack_into("<H", stream, start + 6, len(stream) - body)
        elif kind == "VL":
            stream.extend(b"VL" + u16(name(node[1])) + u16(name(node[2])) + node[3])
        elif kind == "OP":
            stream.extend(b"OP" + u16(name(node[1])) + u16(name(node[2])) + node[3])
        elif kind == "SS":
            stream.extend(b"SS" + u32(len(node[2])) + node[2])
        else:
            raise ValueError(kind)
        entries.append((HEADER_SIZE + start, len(stream) - start))

    for root in TREE:
        emit(root)

    rb_offset = HEADER_SIZE + len(stream)
    stream.extend(b"RB" + u32(0))

    nm_offset = HEADER_SIZE + len(stream)
    stream.extend(b"NM")
    manu_start = HEADER_SIZE + len(stream)
    stream.extend(b"MANU" + u32(len(NAMES)) + u32(0))
    for item in NAMES:
        data = item.encode("latin-1")
        stream.extend(bytes([len(data)]) + data)
    stream.extend(u32(0) + b"ENOD")

    # The string-table footer sits at variable_table_offset - 10.
    stream.extend(u32(nm_offset) + u32(rb_offset) + b"\0\0")
    variable_offset = HEADER_SIZE + len(stream)
    stream.extend(u32(len(entries)))
    for offset, size in entries:
        stream.extend(u32(offset) + u32(size))

    stream.extend(u32(manu_start) + u32(0) + u32(variable_offset) + b"SE")
    return bytes(stream)


def build_file(stream: bytes) -> bytes:
    split = len(stream) // 2
    pieces = [stream[:split], stream[split:]]
    chunks = [lz4_literal(piece) for piece in pieces]

    header = bytearray(b"SNFHFZLC" + u32(len(pieces)) + u32(HEADER_SIZE))
    position = HEADER_SIZE
    for index, (piece, chunk) in enumerate(zip(pieces, chunks)):
        end = position + len(chunk)
        header.extend(u32(len(chunk)) + u32(len(piece)) + u32(0 if index == len(pieces) - 1 else end))
        position = end
    assert len(header) == HEADER_SIZE
    return bytes(header) + b"".join(chunks)


def main() -> None:
    stream = build_stream()
    data = build_file(stream)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(data)
    print(f"wrote {OUT} ({len(data)} bytes, {len(stream)} decoded bytes)")


if __name__ == "__main__":
    main()
