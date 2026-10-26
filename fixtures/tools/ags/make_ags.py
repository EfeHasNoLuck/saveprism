#!/usr/bin/env python3
"""Writes the synthetic Adventure Game Studio fixtures.

The layout follows the behavioral spec in `crates/saveprism-codecs/src/ags/mod.rs`, which was
written from the public Artistic-2.0 AGS engine sources. No game data is used; every value is
invented. AGS strings are an int32 byte length followed by UTF-8 bytes.
"""

import struct
import zlib
from pathlib import Path

SIGNATURE = b"Adventure Game Studio saved game v2"
OUT = Path(__file__).resolve().parents[2] / "synthetic" / "ags"


def u32(value):
    return struct.pack("<I", value)


def i32(value):
    return struct.pack("<i", value)


def string(text):
    data = text.encode("utf-8")
    return i32(len(data)) + data


def tag(name, closing=False):
    if closing:
        return b"</" + name.encode() + b">"
    return b"<" + name.encode() + b">"


def component(name, version, data, compress):
    if compress:
        payload = zlib.compress(data, 9)
        flags = 1
        uncompressed = len(data)
    else:
        payload = data
        flags = 0
        uncompressed = len(data)
    header = u32(24) + u32(flags | 0) + i32(version) + u32(len(payload)) + u32(uncompressed) + u32(0)
    return tag(name) + header + payload + tag(name, True)


def script_modules(globals_data, modules):
    out = i32(len(globals_data)) + globals_data
    out += i32(len(modules))
    for name, data in modules:
        out += string(name)
        out += i32(len(data)) + data
    return out


def game_data():
    globals_data = b"".join(i32(value) for value in (7, -3, 1200, 0, 42))
    out = tag("Components")
    out += component("Game State", 1, b"invented game state bytes", False)
    out += component("Script Modules", 3060200, script_modules(globals_data, [("room", b"\x01\x02\x03\x04")]), True)
    out += component("Plugin Data", 1, b"\x00\x00\x00\x00", True)
    out += tag("Components", True)
    return out


def environment(engine_name, engine_version, guid, title, data_file, data_version, depth, legacy_id):
    body = string(engine_name) + string(engine_version) + string(guid)
    body += string(title) + string(data_file)
    body += i32(data_version) + i32(depth) + i32(legacy_id)
    return u32(4 + len(body)) + body


def screenshot(width, height, depth):
    pixels = bytes((index * 7) & 0xFF for index in range(width * height * ((depth + 7) // 8)))
    compressed = zlib.compress(pixels, 9)
    return i32(width) + i32(height) + i32(depth) + u32(0) + u32(len(compressed)) + compressed


def write_modern():
    env = environment(
        "Adventure Game Studio",
        "3.6.3.10",
        "{4c1e5b62-9f30-4a44-9a3e-2c0f5f4d21aa}",
        "Invented AGS Game",
        "game.ags",
        37,
        32,
        123456789,
    )
    description = "Chapter one: the invented lighthouse"
    image = screenshot(6, 4, 32)
    user = string(description) + u32(1 | 2) + image
    data = game_data()

    out = SIGNATURE + i32(3060300)
    table_pos = len(out)
    out += b"\x00" * 20
    env_pos = len(out)
    out += env
    user_pos = len(out)
    out += user
    game_pos = len(out)
    out += data
    table = u32(env_pos - table_pos) + u32(0) + u32(env_pos) + u32(user_pos) + u32(game_pos)
    out = out[:table_pos] + table + out[table_pos + 20 :]
    (OUT / "agssave.000").write_bytes(out)


def write_older():
    env = environment(
        "Adventure Game Studio",
        "3.5.1.9",
        "{0a0b0c0d-1111-2222-3333-444455556666}",
        "Invented AGS Game 3.5",
        "game.ags",
        25,
        16,
        42,
    )
    user = string("Autosave") + u32(0)
    data = game_data()
    out = SIGNATURE + i32(13) + env + user + data
    (OUT / "agssave.999").write_bytes(out)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    write_modern()
    write_older()


if __name__ == "__main__":
    main()
