#!/usr/bin/env python3
"""Write synthetic SCS `.sii` fixtures from the public SCS Modding Wiki "Units" page.

All units and values are invented. `binary.bsii` carries only the public `BSII` magic and a
little-endian version (the binary body is intentionally opaque and read-only).
"""

from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "fixtures" / "synthetic" / "scs-sii"

PLAIN = """// SavePrism synthetic SII save; invented data
/* a block comment */
SiiNunit
{
player : .data.player
{
    # a line comment
    name: "Mira"
    money: 1200
    speed: 12.5
    active: true
    home: .garage.main
    color: (1.0, 0.5, 0.25)
    note: nil
    empty: ""
    array_test[0]: 1
    array_test[1]: 2
    @include "other.sui"
}

other : .data.other
{
    name: "Ash"
    payload: &3f800000
}
}
"""


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "plain.sii").write_bytes(PLAIN.encode("utf-8"))
    payload = b"\x01\x02\x03\x04binary"
    (OUT / "binary.bsii").write_bytes(b"BSII" + struct.pack("<I", 2) + payload)


if __name__ == "__main__":
    main()
