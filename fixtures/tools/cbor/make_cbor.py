"""Writes synthetic CBOR fixtures from RFC 8949 (STD 94)."""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "cbor"


def head(major, value):
    base = major << 5
    if value < 24:
        return bytes([base | value])
    if value <= 0xFF:
        return bytes([base | 24, value])
    if value <= 0xFFFF:
        return bytes([base | 25]) + struct.pack(">H", value)
    if value <= 0xFFFFFFFF:
        return bytes([base | 26]) + struct.pack(">I", value)
    return bytes([base | 27]) + struct.pack(">Q", value)


def text(value):
    data = value.encode("utf-8")
    return head(3, len(data)) + data


def byte_string(value):
    return head(2, len(value)) + value


def array(items):
    return head(4, len(items)) + b"".join(items)


def map_(pairs):
    return head(5, len(pairs)) + b"".join(key + value for key, value in pairs)


def integer(value):
    if value >= 0:
        return head(0, value)
    return head(1, -1 - value)


def half(value):
    # Only the values used here: 1.5, 0.0, -0.0, 65504, infinity, undefined NaN.
    if value == 1.5:
        return b"\xf9\x3e\x00"
    if value == 0.0:
        return b"\xf9\x00\x00"
    if value == 65504:
        return b"\xf9\x7b\xff"
    if value == float("inf"):
        return b"\xf9\x7c\x00"
    raise ValueError(value)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    # Definite-length tree with every major type this codec reads.
    document = map_(
        [
            (text("name"), text("Mira")),
            (text("level"), integer(12)),
            (text("negative"), integer(-500)),
            (text("float16"), half(1.5)),
            (text("float32"), b"\xfa" + struct.pack(">f", -2.5)),
            (text("float64"), b"\xfb" + struct.pack(">d", 3.25)),
            (text("alive"), b"\xf5"),
            (text("empty"), b"\xf6"),
            (text("blob"), byte_string(bytes([0, 1, 2, 255]))),
            (text("items"), array([integer(1), text("two"), b"\xf4"])),
            (text("nested"), map_([(text("key"), text("value"))])),
            (text("created"), b"\xc1" + integer(1_700_000_000)),
            (text("big"), b"\xc2" + byte_string(b"\x01\x00")),
            (text("simple"), b"\xf8\x20"),
        ]
    )
    (OUT / "player.cbor").write_bytes(document)

    # Indefinite-length map, array and text string, and a non-shortest integer.
    indefinite = b"\xbf"
    indefinite += text("title") + b"\x7f" + text("The ") + text("Long ") + text("Road") + b"\xff"
    indefinite += text("numbers") + b"\x9f" + b"\x18\x05" + integer(6) + b"\xff"
    indefinite += text("bytes") + b"\x5f" + byte_string(b"ab") + byte_string(b"cd") + b"\xff"
    indefinite += text("flag") + b"\xf5"
    indefinite += b"\xff"
    (OUT / "indefinite.cbor").write_bytes(indefinite)

    # A top-level indefinite array with an empty map.
    (OUT / "sequence.cbor").write_bytes(b"\x9f\xa0\xf6\xff")


if __name__ == "__main__":
    main()
