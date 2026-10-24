#!/usr/bin/env python3
"""Writes the synthetic zip-container saves under fixtures/synthetic/zip/.

Python 3 standard library only. All data is invented; the Marshal payload is copied from the
existing synthetic RPG Maker VX Ace fixture so the inner codec is a real one.

    python fixtures/tools/zip/make_zip.py
"""

import json
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "zip"
MARSHAL = ROOT / "synthetic" / "rpgmaker-vxace" / "Save01.rvdata2"
DATE = (2026, 10, 24, 12, 0, 0)


def info(name, method):
    entry = zipfile.ZipInfo(name, date_time=DATE)
    entry.compress_type = method
    entry.create_system = 3
    entry.external_attr = 0o600 << 16
    return entry


def write(name, entries):
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / name
    with zipfile.ZipFile(path, "w") as archive:
        for entry_name, data, method in entries:
            archive.writestr(info(entry_name, method), data)
    print(path.relative_to(ROOT))


def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


single = {
    "player": {
        "name": "Ada",
        "level": 12,
        "gold": 340,
        "alive": True,
        "stats": {"speed": 1.5, "luck": 0.25},
    },
    "quests": ["intro", "garden"],
    "day": 3,
}

es3 = {
    "gold": {"__type": "int", "value": 340},
    "playerName": {"__type": "string", "value": "Ada"},
    "position": {"__type": "UnityEngine.Vector3", "value": {"x": 1.5, "y": 0.0, "z": -2.25}},
    "visited": {"__type": "System.Collections.Generic.List`1[[System.String]]",
                "value": ["intro", "garden", "harbor"]},
}


def main():
    write(
        "single-json.zip",
        [("save.json", compact(single), zipfile.ZIP_DEFLATED)],
    )
    write(
        "stored-es3.zip",
        [
            ("save.es3", compact(es3), zipfile.ZIP_STORED),
            ("meta.txt", b"slots: 1\ncreated: invented\n", zipfile.ZIP_STORED),
        ],
    )
    payload = MARSHAL.read_bytes()
    write(
        "marshal-payload.zip",
        [
            ("save.rvdata2", payload, zipfile.ZIP_DEFLATED),
            ("readme.txt", b"invented container fixture\n", zipfile.ZIP_STORED),
        ],
    )


if __name__ == "__main__":
    main()
