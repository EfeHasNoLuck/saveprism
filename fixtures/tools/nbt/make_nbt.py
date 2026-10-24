"""Writes synthetic NBT fixtures from the public NBT format description.

Java edition uses big-endian numbers and 16-bit string lengths. Bedrock edition writes the same
tags with little-endian numbers and may put an 8-byte header (storage version, payload length)
in front of level.dat.
"""

from pathlib import Path
import gzip
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "nbt"


class Writer:
    def __init__(self, endian):
        self.endian = endian
        self.data = bytearray()

    def u8(self, value):
        self.data += struct.pack("B", value & 0xFF)

    def u16(self, value):
        self.data += struct.pack(self.endian + "H", value)

    def i16(self, value):
        self.data += struct.pack(self.endian + "h", value)

    def i32(self, value):
        self.data += struct.pack(self.endian + "i", value)

    def i64(self, value):
        self.data += struct.pack(self.endian + "q", value)

    def f32(self, value):
        self.data += struct.pack(self.endian + "f", value)

    def f64(self, value):
        self.data += struct.pack(self.endian + "d", value)

    def string(self, value):
        data = value.encode("utf-8")
        self.u16(len(data))
        self.data += data

    def named(self, tag, name, payload):
        self.u8(tag)
        self.string(name)
        self.data += payload


def scalar(writer, tag, name, value):
    payload = bytearray()
    if tag == 1:
        payload += struct.pack("b", value)
    elif tag == 2:
        payload += struct.pack(writer.endian + "h", value)
    elif tag == 3:
        payload += struct.pack(writer.endian + "i", value)
    elif tag == 4:
        payload += struct.pack(writer.endian + "q", value)
    elif tag == 5:
        payload += struct.pack(writer.endian + "f", value)
    elif tag == 6:
        payload += struct.pack(writer.endian + "d", value)
    writer.named(tag, name, payload)


def string_tag(writer, name, value):
    payload = bytearray()
    payload += struct.pack(writer.endian + "H", len(value.encode("utf-8")))
    payload += value.encode("utf-8")
    writer.named(8, name, payload)


def compound(writer, name, build):
    inner = Writer(writer.endian)
    build(inner)
    inner.u8(0)
    writer.named(10, name, inner.data)


def list_tag(writer, name, element, payloads):
    inner = bytearray()
    inner += struct.pack("B", element)
    inner += struct.pack(writer.endian + "i", len(payloads))
    inner += b"".join(payloads)
    writer.named(9, name, inner)


def int_payload(writer, value):
    return struct.pack(writer.endian + "i", value)


def compound_payload(writer, build):
    inner = Writer(writer.endian)
    build(inner)
    inner.u8(0)
    return inner.data


def int_array(writer, name, values):
    payload = bytearray(struct.pack(writer.endian + "i", len(values)))
    for value in values:
        payload += struct.pack(writer.endian + "i", value)
    writer.named(11, name, payload)


def long_array(writer, name, values):
    payload = bytearray(struct.pack(writer.endian + "i", len(values)))
    for value in values:
        payload += struct.pack(writer.endian + "q", value)
    writer.named(12, name, payload)


def byte_array(writer, name, values):
    payload = bytearray(struct.pack(writer.endian + "i", len(values)))
    payload += bytes(value & 0xFF for value in values)
    writer.named(7, name, payload)


def build_save(writer):
    string_tag(writer, "name", "Aster")
    scalar(writer, 1, "slot", 3)
    scalar(writer, 2, "shard", -1200)
    scalar(writer, 3, "level", 42)
    scalar(writer, 4, "seed", 9_000_000_000)
    scalar(writer, 5, "speed", 1.25)
    scalar(writer, 6, "position", -123.5)
    byte_array(writer, "flags", [1, 0, 127, -1])
    int_array(writer, "scores", [10, -20, 300000])
    long_array(writer, "times", [1, 2**40])
    list_tag(
        writer,
        "friends",
        10,
        [
            compound_payload(
                writer,
                lambda inner: (
                    string_tag(inner, "name", "Bram"),
                    scalar(inner, 3, "level", 12),
                ),
            ),
            compound_payload(
                writer,
                lambda inner: (
                    string_tag(inner, "name", "Cleo"),
                    scalar(inner, 3, "level", 15),
                ),
            ),
        ],
    )
    list_tag(writer, "empty", 0, [])
    compound(writer, "nested", lambda inner: string_tag(inner, "note", "hayır"))


def make(endian, name):
    writer = Writer(endian)
    writer.u8(10)
    writer.string("")
    build_save(writer)
    writer.u8(0)
    return bytes(writer.data)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    java = make(">", "save.nbt")
    (OUT / "save.nbt").write_bytes(java)
    (OUT / "save.nbt.gz").write_bytes(gzip.compress(java, mtime=0))

    bedrock = make("<", "bedrock.dat")
    (OUT / "bedrock.dat").write_bytes(bedrock)

    header = struct.pack("<ii", 9, len(bedrock))
    (OUT / "level.dat").write_bytes(header + bedrock)

    (OUT / "empty.nbt").write_bytes(b"\x0a\x00\x00\x00")


if __name__ == "__main__":
    main()
