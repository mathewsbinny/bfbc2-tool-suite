"""xiso.py - read-only lister/extractor for Xbox / Xbox 360 game discs (XDVDFS).

Usage: python xiso.py list <image.iso> [filter]
       python xiso.py extract <image.iso> <outdir> [filter]     (filter = case-insensitive substring)

XDVDFS: the game partition starts at a disc-type dependent offset (redump-style images contain the
video partition first). Volume descriptor at sector 32 of the partition: "MICROSOFT*XBOX*MEDIA",
root directory sector (u32) and size (u32). Directory entries form a binary tree:
u16 left, u16 right (offsets in dwords), u32 sector, u32 size, u8 attributes, u8 name length, name;
entries are 4-byte aligned and never cross a sector (0xFF padding). All little-endian.

find_partition and walk read the image in the suite's Rust core when it is built and f is a file opened for reading
by path (round F0: bc2_native.xiso_find_partition / xiso_walk, the same partition and the same files in the same
order, the file left where the Python leaves it). Elsewhere, or where the reference would raise or recurse too deep,
the Python runs (find_partition_reference, walk_reference: the reference, kept as the fallback);
BFBC2_PURE_PYTHON=1 or use_native(False) runs the Python only.
"""
import io
import os
import struct
import sys
from collections.abc import Iterator
from typing import Any, BinaryIO

SECTOR = 2048
MAGIC = b'MICROSOFT*XBOX*MEDIA'
PARTITIONS = (0x0, 0xFD90000, 0x2080000, 0x18300000)  # plain/xiso, XGD2, XGD3, XGD1

_NATIVE: list[Any] = []


def _native() -> Any:
    """The Rust core with xiso_walk, or None: not built, or BFBC2_PURE_PYTHON set."""
    if not _NATIVE:
        m = None
        try:
            root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
            if root not in sys.path:
                sys.path.insert(0, root)
            import suite_paths
            suite_paths.use('core')
            from bfbc2core import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        _NATIVE.append(m if m is not None and hasattr(m, 'xiso_walk') else None)
    return _NATIVE[0]


def use_native(on: bool) -> bool:
    """The Rust core on (when built, whatever BFBC2_PURE_PYTHON says) or off for this process; True when it runs."""
    _NATIVE.clear()
    if not on:
        _NATIVE.append(None)
        return False
    saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
    try:
        return _native() is not None
    finally:
        if saved is not None:
            os.environ['BFBC2_PURE_PYTHON'] = saved


def _path_of(f: BinaryIO) -> str | None:
    """The path of a plain file opened for reading in binary (what the Rust core reads), else None."""
    if type(f) is not io.BufferedReader or f.closed or 'b' not in getattr(f, 'mode', ''):
        return None
    name = getattr(f, 'name', None)
    return name if isinstance(name, str) and os.path.isfile(name) else None


def find_partition(f: BinaryIO) -> int:
    """The game partition's offset: in the Rust core when it is built (round F0), else find_partition_reference."""
    nat = _native()
    path = _path_of(f) if nat is not None else None
    if path is not None:
        base = nat.xiso_find_partition(path)
        if base is not None:
            f.seek(base + 32 * SECTOR + len(MAGIC))     # where the reference's last read leaves the file
            return int(base)
    return find_partition_reference(f)


def find_partition_reference(f: BinaryIO) -> int:
    """The Python search (the reference)."""
    for base in PARTITIONS:
        f.seek(base + 32 * SECTOR)
        if f.read(20) == MAGIC:
            return base
    raise SystemExit('no XDVDFS volume found')


def walk(f: BinaryIO, base: int, sector: int, size: int, prefix: str = '') -> Iterator[tuple[str, int, int]]:
    """Every file under a directory, as (path, sector, size): in the Rust core when it is built (round F0), else
    walk_reference."""
    nat = _native()
    path = _path_of(f) if nat is not None else None
    got = nat.xiso_walk(path, base, sector, size, prefix) if path is not None else None
    if got is None:
        yield from walk_reference(f, base, sector, size, prefix)
        return
    files, pos = got
    f.seek(pos)                                      # where the reference's last read leaves the file
    yield from files


def walk_reference(f: BinaryIO, base: int, sector: int, size: int, prefix: str = '') -> Iterator[tuple[str, int, int]]:
    """The Python walk (the reference)."""
    f.seek(base + sector * SECTOR)
    table = f.read(size)
    seen: set[int] = set()

    def node(off: int) -> Iterator[tuple[str, int, int]]:
        if off in seen or off + 14 > len(table): return
        seen.add(off)
        left, right, sec, sz, attr, nlen = struct.unpack_from('<HHIIBB', table, off)
        if left == 0xFFFF: return
        name = table[off + 14: off + 14 + nlen].decode('latin-1')
        if left: yield from node(left * 4)
        path = prefix + name
        if attr & 0x10:
            if sz: yield from walk_reference(f, base, sec, sz, path + '/')
        else:
            yield path, sec, sz
        if right: yield from node(right * 4)

    yield from node(0)


def main() -> int:
    cmd, iso = sys.argv[1], sys.argv[2]
    with open(iso, 'rb') as f:
        base = find_partition(f)
        f.seek(base + 32 * SECTOR + 20)
        root_sec, root_size = struct.unpack('<II', f.read(8))
        files = list(walk(f, base, root_sec, root_size))
        if cmd == 'list':
            flt = sys.argv[3].lower() if len(sys.argv) > 3 else ''
            print(f'partition at 0x{base:X}, {len(files)} files')
            for path, _sec, sz in files:
                if flt in path.lower(): print(f'{sz:12d}  {path}')
        elif cmd == 'extract':
            out = sys.argv[3]; flt = sys.argv[4].lower() if len(sys.argv) > 4 else ''
            n = 0
            for path, sec, sz in files:
                if flt not in path.lower(): continue
                dst = os.path.join(out, *path.split('/'))
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                f.seek(base + sec * SECTOR)
                with open(dst, 'wb') as o:
                    left = sz
                    while left:
                        chunk = f.read(min(left, 1 << 22))
                        if not chunk:           # the image ends inside the file (truncated, or a corrupt entry)
                            raise SystemExit(f'{path}: the image ends {left:,} bytes before the end of the file')
                        o.write(chunk); left -= len(chunk)
                n += 1
            print(f'extracted {n} files to {out}')
    return 0


if __name__ == '__main__':
    _ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    if _ROOT not in sys.path:
        sys.path.insert(0, _ROOT)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('xiso', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main())
