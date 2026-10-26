"""Build synthetic AliceSoft and Artemis saves from their public layouts.

The AliceSoft System 3.x layout follows the CC0 Kaitai description
(https://gist.github.com/kichikuou/e80b96fdc8184166c02c674ade907f4f); the System 4
GSave layout follows the public libsys4 savefile notes. The Artemis envelope follows the
public BOWS/BOWG samples. Values are invented; these files contain no game data.
"""
import pathlib
import struct
import sys
import zlib

out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)


def sjis(text):
    return text.encode("shift_jis")


# ---------------------------------------------------------------- System 3.x (.asd)
HEADER = 4096
SYS3_VERSION = 0x350200
magic = b"System3.8 SaveData(c)ALICE-SOFT".ljust(32, b"\0")
strings = ["Mira", "放課後の教室", "", "Gold"]
str_block = struct.pack("<IIII", sum(len(sjis(s)) + 1 for s in strings), len(strings),
                        max(len(sjis(s)) for s in strings), 0)
for text in strings:
    str_block += sjis(text) + b"\0"

stack_block = struct.pack("<IIII", 0, 0, 0, 0)
pages = [(0, [100, 250, 65535, 7]), (3, [1, 0, 42])]
page_blocks = []
for page, values in pages:
    body = b"".join(struct.pack("<H", v) for v in values)
    page_blocks.append((page, struct.pack("<IIII", len(body), page, 0, 0) + body))

header = bytearray(HEADER)
header[0:32] = magic
struct.pack_into("<I", header, 32, SYS3_VERSION)
header[36:64] = "Synthetic Alice".encode("shift_jis").ljust(28, b"\0")
struct.pack_into("<I", header, 2144, HEADER)
str_offset = HEADER + len(stack_block)
struct.pack_into("<I", header, 2148, str_offset)
pos = str_offset + len(str_block)
for page, block in page_blocks:
    struct.pack_into("<I", header, 2160 + page * 4, pos)
    pos += len(block)
data = bytes(header) + stack_block + str_block + b"".join(b for _, b in page_blocks)
(out / "system38.asd").write_bytes(data)

# ---------------------------------------------------------------- System 4 GSave (.asd)
key = b"savekey\0"
group = b"main\0"
globals_def = [
    (10, 1250, b"gold\0", 1),
    (12, 0, b"hero\0", 1),
    (47, 1, b"seen\0", 1),
    (11, struct.unpack("<I", struct.pack("<f", 1.5))[0], b"rate\0", 1),
]
records = struct.pack("<ii", 1000, 0) + b"\0" + struct.pack("<i", len(globals_def))
records += b"".join(struct.pack("<i", i) for i in range(len(globals_def)))
globals_blob = b"".join(struct.pack("<ii", t, v) + name + struct.pack("<i", 1)
                        for t, v, name, _ in globals_def)
strings_blob = b"Mira\0"

header_len = len(key) + 4 * 4 + 5 * 8 + len(group)
records_offset = header_len
globals_offset = records_offset + len(records)
strings_offset = globals_offset + len(globals_blob)
arrays_offset = strings_offset + len(strings_blob)
keyvals_offset = arrays_offset

head = key + struct.pack("<iiii", 1000, 5, 56, len(globals_def))
head += struct.pack("<IiIiIiIiIi",
                    records_offset, 1,
                    globals_offset, len(globals_def),
                    strings_offset, 1,
                    arrays_offset, 0,
                    keyvals_offset, 0)
head += group
raw = head + records + globals_blob + strings_blob
envelope = b"GD\x01\x01" + struct.pack("<I", len(raw)) + zlib.compress(raw, 9)
(out / "system4.asd").write_bytes(envelope)
(out / "system4.asdbak").write_bytes(envelope)

# ---------------------------------------------------------------- Artemis BOWG
body = b"\x01\x00\x00\x00system/first.iet\x00\x03\x00\x00\x00g.script\\x00"
artemis = b"BOWG" + struct.pack("<II", 1002, len(body)) + zlib.compress(body, 9)
(out / "saveg.dat").write_bytes(artemis)
print("wrote", sorted(p.name for p in out.iterdir()))
