#!/usr/bin/env python3
"""Write the synthetic Easy Save 3 fixtures with bare (unquoted) dictionary keys.

Easy Save 3 writes a dictionary whose key type is not a string as an object with bare keys, e.g.
`{0 : 159, 12 : 6}` or `{1:"2", 3:"4"}`. That is not valid JSON. The values are invented; no game
data or key list is used.

The encrypted fixture uses the layout documented in `crates/saveprism-codecs/src/es3/aes.rs`:
`iv[16] || AES-128-CBC(key, iv, PKCS7)(payload)` with
`key = PBKDF2-HMAC-SHA1("synthetic-test-key", salt = iv, 100 rounds, 16 bytes)`.
The IV is fixed so the file is reproducible.

Usage: python fixtures/tools/es3/make_es3.py
"""

from __future__ import annotations

import hashlib
import pathlib

OUT = pathlib.Path(__file__).resolve().parents[2] / "synthetic" / "es3"
KEYED = pathlib.Path(__file__).resolve().parents[2] / "keyed" / "es3-aes"
PASSWORD = "synthetic-test-key"

# Pretty: 4 space indent, a space on both sides of every colon. The dictionary keys are bare.
PRETTY = """{
    "playedMaps" : {
        "__type" : "System.Collections.Generic.Dictionary`2[[System.Int32, mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089],[System.Int32, mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089]],mscorlib",
        "value" : {
            0 : 159,
            12 : 6,
            2 : 1,
            -3 : 7
        }
    },
    "zoomLevels" : {
        "__type" : "dict[int, float]",
        "value" : {
            -1 : 0.5,
            1.5 : 2.25
        }
    }
}
"""

# Compact, with the `values` key the observed shape uses for some dictionaries.
COMPACT = (
    '{"settings": {"__type": "dict[int, int]", "value": {5:10, 6:20}},'
    ' "scores": {"__type": "dict[int, str]", "values": {1:"2", 3:"4"}}}'
)

BOOL_KEYS = '{"toggles": {"__type": "dict[bool, str]", "value": {true:"on", false:"off"}}}'

# A dictionary whose key type is a struct: Easy Save 3 writes the key as an object before the colon.
PRETTY_OBJECT_KEYS = """{
    "LocalPlayerOutfit" : {
        "__type" : "PlayerOutfit,Assembly-CSharp",
        "value" : {
            "Items" : {
                {
                    "low" : 16,
                    "high" : 0
                } : {
                    "guid" : {
                        "value" : "00000000-0000-0000-0000-000000000001"
                    },
                    "materialOption" : 0
                },
                {
                    "low" : 8,
                    "high" : 0
                } : {
                    "guid" : {
                        "value" : "00000000-0000-0000-0000-000000000002"
                    },
                    "materialOption" : 1
                }
            }
        }
    }
}
"""

# Compact, and one array key: Easy Save 3 accepts any value as a dictionary key.
COMPACT_OBJECT_KEYS = (
    '{"grid": {"__type": "dict[Vector2Int, int]",'
    ' "value": {{"x": 1, "y": 2}:10, [3, 4]:20}}}'
)


def aes_encrypt(payload: bytes, iv: bytes) -> bytes:
    from cryptography.hazmat.primitives import padding
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

    key = hashlib.pbkdf2_hmac("sha1", PASSWORD.encode("utf-8"), iv, 100, 16)
    padder = padding.PKCS7(128).padder()
    padded = padder.update(payload) + padder.finalize()
    encryptor = Cipher(algorithms.AES(key), modes.CBC(iv)).encryptor()
    cipher = encryptor.update(padded) + encryptor.finalize()
    return iv + cipher


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "dict-int-pretty.es3").write_bytes(PRETTY.encode("utf-8"))
    (OUT / "dict-int-compact.es3").write_bytes(COMPACT.encode("utf-8"))
    (OUT / "dict-bool.es3").write_bytes(BOOL_KEYS.encode("utf-8"))
    (OUT / "dict-object-key-pretty.es3").write_bytes(
        PRETTY_OBJECT_KEYS.encode("utf-8")
    )
    (OUT / "dict-object-key-compact.es3").write_bytes(
        COMPACT_OBJECT_KEYS.encode("utf-8")
    )
    # Encrypted fixtures live with the other password-protected files: detection cannot find a
    # password, so the golden corpus must not scan this one.
    KEYED.mkdir(parents=True, exist_ok=True)
    (KEYED / "dict-int-aes.es3").write_bytes(
        aes_encrypt(COMPACT.encode("utf-8"), bytes(range(32, 48)))
    )


if __name__ == "__main__":
    main()
