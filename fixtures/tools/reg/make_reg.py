#!/usr/bin/env python3
"""Writes synthetic Windows Registry export (`.reg`) fixtures.

Written from the public `.reg` format (header line, `[key]` sections, `"name"=value` data,
`dword:`, `hex:`, `hex(b):`, `hex(2):`, `hex(7):`, deletion `-`, `regedit` line wrapping).
Nothing is imported into the registry and no real game data is used.

    python fixtures/tools/reg/make_reg.py

writes
    fixtures/synthetic/reg/unity-playerprefs.reg  UTF-16LE BOM, CRLF, Unity PlayerPrefs values
    fixtures/synthetic/reg/regedit4-ansi.reg      REGEDIT4, ANSI (Windows-1252)
    fixtures/synthetic/reg/mixed-types.reg        REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ, binary,
                                                  a deletion line
"""

import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.normpath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(FIXTURES, "synthetic", "reg")

MAX_LINE = 80


def unity_hash(name):
    """The `_h` suffix Unity appends: modified djb2 with XOR."""
    value = 5381
    for byte in name.encode("utf-8"):
        value = ((value * 33) ^ byte) & 0xFFFFFFFF
    return value


def unity(name):
    return "%s_h%d" % (name, unity_hash(name))


def hex_cells(data):
    return ["%02x" % byte for byte in data]


def wrap_hex(prefix, data):
    """`prefix` plus comma-separated bytes wrapped like regedit (`,\\` + two spaces)."""
    cells = hex_cells(data)
    line = prefix
    width = len(prefix)
    out = []
    for index, cell in enumerate(cells):
        text = cell if index + 1 == len(cells) else cell + ","
        if width > 2 and width + len(text) + 1 > MAX_LINE:
            out.append(line + "\\")
            line = "  "
            width = 2
        line += text
        width += len(text)
    out.append(line)
    return "\r\n".join(out)


def cp1252(text):
    return text.encode("cp1252")


def utf16le_bytes(text):
    return text.encode("utf-16le")


def unity_playerprefs():
    key = "HKEY_CURRENT_USER\\Software\\Northwind Labs\\Prism Runner"
    levels = key + "\\Levels"
    lines = [
        "Windows Registry Editor Version 5.00",
        "",
        "[%s]" % key,
        '"%s"=dword:0000012c' % unity("HighScore"),
        '"%s"=dword:ffffffff' % unity("Lives"),
        '"%s"=%s' % (unity("Volume"), wrap_hex("hex(b):", struct.pack("<d", 0.75))),
        '"%s"=hex:%s' % (
            unity("PlayerName"),
            ",".join(hex_cells("Élodie".encode("utf-8") + b"\x00")),
        ),
        '"%s"=%s'
        % (
            unity("Motto"),
            wrap_hex(
                "hex:",
                (
                    "Grüße aus dem Prismenlauf! Jede Ebene öffnet ein neues Fenster, "
                    "und der letzte Sprung zählt doppelt so viel wie der erste."
                ).encode("utf-8")
                + b"\x00",
            ),
        ),
        "",
        "[%s]" % levels,
        '"%s"=dword:00000003' % unity("Unlocked"),
        '"%s"=%s' % (unity("BestTime"), wrap_hex("hex(b):", struct.pack("<d", 83.25))),
        "",
    ]
    return ("\r\n".join(lines)).encode("utf-16le")


def regedit4_ansi():
    lines = [
        "REGEDIT4",
        "",
        "; exported by a player, plain ANSI",
        "[HKEY_CURRENT_USER\\Software\\Legacy Tools\\AnsiGame]",
        '"Score"=dword:0000002a',
        '"Greeting"="Café"',
        "",
    ]
    return cp1252("\r\n".join(lines))


def mixed_types():
    key = "HKEY_CURRENT_USER\\Software\\Northwind Labs\\Prism Runner\\Settings"
    install = utf16le_bytes("%LOCALAPPDATA%\\Prism Runner") + b"\x00\x00"
    multi = utf16le_bytes("default") + b"\x00\x00" + utf16le_bytes("practice") + b"\x00\x00"
    lines = [
        "Windows Registry Editor Version 5.00",
        "",
        "[%s]" % key,
        '"DisplayName"="Prism Runner"',
        '"InstallPath"=%s' % wrap_hex("hex(2):", install),
        '"Profiles"=%s' % wrap_hex("hex(7):", multi),
        '"Seed"=hex:de,ad,be,ef',
        '"OldCache"=-',
        "",
    ]
    return ("\r\n".join(lines)).encode("utf-16le")


def main():
    os.makedirs(OUT, exist_ok=True)
    files = {
        "unity-playerprefs.reg": b"\xff\xfe" + unity_playerprefs(),
        "regedit4-ansi.reg": regedit4_ansi(),
        "mixed-types.reg": b"\xff\xfe" + mixed_types(),
    }
    for name, data in files.items():
        with open(os.path.join(OUT, name), "wb") as handle:
            handle.write(data)
    print("wrote %d files to %s" % (len(files), OUT))


if __name__ == "__main__":
    main()
