#!/usr/bin/env python3
"""Write synthetic BioWare fixtures: a GFF V3.2 character and module, an ERF save holding
both GFFs (KOTOR style), and a GFF V4.0 Dragon Age: Origins-style save.

Everything is built from BioWare's public format documents (the GFF, ERF and Key/BIF PDFs
shipped with the Neverwinter Nights toolset, mirrored on nwn.wiki) and the public Dragon Age
toolset wiki "GFF" page. Values are invented; no game file or player data is used.

Usage: python fixtures/tools/bioware/make_bioware.py
"""

from __future__ import annotations

import pathlib
import struct

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = ROOT / "synthetic" / "bioware"

BYTE, CHAR, WORD, SHORT, DWORD, INT = 0, 1, 2, 3, 4, 5
DWORD64, INT64, FLOAT, DOUBLE = 6, 7, 8, 9
CEXOSTRING, RESREF, CEXOLOCSTRING, VOID, STRUCT, LIST = 10, 11, 12, 13, 14, 15


def u16(value: int) -> bytes:
    return struct.pack("<H", value)


def u32(value: int) -> bytes:
    return struct.pack("<I", value)


def i32(value: int) -> bytes:
    return struct.pack("<i", value)


def u64(value: int) -> bytes:
    return struct.pack("<Q", value)


def i64(value: int) -> bytes:
    return struct.pack("<q", value)


def f32(value: float) -> bytes:
    return struct.pack("<f", value)


def f64(value: float) -> bytes:
    return struct.pack("<d", value)


class Gff3:
    """A GFF V3.2 builder. `build(root_fields)` writes the whole file."""

    def __init__(self, file_type: str) -> None:
        self.file_type = file_type.encode("ascii")
        assert len(self.file_type) == 4
        self.structs: list[dict] = []
        self.fields: list[list[int]] = []
        self.labels: list[bytes] = []
        self.label_ids: dict[bytes, int] = {}
        self.data = bytearray()
        self.field_indices = bytearray()
        self.list_indices = bytearray()

    def _label(self, text: str) -> int:
        raw = text.encode("ascii")
        assert 0 < len(raw) <= 16, text
        if raw not in self.label_ids:
            self.label_ids[raw] = len(self.labels)
            self.labels.append(raw)
        return self.label_ids[raw]

    def _data(self, payload: bytes) -> int:
        offset = len(self.data)
        self.data += payload
        return offset

    def _struct(self, kind: int, fields: list[tuple]) -> int:
        st = {"kind": kind, "indices": [], "data_or": 0}
        index = len(self.structs)
        self.structs.append(st)
        for entry in fields:
            text, type_id, value = entry
            self._field(st, text, type_id, value)
        return index

    def _field(self, st: dict, text: str, type_id: int, value: object) -> None:
        index = len(self.fields)
        self.fields.append([type_id, self._label(text), 0])
        st["indices"].append(index)

        def direct(raw: int) -> None:
            self.fields[index][2] = raw & 0xFFFFFFFF

        def indirect(payload: bytes) -> None:
            self.fields[index][2] = self._data(payload)

        if type_id in (BYTE, CHAR, WORD, SHORT):
            direct(int(value))
        elif type_id in (DWORD, INT):
            direct(int(value))
        elif type_id == FLOAT:
            direct(struct.unpack("<I", f32(float(value)))[0])
        elif type_id == DWORD64:
            indirect(u64(int(value)))
        elif type_id == INT64:
            indirect(i64(int(value)))
        elif type_id == DOUBLE:
            indirect(f64(float(value)))
        elif type_id == CEXOSTRING:
            raw = str(value).encode("latin-1")
            indirect(u32(len(raw)) + raw)
        elif type_id == RESREF:
            raw = str(value).encode("latin-1")
            assert len(raw) <= 16
            indirect(bytes([len(raw)]) + raw)
        elif type_id == CEXOLOCSTRING:
            strref, strings = value
            body = u32(strref & 0xFFFFFFFF) + u32(len(strings))
            for language_id, text_value in strings:
                raw = text_value.encode("latin-1")
                body += u32(language_id) + u32(len(raw)) + raw
            indirect(u32(len(body)) + body)
        elif type_id == VOID:
            raw = bytes(value)
            indirect(u32(len(raw)) + raw)
        elif type_id == STRUCT:
            direct(self._struct(0, value))
        elif type_id == LIST:
            children = [self._struct(0, item) for item in value]
            offset = len(self.list_indices)
            self.list_indices += u32(len(children))
            for child in children:
                self.list_indices += u32(child)
            direct(offset)
        else:
            raise ValueError(type_id)

    def build(self, root_fields: list[tuple]) -> bytes:
        self._struct(0xFFFFFFFF, root_fields)
        for st in self.structs:
            if len(st["indices"]) == 1:
                st["data_or"] = st["indices"][0]
            elif not st["indices"]:
                st["data_or"] = 0
            else:
                offset = len(self.field_indices)
                for field_index in st["indices"]:
                    self.field_indices += u32(field_index)
                st["data_or"] = offset
        struct_offset = 56
        field_offset = struct_offset + len(self.structs) * 12
        label_offset = field_offset + len(self.fields) * 12
        data_offset = label_offset + len(self.labels) * 16
        field_indices_offset = data_offset + len(self.data)
        list_indices_offset = field_indices_offset + len(self.field_indices)
        out = bytearray(self.file_type + b"V3.2")
        out += u32(struct_offset) + u32(len(self.structs))
        out += u32(field_offset) + u32(len(self.fields))
        out += u32(label_offset) + u32(len(self.labels))
        out += u32(data_offset) + u32(len(self.data))
        out += u32(field_indices_offset) + u32(len(self.field_indices))
        out += u32(list_indices_offset) + u32(len(self.list_indices))
        assert len(out) == 56
        for st in self.structs:
            out += u32(st["kind"]) + u32(st["data_or"]) + u32(len(st["indices"]))
        for type_id, label, value in self.fields:
            out += u32(type_id) + u32(label) + u32(value)
        for text in self.labels:
            out += text + bytes(16 - len(text))
        out += self.data
        out += self.field_indices
        out += self.list_indices
        return bytes(out)


def character_bic() -> bytes:
    return Gff3("BIC ").build(
        [
            ("Tag", CEXOSTRING, "mira_rune"),
            ("FirstName", CEXOLOCSTRING, (-1, [(0, "Mira"), (2, "Mira")])),
            ("LastName", CEXOLOCSTRING, (-1, [(0, "Rune")])),
            ("Level", BYTE, 7),
            ("Alignment", CHAR, -5),
            ("HitPoints", SHORT, 88),
            ("MaxHP", WORD, 120),
            ("Gold", DWORD, 500),
            ("Experience", INT, 2500),
            ("UUID", DWORD64, 0x1122334455667788),
            ("Playtime", INT64, -12),
            ("Weight", FLOAT, 12.5),
            ("PosX", DOUBLE, 1.5),
            ("Portrait", RESREF, "po_mira"),
            ("Blob", VOID, b"SYNTHETIC"),
            (
                "Stats",
                STRUCT,
                [
                    ("Str", BYTE, 14),
                    ("Dex", BYTE, 12),
                    ("Con", BYTE, 13),
                    ("Notes", CEXOSTRING, "Cloak of the north"),
                ],
            ),
            (
                "Inventory",
                LIST,
                [
                    [
                        ("Name", CEXOSTRING, "Potion of Clarity"),
                        ("Count", DWORD, 3),
                        ("Weight", FLOAT, 0.5),
                    ],
                    [
                        ("Name", CEXOSTRING, "Iron Key"),
                        ("Count", DWORD, 1),
                        ("Weight", FLOAT, 0.1),
                    ],
                ],
            ),
        ]
    )


def module_ifo() -> bytes:
    return Gff3("IFO ").build(
        [
            ("ModName", CEXOSTRING, "The Ashen Vale"),
            ("ModID", CEXOSTRING, "ashen_vale"),
            ("MinGameVersion", DWORD, 1),
            ("ExpansionPack", DWORD, 2),
        ]
    )


def erf_save(resources: list[tuple[bytes, int, bytes]]) -> bytes:
    ordered = sorted(resources, key=lambda item: item[0])
    count = len(ordered)
    description = b"Invented save\x00"
    localized = u32(0) + u32(len(description)) + description
    localized_offset = 160
    key_offset = localized_offset + len(localized)
    resource_offset = key_offset + count * 24
    data_offset = resource_offset + count * 8
    header = bytearray(b"SAV " + b"V1.0")
    header += u32(1) + u32(len(localized)) + u32(count)
    header += u32(localized_offset) + u32(key_offset) + u32(resource_offset)
    header += u32(126) + u32(298) + u32(0)
    header += bytes(116)
    assert len(header) == 160
    keys = bytearray()
    for index, (name, res_type, _payload) in enumerate(ordered):
        assert len(name) <= 16
        keys += name + bytes(16 - len(name)) + u32(index) + u16(res_type) + u16(0)
    table = bytearray()
    data = bytearray()
    for _name, _res_type, payload in ordered:
        table += u32(data_offset + len(data)) + u32(len(payload))
        data += payload
    return bytes(header) + localized + bytes(keys) + bytes(table) + bytes(data)


def gff4_save() -> bytes:
    # Header fields 1-5 are always big endian; the rest follow the target platform.
    text = "Dragon Age"
    ecd = u32(len(text)) + text.encode("utf-16-le")
    chunk = bytearray()
    chunk += u32(4242)  # UINT32 at 0
    chunk += f32(1.25)  # FLOAT32 at 4
    string_at = len(chunk)
    chunk += ecd  # ECString at 8
    vector_at = len(chunk)
    chunk += f32(1.0) + f32(2.0) + f32(3.0)  # Vector3f at 32
    list_at = len(chunk)
    chunk += u32(3) + u32(7) + u32(11) + u32(13)  # list of UINT32 at 44
    fields = [
        (0x00000001, 4, 0),
        (0x00000002, 8, 4),
        (0x00000003, 14, string_at),
        (0x00000004, 10, vector_at),
        (0x00000005, 0x80000004, list_at),
    ]
    header_len = 28
    structs_len = 16
    field_offset = header_len + structs_len
    data_offset = field_offset + len(fields) * 12
    header = bytearray(b"GFF " + b"V4.0" + b"PC  " + b"SAV " + b"V1.0")
    header += u32(structs_len // 16) + u32(data_offset)
    assert len(header) == header_len
    struct_entry = u32(0xFFFFFFFF) + u32(len(fields)) + u32(field_offset) + u32(len(chunk))
    field_entries = bytearray()
    for label, kind, index in fields:
        field_entries += u32(label) + u32(kind) + u32(index)
    return bytes(header) + struct_entry + bytes(field_entries) + bytes(chunk)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    character = character_bic()
    module = module_ifo()
    (OUT / "character.bic").write_bytes(character)
    (OUT / "module.ifo").write_bytes(module)
    (OUT / "kotor-save.sav").write_bytes(
        erf_save([(b"character", 2015, character), (b"module", 2014, module)])
    )
    (OUT / "dao-save.das").write_bytes(gff4_save())


if __name__ == "__main__":
    main()
