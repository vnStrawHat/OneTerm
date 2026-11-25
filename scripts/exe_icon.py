"""scripts/exe_icon.py <file.exe>: the icon groups of an executable's resources and the sizes of
their images, read from its PE headers; it fails when there is none (Quadra's specs/16, "An
executable's icon")."""
import struct
import sys


def resources(data):
    """The resource section: its bytes, and its virtual address."""
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    sections, optional = struct.unpack_from("<H", data, pe + 6)[0], struct.unpack_from("<H", data, pe + 20)[0]
    table = pe + 24 + optional
    for k in range(sections):
        at = table + 40 * k
        name = data[at:at + 8].rstrip(b"\0")
        size, address, raw_size, raw = struct.unpack_from("<IIII", data, at + 8)
        if name == b".rsrc":
            return data[raw:raw + raw_size], address
    return None, 0


def entries(rsrc, at):
    """The entries of a resource directory: (id, offset, is a directory)."""
    named, numbered = struct.unpack_from("<HH", rsrc, at + 12)
    out = []
    for k in range(named + numbered):
        ident, offset = struct.unpack_from("<II", rsrc, at + 16 + 8 * k)
        out.append((ident, offset & 0x7FFFFFFF, bool(offset & 0x80000000)))
    return out


def leaf(rsrc, address, at):
    """The data of the first language of a name."""
    while True:
        _, offset, deeper = entries(rsrc, at)[0]
        if not deeper:
            rva, size = struct.unpack_from("<II", rsrc, offset)
            return rsrc[rva - address:rva - address + size]
        at = offset


def main():
    data = open(sys.argv[1], "rb").read()
    rsrc, address = resources(data)
    groups = []
    if rsrc:
        for kind, offset, _ in entries(rsrc, 0):
            if kind != 14:  # RT_GROUP_ICON
                continue
            for name, at, _ in entries(rsrc, offset):
                group = leaf(rsrc, address, at)
                count = struct.unpack_from("<H", group, 4)[0]
                sizes = [group[6 + 14 * k] or 256 for k in range(count)]
                groups.append((name, sizes))
    for name, sizes in groups:
        print(f"icon group {name}: {' '.join(map(str, sizes))}")
    if not groups:
        print(f"exe_icon: no icon in {sys.argv[1]}", file=sys.stderr)
        sys.exit(1)


main()
