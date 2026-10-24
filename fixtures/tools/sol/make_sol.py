#!/usr/bin/env python3
"""Generate invented Adobe Flash Local Shared Object examples from public AMF specs."""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]


def u29(value):
    value &= 0x1FFFFFFF
    if value < 0x80:
        return bytes([value])
    if value < 0x4000:
        return bytes([(value >> 7) | 0x80, value & 0x7F])
    if value < 0x200000:
        return bytes([(value >> 14) | 0x80, ((value >> 7) & 0x7F) | 0x80, value & 0x7F])
    return bytes([
        ((value >> 22) & 0x7F) | 0x80,
        ((value >> 15) & 0x7F) | 0x80,
        ((value >> 8) & 0x7F) | 0x80,
        value & 0xFF,
    ])


def amf3_string(value):
    raw = value.encode("utf-8")
    return u29((len(raw) << 1) | 1) + raw


def amf3_int(value):
    return b"\x04" + u29(value)


def amf3_double(value):
    return b"\x05" + struct.pack(">d", value)


def amf0_string(value, long=False):
    raw = value.encode("utf-8")
    return (b"\x0c" + struct.pack(">I", len(raw)) if long else b"\x02" + struct.pack(">H", len(raw))) + raw


def amf0_object(pairs, typed=None):
    prefix = b"\x10" + struct.pack(">H", len(typed.encode())) + typed.encode() if typed else b"\x03"
    return prefix + b"".join(struct.pack(">H", len(k.encode())) + k.encode() + v for k, v in pairs) + b"\0\0\x09"


def header(version, body):
    name = b"synthetic"
    data = b"\0\xbf\0\0\0\0TCSO\0\x04\0\0\0\0" + struct.pack(">H", len(name)) + name + struct.pack(">I", version) + body
    return data[:2] + struct.pack(">I", len(data) - 6) + data[6:]


def pairs_amf0(pairs):
    return b"".join(struct.pack(">H", len(k.encode())) + k.encode() + value + b"\0" for k, value in pairs)


def amf0_file():
    shared = amf0_object([("hp", b"\0" + struct.pack(">d", 23.0))])
    body = pairs_amf0([
        ("score", b"\0" + struct.pack(">d", 1234.5)),
        ("title", amf0_string("invented save")),
        ("settings", amf0_object([("music", b"\x01\x01"), ("volume", b"\0" + struct.pack(">d", .75))])),
        ("items", b"\x08" + struct.pack(">I", 99) + amf0_object([("potion", b"\0" + struct.pack(">d", 3.0))])[1:]),
        ("party", b"\x0a" + struct.pack(">I", 2) + b"\x05" + b"\x02\0\x02hi"),
        ("shared", shared),
        ("sharedAgain", b"\x07\0\x03"),
        ("savedAt", b"\x0b" + struct.pack(">d", 123456.0) + b"\0\0"),
        ("typed", amf0_object([("id", b"\0" + struct.pack(">d", 7.0))], "com.example.Item")),
        ("longText", amf0_string("a longer invented string", long=True)),
    ])
    return header(0, body)


def object_with_one_trait(name, value):
    # Inline object, inline non-dynamic trait, one sealed member.
    return b"\x0a\x13" + amf3_string("Avatar") + amf3_string(name) + value


def amf3_file():
    second_trait_ref = b"\x0a\x01" + amf3_int(8)
    assoc_array = b"\x09\x05" + amf3_string("kind") + b"\x06" + amf3_string("bag") + b"\x01" + amf3_int(1) + amf3_int(2)
    dynamic_object = b"\x0a\x0b" + amf3_string("Profile") + amf3_string("rank") + amf3_int(4) + amf3_string("")
    byte_array = b"\x0c\x07" + b"\x00\x7f\xff"
    vector = b"\x0d\x05\x00" + struct.pack(">ii", -2, 9)
    values = [
        ("zero", amf3_int(0)),
        ("negative", amf3_int(-123)),
        ("min29", amf3_int(-(1 << 28))),
        ("max29", amf3_int((1 << 28) - 1)),
        ("double", amf3_double(3.125)),
        ("firstText", b"\x06" + amf3_string("shared phrase")),
        ("againText", b"\x06" + u29(12)),
        ("avatar", object_with_one_trait("level", amf3_int(12))),
        ("avatar2", second_trait_ref),
        ("profile", dynamic_object),
        ("inventory", assoc_array),
        ("data", byte_array),
        ("vector", vector),
        ("avatarAgain", b"\x0a\x00"),
    ]
    return header(3, b"".join(amf3_string(name) + value + b"\0" for name, value in values))


def main():
    target = ROOT / "synthetic" / "sol"
    target.mkdir(parents=True, exist_ok=True)
    (target / "amf0.sol").write_bytes(amf0_file())
    (target / "amf3.sol").write_bytes(amf3_file())


if __name__ == "__main__":
    main()
