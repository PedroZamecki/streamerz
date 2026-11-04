#!/usr/bin/env python3
"""Check that the Windows executable's default icon is the shipped Streamerz icon."""
import struct
import sys
from pathlib import Path


def u16(data, offset):
    return struct.unpack_from('<H', data, offset)[0]


def u32(data, offset):
    return struct.unpack_from('<I', data, offset)[0]


def check(exe_path, ico_path):
    exe = Path(exe_path).read_bytes()
    ico = Path(ico_path).read_bytes()
    if exe[:2] != b'MZ' or ico[:4] != b'\0\0\1\0':
        raise ValueError('invalid executable or icon file')
    pe = u32(exe, 0x3c)
    if exe[pe:pe + 4] != b'PE\0\0':
        raise ValueError('invalid PE signature')
    coff = pe + 4
    optional = coff + 20
    magic = u16(exe, optional)
    data_dir = optional + (112 if magic == 0x20b else 96 if magic == 0x10b else 0)
    if data_dir == optional:
        raise ValueError('unknown PE format')
    resource_rva = u32(exe, data_dir + 16)
    sections = optional + u16(exe, coff + 16)
    entries = []
    for n in range(u16(exe, coff + 2)):
        section = sections + n * 40
        size, rva, raw_size, raw = struct.unpack_from('<IIII', exe, section + 8)
        entries.append((rva, max(size, raw_size), raw))

    def offset(rva):
        for start, size, raw in entries:
            if start <= rva < start + size:
                return raw + rva - start
        raise ValueError('resource address is outside the executable')

    root = offset(resource_rva)

    def children(relative):
        start = root + relative
        count = u16(exe, start + 12) + u16(exe, start + 14)
        return [struct.unpack_from('<II', exe, start + 16 + i * 8) for i in range(count)]

    def lookup(relative, resource_id):
        for name, target in children(relative):
            if name == resource_id:
                return target
        raise ValueError(f'missing resource {resource_id}')

    def directory(target):
        if not target & 0x80000000:
            raise ValueError('expected resource directory')
        return target & 0x7fffffff

    def data(target):
        while target & 0x80000000:
            nodes = children(directory(target))
            if len(nodes) != 1:
                raise ValueError('expected one icon language')
            target = nodes[0][1]
        record = root + target
        pos, size = offset(u32(exe, record)), u32(exe, record + 4)
        return exe[pos:pos + size]

    icon_dir = directory(lookup(0, 3))
    groups = children(directory(lookup(0, 14)))
    if not groups or groups[0][0] != 1:
        raise ValueError('default icon group is not Streamerz icon group 1')
    group = data(groups[0][1])
    count = u16(ico, 4)
    if group[:4] != b'\0\0\1\0' or u16(group, 4) != count:
        raise ValueError('icon group does not match the source icon')
    for n in range(count):
        src = 6 + n * 16
        dst = 6 + n * 14
        if group[dst:dst + 12] != ico[src:src + 12]:
            raise ValueError(f'icon image {n} has different dimensions or size')
        size, image_offset = struct.unpack_from('<II', ico, src + 8)
        image_id = u16(group, dst + 12)
        if data(lookup(icon_dir, image_id)) != ico[image_offset:image_offset + size]:
            raise ValueError(f'icon image {n} differs from the source')
    print(f'Windows executable has the Streamerz default icon ({count} sizes).')


if __name__ == '__main__':
    try:
        check(sys.argv[1], 'assets/brand/streamerz.ico')
    except (OSError, IndexError, ValueError, struct.error) as exc:
        sys.exit(f'Windows icon check failed: {exc}')
