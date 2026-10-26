#!/usr/bin/env python3
"""Writes the synthetic Emuera 1.808+ binary fixtures.

The layout follows the behavioral spec in `crates/saveprism-codecs/src/emuera/mod.rs`, which was
written from the public Emuera sources (behavior only). This generator is an independent
implementation of the same format, so it also checks the codec's writer against a second writer.
Every value is invented.
"""

import struct
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "synthetic" / "emuera"
HEADER = bytes([0x89, 0x45, 0x52, 0x41, 0x0D, 0x0A, 0x1A, 0x0A])
VERSION = 1808

TYPE_INT = 0x00
TYPE_INT_ARRAY = 0x01
TYPE_INT_ARRAY_2D = 0x02
TYPE_INT_ARRAY_3D = 0x03
TYPE_STR = 0x10
TYPE_STR_ARRAY = 0x11
TYPE_STR_ARRAY_2D = 0x12
TYPE_STR_ARRAY_3D = 0x13
SEPARATOR = 0xFD
EOC = 0xFE
EOF = 0xFF

BYTE = 0xCF
I16 = 0xD0
I32 = 0xD1
I64 = 0xD2
STRING = 0xD8
EOA1 = 0xE0
EOA2 = 0xE1
ZERO = 0xF0
ZERO_A1 = 0xF1
ZERO_A2 = 0xF2


def u32(value):
    return struct.pack("<I", value)


def i32(value):
    return struct.pack("<i", value)


def i64(value):
    return struct.pack("<q", value)


def net_string(text):
    data = text.encode("utf-16-le")
    length = len(data)
    out = bytearray()
    while True:
        byte = length & 0x7F
        length >>= 7
        if length:
            byte |= 0x80
        out.append(byte)
        if not length:
            break
    return bytes(out) + data


def put_int(value):
    if 0 <= value <= BYTE:
        return bytes([value])
    if -32768 <= value <= 32767:
        return bytes([I16]) + struct.pack("<h", value)
    if -(2**31) <= value <= 2**31 - 1:
        return bytes([I32]) + i32(value)
    return bytes([I64]) + i64(value)


def int_array(values):
    out = bytearray(i32(len(values)))
    zeros = 0
    for value in values:
        if value == 0:
            zeros += 1
        else:
            if zeros:
                out += bytes([ZERO]) + put_int(zeros)
                zeros = 0
            out += put_int(value)
    out += bytes([EOF])
    return bytes(out)


def int_array_2d(rows):
    length0 = len(rows)
    length1 = len(rows[0]) if rows else 0
    out = bytearray(i32(length0) + i32(length1))
    zero_count = 0
    zero_rows = 0
    for row in rows:
        assert len(row) == length1
        for value in row:
            if value == 0:
                zero_count += 1
            else:
                if zero_rows:
                    out += bytes([ZERO_A1]) + put_int(zero_rows)
                    zero_rows = 0
                if zero_count:
                    out += bytes([ZERO]) + put_int(zero_count)
                    zero_count = 0
                out += put_int(value)
        if zero_count == length1:
            zero_rows += 1
        else:
            out.append(EOA1)
        zero_count = 0
    out += bytes([EOF])
    return bytes(out)


def int_array_3d(planes):
    length0 = len(planes)
    length1 = len(planes[0]) if planes else 0
    length2 = len(planes[0][0]) if length1 else 0
    out = bytearray(i32(length0) + i32(length1) + i32(length2))
    zero = 0
    zero_rows = 0
    zero_planes = 0
    for plane in planes:
        assert len(plane) == length1
        for row in plane:
            assert len(row) == length2
            for value in row:
                if value == 0:
                    zero += 1
                else:
                    if zero_planes:
                        out += bytes([ZERO_A2]) + put_int(zero_planes)
                        zero_planes = 0
                    if zero_rows:
                        out += bytes([ZERO_A1]) + put_int(zero_rows)
                        zero_rows = 0
                    if zero:
                        out += bytes([ZERO]) + put_int(zero)
                        zero = 0
                    out += put_int(value)
            if zero == length2:
                zero_rows += 1
            else:
                out.append(EOA1)
            zero = 0
        if zero_rows == length1:
            zero_planes += 1
        else:
            out.append(EOA2)
        zero_rows = 0
    out += bytes([EOF])
    return bytes(out)


def string_array(values):
    out = bytearray(i32(len(values)))
    zeros = 0
    for value in values:
        if not value:
            zeros += 1
        else:
            if zeros:
                out += bytes([ZERO]) + put_int(zeros)
                zeros = 0
            out += bytes([STRING]) + net_string(value)
    out += bytes([EOF])
    return bytes(out)


def string_array_2d(rows):
    length0 = len(rows)
    length1 = len(rows[0]) if rows else 0
    out = bytearray(i32(length0) + i32(length1))
    zero_count = 0
    zero_rows = 0
    for row in rows:
        assert len(row) == length1
        for value in row:
            if not value:
                zero_count += 1
            else:
                if zero_rows:
                    out += bytes([ZERO_A1]) + put_int(zero_rows)
                    zero_rows = 0
                if zero_count:
                    out += bytes([ZERO]) + put_int(zero_count)
                    zero_count = 0
                out += bytes([STRING]) + net_string(value)
        if zero_count == length1:
            zero_rows += 1
        else:
            out.append(EOA1)
        zero_count = 0
    out += bytes([EOF])
    return bytes(out)


def string_array_3d(planes):
    length0 = len(planes)
    length1 = len(planes[0]) if planes else 0
    length2 = len(planes[0][0]) if length1 else 0
    out = bytearray(i32(length0) + i32(length1) + i32(length2))
    zero = 0
    zero_rows = 0
    zero_planes = 0
    for plane in planes:
        assert len(plane) == length1
        for row in plane:
            assert len(row) == length2
            for value in row:
                if not value:
                    zero += 1
                else:
                    if zero_planes:
                        out += bytes([ZERO_A2]) + put_int(zero_planes)
                        zero_planes = 0
                    if zero_rows:
                        out += bytes([ZERO_A1]) + put_int(zero_rows)
                        zero_rows = 0
                    if zero:
                        out += bytes([ZERO]) + put_int(zero)
                        zero = 0
                    out += bytes([STRING]) + net_string(value)
            if zero == length2:
                zero_rows += 1
            else:
                out.append(EOA1)
            zero = 0
        if zero_rows == length1:
            zero_planes += 1
        else:
            out.append(EOA2)
        zero_rows = 0
    out += bytes([EOF])
    return bytes(out)


def record(kind, key, payload):
    return bytes([kind]) + net_string(key) + payload


def int_var(key, value):
    return record(TYPE_INT, key, put_int(value))


def str_var(key, value):
    return record(TYPE_STR, key, net_string(value))


def int_array_var(key, values):
    return record(TYPE_INT_ARRAY, key, int_array(values))


def int_array_2d_var(key, rows):
    return record(TYPE_INT_ARRAY_2D, key, int_array_2d(rows))


def int_array_3d_var(key, planes):
    return record(TYPE_INT_ARRAY_3D, key, int_array_3d(planes))


def str_array_var(key, values):
    return record(TYPE_STR_ARRAY, key, string_array(values))


def str_array_2d_var(key, rows):
    return record(TYPE_STR_ARRAY_2D, key, string_array_2d(rows))


def str_array_3d_var(key, planes):
    return record(TYPE_STR_ARRAY_3D, key, string_array_3d(planes))


def header(file_type, game_code, game_version, description):
    return (
        HEADER
        + u32(VERSION)
        + u32(0)
        + bytes([file_type])
        + i64(game_code)
        + i64(game_version)
        + net_string(description)
    )


def character(index):
    out = bytearray()
    out += int_var("NO", index)
    out += str_var("NAME", "主人公" if index == 0 else "ヒロイン")
    out += str_var("CALLNAME", "あなた" if index == 0 else "彼女")
    out += str_var("NICKNAME", "")
    out += str_var("MASTERNAME", "")
    out += int_array_var("BASE", [10 + index, 20, 30, 40, 50, 60])
    out += int_array_var("MAXBASE", [99, 88, 77, 66, 55, 44])
    out += int_array_var("ABL", [1, 0, 3, 0, 0, 2])
    out += int_array_var("TALENT", [0, 1, 0, 0])
    out += int_array_var("EXP", [0, 0, 5, 6, 7])
    out += int_array_var("MARK", [0, 0, 0, 9])
    out += int_array_var("CFLAG", [0, 100, 0, 0, 0, 0])
    out += str_array_var("CSTR", ["", "invented", "", "first"])
    out += int_array_2d_var("CDFLAG", [[1, 2, 3], [0, 5, 6]])
    out += bytes([SEPARATOR])
    out += int_array_var("MYVAR", [7, 0, 9])
    out += bytes([EOC])
    return bytes(out)


def normal_save():
    out = bytearray(header(0, 99887766, 12, "chapter one"))
    out += i64(2)
    out += character(0)
    out += character(1)
    out += int_array_var("FLAG", [0, 0, 1, 0, 0, 3])
    out += int_array_var("TFLAG", [0, 4])
    out += int_var("DAY", 12)
    out += int_var("MONEY", 123456)
    out += int_var("NEGATIVE", -80000)
    out += int_var("BIG", 2**40)
    out += int_array_var("ITEM", [0, 0, 2, 0])
    out += str_array_var("STR", ["", "エラ", "", "放課後"])
    out += str_var("SAVEDATA_TEXT", "2026/10/26 12:00:00 ")
    out += int_array_2d_var("DA", [[1, 0, 3], [0, 0, 0], [4, 5, 0]])
    out += int_array_3d_var(
        "TA",
        [
            [[1, 0, 3], [0, 0, 0], [4, 5, 6]],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
            [[7, 0, 0], [0, 8, 0], [0, 0, 9]],
        ],
    )
    out += str_array_2d_var("SA", [["a", "", "c"], ["", "", ""], ["", "e", ""]])
    out += str_array_3d_var(
        "SB",
        [
            [["x", "", ""], ["", "", ""], ["", "", "z"]],
            [["", "", ""], ["", "", ""], ["", "", ""]],
        ],
    )
    out += bytes([EOF])
    (OUT / "save00.sav").write_bytes(bytes(out))

    minimal = bytearray(header(0, 99887766, 1, ""))
    minimal += i64(0)
    minimal += int_var("MONEY", 3)
    minimal += str_var("NAME", "nobody")
    minimal += bytes([EOF])
    (OUT / "save01.sav").write_bytes(bytes(minimal))


def global_save():
    out = bytearray(header(1, 99887766, 12, "global values"))
    out += int_array_var("GLOBAL", [0, 1, 0, 0, 5])
    out += str_array_var("GLOBALS", ["", "invented", "", "グローバル"])
    out += int_array_var("RANDDATA", [1, 2, 3, 4])
    out += bytes([EOF])
    (OUT / "global.sav").write_bytes(bytes(out))


def variable_save():
    out = bytearray(header(2, 99887766, 12, "saved variables"))
    out += int_var("X", 4)
    out += int_array_var("Y", [0, 0, 2, 3])
    out += str_var("Z", "テキスト")
    out += bytes([EOF])
    (OUT / "var_01.dat").write_bytes(bytes(out))


def character_save():
    out = bytearray(header(3, 99887766, 12, "saved characters"))
    out += i64(1)
    out += character(1)
    out += bytes([EOF])
    (OUT / "chara_00.dat").write_bytes(bytes(out))


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    normal_save()
    global_save()
    variable_save()
    character_save()


if __name__ == "__main__":
    main()
