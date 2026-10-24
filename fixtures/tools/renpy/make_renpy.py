#!/usr/bin/env python3
"""Writes the synthetic Ren'Py saves and persistent files under fixtures/synthetic/renpy/.

Python 3 standard library only. All data is invented; no Ren'Py code is used. The game classes are
small stand-ins whose module and class names are the ones Ren'Py uses, so `pickle` writes the same
class references a game would. The Ren'Py 7 (Python 2) save is written by a tiny writer of what
Python 2.7's cPickle writes with protocol 2, since Python 3 cannot write Python 2 strings.

    python fixtures/tools/renpy/make_renpy.py
"""

import io
import json
import pickle
import struct
import sys
import types
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "renpy"
DATE = (2026, 10, 24, 12, 0, 0)


# ----------------------------------------------------------------------------------------------
# Stand-in classes with Ren'Py's names


def module(name):
    mod = types.ModuleType(name)
    sys.modules[name] = mod
    parent, _, child = name.rpartition(".")
    if parent:
        setattr(sys.modules[parent], child, mod)
    return mod


def place(cls, module_name, name):
    cls.__module__ = module_name
    cls.__qualname__ = name
    cls.__name__ = name
    setattr(sys.modules[module_name], name, cls)
    return cls


for name in ["renpy", "renpy.revertable", "renpy.rollback", "renpy.persistent",
             "renpy.preferences", "renpy.python", "store"]:
    module(name)


class RevertableDict(dict):
    pass


class RevertableList(list):
    pass


class RevertableSet(set):
    """Pickled the way Ren'Py's RevertableSet is: NEWOBJ, then the state ({member: True},)."""

    def __getstate__(self):
        return ({member: True for member in sorted(self)},)

    def __setstate__(self, state):
        self.update(state[0].keys())

    __reduce__ = object.__reduce__
    __reduce_ex__ = object.__reduce_ex__


class RevertableObject(object):
    pass


class RollbackLog(RevertableObject):
    pass


class Rollback(RevertableObject):
    pass


class Persistent(object):
    pass


class Preferences(object):
    pass


class Inventory(RevertableObject):
    pass


place(RevertableDict, "renpy.revertable", "RevertableDict")
place(RevertableList, "renpy.revertable", "RevertableList")
place(RevertableSet, "renpy.revertable", "RevertableSet")
place(RevertableObject, "renpy.revertable", "RevertableObject")
place(RollbackLog, "renpy.rollback", "RollbackLog")
place(Rollback, "renpy.rollback", "Rollback")
place(Persistent, "renpy.persistent", "Persistent")
place(Preferences, "renpy.preferences", "Preferences")
place(Inventory, "store", "Inventory")


def make(cls, **state):
    obj = cls.__new__(cls)
    obj.__dict__.update(state)
    return obj


# ----------------------------------------------------------------------------------------------
# Game state


def game_state(extra_roots=None):
    """`(roots, log)` as Ren'Py's `freeze` returns them, with invented store variables."""
    shared_name = "".join(["Eil", "een"])  # one object, stored under two names
    inventory = make(Inventory, gold=12, items=RevertableList(["map", "lantern"]), owner=shared_name)
    roots = {
        "store.money": 120,
        "store.hero_name": "Sylvie",
        "store.met_eileen": True,
        "store.door_open": False,
        "store.last_choice": None,
        "store.karma": -7,
        "store.steps": 123456,
        "store.bank_balance": 2 ** 70,
        "store.huge_number": 3 ** 200,
        "store.motto": "Çok güzel — 日本語 \U0001F600",
        "store.stats": RevertableDict(luck=0.75, speed=1.5),
        "store.visited": RevertableList(["park", "cafe"]),
        "store.flags_seen": RevertableSet({"intro", "date1"}),
        "store.position": (3, 4),
        "store.player_name": shared_name,
        "store.companion_name": shared_name,
        "store.inventory": inventory,
        "store.backpack": inventory,
        "store.mystery.clue_count": 3,
        "store._last_say_who": "e",
        "store._window": False,
        "store.nvl_list": None,
        "store.save_name": "",
        "store.mouse_visible": True,
    }
    if extra_roots:
        roots.update(extra_roots)
    current = make(
        Rollback,
        context=None,
        stores={"store": {"money": 100}},
        objects=[],
        checkpoint=True,
        inventory_snapshot=inventory,
    )
    log = make(
        RollbackLog,
        log=RevertableList([current]),
        current=current,
        rollback_limit=50,
        forward=[],
        mutated={},
    )
    return roots, log


# ----------------------------------------------------------------------------------------------
# A tiny Python 2.7 cPickle (protocol 2) writer for the Ren'Py 7 save


class Py2Str(bytes):
    """A Python 2 `str` (byte string)."""


class Py2Long(int):
    """A Python 2 `long`."""


class Py2Obj:
    """An instance: NEWOBJ (new-style class) or OBJ (old-style class), then its state."""

    def __init__(self, module_name, class_name, state=None, items=None, entries=None, old=False):
        self.module = module_name
        self.name = class_name
        self.state = state
        self.items = items
        self.entries = entries
        self.old = old


class Py2Pickler:
    def __init__(self):
        self.out = bytearray(b"\x80\x02")
        self.memo = {}
        self.keep = []  # keeps memoized objects alive so their ids stay unique

    def put(self, key):
        index = len(self.memo)
        self.memo[key] = index
        self.out += b"q" + bytes([index]) if index < 256 else b"r" + struct.pack("<I", index)

    def get(self, key):
        index = self.memo[key]
        self.out += b"h" + bytes([index]) if index < 256 else b"j" + struct.pack("<I", index)

    def memoized(self, obj):
        self.keep.append(obj)
        return id(obj) in self.memo

    def save_global(self, module_name, name):
        key = ("global", module_name, name)
        if key in self.memo:
            self.get(key)
            return
        self.out += b"c" + module_name.encode() + b"\n" + name.encode() + b"\n"
        self.put(key)

    def batch_appends(self, items):
        if len(items) == 1:
            self.save(items[0])
            self.out += b"a"
        elif items:
            self.out += b"("
            for item in items:
                self.save(item)
            self.out += b"e"

    def batch_setitems(self, entries):
        if len(entries) == 1:
            key, value = entries[0]
            self.save(key)
            self.save(value)
            self.out += b"s"
        elif entries:
            self.out += b"("
            for key, value in entries:
                self.save(key)
                self.save(value)
            self.out += b"u"

    def save(self, obj):
        if obj is None:
            self.out += b"N"
        elif obj is True:
            self.out += b"\x88"
        elif obj is False:
            self.out += b"\x89"
        elif isinstance(obj, Py2Long):
            data = int(obj).to_bytes((int(obj).bit_length() >> 3) + 1, "little", signed=True)
            self.out += b"\x8a" + bytes([len(data)]) + data
        elif isinstance(obj, int):
            if 0 <= obj <= 0xFF:
                self.out += b"K" + bytes([obj])
            elif 0 <= obj <= 0xFFFF:
                self.out += b"M" + struct.pack("<H", obj)
            elif -2 ** 31 <= obj < 2 ** 31:
                self.out += b"J" + struct.pack("<i", obj)
            else:
                self.out += b"I" + str(obj).encode() + b"\n"
        elif isinstance(obj, float):
            self.out += b"G" + struct.pack(">d", obj)
        elif isinstance(obj, Py2Str):
            if self.memoized(obj):
                self.get(id(obj))
                return
            data = bytes(obj)
            if len(data) < 256:
                self.out += b"U" + bytes([len(data)]) + data
            else:
                self.out += b"T" + struct.pack("<i", len(data)) + data
            self.put(id(obj))
        elif isinstance(obj, str):
            if self.memoized(obj):
                self.get(id(obj))
                return
            data = obj.encode("utf-8")
            self.out += b"X" + struct.pack("<I", len(data)) + data
            self.put(id(obj))
        elif isinstance(obj, tuple):
            if not obj:
                self.out += b")"
                return
            if self.memoized(obj):
                self.get(id(obj))
                return
            if len(obj) <= 3:
                for item in obj:
                    self.save(item)
                self.out += bytes([0x84 + len(obj)])
            else:
                self.out += b"("
                for item in obj:
                    self.save(item)
                self.out += b"t"
            self.put(id(obj))
        elif isinstance(obj, list):
            if self.memoized(obj):
                self.get(id(obj))
                return
            self.out += b"]"
            self.put(id(obj))
            self.batch_appends(obj)
        elif isinstance(obj, dict):
            if self.memoized(obj):
                self.get(id(obj))
                return
            self.out += b"}"
            self.put(id(obj))
            self.batch_setitems(list(obj.items()))
        elif isinstance(obj, Py2Obj):
            if self.memoized(obj):
                self.get(id(obj))
                return
            if obj.old:
                self.out += b"("
                self.save_global(obj.module, obj.name)
                self.out += b"o"
            else:
                self.save_global(obj.module, obj.name)
                self.out += b")\x81"
            self.put(id(obj))
            if obj.items is not None:
                self.batch_appends(obj.items)
            if obj.entries is not None:
                self.batch_setitems(obj.entries)
            if obj.state is not None:
                self.save(obj.state)
                self.out += b"b"
        else:
            raise TypeError(type(obj))

    def dumps(self, obj):
        self.save(obj)
        self.out += b"."
        return bytes(self.out)


def py2_state():
    s = Py2Str
    shared = "Lucy's cat"
    stats = Py2Obj("renpy.python", "RevertableDict", entries=[(s(b"luck"), 0.5), (s(b"speed"), 2.25)])
    items = Py2Obj("renpy.python", "RevertableList", items=[s(b"rope"), "épée"])
    inventory = Py2Obj("store", "Inventory", state={s(b"gold"): 5, s(b"items"): items})
    legacy = Py2Obj("store", "OldStyleNote", state={s(b"text"): s(b"plain ascii")}, old=True)
    roots = Py2Obj("renpy.python", "RevertableDict", entries=[
        (s(b"store.money"), 250),
        (s(b"store.hero_name"), "Lucy"),
        (s(b"store.nickname"), s(b"Lu")),
        (s(b"store.met_cat"), True),
        (s(b"store.stats"), stats),
        (s(b"store.big_steps"), 2 ** 40),
        (s(b"store.bigger"), Py2Long(2 ** 80)),
        (s(b"store.pet_a"), shared),
        (s(b"store.pet_b"), shared),
        (s(b"store.inventory"), inventory),
        (s(b"store.note"), legacy),
        (s(b"store._rollback"), True),
    ])
    log = Py2Obj("renpy.python", "RollbackLog", state={
        s(b"log"): Py2Obj("renpy.python", "RevertableList", items=[]),
        s(b"rollback_limit"): 50,
        s(b"current_inventory"): inventory,
    })
    return (roots, log)


def check_py2(data):
    """Loads the Python 2 pickle with Python 3 (as Ren'Py 8 would) to check the writer."""
    module("renpy.python")
    for name, base in [("RevertableDict", dict), ("RevertableList", list),
                       ("RollbackLog", object)]:
        place(type(name, (base,), {}), "renpy.python", name)
    place(type("OldStyleNote", (object,), {}), "store", "OldStyleNote")
    roots, log = pickle.loads(data, encoding="utf-8")
    assert roots["store.money"] == 250
    assert roots["store.hero_name"] == "Lucy"
    assert roots["store.big_steps"] == 2 ** 40
    assert roots["store.bigger"] == 2 ** 80
    assert roots["store.stats"]["speed"] == 2.25
    assert roots["store.pet_a"] is roots["store.pet_b"]
    assert log.current_inventory is roots["store.inventory"]
    assert roots["store.note"].text == "plain ascii"


# ----------------------------------------------------------------------------------------------
# Files


def tiny_png(width=4, height=3):
    rows = b"".join(
        b"\x00" + b"".join(bytes([40 * y, 90, 200 - 30 * x]) for x in range(width))
        for y in range(height)
    )

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows))
            + chunk(b"IEND", b""))


def write_save(path, log, version, signatures=None, name="Chapter 1"):
    meta = {"_save_name": name, "_renpy_version": [int(p) for p in version.split(".")[:3]],
            "_version": "1.0", "_ctime": 1793000000.0, "_game_runtime": 754.25}
    entries = [
        ("screenshot.png", tiny_png()),
        ("extra_info", name.encode("utf-8")),
        ("json", json.dumps(meta).encode("utf-8")),
        ("renpy_version", version.encode("utf-8")),
        ("log", log),
    ]
    if signatures is not None:
        entries.append(("signatures", signatures.encode("utf-8")))
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        for entry_name, data in entries:
            info = zipfile.ZipInfo(entry_name, date_time=DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 0
            info.external_attr = 0o600 << 16
            archive.writestr(info, data)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(buffer.getvalue())


SIGNATURE = ("signature SW52ZW50ZWQtcHVibGljLWtleS1mb3ItdGVzdHM= "
             "SW52ZW50ZWQtc2lnbmF0dXJlLWJ5dGVzLW5vdC1yZWFsLWF0LWFsbA==\n")


def persistent_bytes(signed):
    prefs = make(Preferences, fullscreen=False, text_cps=40, volumes={"music": 0.8, "sfx": 1.0})
    persistent = make(
        Persistent,
        _seen_ever={"start": True, "chapter1": True},
        _changed={"_seen_ever": 12.5},
        _preferences=prefs,
        ending_good=True,
        play_count=4,
        best_time=123.5,
        player_title="Captain",
    )
    data = zlib.compress(pickle.dumps(persistent, protocol=2), 3)
    if signed:
        data += SIGNATURE.encode("utf-8")
    return data


def main():
    write_save(OUT / "renpy8-0" / "1-1-LT1.save",
               pickle.dumps(game_state(), protocol=2), "8.0.3.22090809")
    write_save(OUT / "renpy8-1-signed" / "1-1-LT1.save",
               pickle.dumps(game_state(), protocol=2), "8.1.3.23091805", signatures=SIGNATURE)
    diary = RevertableList(["Diary entry %04d: nothing much happened." % i for i in range(3000)])
    proto5 = game_state({"store.diary": diary, "store.blob": b"\x00\x01\x02",
                         "store.lucky_numbers": {7, 13, 42}})
    write_save(OUT / "renpy8-3" / "1-1-LT1.save", pickle.dumps(proto5, protocol=5),
               "8.3.4.24120703")
    py2 = Py2Pickler().dumps(py2_state())
    check_py2(py2)
    write_save(OUT / "renpy7" / "1-1-LT1.save", py2, "7.3.5.606")
    for folder, signed in [("persistent", False), ("persistent-signed", True)]:
        path = OUT / folder / "persistent"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(persistent_bytes(signed))
    print("wrote", OUT)


if __name__ == "__main__":
    main()
