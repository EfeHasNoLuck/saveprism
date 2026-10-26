#!/usr/bin/env python3
"""Generate invented Pixel Crushers Save System (Unity) save files.

The shape follows the public Pixel Crushers documentation (behaviour only):
- `SavedGameData` holds a version, the scene name and a list of save records.
- Every `SaveRecord` has a key, a scene index and the saver's data as a string.
- `JsonDataSerializer` writes the object with Unity's `JsonUtility`, so the outer document is
  JSON; `prettyPrint` only changes the spacing.
- The Dialogue System's saver stores `PersistentDataManager.GetSaveData()` there, which the
  manual calls "human-readable Lua code" (`Variable["Gold"]=10; ...`).

All class names, keys and values are invented. Nothing is executed.

    python fixtures/tools/pixelcrushers/make_pixelcrushers.py

writes
    fixtures/synthetic/pixelcrushers/save0.dat   compact JSON, one JSON record, one Lua record,
                                                 one plain string record
    fixtures/synthetic/pixelcrushers/save1.dat   pretty-printed JSON, two JSON records
"""

import json
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "pixelcrushers"


def compact(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def pretty(value):
    return json.dumps(value, indent=2, ensure_ascii=False)


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    player = compact(
        {
            "position": {"x": 12.5, "y": 0.0, "z": -3.25},
            "rotation": 180.0,
            "health": 87,
            "name": "Mira",
            "items": ["sword", "potion"],
        }
    )
    lua = (
        'Variable["Gold"]=10; Variable["Hero"]="Mira"; Variable["HasMap"]=true; '
        'Variable["LastItem"]=nil; Item["Sword"].State="active"; '
        'Quest["Find the cat"]={State="Active",Entries={[0]="success",[1]="unassigned"}};'
    )
    save0 = {
        "version": 3,
        "sceneName": "Village",
        "saveRecords": [
            {"key": "Player", "sceneIndex": 0, "data": player},
            {"key": "DialogueSystemSaver", "sceneIndex": 0, "data": lua},
            {"key": "PlainSaver", "sceneIndex": 0, "data": "not json, not lua"},
        ],
    }
    (OUT / "save0.dat").write_text(compact(save0), encoding="utf-8", newline="")

    save1 = {
        "version": 4,
        "sceneName": "Forest",
        "saveRecords": [
            {
                "key": "Inventory",
                "sceneIndex": 1,
                "data": compact({"gold": 250, "slots": [{"id": 3, "count": 2}]}),
            },
            {
                "key": "SpawnedObjectManager",
                "sceneIndex": 1,
                "data": compact({"prefabs": [], "destroyed": ["Orc"]}),
            },
        ],
    }
    (OUT / "save1.dat").write_text(pretty(save1), encoding="utf-8", newline="")


if __name__ == "__main__":
    main()
