#!/usr/bin/env python3
"""Generate invented KiriKiri / KAG TJS2 save fixtures.

The text is a TJS2 `saveStruct` expression (dictionary or array), written the way KiriKiri Z does
(public BSD-style source, `tjsDictionary.cpp` / `tjsArray.cpp`). The wrappers follow
`base/TextStream.cpp`: plain UTF-16LE with a BOM, the FE FE mode-1 and mode-0 scramble, and the
FE FE mode-2 zlib stream with two little-endian u64 lengths. All values are invented.
"""

from pathlib import Path
import struct
import zlib

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "kirikiri"

SAVE = """(const) %[
 "name" => "Mira",
 "gold" => 120,
 "rate" => 0x1.8000000000000p+1 /* 3 */,
 "chapter" => 4,
 "items" => (const) [
  "sword",
  "shield",
  7
 ],
 "flags" => (const) %[
  "met_guard" => 1,
  "seen_intro" => 0
 ],
 "empty" => void,
 "nick" => null,
 "blob" => <% DE AD BE EF %>
]"""

SYSTEM = """(const) [
 (const) %[
  "volume" => 0x1.0000000000000p-1 /* 0.5 */,
  "skip" => 0
 ],
 "system data"
]"""


def utf16(text: str) -> bytes:
    return text.encode("utf-16-le")


def mode0_unit(unit: int) -> int:
    if unit >= 0x20:
        return (unit ^ (((unit & 0xFE) << 8) ^ 1)) & 0xFFFF
    return unit


def mode1_unit(unit: int) -> int:
    return ((unit & 0xAAAA) >> 1) | ((unit & 0x5555) << 1)


def scramble(text: str, transform) -> bytes:
    body = utf16(text)
    units = struct.unpack(f"<{len(body) // 2}H", body)
    return b"".join(struct.pack("<H", transform(unit)) for unit in units)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, text in [("save01.ksd", SAVE), ("system.kdt", SYSTEM)]:
        (OUT / name).write_bytes(b"\xFF\xFE" + utf16(text))
    (OUT / "scrambled.ksd").write_bytes(b"\xFE\xFE\x01\xFF\xFE" + scramble(SAVE, mode1_unit))
    (OUT / "scrambled-mode0.ksd").write_bytes(
        b"\xFE\xFE\x00\xFF\xFE" + scramble(SAVE, mode0_unit)
    )
    body = utf16(SAVE)
    compressed = zlib.compress(body, 6)
    (OUT / "compressed.ksd").write_bytes(
        b"\xFE\xFE\x02\xFF\xFE"
        + struct.pack("<QQ", len(compressed), len(body))
        + compressed
    )


if __name__ == "__main__":
    main()
