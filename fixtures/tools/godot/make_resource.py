#!/usr/bin/env python3
"""Generate invented Godot text and binary resource samples (`.tres`, `.res`, `.res` + RSCC).

The binary bytes follow Godot's MIT-licensed `ResourceFormatSaverBinary` layout: `RSRC`, the
endianness/real flags, engine and format versions, the type, metadata offset, the format-specific
header (flags/uid/11 reserved fields in format 5, 14 reserved fields in format 3), the string
table, external and internal resource tables and one resource whose properties are binary
Variants with the resource format's own type ids. `RSCC` files use `FileAccessCompressed`
(mode, block size, uncompressed length, block-size table, blocks, trailing magic).
The text samples use the public Godot text-resource syntax. All values are invented.
"""

from pathlib import Path
import gzip
import struct
import zlib

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "godot"


def word(value: int) -> bytes:
    return struct.pack("<I", value)


def variant(kind: int, payload: bytes = b"") -> bytes:
    return word(kind) + payload


def unicode_string(value: str) -> bytes:
    raw = value.encode("utf-8") + b"\0"
    return word(len(raw)) + raw


def inline_string(value: str) -> bytes:
    raw = value.encode("utf-8")
    return word(len(raw) | 0x80000000) + raw


def vstr(value: str) -> bytes:
    return variant(5, unicode_string(value))


def vbool(value: bool) -> bytes:
    return variant(2, word(1 if value else 0))


def vint(value: int) -> bytes:
    return variant(3, struct.pack("<i", value))


def vint64(value: int) -> bytes:
    return variant(40, struct.pack("<q", value))


def vfloat(value: float, real64: bool) -> bytes:
    return variant(4, struct.pack("<d" if real64 else "<f", value))


def vdouble(value: float) -> bytes:
    return variant(41, struct.pack("<d", value))


def vvector2(x: float, y: float, real64: bool) -> bytes:
    return variant(10, struct.pack("<dd" if real64 else "<ff", x, y))


def vvector3i(x: int, y: int, z: int) -> bytes:
    return variant(47, struct.pack("<iii", x, y, z))


def vcolor(r: float, g: float, b: float, a: float, godot4: bool, real64: bool) -> bytes:
    if godot4:
        return variant(20, struct.pack("<ffff", r, g, b, a))
    return variant(20, struct.pack("<dddd" if real64 else "<ffff", r, g, b, a))


def varray(items: list[bytes]) -> bytes:
    return variant(30, word(len(items)) + b"".join(items))


def vdictionary(items: list[tuple[bytes, bytes]]) -> bytes:
    return variant(26, word(len(items)) + b"".join(key + value for key, value in items))


def vpacked_int32(values: list[int]) -> bytes:
    return variant(32, word(len(values)) + struct.pack(f"<{len(values)}i", *values))


def vpacked_string(values: list[str]) -> bytes:
    return variant(34, word(len(values)) + b"".join(unicode_string(value) for value in values))


def vpacked_byte(data: bytes) -> bytes:
    return variant(31, word(len(data)) + data + bytes((-len(data)) % 4))


def vnode_path(names: list[str], subnames: list[str], absolute: bool) -> bytes:
    snc = len(subnames) | (0x8000 if absolute else 0)
    payload = struct.pack("<H", len(names)) + struct.pack("<H", snc)
    payload += b"".join(inline_string(name) for name in names + subnames)
    return variant(22, payload)


def vexternal(index: int) -> bytes:
    return variant(24, word(3) + word(index))


def write_rsrc(
    resource_type: str,
    strings: list[str],
    external: list[tuple[str, str]],
    resource_path: str,
    class_name: str,
    properties: list[tuple[str, bytes]],
    *,
    godot4: bool,
    real64: bool,
) -> bytes:
    table = {name: index for index, name in enumerate(strings)}
    out = bytearray(b"RSRC")
    out += word(0)
    out += word(1 if real64 else 0)
    if godot4:
        out += word(4) + word(2) + word(5)
    else:
        out += word(3) + word(4) + word(3)
    out += unicode_string(resource_type)
    out += struct.pack("<Q", 0)
    if godot4:
        out += word(0)
        out += struct.pack("<Q", 0)
        out += bytes(11 * 4)
    else:
        out += bytes(14 * 4)
    out += word(len(strings))
    for value in strings:
        out += unicode_string(value)
    out += word(len(external))
    for ext_type, ext_path in external:
        out += unicode_string(ext_type)
        out += unicode_string(ext_path)
    out += word(1)
    out += unicode_string(resource_path)
    offset_at = len(out)
    out += struct.pack("<Q", 0)
    resource_at = len(out)
    out += unicode_string(class_name)
    out += word(len(properties))
    for name, payload in properties:
        out += word(table[name])
        out += payload
    struct.pack_into("<Q", out, offset_at, resource_at)
    out += b"RSRC"
    return bytes(out)


def rscc(inner: bytes, mode: int, block_size: int) -> bytes:
    blocks = len(inner) // block_size + 1
    compressed = []
    for index in range(blocks):
        length = len(inner) % block_size if index == blocks - 1 else block_size
        chunk = inner[index * block_size:index * block_size + length]
        if mode == 1:
            compressed.append(zlib.compress(chunk))
        elif mode == 3:
            compressed.append(gzip.compress(chunk, mtime=0))
        else:
            raise ValueError(mode)
    out = bytearray(b"RSCC")
    out += word(mode) + word(block_size) + word(len(inner))
    for block in compressed:
        out += word(len(block))
    for block in compressed:
        out += block
    out += b"RSCC"
    return bytes(out)


TEXT4 = '''; Invented Godot 4 text resource.
[gd_resource type="Resource" script_class="PlayerData" load_steps=3 format=3 uid="uid://invented1"]

[ext_resource type="Script" path="res://player.gd" id="1_script"]

[sub_resource type="Resource" id="Resource_stats"]
script = ExtResource("1_script")
gold = 100
ratio = 1.5

[resource]
script = ExtResource("1_script")
name = "Mira"
level = 42
position = Vector2(12.5, -4)
stats = SubResource("Resource_stats")
tags = Array[String](["north", "south"])
colors = Dictionary[String, int]({"a": 1})
inventory = ["potion", 3]
options = {"hard": false}
'''

TEXT3 = '''; Invented Godot 3 text resource.
[gd_resource type="Resource" load_steps=2 format=2]

[ext_resource path="res://player.gd" type="Script" id=1]

[sub_resource type="Resource" id=1]
gold = 50

[resource]
script = ExtResource( 1 )
name = "Old"
level = 18
stats = SubResource( 1 )
'''


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "resource4.tres").write_text(TEXT4, encoding="utf-8", newline="\n")
    (OUT / "resource3.tres").write_text(TEXT3, encoding="utf-8", newline="\n")

    strings4 = [
        "script", "player_name", "gold", "xp", "level", "hp", "accuracy", "alive",
        "position", "tint", "cell", "quest", "inventory", "stats", "packed", "labels",
        "blob", "reference",
    ]
    properties4 = [
        ("script", vexternal(0)),
        ("player_name", vstr("Mira 南")),
        ("gold", vint(1234)),
        ("xp", vint64(9_007_199_254_740_991)),
        ("level", vint(42)),
        ("hp", vfloat(1.5, False)),
        ("accuracy", vdouble(0.125)),
        ("alive", vbool(True)),
        ("position", vvector2(12.5, -4.0, False)),
        ("tint", vcolor(0.2, 0.4, 0.8, 1.0, True, False)),
        ("cell", vvector3i(3, -2, 1)),
        ("quest", vnode_path(["World", "Player"], ["position"], True)),
        ("inventory", varray([vstr("potion"), vint(3)])),
        ("stats", vdictionary([
            (vstr("str"), vint(7)),
            (vint(2), vbool(False)),
        ])),
        ("packed", vpacked_int32([7, 11, 13])),
        ("labels", vpacked_string(["north", "南"])),
        ("blob", vpacked_byte(b"SAVE")),
        ("reference", vexternal(0)),
    ]
    resource4 = write_rsrc(
        "Resource", strings4, [("Script", "res://player.gd")], "local://1", "Resource",
        properties4, godot4=True, real64=False,
    )
    (OUT / "resource4.res").write_bytes(resource4)
    (OUT / "resource4-deflate.res").write_bytes(rscc(resource4, 1, 4096))
    (OUT / "resource4-gzip.res").write_bytes(rscc(resource4, 3, 4096))

    strings3 = ["name", "level", "ratio", "flags", "bytes", "stats"]
    properties3 = [
        ("name", vstr("Old save")),
        ("level", vint(18)),
        ("ratio", vfloat(2.5, False)),
        ("flags", varray([vbool(True), vbool(False)])),
        ("bytes", vpacked_byte(b"SAVE")),
        ("stats", vdictionary([
            (vstr("hp"), vint(90)),
            (vstr("items"), varray([vstr("map"), vint(1)])),
        ])),
    ]
    resource3 = write_rsrc(
        "Resource", strings3, [], "local://1", "Resource", properties3,
        godot4=False, real64=False,
    )
    (OUT / "resource3.res").write_bytes(resource3)

    (OUT / ".fastlz-header.rscc").write_bytes(
        b"RSCC" + word(0) + word(4096) + word(8) + word(4) + b"xxxx" + b"RSCC"
    )


if __name__ == "__main__":
    main()
