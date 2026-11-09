"""stfs.py - read-only lister/extractor for Xbox 360 content packages (STFS: LIVE / PIRS / CON).

Usage: python stfs.py info <package>
       python stfs.py list <package> [filter]
       python stfs.py extract <package> <outdir> [filter]      (filter = case-insensitive substring)

Format (public STFS documentation, Free60): header with display name (UTF-16BE at 0x411), header size
(u32 BE at 0x340) and the volume descriptor at 0x379 (block separation u8 at 0x37B, file-table block
count u16 LE at 0x37C, file-table first block u24 LE at 0x37E). Data blocks are 0x1000 bytes from the
first 4 KiB boundary after the header, with a hash table interleaved before every 0xAA data blocks (and
a level-1 table every 0x70E4); hash entries (0x18 bytes: SHA-1, status, next block u24 BE) chain the
blocks of fragmented files. Packages that are not read-only keep two copies of each hash table: the
live copy of the top table is named by block separation bit 0x02, that of a lower table by status bit 0x40
of its entry in the table above (the allocated block count, u32 BE at 0x395, gives the number of levels; as
Velocity reads them). File table entries are 0x40 bytes: name (0x28), flags (low 6 bits = name length,
0x40 = blocks consecutive, 0x80 = directory), block count u24 LE at 0x29, first block u24 LE at 0x2F, parent index u16 BE at 0x32 (0xFFFF = root), size u32 BE at 0x34.
"""
import os
import struct
import sys

BLOCK = 0x1000


class Package:
    def __init__(self, path):
        self.f = open(path, 'rb')
        h = self.f.read(0x1000)
        self.magic = h[:4]
        self.name = h[0x411:0x411 + 0x80].decode('utf-16-be', 'ignore').split('\0')[0]
        self.header_size = struct.unpack_from('>I', h, 0x340)[0]
        self.separation = h[0x37B]
        self.ft_blocks = struct.unpack_from('<H', h, 0x37C)[0]
        self.ft_start = int.from_bytes(h[0x37E:0x381], 'little')
        self.base = (self.header_size + 0xFFF) & ~0xFFF
        self.allocated = struct.unpack_from('>I', h, 0x395)[0]
        self.top_level = 0 if self.allocated <= 0xAA else 1 if self.allocated <= 0x70E4 else 2
        # read-only packages (separation bit 0 set) keep one hash table per level; others keep two
        self.shift = 0 if (self.separation & 1) else 1
        self.entries = self._file_table()

    def _fix(self, block):
        """Data block number -> raw block number: a level-0 hash table precedes every 0xAA data blocks
        (the first one included), a level-1 table every 0x70E4, a level-2 table after that."""
        n = (((block + 0xAA) // 0xAA) << self.shift) + block
        if block < 0xAA:
            return n
        if block < 0x70E4:
            return n + (((block + 0x70E4) // 0x70E4) << self.shift)
        return (1 << self.shift) + n + (((block + 0x70E4) // 0x70E4) << self.shift)

    def _addr(self, block):
        return self.base + self._fix(block) * BLOCK

    def _level0_raw(self, block):
        """Raw block of the first copy of the level-0 hash table holding a data block's entry."""
        if block < 0xAA:
            return 0
        raw = (block // 0xAA) * (0xAA + (1 << self.shift)) + (((block // 0x70E4) + 1) << self.shift)
        if block // 0x70E4:
            raw += 1 << self.shift
        return raw

    def _level1_raw(self, block):
        """Raw block of the first copy of the level-1 hash table above a data block's level-0 table."""
        if block < 0x70E4:
            return 0xAA + (1 << self.shift)
        raw = (block // 0x70E4) * (0x70E4 + (0xAB << self.shift)) + (((block // 0x4AF768) + 1) << self.shift)
        return raw if block // 0x4AF768 == 0 else raw + (1 << self.shift)

    def _status(self, raw, index):
        self.f.seek(self.base + raw * BLOCK + index * 0x18 + 0x14)
        return self.f.read(1) or bytes(1)

    def _hash_entry(self, block):
        """Level-0 hash entry of a data block, from the live copy of its table."""
        live = 0
        if self.shift:                                    # two copies of each table
            top_copy = 1 if self.separation & 2 else 0
            if self.top_level == 0:
                live = top_copy
            elif self.top_level == 1:
                live = self._status(self._level1_raw(block) + top_copy, (block // 0xAA) % 0xAA)[0] >> 6 & 1
            else:
                l2 = 0x70E4 + (0xAB << self.shift) + top_copy
                l1_copy = self._status(l2, (block // 0x70E4) % 0xAA)[0] >> 6 & 1
                live = self._status(self._level1_raw(block) + l1_copy, (block // 0xAA) % 0xAA)[0] >> 6 & 1
        self.f.seek(self.base + (self._level0_raw(block) + live) * BLOCK + (block % 0xAA) * 0x18)
        return self.f.read(0x18)

    def _next(self, block):
        e = self._hash_entry(block)
        return int.from_bytes(e[0x15:0x18], 'big')

    def _blocks(self, start, count, consecutive):
        if consecutive:
            return list(range(start, start + count))
        out, b = [], start
        for _ in range(count):
            out.append(b)
            b = self._next(b)
        return out

    def _file_table(self):
        raw = b''
        for b in self._blocks(self.ft_start, self.ft_blocks, False):
            self.f.seek(self._addr(b))
            raw += self.f.read(BLOCK)
        entries = []
        for i in range(0, len(raw), 0x40):
            e = raw[i:i + 0x40]
            flags = e[0x28]
            if not flags & 0x3F:
                break
            entries.append({'name': e[:flags & 0x3F].decode('latin-1'), 'dir': bool(flags & 0x80),
                            'consecutive': bool(flags & 0x40), 'count': int.from_bytes(e[0x29:0x2C], 'little'),
                            'start': int.from_bytes(e[0x2F:0x32], 'little'), 'parent': struct.unpack_from('>H', e, 0x32)[0],
                            'size': struct.unpack_from('>I', e, 0x34)[0]})
        for e in entries:
            parts, p = [e['name']], e['parent']
            while p != 0xFFFF and p < len(entries):
                parts.append(entries[p]['name'])
                p = entries[p]['parent']
            e['path'] = '/'.join(reversed(parts))
        return entries

    def read(self, e):
        out = bytearray()
        for b in self._blocks(e['start'], e['count'], e['consecutive']):
            self.f.seek(self._addr(b))
            out += self.f.read(BLOCK)
        return bytes(out[:e['size']])


def main(argv):
    if len(argv) < 3 or argv[1] not in ('info', 'list', 'extract') or (argv[1] == 'extract' and len(argv) < 4):
        print(__doc__)                 # extract without its output folder: the usage, not an IndexError
        return 1
    pkg = Package(argv[2])
    if argv[1] == 'info':
        print(f'{pkg.magic!r} "{pkg.name}" header {pkg.header_size:#x} separation {pkg.separation} '
              f'file table {pkg.ft_blocks} blocks at {pkg.ft_start}; {len(pkg.entries)} entries')
        return 0
    flt = (argv[3] if argv[1] == 'list' and len(argv) > 3 else argv[4] if len(argv) > 4 else '').lower()
    for e in pkg.entries:
        if e['dir'] or flt not in e['path'].lower():
            continue
        if argv[1] == 'list':
            print(f'{e["size"]:>12}  {e["path"]}')
        elif argv[1] == 'extract':
            dst = os.path.join(argv[3], *e['path'].split('/'))
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            open(dst, 'wb').write(pkg.read(e))
            print(f'{e["size"]:>12}  {dst}')
    return 0


if __name__ == '__main__':
    sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'core'))
    from bfbc2core import errors
    errors.setup('stfs', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
