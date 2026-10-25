#!/usr/bin/env python3
"""Write the synthetic structured-text fixtures (XML, INI, properties, TOML, YAML, CSV, plist).

All data is invented. The files deliberately mix comments, odd spacing, CRLF, a BOM, Unicode,
entities, CDATA, repeated elements/sections, quoted CSV cells with embedded commas/quotes/newlines,
and every plist scalar type. The binary plist is written from the published `bplist00` layout
(header, object table, offset table, 32-byte trailer).

Usage: python fixtures/tools/text/make_text.py
"""

from __future__ import annotations

import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "synthetic"


def write(path: Path, text: str, *, encoding: str = "utf-8") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(text.encode(encoding))


def xml() -> None:
    write(
        ROOT / "xml" / "save.xml",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        "<!-- SavePrism synthetic save; invented data -->\n"
        '<save version="2" created="2026-10-24T12:00:00Z">\n'
        '  <player id="1" name="Mira &amp; Rune">\n'
        '    <stats hp="42" mana="7.5"/>\n'
        "    <inventory>\n"
        '      <item slot="1" kind="sword">Old Blade</item>\n'
        '      <item slot="2" kind="shield"><![CDATA[Iron <Guard>]]></item>\n'
        '      <item slot="3" kind="potion">Potion &amp; Herb</item>\n'
        "    </inventory>\n"
        "    <bio>  A line with leading spaces\nand a second line  </bio>\n"
        '    <?note keep="me"?>\n'
        "    <empty></empty>\n"
        "  </player>\n"
        "</save>\n",
    )
    write(
        ROOT / "xml" / "crlf.xml",
        '<?xml version="1.0"?>\r\n<root>\r\n  <name>Birim \u0130stanbul</name>\r\n'
        '  <x a="1">first</x>\r\n  <x a="2">second</x>\r\n</root>\r\n',
    )


def ini() -> None:
    write(
        ROOT / "ini" / "settings.ini",
        "\ufeff; SavePrism synthetic INI; invented data\r\n"
        "[Audio]\r\n"
        "  volume = 0.75   ; soft\r\n"
        "music=on\r\n"
        "[Video]\r\n"
        "width=1280\r\n"
        "height=720\r\n"
        "title = Birim \u0130stanbul\r\n"
        "[Audio]\r\n"
        "volume = 0.5\r\n",
    )
    write(
        ROOT / "ini" / "plain.cfg",
        "# no sections here\nhost = localhost\nport = 8080\n",
    )


def properties() -> None:
    write(
        ROOT / "properties" / "settings.properties",
        "# SavePrism synthetic properties\n"
        "! invented data\n"
        "player.name = Mira Rune\n"
        "player.level=42\n"
        "greeting = hello \\\n"
        "    world\n"
        "unicode = caf\\u00e9 \\u0130stanbul\n"
        "path = C:\\\\Users\\\\Mira\\\\Saves\n"
        "empty=\n"
        "# trailing comment\n",
    )


def toml() -> None:
    write(
        ROOT / "toml" / "settings.toml",
        '# SavePrism synthetic TOML\n'
        'title = "SavePrism"\n'
        "\n"
        "[player]\n"
        'name = "Mira"   # the hero\n'
        "level = 0x2a\n"
        "ratio = 1.5e0\n"
        "active = true\n"
        "born = 2026-10-24T12:00:00Z\n"
        'tags = ["ranger", "scout"]\n'
        "\n"
        "[player.stats]\n"
        "hp = 42\n"
        "\n"
        "[quest]\n"
        "title = 'First Quest'\n"
        "\n"
        "[[quest.steps]]\n"
        'name = "Start"\n'
        "done = false\n"
        "\n"
        "[[quest.steps]]\n"
        'name = "Boss"\n'
        "done = true\n",
    )


def yaml() -> None:
    write(
        ROOT / "yaml" / "save.yaml",
        "# SavePrism synthetic YAML\n"
        "save:\n"
        "  version: 2\n"
        "  player:\n"
        "    name: Mira\n"
        "    level: 42\n"
        "    ratio: 1.5\n"
        "    active: true\n"
        '    title: "the hero"\n'
        "    quote: 'it''s fine'\n"
        "    bio: |\n"
        "      line one\n"
        "      line two\n"
        "  inventory:\n"
        "    - {slot: 1, kind: sword, name: Old Blade}\n"
        "    - slot: 2\n"
        "      kind: shield\n"
        "  base: &base\n"
        "    x: 1\n"
        "  copy: *base\n"
        "  tagged: !!str 5\n"
        "  empty:\n",
    )
    write(
        ROOT / "yaml" / "flow.yml",
        "---\r\nflow: {a: 1, b: [true, null]}\r\nlist: [1, 2.5, \"x\"]\r\n",
    )


def csv() -> None:
    write(
        ROOT / "csv" / "save.csv",
        "name,class,note,level\r\n"
        'Mira,"Ranger, Scout","said ""hi""",42\r\n'
        '"\u0130stanbul",Mage,"multi\nline",7\r\n',
    )
    write(
        ROOT / "csv" / "no-header.csv",
        '1,2,3\n"a,b",c,"d""e"\n',
    )


def plist() -> None:
    write(
        ROOT / "plist" / "save.plist",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
        '"http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
        '<plist version="1.0">\n'
        "<dict>\n"
        "  <key>name</key><string>Mira &amp; Rune</string>\n"
        "  <key>level</key><integer>42</integer>\n"
        "  <key>ratio</key><real>1.5</real>\n"
        "  <key>active</key><true/>\n"
        "  <key>items</key><array><string>a</string><string>b</string></array>\n"
        "  <key>born</key><date>2020-01-02T03:04:05Z</date>\n"
        "  <key>blob</key><data>AAEC</data>\n"
        "</dict>\n"
        "</plist>\n",
    )
    write(
        ROOT / "plist" / "unicode.plist",
        '<?xml version="1.0" encoding="UTF-8"?>\r\n'
        '<plist version="1.0">\r\n<dict>\r\n'
        "  <key>city</key>\r\n  <string>\u0130stanbul</string>\r\n"
        "  <key>empty</key>\r\n  <string></string>\r\n"
        "  <key>nested</key>\r\n  <array><dict><key>k</key><string>v</string></dict></array>\r\n"
        "</dict>\r\n</plist>\r\n",
    )


def template_plist(objects: list[bytes], top: int, ref_size: int, offset_size: int) -> bytes:
    out = bytearray(b"bplist00")
    offsets = []
    for item in objects:
        offsets.append(len(out))
        out += item
    table_at = len(out)
    for offset in offsets:
        out += offset.to_bytes(offset_size, "big")
    out += b"\x00" * 6
    out += bytes([offset_size, ref_size])
    out += len(objects).to_bytes(8, "big")
    out += top.to_bytes(8, "big")
    out += table_at.to_bytes(8, "big")
    return bytes(out)


def ascii_obj(text: str) -> bytes:
    raw = text.encode("ascii")
    return bytes([0x50 | len(raw)]) + raw if len(raw) < 15 else b"\x5f" + int_obj(len(raw)) + raw


def utf16_obj(text: str) -> bytes:
    raw = text.encode("utf-16-be")
    count = len(raw) // 2
    return bytes([0x60 | count]) + raw if count < 15 else b"\x6f" + int_obj(count) + raw


def int_obj(value: int) -> bytes:
    return b"\x10" + bytes([value & 0xFF])


def real_obj(value: float) -> bytes:
    return b"\x23" + struct.pack(">d", value)


def array_obj(refs: list[int]) -> bytes:
    return bytes([0xA0 | len(refs)]) + bytes(refs)


def dict_obj(keys: list[int], values: list[int]) -> bytes:
    return bytes([0xD0 | len(keys)]) + bytes(keys) + bytes(values)


def binary_plist() -> None:
    # 0: root dict; 1..: keys/values. One-byte object references.
    objects = [
        dict_obj([1, 2, 4, 6, 8, 10], [3, 5, 7, 9, 11, 13]),
        ascii_obj("name"),
        ascii_obj("level"),
        ascii_obj("Mira"),
        ascii_obj("ratio"),
        int_obj(42),
        ascii_obj("active"),
        real_obj(1.5),
        ascii_obj("items"),
        b"	",  # true
        ascii_obj("city"),
        array_obj([12, 14]),
        ascii_obj("Old Blade"),
        utf16_obj("İstanbul"),
        ascii_obj("Iron Guard"),
    ]
    data = template_plist(objects, 0, 1, 2)
    (ROOT / "plist" / "binary.plist").parent.mkdir(parents=True, exist_ok=True)
    (ROOT / "plist" / "binary.plist").write_bytes(data)


def binary_plist_from_stdlib() -> None:
    """A second binary plist written by Python's own `plistlib` (PSF), as a real-writer check."""
    import plistlib

    data = {
        "name": "Mira",
        "level": 42,
        "ratio": 1.5,
        "active": True,
        "items": ["a", "b"],
        "nested": {"k": "v"},
        "blob": bytes([0, 1, 2]),
        "uid": plistlib.UID(7),
        "big": "\u0130stanbul",
    }
    (ROOT / "plist" / "binary-real.plist").write_bytes(
        plistlib.dumps(data, fmt=plistlib.FMT_BINARY, sort_keys=True)
    )


def main() -> None:
    xml()
    ini()
    properties()
    toml()
    yaml()
    csv()
    plist()
    binary_plist()
    binary_plist_from_stdlib()


if __name__ == "__main__":
    main()
