"""Build synthetic RPG Maker MZ saves the way MZ writes them.

MZ: pako.deflate(json, {to: "string", level: 1}) -> binary string (one char per byte)
-> fs.writeFileSync(path, str) -> UTF-8 on disk. pako is a port of zlib, so Python's
zlib at level 1 stands in for it. JSON text is compact (JSON.stringify).
"""
import json
import pathlib
import sys
import zlib

out_dir = pathlib.Path(sys.argv[1])
out_dir.mkdir(parents=True, exist_ok=True)


def stringify(obj):
    return json.dumps(obj, ensure_ascii=False, separators=(",", ":"))


def write_mz(name, obj):
    text = stringify(obj)
    deflated = zlib.compress(text.encode("utf-8"), 1)
    binary_string = deflated.decode("latin-1")
    (out_dir / name).write_bytes(binary_string.encode("utf-8"))


actor = {
    "_name": "Reid", "_nickname": "", "_profile": "", "_hp": 450, "_mp": 90, "_tp": 0,
    "_level": 7, "_exp": {"1": 1820}, "_skills": [8, 9, 10], "_equips": [
        {"_dataClass": "weapon", "_itemId": 1, "@": "Game_Item"},
        {"_dataClass": "armor", "_itemId": 2, "@": "Game_Item"},
    ],
    "_actorId": 1, "_classId": 1, "@": "Game_Actor",
}

file1 = {
    "system": {"_saveEnabled": True, "_menuEnabled": True, "_saveCount": 3,
               "_versionId": 81230456, "_framesOnSave": 21456, "@": "Game_System"},
    "screen": {"_brightness": 255, "_tone": [0, 0, 0, 0], "@": "Game_Screen"},
    "timer": {"_frames": 0, "_working": False, "@": "Game_Timer"},
    "switches": {"_data": [None, True, False, True], "@": "Game_Switches"},
    "variables": {"_data": [None, 12, 0, 3.5, "Gate key"], "@": "Game_Variables"},
    "selfSwitches": {"_data": {"1,2,A": True}, "@": "Game_SelfSwitches"},
    "actors": {"_data": [None, actor], "@": "Game_Actors"},
    "party": {"_gold": 1250, "_steps": 4321, "_items": {"1": 5, "7": 1},
              "_weapons": {"1": 1}, "_armors": {}, "_actors": [1], "@": "Game_Party"},
    "map": {"_mapId": 4, "_displayX": 0, "_displayY": 0, "@": "Game_Map"},
    "player": {"_x": 8, "_y": 12, "_direction": 2, "_moveSpeed": 4, "@": "Game_Player"},
}

global_info = [
    None,
    {"title": "Synthetic Quest", "characters": [["Actor1", 0]], "faces": [["Actor1", 0]],
     "playtime": "00:05:57", "timestamp": 1761090000000, "@": "Object"},
]

config = {"alwaysDash": False, "commandRemember": False, "touchUI": True,
          "bgmVolume": 90, "bgsVolume": 90, "meVolume": 90, "seVolume": 90}

# A larger save so the deflate stream has several blocks and many bytes >= 0x80.
big = dict(file1)
big["variables"] = {"_data": [None] + [i * 7 % 1000 for i in range(1, 6000)], "@": "Game_Variables"}
big["switches"] = {"_data": [None] + [i % 3 == 0 for i in range(1, 6000)], "@": "Game_Switches"}

write_mz("file1.rmmzsave", file1)
write_mz("global.rmmzsave", global_info)
write_mz("config.rmmzsave", config)
write_mz("big.rmmzsave", big)
print("wrote", sorted(p.name for p in out_dir.iterdir()))
