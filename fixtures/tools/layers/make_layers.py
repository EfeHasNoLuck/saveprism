#!/usr/bin/env python3
"""Write the synthetic lzip and Base64URL fixtures.

The lzip member is the public format from the Lzip manual: `LZIP`, version 1, a coded dictionary
size, a raw LZMA-302eos stream (taken from Python's LZMA-alone stream with its 13-byte header
removed) and the 20-byte trailer (CRC-32, data size, member size). The Base64URL text uses the
RFC 4648 section 5 alphabet without padding.
"""

from pathlib import Path
import base64
import lzma
import struct
import zlib

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "fixtures" / "synthetic" / "layers"


def main() -> None:
    plain = (FIXTURES / "save.json").read_bytes()

    alone = lzma.compress(plain, format=lzma.FORMAT_ALONE)
    props, dict_size = alone[0], struct.unpack("<I", alone[1:5])[0]
    assert props == 0x5D, props
    assert dict_size & (dict_size - 1) == 0, dict_size
    log = dict_size.bit_length() - 1
    assert 12 <= log <= 29, log
    stream = alone[13:]
    ds = log
    trailer = struct.pack("<I", zlib.crc32(plain))
    trailer += struct.pack("<Q", len(plain))
    member_size = 6 + len(stream) + 20
    trailer += struct.pack("<Q", member_size)
    (FIXTURES / "save.json.lz").write_bytes(b"LZIP" + bytes([1, ds]) + stream + trailer)

    (FIXTURES / "save.url.b64").write_bytes(
        base64.urlsafe_b64encode(
            b'{"player":{"name":"Ada","level":12,"gold":340},"day":3,"extra":">>"}'
        ).rstrip(b"=")
    )


if __name__ == "__main__":
    main()
