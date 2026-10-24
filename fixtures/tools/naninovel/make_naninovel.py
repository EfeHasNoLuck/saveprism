#!/usr/bin/env python3
"""Writes the synthetic Naninovel save fixtures (invented data, Python standard library only).

A Naninovel slot is JSON: `objectJsonMap.keys` holds service type names and
`objectJsonMap.values` holds each service's state as a JSON string. The custom variables are in
`Naninovel.CustomVariableManager+GameState`; `value.type` is 0 (text), 1 (number) or 2 (boolean).

Outputs:
  fixtures/synthetic/naninovel/GameSave001.json         plain JSON (text save slots)
  fixtures/synthetic/naninovel/GameSave001-gzip.nson    gzip-compressed JSON
  fixtures/synthetic/naninovel/GameSave001-deflate.nson raw DEFLATE (the default .nson slot)

Layout sources: naninovel.com/guide/state-management and /guide/custom-variables; the state-map
field names were cross-checked against public MIT-licensed readers. No game data is used.
"""

import gzip
import json
import pathlib
import zlib

HERE = pathlib.Path(__file__).resolve()
ROOT = HERE.parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "naninovel"
CUSTOM = (
    "Naninovel.CustomVariableManager+GameState, Elringus.Naninovel.Runtime, "
    "Version=0.0.0.0, Culture=neutral, PublicKeyToken=null"
)
INVENTORY = "Game.InventoryService+State, Assembly-CSharp"

CUSTOM_STATE = {
    "LocalVariables": [
        {
            "name": "coins",
            "scope": 0,
            "value": {
                "type": 1,
                "stringValue": "",
                "numericValue": 50,
                "booleanValue": False,
            },
        },
        {
            "name": "hero_name",
            "scope": 0,
            "value": {
                "type": 0,
                "stringValue": "Aster",
                "numericValue": 0,
                "booleanValue": False,
            },
        },
        {
            "name": "has_key",
            "scope": 0,
            "value": {
                "type": 2,
                "stringValue": "",
                "numericValue": 0,
                "booleanValue": True,
            },
        },
    ],
    "GlobalVariables": [
        {
            "name": "g_cleared",
            "scope": 1,
            "value": {
                "type": 2,
                "stringValue": "",
                "numericValue": 0,
                "booleanValue": False,
            },
        }
    ],
}

INVENTORY_STATE = {"items": {"potion": 3, "key": 1}, "capacity": 20}


def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def main():
    save = {
        "objectJsonMap": {
            "keys": [CUSTOM, INVENTORY],
            "values": [compact(CUSTOM_STATE), compact(INVENTORY_STATE)],
        },
        "playbackSpot": {
            "scriptPath": "Chapter1",
            "lineIndex": 12,
            "inlineIndex": 0,
        },
        "rollbackStackJson": "[]",
        "thumbnailBase64": "",
    }
    data = compact(save).encode("utf-8")
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "GameSave001.json").write_bytes(data)
    (OUT / "GameSave001-gzip.nson").write_bytes(
        gzip.compress(data, compresslevel=9, mtime=0)
    )
    encoder = zlib.compressobj(level=9, wbits=-15)
    (OUT / "GameSave001-deflate.nson").write_bytes(
        encoder.compress(data) + encoder.flush()
    )
    print(f"wrote 3 files to {OUT}")


if __name__ == "__main__":
    main()
