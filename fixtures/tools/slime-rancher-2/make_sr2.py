#!/usr/bin/env python3
"""Generate an invented Slime Rancher 2 save from the behavioral spec.

The layout follows docs/research/formats-slime-rancher.md; names and values are made up and no
bytes of a real save are copied. Only records the codec reads are complete; the rest are small
stand-ins the codec must leave alone (an unknown record, a world actor `SRAD` version 4, a slot
run whose count does not match).
"""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "slime-rancher-2" / "invented_1.sav"

BEGIN = b"\x00\x30"
END = b"\x00\x40"


def string(value):
    raw = value.encode("utf-8")
    length = len(raw)
    out = bytearray()
    while True:
        byte = length & 0x7F
        length >>= 7
        if length:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out) + raw


def i32(value):
    return struct.pack("<i", value)


def f32(value):
    return struct.pack("<f", value)


def record(tag, version, body):
    return string(tag) + i32(version) + BEGIN + body + END


def name_table(names):
    return record("INDXTBL", 1, i32(len(names)) + b"".join(string(n) for n in names))


def currency(amount):
    return record("SRCRNCY", 1, i32(1) + i32(amount) + i32(250))


def slot(item, count):
    emotions = record("SRSED", 1, i32(2) + i32(1) + f32(0.5) + i32(2) + f32(0.0))
    return record("SRAD", 2, i32(item) + i32(count) + i32(0) + emotions + b"\x00")


def slots(entries):
    return i32(len(entries)) + b"".join(slot(item, count) for item, count in entries)


ITEMS = [
    "IdentifiableType.InventedPlort",
    "SlimeDefinition.Invented",
    "IdentifiableType.InventedVeggie",
]

lookups = record(
    "INDXTBLLKUP",
    1,
    i32(2)
    + string("IdentifiableType")
    + name_table(ITEMS)
    + string("[1].World.ZoneDefinition")
    + name_table(["meadow", "caves"]),
)

MONEY = 1234
summary = (
    string("20260101120000_1")
    + string("1")
    + i32(0)
    + record(
        "",
        3,
        string("0.0.0 [invented]")
        + i32(1)
        + currency(MONEY)
        + i32(0)
        + i32(5)
        + struct.pack("<d", 7200.0)
        + struct.pack("<q", 0)
        + i32(1),
    )
)

settings = record(
    "SRGAMESETTINGS",
    1,
    i32(1) + record("", 0, string("setting.TutorialsEnabled") + string("on")) + i32(3),
)

actor = record("SRAD", 4, record("SRV3", 1, f32(1.0) + f32(2.0) + f32(3.0)) + i32(7))

player = record(
    "",
    12,
    i32(100)
    + i32(90)
    + i32(0)
    + i32(1)
    + currency(MONEY)
    + i32(0)
    + record("SRUPGRADECOMPONENTS", 2, i32(0))
    + slots([(0, 12), (1, 3), (-1, 0), (-1, 0)]),
)

plot = record("SRLP", 2, i32(1) + slots([(2, 40), (-1, 0)]))

# Two slots behind a count of 3: not a slot group, so the codec must not show them.
mismatch = record("SRTEST", 1, i32(3) + slot(0, 9) + slot(1, 9))

root = record("", 11, lookups + summary + settings + actor + player + plot + mismatch)

OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_bytes(root)
print(f"wrote {OUT} ({len(root)} bytes)")
