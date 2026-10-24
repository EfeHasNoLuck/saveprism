#!/usr/bin/env python3
"""Build synthetic, unencrypted WOLF RPG Editor save slots.

The bytes follow the public layout documented by the MIT-licensed Sinflower/WolfSave
project (header, 0x19 marker, u16-prefixed game name, u16 version, SavePart1..5, the
variable database, SavePart7, final 0x19). All data here is invented; no game data is
included. Python 3 standard library only.
"""

from pathlib import Path
import struct


def u8(*values):
    return bytes(values)


def u16(*values):
    return b"".join(struct.pack("<H", v & 0xFFFF) for v in values)


def u32(*values):
    return b"".join(struct.pack("<I", v & 0xFFFFFFFF) for v in values)


def i32(*values):
    return b"".join(struct.pack("<i", v) for v in values)


def u64(*values):
    return b"".join(struct.pack("<Q", v) for v in values)


def mem16(data: bytes):
    return u16(len(data)) + data


def mem32(data: bytes):
    return u32(len(data)) + data


def text16(text: str):
    return mem16(text.encode("utf-8") + b"\0")


def text32(text: str):
    return mem32(text.encode("utf-8") + b"\0")


def part1_1_1():
    out = u8(1, 2, 3, 4, 5, 6)
    out += u32(1)
    out += u8(1) + u8(2) + u32(10, 11) + u8(1) + u8(0)
    return out


def part1_1(version):
    out = u32(1, 2, 3, 4)
    out += u8(1) + u32(5) + mem32(b"block")
    out += u16(1, 2, 3, 4) + u8(1, 2)
    out += part1_1_1() + part1_1_1()
    out += u16(1, 2, 3, 4, 5, 6) + u8(1, 2, 3)
    out += u32(2) + u32(100, 200)
    if version >= 0x70:
        out += u8(1, 2)
    if version >= 0x73:
        out += u32(1, 2, 3, 4)
    if version >= 0x78:
        out += u32(5)
    if version >= 0x85:
        out += u32(6, 7, 8)
    if version >= 0x8A:
        out += u16(1) + mem16(b"m\0") + u32(9) + u8(3) + u32(10, 11)
    if version >= 0x8B:
        out += u32(12)
    if version >= 0x8C:
        out += u32(13)
    return out


def part1(version):
    out = u32(11, 22, 33)
    if version >= 0x69:
        out += u32(2, 3, 0xFFFFFFFF)
    out += mem32(b"map metadata")
    out += b"".join(mem32(b"meta%02d\0" % i) for i in range(31 if version >= 0x8A else 15 if version >= 0x73 else 7))
    switches = [1, 0, 1, 0]
    out += u32(len(switches))
    if version >= 0x64:
        out += u32(*switches)
    else:
        out += bytes(switches)
    out += u32(1) + part1_1(version)
    if version >= 0x72:
        out += u32(*range(1, 12))
    variables = [1000, -5, 0, 42, 7]
    out += u32(len(variables)) + i32(*variables)
    return out


def part2(version):
    out = u8(1, 2)
    out += u32(*range(1, 20))
    out += u16(1, 2, 3)
    out += u32(4)
    out += u16(5, 6, 7)
    out += u32(*range(8, 17))
    out += u8(3) + u16(4) + u32(5, 6) + u16(7, 8, 9, 10) + u32(11, 12, 13) + u8(14)
    if version <= 96:
        out += u32(15)
    out += u32(16)
    if version >= 98:
        out += u32(17, 18)
    if version >= 100:
        out += u32(19, 20) + u8(21)
        out += u32(1) + mem32(b"party\0")
        out += u32(1) + u32(22)
        out += u32(*range(23, 28))
    if version >= 101:
        out += u32(*range(28, 34))
    if version >= 102:
        out += u16(34)
    if version >= 103:
        out += mem16(b"a\0") + mem16(b"b\0")
    if version >= 104:
        out += u32(35)
    if version >= 106:
        out += u32(36, 37, 38)
    if version >= 108:
        out += mem16(b"c\0") + u32(39, 40, 41) + mem16(b"d\0") + u32(42, 43, 44)
    if version >= 109:
        out += mem16(b"e\0") + mem16(b"f\0") + u8(45)
    if version >= 110:
        out += u8(46)
    if version >= 119:
        out += mem16(b"g\0") + mem16(b"h\0") + mem16(b"i\0")
    if version >= 121:
        out += u32(47)
    if version >= 122:
        out += u32(48)
    if version >= 124:
        out += u32(49)
    if version >= 126:
        out += u32(50)
    if version >= 128:
        out += u32(51)
    if version >= 129:
        out += u32(52, 53)
    if version >= 130:
        out += u32(54)
    if version >= 131:
        out += u32(55)
    if version >= 132:
        out += u32(56, 57, 58, 59)
    if version >= 134:
        out += u8(60)
    if version >= 136:
        out += u8(61)
    if version >= 137:
        out += u32(62, 63, 64, 65)
        out += u32(*range(66, 90))
    if version >= 0x8A:
        out += u32(90, 91, 92, 93)
        out += mem32(b"extra\0")
        out += u32(1) + mem32(b"more\0")
    if version >= 0x8D:
        out += bytes(range(14))
        out += bytes(range(0x100))
        out += u8(1, 2)
    if version >= 0x8E:
        out += u32(94)
    return out


def part3(version):
    out = u32(1)
    out += u32(1) + u32(1) + u32(9) + u8(0)
    out += u32(1) + u32(1) + u32(7)
    out += u32(1)
    if version < 0x6F:
        out += mem16(b"old\0")
    else:
        out += mem32(b"new\0")
    out += u32(1) + u8(1) + u32(5)
    out += u32(1) + u8(1)
    if version < 0x6F:
        out += mem16(b"event\0")
    else:
        out += mem32(b"event\0")
    return out


def part4(version):
    out = part1_1(version)
    out += u32(1) + part1_1(version)
    out += u8(1, 2)
    out += u32(1) + u32(77)
    if version >= 0x8A:
        out += u32(1) + u64(123456789)
    return out


def part5_1(version):
    out = u32(1)
    out += u8(1, 2) + u16(3) + u8(4, 5)
    out += mem16(b"sys\0")
    out += u32(6, 7)
    out += u32(*range(8, 16))
    out += u32(*range(16, 22))
    if version >= 0x69:
        out += u32(*range(22, 40))
    if version >= 0x6B:
        out += u8(40)
    if version >= 0x72:
        out += u8(41, 42) + u32(*range(43, 51))
    if version >= 0x73:
        out += u32(51, 52, 53, 54)
        if version >= 0x74:
            out += u32(55, 56, 57, 58, 59)
        if version >= 0x75:
            out += u32(60, 61, 62, 63, 64, 65)
        out += u32(66, 67, 68, 69, 70, 71)
    if version >= 0x76:
        out += u32(72, 73, 74)
    if version >= 0x81:
        out += u32(*range(75, 96))
        if version >= 0x87:
            out += u32(96, 97, 98, 99, 100)
        if version >= 0x89:
            out += u32(1) + u32(2) + u32(101, 102) + u32(103, 104, 105, 106, 107, 108, 109)
    return out


def part5(version):
    return u16(1) + part5_1(version)


def variable_db():
    out = u8(0)
    out += u32(2)
    # Type 0: no field configuration, one empty record.
    out += i32(1) + u32(1)
    out += i32(-1) + u32(3) + u32(1, 2000, 2) + u32(2)
    for name, first, third in (("Potion", 1234, 5678), ("Elixir", 99, 3)):
        out += i32(first, third)
        out += text32(name)
    return out


def part7():
    return u8(1) + u32(2) + u8(5, 7) + u8(0xFB) + u32(1234)


def make(version, utf8=True, name="Synthetic Wolf Save"):
    body = u8(0x19)
    body += text16(name)
    body += u16(version)
    body += part1(version)
    body += part2(version)
    body += part3(version)
    body += part4(version)
    body += part5(version)
    body += variable_db()
    body += part7()
    body += u8(0x19)

    header = bytearray(0x14)
    header[6] = 0x55 if utf8 else 0x00
    data = bytes(header) + body
    checksum = sum(data[0x14:]) & 0xFF
    data = data[:2] + bytes([checksum]) + data[3:]
    return data


def main():
    root = Path(__file__).resolve().parents[3]
    out_dir = root / "fixtures" / "synthetic" / "wolf"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "SaveData01.sav").write_bytes(make(0x8E, True, "Synthetic Wolf Save"))
    (out_dir / "SaveData02.sav").write_bytes(make(0x63, False, "Old Wolf Save"))
    print(f"wrote {out_dir}")


if __name__ == "__main__":
    main()
