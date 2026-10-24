#!/usr/bin/env python3
"""Write the synthetic QSP save fixtures.

The bytes follow the save/load code of the open-source QSP core (QSPFoundation/qsp,
`qsp/game.c` and `qsp/coding.c`): a CRLF-separated text save whose first two lines are the raw
`QSPSAVEDGAME` signature and the raw engine version, and whose other lines are encoded by
subtracting 5 from every non-zero character (5 is written as -5). The modern encoding is UTF-16LE;
the legacy single-byte encoding is Windows-1251 (Python's `cp1251` codec).

Data is invented: a small adventure with a location, actions, objects, a playlist, included files
and game variables (numbers, strings, a boolean, a tuple and an array with index keys). No game
code or real player data is used.

Usage: python fixtures/tools/qsp/make_qsp.py
"""

from __future__ import annotations

import pathlib

SHIFT = 5
OUT = pathlib.Path(__file__).resolve().parents[2] / "synthetic" / "qsp"


class Save:
    def __init__(self, encoding: str = "utf16") -> None:
        if encoding not in ("utf16", "ansi"):
            raise ValueError(encoding)
        self.encoding = encoding
        self.parts: list[bytes] = []

    def _unit_bytes(self, text: str) -> bytes:
        if self.encoding == "utf16":
            return text.encode("utf-16-le")
        return text.encode("cp1251")

    def _shift(self, data: bytes) -> bytes:
        if self.encoding == "utf16":
            out = bytearray()
            for index in range(0, len(data), 2):
                unit = int.from_bytes(data[index : index + 2], "little")
                if unit == SHIFT:
                    unit = (-SHIFT) & 0xFFFF
                elif unit:
                    unit = (unit - SHIFT) & 0xFFFF
                out += unit.to_bytes(2, "little")
            return bytes(out)
        out = bytearray()
        for byte in data:
            if byte == SHIFT:
                out.append((-SHIFT) & 0xFF)
            elif byte:
                out.append((byte - SHIFT) & 0xFF)
            else:
                out.append(0)
        return bytes(out)

    def _separator(self) -> bytes:
        if self.encoding == "utf16":
            return b"\r\x00\n\x00"
        return b"\r\n"

    def raw(self, text: str) -> None:
        self.parts.append(self._unit_bytes(text) + self._separator())

    def encoded(self, text: str) -> None:
        self.parts.append(self._shift(self._unit_bytes(text)) + self._separator())

    def number(self, value: int | str) -> None:
        self.encoded(str(value))

    def variant(self, value: tuple, type_text: str | None = None) -> None:
        kind = value[0]
        codes = {"undef": 0, "tuple": 1, "num": 2, "bool": 3, "str": 4, "code": 5, "varref": 6}
        self.number(type_text if type_text is not None else codes[kind])
        if kind == "undef":
            return
        if kind == "tuple":
            items = value[1]
            self.number(len(items))
            for item in items:
                self.variant(item)
            return
        if kind in ("num", "bool"):
            self.number(value[1])
            return
        if kind in ("str", "code", "varref"):
            self.encoded(value[1])
            return
        raise ValueError(kind)

    def variable(self, name: str, values: list[tuple], keys: list[tuple[int, tuple]]) -> None:
        self.encoded(name)
        self.number(len(values))
        for value in values:
            self.variant(value)
        self.number(len(keys))
        for index, value in keys:
            self.number(index)
            self.variant(value)

    def bytes(self) -> bytes:
        return b"".join(self.parts)


def make_full(encoding: str) -> bytes:
    save = Save(encoding)
    save.raw("QSPSAVEDGAME")
    save.raw("5.9.6")
    save.number(-1234567)  # checksum (invented, no CRC check in the editor)
    save.number(3723000)  # play time in milliseconds
    save.number(0)  # selected action
    save.number(-1)  # selected object
    save.encoded("images/room.png")
    save.encoded("look")
    save.encoded("A quiet workshop. The kettle is cold.")
    save.encoded("")
    save.encoded("Workshop")
    save.number(63)  # windows state
    save.number(500)  # timer interval
    save.number(2)  # playlist
    save.encoded("audio/theme.ogg")
    save.encoded("audio/rain.ogg")
    save.number(1)  # included files
    save.encoded("epilogue.qsp")
    save.number(2)  # actions
    save.encoded("Inspect the bench")
    save.encoded("images/bench.png")
    save.number(2)  # lines of code
    save.encoded("act 'Take the wrench': goto 'Bench'")
    save.number(10)
    save.encoded("msg 'Nothing else here.'")
    save.number(11)
    save.encoded("Workshop")
    save.number(0)
    save.encoded("Open the door")
    save.encoded("")
    save.number(0)
    save.encoded("")
    save.number(1)
    save.number(2)  # objects
    save.encoded("Kettle")
    save.encoded("images/kettle.png")
    save.encoded("Wrench")
    save.encoded("images/wrench.png")
    save.number(1)  # object groups
    save.encoded("Tools")
    save.encoded("Things on the bench")
    save.encoded("images/tools.png")
    save.number(1)
    save.number(2)
    save.number(4)  # variables
    save.variable(
        "MONEY",
        [("num", 120), ("num", -5)],
        [],
    )
    save.variable(
        "NAME",
        [("str", "Robin")],
        [],
    )
    save.variable(
        "FLAGS",
        [("bool", 1), ("bool", 0), ("undef",)],
        [],
    )
    save.variable(
        "INVENTORY",
        [
            ("str", "wrench"),
            ("str", "bolt"),
            ("tuple", [("num", 1), ("str", "spare key")]),
        ],
        [(0, ("num", 1)), (2, ("str", "pocket"))],
    )
    return save.bytes()


def make_minimal() -> bytes:
    save = Save()
    save.raw("QSPSAVEDGAME")
    save.raw("5.8.0")
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(0)
    save.encoded("")
    save.encoded("")
    save.encoded("")
    save.encoded("")
    save.encoded("")
    save.number(0)
    save.number(500)
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(0)
    return save.bytes()


def make_legacy_ansi() -> bytes:
    save = Save("ansi")
    save.raw("QSPSAVEDGAME")
    save.raw("4.2.0")
    save.number(42)
    save.number(60000)
    save.number(0)
    save.number(0)
    save.encoded("")
    save.encoded("")
    save.encoded("Старая мастерская")
    save.encoded("")
    save.encoded("Мастерская")
    save.number(31)
    save.number(500)
    save.number(0)
    save.number(0)
    save.number(1)
    save.encoded("Осмотреть стол")
    save.encoded("")
    save.number(0)
    save.encoded("Мастерская")
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(2)
    save.variable("ДЕНЬГИ", [("num", 300)], [])
    save.variable("ИМЯ", [("str", "Робин")], [])
    return save.bytes()


def make_padded_numbers() -> bytes:
    save = Save()
    save.raw("QSPSAVEDGAME")
    save.raw("5.9.6")
    save.number("+007")  # checksum, non-canonical text
    save.number(" 120 ")  # play time, non-canonical text
    save.number(0)
    save.number(0)
    save.encoded("")
    save.encoded("")
    save.encoded("")
    save.encoded("")
    save.encoded("")
    save.number(0)
    save.number(500)
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(0)
    save.number(1)
    save.encoded("COINS")
    save.number(1)
    save.variant(("num", -3), type_text="+2")  # non-canonical value type
    save.number(0)
    return save.bytes()


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    files = {
        "chapter-one.sav": make_full("utf16"),
        "minimal.sav": make_minimal(),
        "legacy-ansi.sav": make_legacy_ansi(),
        "padded-numbers.sav": make_padded_numbers(),
    }
    for name, data in files.items():
        path = OUT / name
        path.write_bytes(data)
        print(f"wrote {path} ({len(data)} bytes)")


if __name__ == "__main__":
    main()
