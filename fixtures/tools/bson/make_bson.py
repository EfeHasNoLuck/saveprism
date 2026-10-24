"""Writes synthetic BSON fixtures from the BSON 1.1 specification."""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "bson"


def cstring(value):
    return value.encode("utf-8") + b"\x00"


def string(value):
    data = value.encode("utf-8")
    return struct.pack("<i", len(data) + 1) + data + b"\x00"


def document(elements):
    body = b"".join(elements)
    return struct.pack("<i", len(body) + 5) + body + b"\x00"


def element(kind, name, payload):
    return bytes([kind]) + cstring(name) + payload


def double(name, value):
    return element(0x01, name, struct.pack("<d", value))


def text_(name, value):
    return element(0x02, name, string(value))


def subdocument(name, elements):
    return element(0x03, name, document(elements))


def array(name, values):
    return element(0x04, name, document([value(str(index)) for index, value in enumerate(values)]))


def binary(name, subtype, data):
    return element(0x05, name, struct.pack("<i", len(data)) + bytes([subtype]) + data)


def objectid(name, data):
    return element(0x07, name, data)


def boolean(name, value):
    return element(0x08, name, b"\x01" if value else b"\x00")


def date(name, millis):
    return element(0x09, name, struct.pack("<q", millis))


def null(name):
    return element(0x0A, name, b"")


def regex(name, pattern, options):
    return element(0x0B, name, cstring(pattern) + cstring(options))


def int32(name, value):
    return element(0x10, name, struct.pack("<i", value))


def timestamp(name, increment, seconds):
    return element(0x11, name, struct.pack("<II", increment, seconds))


def int64(name, value):
    return element(0x12, name, struct.pack("<q", value))


def decimal128(name, data):
    return element(0x13, name, data)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    save = document(
        [
            text_("name", "Rin"),
            double("health", 87.5),
            int32("level", 21),
            int64("experience", 9_000_000_000),
            boolean("alive", True),
            null("nickname"),
            objectid("playerId", bytes(range(12))),
            date("lastPlayed", 1_700_000_000_000),
            binary("icon", 0, bytes([1, 2, 3, 4])),
            timestamp("updated", 2, 1_700_000_000),
            decimal128("precise", bytes(range(16))),
            regex("tag", "^hero", "i"),
            subdocument("position", [double("x", 1.5), double("y", -2.25)]),
            element(0x04, "inventory", document([text_("0", "sword"), int32("1", 3)])),
        ]
    )
    (OUT / "player.bson").write_bytes(save)

    (OUT / "empty.bson").write_bytes(document([]))


if __name__ == "__main__":
    main()
