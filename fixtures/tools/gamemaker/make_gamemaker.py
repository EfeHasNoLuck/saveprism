#!/usr/bin/env python3
"""Generate invented GameMaker and Pixel Game Maker MV save fixtures.

GameMaker saves are commonly an INI file (`ini_open`), a `json_encode` / `json_stringify`
document, or the hexadecimal dump of `ds_map_write`. The `ds_map` bytes follow the public
reverse-engineered layout (magic 402, u32 entry count, typed little-endian objects). The
Pixel Game Maker MV save is the plain JSON file the engine keeps at
`%LOCALAPPDATA%/<project>/save/play_LOADFILE#.json`. All values are invented.
"""

from pathlib import Path
import base64
import json
import struct

FIXTURES = Path(__file__).resolve().parents[2]
GM = FIXTURES / "synthetic" / "gamemaker"
PGM = FIXTURES / "synthetic" / "pixel-game-maker-mv"

PLAYER = {
    "player": {"name": "Mira", "gold": 120, "level": 7},
    "items": ["sword", "shield"],
}


def real(value: float) -> bytes:
    return struct.pack("<I", 0) + struct.pack("<d", value)


def text(value: str) -> bytes:
    raw = value.encode("utf-8")
    return struct.pack("<II", 1, len(raw)) + raw


def ds_map() -> bytes:
    entries = [
        (text("gold"), real(120.0)),
        (text("name"), text("Mira")),
        (real(3.14), text("pi")),
    ]
    out = struct.pack("<II", 402, len(entries))
    for key, value in entries:
        out += key + value
    return out


def main() -> None:
    GM.mkdir(parents=True, exist_ok=True)
    PGM.mkdir(parents=True, exist_ok=True)

    (GM / "save.ini").write_bytes(
        b'[player]\nname=Mira\ngold=120\ntitle="The Hero"\n\n[flags]\nmet_guard="1"\n',
    )
    (GM / "save.json").write_bytes(
        json.dumps(PLAYER, separators=(",", ":")).encode("utf-8"),
    )
    (GM / "save-base64.txt").write_bytes(
        base64.b64encode(json.dumps(PLAYER, separators=(",", ":")).encode("utf-8")),
    )
    dump = ds_map().hex().upper()
    (GM / "ds-map.txt").write_bytes(dump.encode("ascii") + b"\n")
    (GM / "ds-map-lower.txt").write_bytes(dump.lower().encode("ascii"))

    (PGM / "play_LOADFILE1.json").write_bytes(
        json.dumps(
            {
                "variables": {"gold": 120, "hp": 35, "name": "Mira"},
                "switches": {"met_guard": True},
                "version": 1,
            },
            indent=2,
        ).encode("utf-8")
        + b"\n",
    )


if __name__ == "__main__":
    main()
