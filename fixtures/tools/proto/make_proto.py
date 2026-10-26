#!/usr/bin/env python3
"""Write synthetic schema-less Protocol Buffers fixtures from the public wire-format guide.

Encoding rules: <https://protobuf.dev/programming-guides/encoding/> (varints, the tag
(field_number << 3) | wire_type, LEN payloads, groups). All data is invented.
"""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "proto"


def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            byte |= 0x80
        out.append(byte)
        if not value:
            return bytes(out)


def tag(field: int, wire: int) -> bytes:
    return varint((field << 3) | wire)


def varint_field(field: int, value: int) -> bytes:
    return tag(field, 0) + varint(value)


def fixed32_field(field: int, value: int) -> bytes:
    return tag(field, 5) + struct.pack("<I", value)


def fixed64_field(field: int, value: int) -> bytes:
    return tag(field, 1) + struct.pack("<Q", value)


def bytes_field(field: int, value: bytes) -> bytes:
    return tag(field, 2) + varint(len(value)) + value


def text_field(field: int, value: str) -> bytes:
    return bytes_field(field, value.encode("utf-8"))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)

    player = text_field(1, "Ash") + varint_field(2, 7) + text_field(3, "Pallet Town")
    (OUT / "player.pb").write_bytes(player)

    inner = varint_field(1, 7) + text_field(2, "Ash")
    nested = bytes_field(1, inner) + text_field(2, "top level")
    (OUT / "nested.pb").write_bytes(nested)

    # Field 2 uses a non-minimal two-byte tag (0x90 0x00 for tag 0x10); unchanged fields must
    # keep it when another value is edited.
    widths = fixed32_field(1, 0x01020304)
    widths += b"\x90\x00" + varint(1)
    widths += fixed64_field(3, 0x0102030405060708)
    widths += text_field(4, "ok")
    (OUT / "widths.pb").write_bytes(widths)

    group = tag(1, 3) + varint_field(1, 1) + tag(1, 4) + varint_field(2, 5)
    (OUT / "groups.pb").write_bytes(group)


if __name__ == "__main__":
    main()
