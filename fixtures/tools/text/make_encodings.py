#!/usr/bin/env python3
"""Write synthetic text-save fixtures in the legacy and Unicode encodings.

All data is invented. The bytes are produced by Python's standard codecs, which implement the
WHATWG labels libiconv shares for these files: Shift_JIS, GB18030, CP949 (EUC-KR) and the BOM
forms of UTF-16/UTF-32 (Unicode FAQ "UTF-8, UTF-16, UTF-32 & BOM").

Usage: python fixtures/tools/text/make_encodings.py
"""

from __future__ import annotations

import codecs
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "synthetic" / "encodings"


def write(name: str, text: str, encoding: str, bom: bytes = b"") -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    (ROOT / name).write_bytes(bom + text.encode(encoding))


def main() -> None:
    write(
        "json-shiftjis.json",
        '{"name":"ミラ","city":"東京","level":42,"items":["薬",false,null]}',
        "shift_jis",
    )
    write(
        "json-utf16le.json",
        '{\r\n  "name": "Mira",\r\n  "city": "Zürich",\r\n  "level": 42\r\n}\r\n',
        "utf-16-le",
        codecs.BOM_UTF16_LE,
    )
    write(
        "json-utf16be.json",
        '{"name":"Mira","city":"Zürich","level":42}',
        "utf-16-be",
        codecs.BOM_UTF16_BE,
    )
    write(
        "json-utf32le.json",
        '{"name":"Mira","value":7}',
        "utf-32-le",
        codecs.BOM_UTF32_LE,
    )
    write(
        "json-utf32be.json",
        '{"name":"Mira","value":7}',
        "utf-32-be",
        codecs.BOM_UTF32_BE,
    )
    write(
        "xml-gb18030.xml",
        '<?xml version="1.0" encoding="GB18030"?>\n'
        '<save version="1">\n'
        '  <player name="米拉">\n'
        '    <city>北京</city>\n'
        '    <item id="1">药草</item>\n'
        '  </player>\n'
        '</save>\n',
        "gb18030",
    )
    write(
        "ini-cp949.ini",
        "[Player]\r\nname = 미라\r\ncity = 서울\r\n[Items]\r\npotion = 3\r\n",
        "cp949",
    )
    write(
        "properties-utf16le.properties",
        "# invented\nplayer.name = ミラ\nplayer.city = 東京\nlevel = 42\n",
        "utf-16-le",
        codecs.BOM_UTF16_LE,
    )
    write(
        "toml-shiftjis.toml",
        'title = "ミラの冒険"\nlevel = 42\n[player]\nname = "ミラ"\n',
        "shift_jis",
    )
    write(
        "yaml-gb18030.yaml",
        "save:\n  player:\n    name: 米拉\n    city: 北京\n  level: 42\n",
        "gb18030",
    )
    write(
        "csv-utf16be.csv",
        "name,city,level\r\nMira,Zürich,42\r\nミラ,東京,7\r\n",
        "utf-16-be",
        codecs.BOM_UTF16_BE,
    )
    print("wrote", sorted(path.name for path in ROOT.iterdir()))


if __name__ == "__main__":
    main()
