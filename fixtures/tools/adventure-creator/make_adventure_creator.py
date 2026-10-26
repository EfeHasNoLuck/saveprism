#!/usr/bin/env python3
"""Generate invented Adventure Creator (Unity) save files.

The layout follows the public AC scripting guide (behaviour only):
- `SaveSystem` joins the serialized main data and the serialized scene data with `||` and
  lists `*DOUBLEPIPE*` as the label replacement, so a real label never contains the divider.
- `FileFormatHandler_Json` joins per-scene records with `|ROOMDELIMITER|`.
- `FileFormatHandler_Xml` serializes the same objects as XML.
- `FileFormatHandler_Binary` serializes .NET BinaryFormatter (MS-NRBF) data and hands it
  around as a Base64 string.

The class names and values are invented. Nothing is executed and no game files are used.

    python fixtures/tools/adventure-creator/make_adventure_creator.py

writes
    fixtures/synthetic/adventure-creator/village.json.save       main || scene |ROOMDELIMITER| scene
    fixtures/synthetic/adventure-creator/village.xml.save        main || scene list
    fixtures/synthetic/adventure-creator/village.binary.save     Base64(BinaryFormatter) || Base64(BinaryFormatter)
"""

import base64
import json
import struct
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "adventure-creator"

# MS-NRBF record types (public .NET Remoting: Binary Format Data Structure).
HEADER = 0
CLASS_WITH_ID = 1
SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES = 4
BINARY_OBJECT_STRING = 6
BINARY_ARRAY = 7
MEMBER_PRIMITIVE_TYPED = 8
MESSAGE_END = 11
BINARY_LIBRARY = 12
ARRAY_SINGLE_PRIMITIVE = 15

T_PRIMITIVE = 0
T_STRING = 1
T_PRIMITIVE_ARRAY = 7

P_BOOLEAN = 1
P_INT32 = 8
P_SINGLE = 11


def i32(value):
    return struct.pack("<i", value)


def lp(text):
    raw = text.encode("utf-8")
    out = bytearray()
    length = len(raw)
    while length >= 0x80:
        out.append((length & 0x7F) | 0x80)
        length >>= 7
    out.append(length)
    out.extend(raw)
    return bytes(out)


def header(root_id):
    return bytes([HEADER]) + i32(root_id) + i32(0) + i32(1) + i32(0)


def primitive(ptype, value):
    if ptype == P_BOOLEAN:
        return bytes([1 if value else 0])
    if ptype == P_INT32:
        return i32(value)
    if ptype == P_SINGLE:
        return struct.pack("<f", value)
    raise ValueError(ptype)


class Builder:
    def __init__(self):
        self.body = bytearray()
        self.next_object = 10

    def object_id(self):
        value = self.next_object
        self.next_object += 1
        return value

    def string(self, text):
        object_id = self.object_id()
        return BINARY_OBJECT_STRING, object_id, bytes([BINARY_OBJECT_STRING]) + i32(object_id) + lp(text)

    def class_record(self, object_id, name, members):
        """`members` is a list of (name, type byte, extra, encoded value or None)."""
        out = bytearray([SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES])
        out += i32(object_id)
        out += lp(name)
        out += i32(len(members))
        for member_name, _, _, _ in members:
            out += lp(member_name)
        out += bytes(kind for _, kind, _, _ in members)
        for _, kind, extra, _ in members:
            if kind == T_STRING:
                out += b""
            elif kind == T_PRIMITIVE:
                out += bytes([extra])
            elif kind == T_PRIMITIVE_ARRAY:
                out += bytes([extra])
            else:
                raise ValueError(kind)
        for _, _, _, value in members:
            out += value
        return i32(object_id), bytes(out)

    def finish(self, records, root_id):
        return header(root_id) + bytes(self.body) + b"".join(records) + bytes([MESSAGE_END])


def object_string(object_id, text):
    return bytes([BINARY_OBJECT_STRING]) + i32(object_id) + lp(text)


def save_data_stream():
    """A SaveData-like object: player id, time scale, scene name and variable text."""
    builder = Builder()
    root_id = builder.object_id()
    name_id = builder.object_id()
    variables_id = builder.object_id()
    members = [
        ("currentPlayerID", T_PRIMITIVE, P_INT32, primitive(P_INT32, 0)),
        ("timeScale", T_PRIMITIVE, P_SINGLE, primitive(P_SINGLE, 1.0)),
        ("previousSceneName", T_STRING, 0, object_string(name_id, "Village")),
        ("runtimeVariablesData", T_STRING, 0, object_string(variables_id, "10000:1|10001:Mira")),
    ]
    _, record = builder.class_record(root_id, "SaveData", members)
    return builder.finish([record], root_id)


def scene_data_stream(name, scene_number, local_variables):
    builder = Builder()
    root_id = builder.object_id()
    name_id = builder.object_id()
    local_id = builder.object_id()
    members = [
        ("sceneNumber", T_PRIMITIVE, P_INT32, primitive(P_INT32, scene_number)),
        ("sceneName", T_STRING, 0, object_string(name_id, name)),
        ("localVariablesData", T_STRING, 0, object_string(local_id, local_variables)),
    ]
    _, record = builder.class_record(root_id, "SingleLevelData", members)
    return builder.finish([record], root_id)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for stale in OUT.glob("*.nrbf.tmp"):
        stale.unlink()

    main_data = {
        "mainData": {
            "currentPlayerID": 0,
            "timeScale": 1.0,
            "previousSceneIndex": 0,
            "previousSceneName": "Village",
            "runtimeVariablesData": "10000:1|10001:Mira",
        },
        "playerData": [{"playerID": 0, "sceneIndex": 0, "sceneName": "Village"}],
    }
    scene_one = {
        "sceneNumber": 0,
        "sceneName": "Village",
        "localVariablesData": "20000:1|20001:2",
    }
    scene_two = {
        "sceneNumber": 1,
        "sceneName": "Forest",
        "localVariablesData": "30000:0",
    }

    compact = lambda value: json.dumps(value, separators=(",", ":"), ensure_ascii=False)
    json_save = "||".join(
        [
            compact(main_data),
            compact(scene_one) + "|ROOMDELIMITER|" + compact(scene_two),
        ]
    )
    (OUT / "village.json.save").write_text(json_save, encoding="utf-8", newline="")

    def element(name, value):
        if isinstance(value, bool):
            return f"<{name}>{'true' if value else 'false'}</{name}>"
        return f"<{name}>{value}</{name}>"

    xml_main = (
        '<SaveData><mainData>'
        + element("currentPlayerID", 0)
        + element("timeScale", 1)
        + element("previousSceneIndex", 0)
        + element("previousSceneName", "Village")
        + element("runtimeVariablesData", "10000:1|10001:Mira")
        + "</mainData><playerData><playerID>0</playerID><sceneIndex>0</sceneIndex>"
        + element("sceneName", "Village")
        + "</playerData></SaveData>"
    )
    xml_scenes = (
        "<ArrayOfSingleLevelData>"
        + "".join(
            "<SingleLevelData>"
            + element("sceneNumber", scene["sceneNumber"])
            + element("sceneName", scene["sceneName"])
            + element("localVariablesData", scene["localVariablesData"])
            + "</SingleLevelData>"
            for scene in (scene_one, scene_two)
        )
        + "</ArrayOfSingleLevelData>"
    )
    (OUT / "village.xml.save").write_text(
        xml_main + "||" + xml_scenes, encoding="utf-8", newline=""
    )

    main_bytes = save_data_stream()
    scene_bytes = scene_data_stream("Village", 0, "20000:1|20001:2")
    binary_save = base64.b64encode(main_bytes).decode("ascii") + "||" + base64.b64encode(
        scene_bytes
    ).decode("ascii")
    (OUT / "village.binary.save").write_text(binary_save, encoding="utf-8", newline="")


if __name__ == "__main__":
    main()
