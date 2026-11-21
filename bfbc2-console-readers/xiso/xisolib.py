"""xisolib.py - read-only reader of Xbox / Xbox 360 game discs (XDVDFS): the library of `bfbc2 xiso list |
extract` (round RT3 retired the command line xiso.py: bfbc2.exe answers it alone, bfbc2core.native.RETIRED_CLI).

XDVDFS: the game partition starts at a disc-type dependent offset (redump-style images contain the
video partition first). Volume descriptor at sector 32 of the partition: "MICROSOFT*XBOX*MEDIA",
root directory sector (u32) and size (u32). Directory entries form a binary tree:
u16 left, u16 right (offsets in dwords), u32 sector, u32 size, u8 attributes, u8 name length, name;
entries are 4-byte aligned and never cross a sector (0xFF padding). All little-endian.

find_partition and walk read the image in the suite's Rust core when it is built: a file opened for reading by path
(round F0: bc2_native.xiso_find_partition / xiso_walk) or, since round RT3, an image in memory (io.BytesIO, or
find_partition_bytes / walk_bytes on bytes: bc2_native.xiso_find_partition_bytes / xiso_walk_bytes) - the same
partition and the same files in the same order, the file left where the Python leaves it. Elsewhere (another file
object), or where the reference would raise or recurse too deep, the Python runs (find_partition_reference,
walk_reference: the reference, kept as the fallback); BFBC2_PURE_PYTHON=1 or use_native(False) runs the Python only.
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


def _memory_of(f: BinaryIO) -> bytes | None:
    """An image in memory (round RT3: an io.BytesIO, open) as bytes, when the Rust core has its bytes entry point."""
    if type(f) is not io.BytesIO or f.closed or not hasattr(_native(), 'xiso_walk_bytes'):
        return None
    return f.getvalue()


def find_partition(f: BinaryIO) -> int:
    """The game partition's offset: in the Rust core when it is built (round F0; an io.BytesIO too since round RT3),
    else find_partition_reference."""
    nat = _native()
    path = _path_of(f) if nat is not None else None
    data = _memory_of(f) if nat is not None and path is None else None
    if path is not None or data is not None:
        base = nat.xiso_find_partition(path) if path is not None else nat.xiso_find_partition_bytes(data)
        if base is not None:
            f.seek(base + 32 * SECTOR + len(MAGIC))     # where the reference's last read leaves the file
            return int(base)
    return find_partition_reference(f)


def find_partition_bytes(data: bytes) -> int:
    """find_partition of an image held in memory (round RT3)."""
    return find_partition(io.BytesIO(data))


def walk_bytes(data: bytes, base: int, sector: int, size: int, prefix: str = '') -> list[tuple[str, int, int]]:
    """list(walk(...)) of an image held in memory (round RT3)."""
    return list(walk(io.BytesIO(data), base, sector, size, prefix))


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
    data = _memory_of(f) if nat is not None and path is None else None
    got = (nat.xiso_walk(path, base, sector, size, prefix) if path is not None else
           nat.xiso_walk_bytes(data, base, sector, size, prefix) if data is not None else None)
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
