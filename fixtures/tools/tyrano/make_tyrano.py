#!/usr/bin/env python3
"""Write the synthetic TyranoScript fixtures.

TyranoScript saves are JSON text passed through JavaScript `escape()` (the desktop builds) or
`encodeURIComponent()` (browser storage and some builds), so the fixtures are written with the
same two encodings, implemented here from the JavaScript specifications. The game data is
invented (no real player data) and the files are written as ASCII bytes.

Run from the repository root:

    python fixtures/tools/tyrano/make_tyrano.py
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "tyrano"

# `escape()` leaves these ASCII characters (and the alphanumerics) unescaped.
ESCAPE_SAFE = set(
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789@*_+-./"
)
# `encodeURIComponent()` leaves these ASCII characters (and the alphanumerics) unescaped.
URI_SAFE = set(
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()"
)


def js_escape(text: str) -> str:
    """ECMAScript `escape`: UTF-16 code units, `%XX` up to 0xFF, `%uXXXX` above."""
    out: list[str] = []
    units = text.encode("utf-16-be")
    for index in range(0, len(units), 2):
        unit = (units[index] << 8) | units[index + 1]
        character = chr(unit)
        if character in ESCAPE_SAFE:
            out.append(character)
        elif unit <= 0xFF:
            out.append("%{:02X}".format(unit))
        else:
            out.append("%u{:04X}".format(unit))
    return "".join(out)


def encode_uri_component(text: str) -> str:
    """ECMAScript `encodeURIComponent`: UTF-8 bytes as uppercase `%XX`."""
    out: list[str] = []
    for character in text:
        if character in URI_SAFE:
            out.append(character)
        else:
            for byte in character.encode("utf-8"):
                out.append("%{:02X}".format(byte))
    return "".join(out)


def json_text(value: object) -> str:
    """The compact JSON text JavaScript's `JSON.stringify` produces for this data."""
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def slot(number: int, variables: dict[str, object]) -> dict[str, object]:
    return {
        "stat": {"f": variables},
        "title": "スロット {} のセーブ".format(number),
        "time": 1761000000 + number,
    }


def data_document() -> dict[str, object]:
    return {
        "data": [
            slot(
                1,
                {
                    "money": 1250,
                    "name": "旅人 アリス",
                    "hasKey": True,
                    "inventory": {"potion": 3, "herbs": 0, "map": True},
                    "visited": ["森 の 入口", "古い 井戸"],
                    "luck": 1.5,
                },
            ),
            slot(
                2,
                {
                    "money": 40,
                    "name": "Boris the merchant",
                    "hasKey": False,
                    "inventory": {"rope": 1, "lamp": False},
                    "quest": {"name": "失われた 鍵", "done": False, "steps": 2},
                },
            ),
            slot(
                3,
                {
                    "money": 999999,
                    "name": "星 の 子供",
                    "hasKey": True,
                    "inventory": {"star": 12, "crown": True},
                    "note": "Thank you! また 会おう",
                    "emoji": "😀",
                },
            ),
        ]
    }


def system_document() -> dict[str, object]:
    return {
        "sf": {
            "config": {"volume": 0.8, "skipUnread": False, "language": "日本語"},
            "cg": {"alice": True, "boris": False, "ending": {"normal": True}},
            "ending": {"trueEnd": False, "seen": ["森 の 入口"]},
            "playerName": "アリス 旅人",
        }
    }


def write(name: str, text: str) -> None:
    path = OUT / name
    path.write_bytes(text.encode("ascii"))
    print(path.relative_to(ROOT).as_posix())


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    data = json_text(data_document())
    system = json_text(system_document())
    write("sample_tyrano_data.sav", js_escape(data))
    write("sample_tyrano_data_uri.sav", encode_uri_component(data))
    write("sample_tyrano_sf.sav", js_escape(system))


if __name__ == "__main__":
    main()
