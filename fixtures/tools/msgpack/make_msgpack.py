"""Writes synthetic MessagePack fixtures from the public MessagePack specification."""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "msgpack"


def pack(value):
    if value is None:
        return b"\xc0"
    if value is True:
        return b"\xc3"
    if value is False:
        return b"\xc2"
    if isinstance(value, int):
        if 0 <= value <= 0x7F:
            return bytes([value])
        if -32 <= value < 0:
            return bytes([value & 0xFF])
        if value >= 0:
            if value <= 0xFF:
                return b"\xcc" + bytes([value])
            if value <= 0xFFFF:
                return b"\xcd" + struct.pack(">H", value)
            if value <= 0xFFFFFFFF:
                return b"\xce" + struct.pack(">I", value)
            return b"\xcf" + struct.pack(">Q", value)
        if value >= -0x80:
            return b"\xd0" + struct.pack(">b", value)
        if value >= -0x8000:
            return b"\xd1" + struct.pack(">h", value)
        if value >= -0x80000000:
            return b"\xd2" + struct.pack(">i", value)
        return b"\xd3" + struct.pack(">q", value)
    if isinstance(value, float):
        return b"\xcb" + struct.pack(">d", value)
    if isinstance(value, str):
        return pack_str(value.encode("utf-8"))
    if isinstance(value, bytes):
        return b"\xc6" + struct.pack(">I", len(value)) + value
    if isinstance(value, list):
        return pack_array(value)
    if isinstance(value, dict):
        return pack_map(value)
    raise TypeError(type(value))


def pack_str(data):
    if len(data) <= 31:
        return bytes([0xA0 | len(data)]) + data
    return b"\xda" + struct.pack(">H", len(data)) + data


def pack_array(items):
    return b"\xdd" + struct.pack(">I", len(items)) + b"".join(pack(item) for item in items)


def pack_map(items):
    out = b"\xdf" + struct.pack(">I", len(items))
    for key, value in items.items():
        out += pack(key) + pack(value)
    return out


def wide_int(value):
    """A non-shortest uint32 encoding, to check that untouched widths stay untouched."""
    return b"\xce" + struct.pack(">I", value)


def timestamp(seconds):
    return b"\xd6\xff" + struct.pack(">I", seconds)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    save = {
        "name": "Ash",
        "level": 7,
        "health": 92.5,
        "alive": True,
        "nickname": None,
        "inventory": ["potion", "rope", "lamp"],
        "stats": {"strength": 12, "agility": -3, "luck": 0.25},
        "photo": bytes(range(8)),
        "seed": 1 << 63,
    }
    (OUT / "player.msgpack").write_bytes(pack(save))

    # Non-shortest headers and every width, plus an application extension.
    body = b"\x85"
    body += pack("fix") + b"\x01"
    body += pack("uint32-wide") + wide_int(1)
    body += pack("float32") + b"\xca" + struct.pack(">f", 1.5)
    body += pack("str16") + b"\xda" + struct.pack(">H", 3) + b"abc"
    body += pack("created") + timestamp(1_700_000_000)
    (OUT / "widths.msgpack").write_bytes(body)

    empty = b"\x90"
    (OUT / "empty-array.msgpack").write_bytes(empty)


if __name__ == "__main__":
    main()
