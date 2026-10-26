#!/usr/bin/env python3
"""Write synthetic emulator-save fixtures.

Snes9x: public `docs/snapshots.txt` (block name, six-digit length, data; `SRA` is save RAM).
RetroArch: `RASTATE`, version 1, then blocks of a 4-byte tag, a little-endian u32 length and the
payload padded to 8 bytes. `.srm` is raw battery save bytes.
"""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "emu"


def block(name: str, data: bytes) -> bytes:
    assert len(name) == 3
    return name.encode("ascii") + b":" + f"{len(data):06d}".encode("ascii") + b":" + data


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)

    snapshot = b"#!s9xsnp:0006\n"
    snapshot += block("NAM", b"test.smc\x00\x00")
    snapshot += block("CPU", bytes(range(8)))
    snapshot += block("SRA", bytes(range(16)))
    (OUT / "snapshot.000").write_bytes(snapshot)

    payload = bytes(range(16))
    state = b"RASTATE\x01"
    state += b"MEM " + struct.pack("<I", len(payload)) + payload
    state += b"END " + struct.pack("<I", 0)
    (OUT / "game.state").write_bytes(state)

    (OUT / "game.srm").write_bytes(bytes((i * 37) & 0xFF for i in range(4096)))


if __name__ == "__main__":
    main()
