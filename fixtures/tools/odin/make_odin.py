#!/usr/bin/env python3
"""Writes the synthetic Odin Serializer fixtures.

The layout is the one described by the public, Apache-2.0 licensed
TeamSirenix/odin-serializer repository: BinaryEntryType.cs for the entry codes and
BinaryDataReader.cs / BinaryDataWriter.cs for strings, arrays, type entries and references.
All names and values are invented; no game data is used.
"""

from pathlib import Path
import struct as st

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "odin"


def i8(v):
    return st.pack("<b", v)


def u8(v):
    return st.pack("<B", v)


def i16(v):
    return st.pack("<h", v)


def u16(v):
    return st.pack("<H", v)


def i32(v):
    return st.pack("<i", v)


def u32(v):
    return st.pack("<I", v)


def i64(v):
    return st.pack("<q", v)


def u64(v):
    return st.pack("<Q", v)


def f32(v):
    return st.pack("<f", v)


def f64(v):
    return st.pack("<d", v)


def string(text, wide=True):
    if wide:
        data = text.encode("utf-16le")
        return b"\x01" + i32(len(data) // 2) + data
    data = bytes(ord(ch) for ch in text)
    return b"\x00" + i32(len(data)) + data


def plain(text):
    return string(text, wide=False)


def type_name(type_id, name):
    return b"\x2f" + i32(type_id) + string(name)


def type_id(type_id):
    return b"\x30" + i32(type_id)


def struct(name, type_entry, members, reference_id=None):
    if reference_id is None:
        head = b"\x03" + string(name)
        return head + type_entry + members + b"\x05"
    head = b"\x01" + string(name)
    return head + type_entry + i32(reference_id) + members + b"\x05"


def unnamed_struct(type_entry, members):
    return b"\x04" + type_entry + members + b"\x05"


def unnamed_reference(type_entry, reference_id, members):
    return b"\x02" + type_entry + i32(reference_id) + members + b"\x05"


def array(length, members):
    return b"\x06" + i64(length) + members + b"\x07"


def named_int(name, value):
    return b"\x17" + string(name) + i32(value)


def unnamed_int(value):
    return b"\x18" + i32(value)


def named_string(name, value, wide=True):
    return b"\x27" + string(name) + string(value, wide=wide)


def unnamed_string(value, wide=True):
    return b"\x28" + string(value, wide=wide)


def named_bool(name, value):
    return b"\x2b" + string(name) + (b"\x01" if value else b"\x00")


def named_float(name, value):
    return b"\x1f" + string(name) + f32(value)


def named_double(name, value):
    return b"\x21" + string(name) + f64(value)


def named_long(name, value):
    return b"\x1b" + string(name) + i64(value)


def named_ulong(name, value):
    return b"\x1d" + string(name) + u64(value)


def named_ushort(name, value):
    return b"\x15" + string(name) + u16(value)


def named_sbyte(name, value):
    return b"\x0f" + string(name) + i8(value)


def named_char(name, value):
    return b"\x25" + string(name) + u16(value)


def named_null(name):
    return b"\x2d" + string(name)


def unnamed_null():
    return b"\x2e"


def named_guid(name, payload):
    return b"\x29" + string(name) + payload


def named_decimal(name, payload):
    return b"\x23" + string(name) + payload


def internal_reference(name=None):
    if name is None:
        return b"\x0a"
    return b"\x09" + string(name)


def external_index(name=None, value=0):
    if name is None:
        return b"\x0c" + i32(value)
    return b"\x0b" + string(name) + i32(value)


def external_guid(name, payload):
    return b"\x0d" + string(name) + payload


def external_string(name, value):
    return b"\x32" + string(name) + string(value)


def primitive_array(count, bytes_per_element, payload):
    return b"\x08" + i32(count) + i32(bytes_per_element) + payload


def build_player():
    inventory = array(
        3,
        unnamed_string("sword")
        + plain_unnamed("shield")
        + unnamed_string("potion"),
    )
    player = struct(
        "player",
        type_name(0, "Game.SaveData, Assembly-CSharp"),
        named_int("gold", 120)
        + named_string("name", "Rin")
        + plain_named("title", "Ranger")
        + named_float("hp", 87.5)
        + named_double("xp", 1543.25)
        + named_bool("alive", True)
        + named_sbyte("morale", -3)
        + named_ushort("level", 21)
        + named_long("playtime", 30_000_000_000)
        + named_char("initial", ord("R"))
        + named_null("pet")
        + named_guid("account", bytes(range(16)))
        + named_decimal("rate", bytes(range(16, 32)))
        + struct(
            "inventory",
            type_id(0),
            inventory,
            reference_id=1,
        )
        + unnamed_struct(
            type_name(1, "Game.Party, Assembly-CSharp"),
            named_int("count", 2)
            + array(2, unnamed_int(7) + unnamed_int(9)),
        ),
    )
    return player


def plain_named(name, value):
    return b"\x27" + string(name) + plain(value)


def plain_unnamed(value):
    return b"\x28" + plain(value)


def build_types():
    entries = [
        # TypeName defines id 10, a later TypeID reuses it.
        struct(
            "first",
            type_name(10, "Game.First, Assembly-CSharp"),
            named_int("value", 1) + named_string("text", "one"),
            reference_id=100,
        ),
        unnamed_reference(
            type_id(10),
            101,
            named_int("value", 2)
            + named_string("text", "two", wide=False)
            + internal_reference("self")
            + internal_reference(),
        ),
        unnamed_struct(
            type_id(10),
            named_int("value", 3) + unnamed_null(),
        ),
        external_index("externalIndex", 5),
        external_index(None, -2),
        external_guid("externalGuid", bytes(range(32, 48))),
        external_string("externalString", "scene.unity"),
        primitive_array(4, 4, b"".join(i32(v) for v in (1, 2, 3, 4))),
        primitive_array(2, 8, f64(1.5) + f64(-2.5)),
        array(
            3,
            named_int("a", 1) + unnamed_int(2) + named_string("c", "three"),
        ),
        array(0, b""),
        named_bool("flag", False),
    ]
    return b"".join(entries)


ODIN_JSON = """{
  "$id": 0,
  "$type": "Game.SaveData, Assembly-CSharp",
  "gold": 120,
  "name": "Rin",
  "inventory": {
    "$id": 1,
    "$type": "Game.Inventory, Assembly-CSharp",
    "owner": {
      "$ref": 0
    },
    "items": [
      "sword",
      "shield",
      "potion"
    ]
  },
  "alive": true,
  "rate": 0.5
}
"""


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "player.odin").write_bytes(build_player())
    (OUT / "types.odin").write_bytes(build_types())
    (OUT / "odin-json.json").write_text(ODIN_JSON, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
