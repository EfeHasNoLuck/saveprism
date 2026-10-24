#!/usr/bin/env python3
"""Generate invented Godot ConfigFile and binary Variant samples.

The bytes follow the public Godot "Binary serialization API" layout: Godot 4 uses its own
Variant::Type numbers, Godot 3 the older table, strings are u32 length + UTF-8 + zero padding,
arrays/dictionaries carry a u32 count, and PackedByteArray data is padded to four bytes.
Godot 3/4 packed string arrays store length + 1 and a trailing NUL, as Godot's writer does.
"""

from pathlib import Path
import struct

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "godot"


def word(value: int) -> bytes:
    return struct.pack("<I", value)


def header(kind: int, flags: int = 0) -> bytes:
    return word(kind | (flags << 16))


def string(value: str) -> bytes:
    raw = value.encode("utf-8")
    return word(len(raw)) + raw + bytes((-len(raw)) % 4)


def variant(kind: int, payload: bytes = b"", flags: int = 0) -> bytes:
    return header(kind, flags) + payload


def vstr(value: str) -> bytes:
    return variant(4, string(value))


def vint(value: int, wide: bool = False) -> bytes:
    return variant(2, struct.pack("<q" if wide else "<i", value), int(wide))


def vfloat(value: float, wide: bool = False) -> bytes:
    return variant(3, struct.pack("<d" if wide else "<f", value), int(wide))


def vbool(value: bool) -> bytes:
    return variant(1, word(1 if value else 0))


def array4(items: list[bytes]) -> bytes:
    return variant(28, word(len(items)) + b"".join(items))


def array3(items: list[bytes]) -> bytes:
    return variant(19, word(len(items)) + b"".join(items))


def dictionary(items: list[tuple[bytes, bytes]], old: bool = False) -> bytes:
    payload = word(len(items))
    for key, value in items:
        payload += key + value
    return variant(18 if old else 27, payload)


def typed_array4(builtin_type: int, items: list[bytes]) -> bytes:
    # Bits 16-17 = 0b01: the descriptor is a builtin type id.
    payload = word(builtin_type) + word(len(items)) + b"".join(items)
    return variant(28, payload, 0b01)


def vector2(x: float, y: float) -> bytes:
    return variant(5, struct.pack("<ff", x, y))


def color(r: float, g: float, b: float, a: float) -> bytes:
    return variant(20, struct.pack("<ffff", r, g, b, a))


def packed_int32(values: list[int]) -> bytes:
    return variant(30, word(len(values)) + struct.pack(f"<{len(values)}i", *values))


def packed_byte3(data: bytes) -> bytes:
    return variant(20, word(len(data)) + data + bytes((-len(data)) % 4))


def packed_string4(values: list[str]) -> bytes:
    payload = word(len(values))
    for value in values:
        raw = value.encode("utf-8") + b"\0"
        payload += word(len(raw)) + raw + bytes((-len(raw)) % 4)
    return variant(34, payload)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    config = '''; Invented values; spacing and comments are intentional.
[Player]
name   = "Mira\\nRune" ; escaped string
level=  42
ratio = 1.25
alive = true
empty = null
position = Vector2(12.5, -4)
grid = Vector3i(1, 2, 3)
tint = Color(0.2, 0.4, 0.8, 1)
area = Rect2(1, 2, 32, 48)
clip = Vector4(0, 0, 1, 1)
labels = PackedStringArray("north", "南")
ids = PackedInt32Array(7, 11, 13)
alias = &"hero"
path = NodePath("World/Player")
values = [
  1,
  2.5,
  "three"
]
metadata = {"chapter": 3, "cleared": false}
special_inf = inf
special_neg = -inf
special_nan = nan
scientific = 1e10

[Inventory]
gold = 1000
items = {"potion": 3, "key": 1, "nested": {"a": [1, 2]}}
'''
    (OUT / "config.cfg").write_text(config, encoding="utf-8", newline="\n")

    player = dictionary([
        (vstr("name"), vstr("Mira 南")),
        (vstr("stats"), dictionary([
            (vstr("level"), vint(63)),
            (vstr("xp"), vint(9_007_199_254_740_991, True)),
        ])),
        (vstr("inventory"), array4([vstr("potion"), vint(4), vstr("key"), vint(1)])),
        (vstr("position"), vector2(12.5, -4.0)),
        (vstr("color"), color(0.2, 0.4, 0.8, 1.0)),
        (vstr("packed"), packed_int32([7, 11, 13])),
        (vstr("labels"), packed_string4(["north", "南"])),
        (vstr("by_id"), dictionary([(vint(1), vstr("alpha")), (vint(2), vstr("beta"))])),
        (vstr("levels"), typed_array4(2, [vint(1), vint(2), vint(3)])),
    ])
    (OUT / "godot4.dat").write_bytes(player)

    godot3 = dictionary([
        (vstr("player"), vstr("Old save")),
        (vstr("level"), vint(18)),
        (vstr("ratio"), vfloat(2.5)),
        (vstr("flags"), array3([vbool(True), vbool(False)])),
        (vstr("bytes"), packed_byte3(b"SAVE")),
        (vstr("stats"), dictionary([
            (vstr("hp"), vint(90)),
            (vstr("items"), array3([vstr("map"), vint(1)])),
        ], old=True)),
    ], old=True)
    (OUT / "godot3.sav").write_bytes(godot3)
    (OUT / ".encrypted-header.gdec").write_bytes(b"GDEC" + bytes(12))


if __name__ == "__main__":
    main()
