#!/usr/bin/env python3
"""Write synthetic Bitser fixtures from the ISC `gvx/bitser` `bitser.lua` format table.

Only the byte layout described there is used; all values are invented.
"""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "bitser"


def number(value: int) -> bytes:
    if -27 <= value <= 100:
        return bytes([value + 27])
    if -32768 <= value <= 32767:
        return b"\xfa" + struct.pack("<h", value)
    return b"\xf5" + struct.pack("<i", value)


def short_string(text: str) -> bytes:
    data = text.encode("utf-8")
    assert len(data) < 32
    return bytes([192 + len(data)]) + data


def long_string(text: str) -> bytes:
    data = text.encode("utf-8")
    return b"\xf4" + number(len(data)) + data


def table(items, pairs) -> bytes:
    out = bytearray(b"\xf0")
    out += number(len(items))
    for item in items:
        out += item
    out += number(len(pairs))
    for key, value in pairs:
        out += key + value
    return bytes(out)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)

    (OUT / "player.bitser").write_bytes(
        table([short_string("Ash"), number(7)], [])
    )

    nested = table([short_string("Mira")], [(short_string("hp"), number(42))])
    # seen: root = 0, nested = 1, "Mira" = 2, "hp" = 3; 128 + 1 references the nested table.
    (OUT / "nested.bitser").write_bytes(table([nested, b"\x81"], []))

    instance = b"\xf2" + short_string("Player") + number(0) + number(1)
    instance += short_string("name") + short_string("Ash")
    metatable = b"\xfd" + number(0) + number(0) + short_string("meta")
    types = table(
        [
            b"\xfa" + struct.pack("<h", 1000),
            b"\xf5" + struct.pack("<i", 70000),
            b"\xf6" + struct.pack("<d", 1.5),
            b"\xf9",
            long_string("a" * 40),
            bytes([224 + 3]) + b"res",
            instance,
            metatable,
        ],
        [],
    )
    (OUT / "types.bitser").write_bytes(types)


if __name__ == "__main__":
    main()
