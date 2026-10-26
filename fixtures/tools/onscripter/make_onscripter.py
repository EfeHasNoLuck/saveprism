#!/usr/bin/env python3
"""Writes the synthetic NScripter / ONScripter fixtures.

The layout follows the behavioral spec in `crates/saveprism-codecs/src/onscripter/mod.rs`, which
was written from the public ONScripter (GPL-2.0) sources. Only behavior was used; this generator
and the codec are independent implementations. Every value is invented.
"""

import struct
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "onscripter"
SAVE_MAGIC = b"ONS"


def i32(value):
    return struct.pack("<i", value)


def variable(number, text):
    return i32(number) + text.encode("shift_jis") + b"\x00"


def write_global(path, values):
    out = bytearray()
    for number, text in values:
        out += variable(number, text)
    path.write_bytes(bytes(out))


def write_slot(path, version, body, description):
    out = SAVE_MAGIC + bytes(version) + body
    if description is not None:
        out += b'"' + description.encode("shift_jis") + b'"*'
    path.write_bytes(out)


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    write_global(
        OUT / "global.sav",
        [
            (1234, "invented"),
            (-7, "放課後の教室"),
            (0, ""),
            (999999, "ラスト"),
            (12, "にほんご"),
        ],
    )
    write_global(
        OUT / "gloval.sav",
        [
            (55, "alt spelling"),
            (0, ""),
            (-1, "minus"),
        ],
    )

    # A save slot body: the engine's own int32 marker and a short invented body. Only the trailing
    # description is editable; the body is kept as raw bytes.
    body = i32(2) + i32(0) + i32(1) + i32(0) + bytes(range(0, 64))
    write_slot(OUT / "save1.dat", (2, 8), body, "放課後 1")
    write_slot(OUT / "save10.dat", (2, 5), body, "autosave")


if __name__ == "__main__":
    main()
