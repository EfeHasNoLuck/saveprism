#!/usr/bin/env python3
"""Writes synthetic .NET BinaryFormatter (MS-NRBF) save files.

Written from the public specification ([MS-NRBF]: .NET Remoting: Binary Format Data
Structure) and nothing else. No .NET runtime is used and nothing is executed: this script
only emits bytes. The class names, values and assemblies are invented.

    python fixtures/tools/nrbf/make_nrbf.py

writes
    fixtures/synthetic/nrbf/player.sav           Unity-like PlayerData with an int, a float, a
                                                 bool, a string, int[], List<string>,
                                                 Dictionary<string,int>, a nested class, shared
                                                 and single-use string references, nulls
    fixtures/synthetic/nrbf/player.sav.gz        the same stream gzip-wrapped
    fixtures/synthetic/nrbf/system-class.nrbf    a SystemClassWithMembersAndTypes-only stream
    fixtures/synthetic/nrbf/class-with-id.nrbf   ClassWithId reuse and MemberPrimitiveTyped
    fixtures/synthetic/nrbf/jagged.nrbf          a jagged BinaryArray of int[]
    fixtures/synthetic/nrbf/forward.nrbf         a MemberReference to an object defined later
    fixtures/synthetic/nrbf/rags.rsv             a RAGS-like save with invented classes
"""

import gzip
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(ROOT, "synthetic", "nrbf")

# RecordTypeEnum
HEADER = 0
CLASS_WITH_ID = 1
SYSTEM_CLASS_WITH_MEMBERS = 2
CLASS_WITH_MEMBERS = 3
SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES = 4
CLASS_WITH_MEMBERS_AND_TYPES = 5
BINARY_OBJECT_STRING = 6
BINARY_ARRAY = 7
MEMBER_PRIMITIVE_TYPED = 8
MEMBER_REFERENCE = 9
OBJECT_NULL = 10
MESSAGE_END = 11
BINARY_LIBRARY = 12
OBJECT_NULL_MULTIPLE_256 = 13
OBJECT_NULL_MULTIPLE = 14
ARRAY_SINGLE_PRIMITIVE = 15
ARRAY_SINGLE_OBJECT = 16
ARRAY_SINGLE_STRING = 17

# BinaryTypeEnum
T_PRIMITIVE = 0
T_STRING = 1
T_OBJECT = 2
T_SYSTEM_CLASS = 3
T_CLASS = 4
T_OBJECT_ARRAY = 5
T_STRING_ARRAY = 6
T_PRIMITIVE_ARRAY = 7

# PrimitiveTypeEnum
P_BOOLEAN = 1
P_BYTE = 2
P_CHAR = 3
P_DOUBLE = 6
P_INT16 = 7
P_INT32 = 8
P_INT64 = 9
P_SBYTE = 10
P_SINGLE = 11
P_TIMESPAN = 12
P_DATETIME = 13
P_UINT16 = 14
P_UINT32 = 15
P_UINT64 = 16
P_NULL = 17
P_STRING = 18


def i32(value):
    return struct.pack("<i", value)


def u32(value):
    return struct.pack("<I", value)


def i16(value):
    return struct.pack("<h", value)


def u16(value):
    return struct.pack("<H", value)


def i64(value):
    return struct.pack("<q", value)


def u64(value):
    return struct.pack("<Q", value)


def f32(value):
    return struct.pack("<f", value)


def f64(value):
    return struct.pack("<d", value)


def lp_bytes(data):
    out = bytearray()
    length = len(data)
    while length >= 0x80:
        out.append((length & 0x7F) | 0x80)
        length >>= 7
    out.append(length)
    out.extend(data)
    return bytes(out)


def lp(text):
    return lp_bytes(text.encode("utf-8"))


def header(root_id):
    return bytes([HEADER]) + i32(root_id) + i32(0) + i32(1) + i32(0)


def m_primitive(ptype):
    return (T_PRIMITIVE, bytes([ptype]))


def m_string():
    return (T_STRING, b"")


def m_object():
    return (T_OBJECT, b"")


def m_system_class(name):
    return (T_SYSTEM_CLASS, lp(name))


def m_class(name, library_id):
    return (T_CLASS, lp(name) + i32(library_id))


def m_object_array():
    return (T_OBJECT_ARRAY, b"")


def m_string_array():
    return (T_STRING_ARRAY, b"")


def m_primitive_array(ptype):
    return (T_PRIMITIVE_ARRAY, bytes([ptype]))


def class_record(record, object_id, name, members, values, library_id=None):
    """`members` is a list of (name, type-info) pairs; `values` is the already encoded tail."""
    out = bytearray([record])
    out += i32(object_id)
    out += lp(name)
    out += i32(len(members))
    for member_name, _ in members:
        out += lp(member_name)
    if record in (SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES, CLASS_WITH_MEMBERS_AND_TYPES):
        out += bytes(types[0] for types in (member[1] for member in members))
        for _, (_, extra) in members:
            out += extra
    if record in (CLASS_WITH_MEMBERS, CLASS_WITH_MEMBERS_AND_TYPES):
        out += i32(library_id if library_id is not None else 0)
    out += values
    return bytes(out)


def class_with_id(object_id, metadata_id, values):
    return bytes([CLASS_WITH_ID]) + i32(object_id) + i32(metadata_id) + values


def object_string(object_id, text):
    return bytes([BINARY_OBJECT_STRING]) + i32(object_id) + lp(text)


def member_reference(object_id):
    return bytes([MEMBER_REFERENCE]) + i32(object_id)


def object_null():
    return bytes([OBJECT_NULL])


def null_multiple_256(count):
    return bytes([OBJECT_NULL_MULTIPLE_256, count])


def null_multiple(count):
    return bytes([OBJECT_NULL_MULTIPLE]) + i32(count)


def primitive_value(ptype, value):
    if ptype == P_BOOLEAN:
        return bytes([1 if value else 0])
    if ptype == P_BYTE:
        return bytes([value])
    if ptype == P_SBYTE:
        return struct.pack("<b", value)
    if ptype == P_CHAR:
        return value.encode("utf-8")
    if ptype == P_INT16:
        return i16(value)
    if ptype == P_INT32:
        return i32(value)
    if ptype == P_INT64:
        return i64(value)
    if ptype == P_UINT16:
        return u16(value)
    if ptype == P_UINT32:
        return u32(value)
    if ptype == P_UINT64:
        return u64(value)
    if ptype == P_SINGLE:
        return f32(value)
    if ptype == P_DOUBLE:
        return f64(value)
    if ptype == P_TIMESPAN:
        return i64(value)
    if ptype == P_DATETIME:
        return u64(value)
    if ptype == P_STRING:
        return lp(value)
    if ptype == P_NULL:
        return b""
    raise ValueError("unsupported primitive type %d" % ptype)


def member_primitive_typed(ptype, value):
    return bytes([MEMBER_PRIMITIVE_TYPED, ptype]) + primitive_value(ptype, value)


def array_single_primitive(object_id, ptype, values):
    out = bytearray([ARRAY_SINGLE_PRIMITIVE])
    out += i32(object_id) + i32(len(values)) + bytes([ptype])
    for value in values:
        out += primitive_value(ptype, value)
    return bytes(out)


def array_single_records(record, object_id, values):
    out = bytearray([record]) + i32(object_id) + i32(len(values))
    for value in values:
        out += value
    return bytes(out)


def binary_array(object_id, array_type, lengths, binary_type, extra, elements, offset=False):
    out = bytearray([BINARY_ARRAY])
    out += i32(object_id) + bytes([array_type]) + i32(len(lengths))
    for length in lengths:
        out += i32(length)
    if offset:
        for _ in lengths:
            out += i32(0)
    out += bytes([binary_type]) + extra
    for element in elements:
        out += element
    return bytes(out)


def library(library_id, name):
    return bytes([BINARY_LIBRARY]) + i32(library_id) + lp(name)


class Builder:
    def __init__(self):
        self.body = bytearray()
        self.next_object = 10
        self.next_library = 1

    def object_id(self):
        value = self.next_object
        self.next_object += 1
        return value

    def add_library(self, name):
        value = self.next_library
        self.next_library += 1
        self.body += library(value, name)
        return value

    def write(self, name, root_record, root_id):
        data = header(root_id) + bytes(self.body) + root_record + bytes([MESSAGE_END])
        path = os.path.join(OUT, name)
        with open(path, "wb") as handle:
            handle.write(data)
        return data


def make_player():
    builder = Builder()
    unity = builder.add_library("Assembly-CSharp")
    mscorlib = builder.add_library("mscorlib")

    player_id = builder.object_id()

    values = bytearray()
    values += primitive_value(P_INT32, 1250)
    values += primitive_value(P_SINGLE, 87.5)
    values += primitive_value(P_BOOLEAN, True)
    values += primitive_value(P_INT32, 7)
    name_id = builder.object_id()
    values += object_string(name_id, "Ada")

    inventory_id = builder.object_id()
    values += array_single_primitive(inventory_id, P_INT32, [3, 5, 8, 13])

    items_id = builder.object_id()
    item_strings = []
    for text in ("sword", "shield", "potion"):
        item_id = builder.object_id()
        item_strings.append(object_string(item_id, text))
    values += array_single_records(ARRAY_SINGLE_STRING, items_id, item_strings)

    list_id = builder.object_id()
    list_items_id = builder.object_id()
    list_strings = []
    for text in ("alpha", "beta", "unused"):
        text_id = builder.object_id()
        list_strings.append(object_string(text_id, text))
    list_values = member_reference(list_items_id)
    list_values += primitive_value(P_INT32, 2)
    list_values += primitive_value(P_INT32, 3)
    list_members = [
        ("_items", m_string_array()),
        ("_size", m_primitive(P_INT32)),
        ("_version", m_primitive(P_INT32)),
    ]
    values += class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        list_id,
        "System.Collections.Generic.List`1[[System.String, mscorlib]]",
        list_members,
        list_values,
        mscorlib,
    )
    list_body = array_single_records(ARRAY_SINGLE_STRING, list_items_id, list_strings)

    dictionary_id = builder.object_id()
    pairs_id = builder.object_id()
    pair_records = []
    for key, value in (("arrows", 42), ("gems", 7)):
        pair_id = builder.object_id()
        key_id = builder.object_id()
        pair_values = object_string(key_id, key) + primitive_value(P_INT32, value)
        pair_records.append(
            class_record(
                CLASS_WITH_MEMBERS_AND_TYPES,
                pair_id,
                "System.Collections.Generic.KeyValuePair`2",
                [("key", m_string()), ("value", m_primitive(P_INT32))],
                pair_values,
                mscorlib,
            )
        )
    pairs = array_single_records(ARRAY_SINGLE_OBJECT, pairs_id, pair_records)
    dictionary_values = primitive_value(P_INT32, 2)
    dictionary_values += member_reference(pairs_id)
    dictionary_members = [
        ("Count", m_primitive(P_INT32)),
        ("KeyValuePairs", m_object_array()),
    ]
    values += class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        dictionary_id,
        "System.Collections.Generic.Dictionary`2[[System.String, mscorlib],[System.Int32, mscorlib]]",
        dictionary_members,
        dictionary_values,
        mscorlib,
    )
    pairs_body = pairs

    companion_id = builder.object_id()
    companion_name_id = builder.object_id()
    companion_values = primitive_value(P_INT32, 30) + object_string(companion_name_id, "Pip")
    values += class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        companion_id,
        "CompanionData",
        [("hp", m_primitive(P_INT32)), ("nickname", m_string())],
        companion_values,
        unity,
    )

    pet_name_id = builder.object_id()
    values += object_string(pet_name_id, "Mochi")
    values += member_reference(pet_name_id)

    title_id = builder.object_id()
    values += object_string(title_id, "Hero")
    values += member_reference(title_id)
    values += member_reference(title_id)

    # 2020-01-02 03:04:05 UTC in DateTime.ToBinary form (ticks with the UTC kind bit).
    values += primitive_value(P_DATETIME, 637134962450000000 | (1 << 62))
    values += primitive_value(P_INT64, 9000000000)
    values += primitive_value(P_CHAR, "A")
    values += null_multiple_256(3)

    player_members = [
        ("gold", m_primitive(P_INT32)),
        ("health", m_primitive(P_SINGLE)),
        ("alive", m_primitive(P_BOOLEAN)),
        ("level", m_primitive(P_INT32)),
        ("name", m_string()),
        ("inventory", m_primitive_array(P_INT32)),
        ("items", m_string_array()),
        ("list", m_object()),
        ("lookup", m_object()),
        ("companion", m_object()),
        ("petName", m_string()),
        ("petAlias", m_string()),
        ("title", m_string()),
        ("titleAlias", m_string()),
        ("titleAlias2", m_string()),
        ("created", m_primitive(P_DATETIME)),
        ("score", m_primitive(P_INT64)),
        ("initial", m_primitive(P_CHAR)),
        ("emptyA", m_object()),
        ("emptyB", m_object()),
        ("emptyC", m_object()),
    ]
    root = class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        player_id,
        "PlayerData",
        player_members,
        values,
        unity,
    )
    # Objects that the class referred to before they were written.
    builder.body += pairs_body
    builder.body += list_body
    return builder.write("player.sav", root, player_id)


def make_system_class():
    builder = Builder()
    object_id = builder.object_id()
    values = object_string(builder.object_id(), "an invented system class")
    values += object_null() + object_null()
    root = class_record(
        SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES,
        object_id,
        "System.Exception",
        [("_message", m_string()), ("_data", m_object()), ("_inner", m_object())],
        values,
    )
    return builder.write("system-class.nrbf", root, object_id)


def make_class_with_id():
    builder = Builder()
    game = builder.add_library("GameLogic")
    holder_id = builder.object_id()

    point_id = builder.object_id()
    point = class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        point_id,
        "Point",
        [("x", m_primitive(P_INT32)), ("y", m_primitive(P_INT32))],
        primitive_value(P_INT32, 4) + primitive_value(P_INT32, 9),
        game,
    )
    reuse_id = builder.object_id()
    reuse = class_with_id(
        reuse_id,
        point_id,
        primitive_value(P_INT32, 1) + primitive_value(P_INT32, 2),
    )

    values = point + reuse
    values += member_primitive_typed(P_INT32, 42)
    values += member_primitive_typed(P_STRING, "vip")
    values += member_primitive_typed(P_BOOLEAN, True)
    root = class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        holder_id,
        "Holder",
        [
            ("first", m_class("Point", game)),
            ("second", m_class("Point", game)),
            ("bonus", m_object()),
            ("label", m_object()),
            ("active", m_object()),
        ],
        values,
        game,
    )
    return builder.write("class-with-id.nrbf", root, holder_id)


def make_jagged():
    builder = Builder()
    outer_id = builder.object_id()
    first = array_single_primitive(builder.object_id(), P_INT32, [1, 2, 3])
    second = array_single_primitive(builder.object_id(), P_INT32, [4, 5])
    root = binary_array(
        outer_id,
        1,  # Jagged
        [2],
        T_PRIMITIVE_ARRAY,
        bytes([P_INT32]),
        [first, second],
    )
    return builder.write("jagged.nrbf", root, outer_id)


def make_forward():
    builder = Builder()
    game = builder.add_library("Assembly-CSharp")
    root_id = builder.object_id()
    buddy_id = builder.object_id()
    values = primitive_value(P_INT32, 1)
    values += member_reference(buddy_id)
    root = class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        root_id,
        "SaveState",
        [("slot", m_primitive(P_INT32)), ("buddy", m_object())],
        values,
        game,
    )
    buddy = class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        buddy_id,
        "BuddyData",
        [("hp", m_primitive(P_INT32))],
        primitive_value(P_INT32, 77),
        game,
    )
    return builder.write("forward.nrbf", root + buddy, root_id)


def make_rags():
    builder = Builder()
    rags = builder.add_library("RAGS")
    root_id = builder.object_id()
    world_id = builder.object_id()
    player_id = builder.object_id()
    values = primitive_value(P_INT32, 3)
    values += object_string(player_id, "Wanderer")
    values += primitive_value(P_BOOLEAN, False)
    world_values = primitive_value(P_INT32, 20260101)
    world_values += object_string(builder.object_id(), "Ashen Vale")
    values += class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        world_id,
        "RagsWorld",
        [("seed", m_primitive(P_INT32)), ("name", m_string())],
        world_values,
        rags,
    )
    root = class_record(
        CLASS_WITH_MEMBERS_AND_TYPES,
        root_id,
        "RagsSaveData",
        [
            ("version", m_primitive(P_INT32)),
            ("player", m_string()),
            ("hardcore", m_primitive(P_BOOLEAN)),
            ("world", m_object()),
        ],
        values,
        rags,
    )
    return builder.write("rags.rsv", root, root_id)


def main():
    os.makedirs(OUT, exist_ok=True)
    player = make_player()
    with open(os.path.join(OUT, "player.sav.gz"), "wb") as handle:
        handle.write(gzip.compress(player, mtime=0))
    make_system_class()
    make_class_with_id()
    make_jagged()
    make_forward()
    make_rags()


if __name__ == "__main__":
    main()
