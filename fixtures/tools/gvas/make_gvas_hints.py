"""Build the synthetic GVAS fixtures for struct type hint inference.

UE 4.27 GVAS layout as read by `uesave`: a fixed header, the save game class
name, a `None`-terminated property list, and four trailing zero bytes.  Values
inside a MapProperty or SetProperty that are `StructProperty` carry no struct
type in the tag, so a reader needs a type hint.  These files are invented; no
real save bytes are copied.

Run from the repository root:

    python fixtures/tools/gvas/make_gvas_hints.py
"""

from pathlib import Path
import struct

OUT = Path(__file__).resolve().parents[3] / "fixtures" / "synthetic" / "gvas-hints"

GUID_ZERO = b"\x00" * 16


def u32(value):
    return struct.pack("<I", value)


def u16(value):
    return struct.pack("<H", value)


def i32(value):
    return struct.pack("<i", value)


def text(value):
    raw = value.encode("ascii")
    return i32(len(raw) + 1) + raw + b"\x00"


def tag(name, kind, data, extra=b""):
    """One property tag followed by its data."""
    return (
        text(name)
        + text(kind)
        + u32(len(data))
        + u32(0)
        + extra
        + b"\x00"
        + data
    )


def struct_tag(name, kind, data):
    return tag(name, kind, data)


def props(*tags):
    return b"".join(tags) + text("None")


def header(root_class):
    return (
        b"GVAS"
        + u32(2)
        + u32(522)
        + u16(4)
        + u16(27)
        + u16(0)
        + u32(0)
        + text("++UE4+Release-4.27")
        + u32(3)
        + u32(0)
        + text(root_class)
    )


def save(properties):
    return header("/Script/Test.TestSave") + properties + b"\x00\x00\x00\x00"


def map_tag(name, key_type, value_type, data):
    return tag(name, "MapProperty", data, text(key_type) + text(value_type))


def set_tag(name, key_type, data):
    return tag(name, "SetProperty", data, text(key_type))


def struct_value(**fields):
    """A property-list struct value (the `Struct(None)` shape)."""
    out = b""
    for name, (kind, data) in fields.items():
        out += tag(name, kind, data)
    return out + text("None")


def name_prop(value):
    return ("NameProperty", text(value))


def str_prop(value):
    return ("StrProperty", text(value))


def int_prop(value):
    return ("IntProperty", i32(value))


def map_data(entries):
    out = u32(0) + u32(len(entries))
    for key, value in entries:
        out += key + value
    return out


def set_data(keys):
    out = u32(0) + u32(len(keys))
    for key in keys:
        out += key
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)

    # A map whose keys are property-list structs (the default hint would be Guid)
    # and whose values are property-list structs (the default is already right).
    keys_values = map_tag(
        "Scores",
        "StructProperty",
        "StructProperty",
        map_data(
            [
                (
                    struct_value(Name=name_prop("alpha")),
                    struct_value(Points=int_prop(11)),
                ),
                (
                    struct_value(Name=name_prop("beta")),
                    struct_value(Points=int_prop(7)),
                ),
            ]
        ),
    )
    (OUT / "map-struct-keys.sav").write_bytes(save(props(keys_values)))

    # A set of property-list structs (the default hint would be Guid).
    tags = set_tag(
        "Tags",
        "StructProperty",
        set_data(
            [
                struct_value(Tag=str_prop("fire"), Weight=int_prop(3)),
                struct_value(Tag=str_prop("ice"), Weight=int_prop(5)),
            ]
        ),
    )
    (OUT / "set-structs.sav").write_bytes(save(props(tags)))

    # A map with integer keys and Guid values: the default value hint is a
    # property-list struct, so this needs `Ids.Value` = "Guid".
    ids = map_tag(
        "Ids",
        "IntProperty",
        "StructProperty",
        map_data(
            [
                (i32(1), bytes(range(16))),
                (i32(2), bytes(range(16, 32))),
            ]
        ),
    )
    (OUT / "map-guid-values.sav").write_bytes(save(props(ids)))

    # A map whose struct key is four bytes that no known struct type consumes:
    # no hint can give an exact round trip.
    broken = map_tag(
        "Broken",
        "StructProperty",
        "IntProperty",
        map_data([(b"\xde\xad\xbe\xef", i32(1))]),
    )
    (OUT / "map-unresolvable.sav").write_bytes(save(props(broken)))


if __name__ == "__main__":
    main()
