#!/usr/bin/env python3
"""Generate an invented Satisfactory 1.x save byte by byte.

The layout follows the public Satisfactory Wiki "Save files" page and the MIT-licensed
etothepii4/satisfactory-file-parser behaviour for the 1.x body (save header 14, save version 52,
build 424353): the uncompressed header, the zlib chunk list (49 byte version-2 chunk headers),
the body size + UE5 padding, the SaveBodyValidation grids, the level count, a TOC/data blob pair
per level and Unreal tagged properties. No real save bytes are copied.

Run from anywhere:  python fixtures/tools/satisfactory/make_satisfactory.py
"""

from pathlib import Path
import struct
import zlib

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "satisfactory"
SAVE_VERSION = 52
BUILD_VERSION = 424353
CHUNK_SIZE = 131072


def i32(value: int) -> bytes:
    return struct.pack("<i", value)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def f32(value: float) -> bytes:
    return struct.pack("<f", value)


def f64(value: float) -> bytes:
    return struct.pack("<d", value)


def string(value: str) -> bytes:
    encoded = value.encode("utf-8")
    return i32(len(encoded) + 1) + encoded + b"\x00"


def guid() -> bytes:
    return bytes(16)


def tag(name: str, kind: str, payload: bytes, *, subtype: str = "", enum: str = "",
        bool_value: bool = False, map_types: tuple[str, str] = ("", "")) -> bytes:
    head = string(name) + string(kind) + i32(len(payload)) + i32(0)
    if kind == "ArrayProperty":
        head += string(subtype)
    elif kind == "StructProperty":
        head += string(subtype) + guid()
    elif kind == "SetProperty":
        head += string(subtype)
    elif kind == "BoolProperty":
        head += bytes([1 if bool_value else 0])
        payload = b""
    elif kind == "ByteProperty":
        head += string(enum)
    elif kind == "EnumProperty":
        head += string("InventedEnum")
    elif kind == "MapProperty":
        head += string(map_types[0]) + string(map_types[1])
    head += bytes([0])
    return head + payload


def properties(*items: bytes) -> bytes:
    return b"".join(items) + string("None")


def object_reference(level: str, path: str) -> bytes:
    return string(level) + string(path)


def inventory_item(name: str) -> bytes:
    # FInventoryItem: ObjectReference + FGDynamicStruct (no valid struct).
    return object_reference("Persistent_Level", name) + i32(0)


def inventory_stack(count: int) -> bytes:
    return properties(
        tag(
            "Item",
            "StructProperty",
            inventory_item("Persistent_Level:PersistentLevel.InventedItem"),
            subtype="InventoryItem",
        ),
        tag("NumItems", "IntProperty", i32(count)),
    )


def actor_payload(properties_bytes: bytes) -> bytes:
    parent = object_reference("Persistent_Level", "Persistent_Level:PersistentLevel.InventedActor")
    return parent + i32(0) + properties_bytes + i32(0)


def actor_header(type_path: str, instance: str) -> bytes:
    return (
        u32(1)
        + string(type_path)
        + string("Persistent_Level")
        + string(instance)
        + u32(0)
        + u32(1)
        + f32(0) * 4
        + f32(0) * 3
        + f32(1) * 3
        + u32(1)
    )


def blob(contents: bytes) -> bytes:
    return i32(len(contents)) + u32(0) + contents


def level(name: str | None, headers: list[bytes], objects: list[bytes], persistent: bool) -> bytes:
    toc = u32(len(headers)) + b"".join(headers)
    data = u32(len(objects))
    for payload in objects:
        data += i32(SAVE_VERSION) + u32(0) + u32(len(payload)) + payload
    out = b""
    if name is not None:
        out += string(name)
    out += blob(toc)
    out += blob(data)
    if persistent:
        out += i32(0)  # destroyed actors map
    else:
        out += i32(SAVE_VERSION)  # per-streamable-level custom version
        out += i32(0)  # collectables
    return out


def body() -> bytes:
    # SaveBodyValidation: one grid with no children.
    grids = i32(1) + string("MainGrid") + i32(12800) + u32(7) + u32(0)

    sub_headers = [actor_header("/Script/FactoryGame.InventedSubActor", "SubLevel:SubLevel.InventedSubActor")]
    sub_objects = [actor_payload(properties(tag("mResourcesLeft", "IntProperty", i32(11))))]

    main_props = properties(
        tag("mHealth", "FloatProperty", f32(100.0)),
        tag("mGold", "IntProperty", i32(500)),
        tag("mBig", "Int64Property", struct.pack("<q", 9_000_000_000)),
        tag("mRatio", "DoubleProperty", f64(0.5)),
        tag("mFlag", "BoolProperty", b"", bool_value=True),
        tag("mTiny", "ByteProperty", bytes([7]), enum="None"),
        tag("mName", "StrProperty", string("Invented")),
        tag(
            "mInventoryStacks",
            "ArrayProperty",
            u32(2)
            + tag(
                "mInventoryStacks",
                "StructProperty",
                inventory_stack(3) + inventory_stack(42),
                subtype="InventoryStack",
            ),
            subtype="StructProperty",
        ),
    )
    main_headers = [
        actor_header(
            "/Game/FactoryGame/Invented/InventedActor.InventedActor_C",
            "Persistent_Level:PersistentLevel.InventedActor",
        )
    ]
    main_objects = [actor_payload(main_props)]

    rest = grids
    rest += i32(1)  # one sublevel
    rest += level("SubLevel", sub_headers, sub_objects, persistent=False)
    rest += level(None, main_headers, main_objects, persistent=True)
    rest += i32(0)  # unresolved world data
    return i32(len(rest)) + u32(0) + rest


def header() -> bytes:
    out = u32(14) + u32(SAVE_VERSION) + u32(BUILD_VERSION)
    out += string("Invented Save")
    out += string("Persistent_Level")
    out += string("?skiponboarding?sessionName=Invented")
    out += string("Invented")
    out += u32(3600)
    out += u64(638_888_000_000_000_000)
    out += bytes([0])
    out += u32(40)
    out += string("")
    out += u32(0)
    out += string("invented-save-id")
    out += u32(1)
    out += u32(1) + guid()
    out += u32(0)
    return out


def chunk_header(compressed: int, uncompressed: int) -> bytes:
    out = bytearray(49)
    struct.pack_into("<I", out, 0, 0x9E2A83C1)
    struct.pack_into("<I", out, 4, 0x22222222)
    struct.pack_into("<I", out, 8, CHUNK_SIZE)
    out[16] = 3
    struct.pack_into("<Q", out, 17, compressed)
    struct.pack_into("<Q", out, 25, uncompressed)
    struct.pack_into("<Q", out, 33, compressed)
    struct.pack_into("<Q", out, 41, uncompressed)
    return bytes(out)


def build() -> bytes:
    payload = body()
    out = header()
    for start in range(0, len(payload), CHUNK_SIZE):
        raw = payload[start:start + CHUNK_SIZE]
        compressed = zlib.compress(raw)
        out += chunk_header(len(compressed), len(raw)) + compressed
    return out


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / "invented.sav"
    target.write_bytes(build())
    print(target)
