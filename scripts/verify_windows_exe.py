#!/usr/bin/env python3
"""Verify a Minty Launcher x64 PE executable and its embedded Windows icon."""
from __future__ import annotations

import hashlib
import struct
import sys
from pathlib import Path


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"PE verification failed: {message}")


def u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def main() -> None:
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != "--console"):
        raise SystemExit("Usage: python scripts/verify_windows_exe.py path/to/app.exe [--console]")

    path = Path(sys.argv[1])
    console = len(sys.argv) == 3
    require(path.is_file(), f"file not found: {path}")
    data = path.read_bytes()
    require(len(data) >= 1024, f"unexpectedly small executable ({len(data)} bytes)")
    require(data[:2] == b"MZ", "missing DOS MZ signature")

    pe_offset = u32(data, 0x3C)
    require(pe_offset + 24 <= len(data), "invalid PE header offset")
    require(data[pe_offset : pe_offset + 4] == b"PE\0\0", "missing PE signature")

    machine = u16(data, pe_offset + 4)
    section_count = u16(data, pe_offset + 6)
    optional_size = u16(data, pe_offset + 20)
    require(machine == 0x8664, f"expected AMD64/x64 PE, got 0x{machine:04x}")

    optional_offset = pe_offset + 24
    require(optional_offset + optional_size <= len(data), "truncated optional header")
    require(u16(data, optional_offset) == 0x20B, "expected PE32+ optional header")
    expected_subsystem = 3 if console else 2
    require(
        u16(data, optional_offset + 68) == expected_subsystem,
        "expected Windows console subsystem" if console else "expected Windows GUI subsystem",
    )

    if console:
        print(f"Valid Windows PE32+ x64 console executable: {path}")
        print(f"Size: {len(data):,} bytes")
        print(f"SHA-256: {hashlib.sha256(data).hexdigest()}")
        return

    resource_entry = optional_offset + 112 + 2 * 8
    require(resource_entry + 8 <= optional_offset + optional_size, "resource directory entry missing")
    resource_rva, resource_size = struct.unpack_from("<II", data, resource_entry)
    require(resource_rva != 0 and resource_size > 0, "PE has no resource directory")

    section_table = optional_offset + optional_size
    sections: list[tuple[int, int, int, int]] = []
    for index in range(section_count):
        offset = section_table + index * 40
        require(offset + 40 <= len(data), "truncated section table")
        virtual_size, virtual_address, raw_size, raw_pointer = struct.unpack_from("<IIII", data, offset + 8)
        sections.append((virtual_size, virtual_address, raw_size, raw_pointer))

    def rva_to_offset(rva: int) -> int:
        for virtual_size, virtual_address, raw_size, raw_pointer in sections:
            if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
                result = raw_pointer + (rva - virtual_address)
                require(result < len(data), "resource RVA maps outside the file")
                return result
        raise SystemExit("PE verification failed: resource RVA is not inside a section")

    resource_offset = rva_to_offset(resource_rva)
    require(resource_offset + 16 <= len(data), "truncated resource directory")
    _characteristics, _timestamp, _major, _minor, named_count, id_count = struct.unpack_from(
        "<IIHHHH", data, resource_offset
    )
    entry_count = named_count + id_count
    require(resource_offset + 16 + entry_count * 8 <= len(data), "truncated resource entries")

    has_group_icon = False
    for index in range(entry_count):
        name_or_id, _child = struct.unpack_from("<II", data, resource_offset + 16 + index * 8)
        if not (name_or_id & 0x80000000) and (name_or_id & 0xFFFF) == 14:
            has_group_icon = True
            break
    require(has_group_icon, "RT_GROUP_ICON resource (type 14) was not found")

    print(f"Valid Windows PE32+ x64 GUI executable: {path}")
    print("Embedded application icon resource: yes")
    print(f"Size: {len(data):,} bytes")
    print(f"SHA-256: {hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
