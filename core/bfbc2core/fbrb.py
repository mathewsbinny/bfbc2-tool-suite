"""Read-only access to .fbrb bundles without unpacking them (the layout FbrbTool reads, bfbc2-fbrb-tool/FbrbTool.cs).

    entries(path)                    [Entry]: name, type, flags, offset, size, size2 - reads only the header (fast)
    names(path)                      {lower-case entry name: Entry}, *deleted* markers left out
    read(path, [name, ...])          {name: bytes} of the entries asked for (the payload is decompressed once, up to
                                     the last one asked for; a missing name raises KeyError)
    read_many([(path, [name, ...]), ...])
                                     read() of many bundles (on all cores with the Rust core): each result is the
                                     dict, or the exception read() raised

A damaged bundle raises ValueError ("... is not an FbRB bundle" for a file that is not one), a missing entry KeyError,
a file that cannot be read OSError.

Layout: "FbRB", u32 big-endian header length, the header (gzip): u32, u32 string-table length, the strings, u32 entry
count, per entry six u32 (name offset, flags, payload offset, size, size2, type offset), one byte (payload zipped),
u32 payload length; then the payload (gzip when zipped). Unpacking stays with FbrbTool (unpack); packing:

    pack(folder, out)                FbrbTool's `pack <folder> <out>`: the bundle of an unpacked folder (its files and
                                     fbrb.manifest), the same bytes; returns (entries, payload bytes)
    pack_many([(folder, out), ...])  pack() of many folders (on all cores with the Rust core): each result is the
                                     tuple, or the exception pack() raised

pack runs in the Rust core (bc2_formats::fbrb::pack_path: zlib's deflate ported byte for byte, DICE's padded
layouts, the size2 rules) when it is built, else it runs FbrbTool.exe (the suite's build); both write the same
bytes (core/tests/test_fbrb_pack_native.py: every bundle of the game copy). A folder that cannot be packed raises
ValueError (a malformed manifest, an unsafe entry name, an old manifest whose size2 cannot be kept) or OSError (a
missing or unreadable file, a bundle that cannot be written).

entries / names / read / read_many run in the suite's Rust core (rust/, module bc2_native: bc2_formats::fbrb) when it
is built, else in Python here; both give the same entries and bytes and refuse the same bundles
(core/tests/test_fbrb_native.py: every bundle of the game copy). BFBC2_PURE_PYTHON=1 (or use_native(False)) forces
the Python reader; backend() says which one runs.
"""
import contextlib
import gzip
import os
import struct
import zlib
from collections.abc import Iterable, Iterator, Sequence
from dataclasses import dataclass
from os import PathLike
from types import ModuleType
from typing import BinaryIO, cast

from . import dbx as _dbx

DELETED = '*deleted*'
Path = str | PathLike[str]


@dataclass
class Entry:
    name: str
    type: str
    flags: int
    offset: int
    size: int
    size2: int

    @property
    def deleted(self) -> bool:
        return self.type == DELETED and self.size == 0


# ------------------------------------------------------------------------------------------------ backend

def _with_fbrb(m: ModuleType | None) -> ModuleType | None:
    """m when it has the bundle reader (a module built before it was added does not)."""
    return m if m is not None and hasattr(m, 'fbrb_read_many') else None


def _load_native() -> ModuleType | None:
    """The Rust core (found as dbx finds it) unless BFBC2_PURE_PYTHON is set."""
    if os.environ.get(_dbx.PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    return _with_fbrb(_dbx._import_native())


_native = _load_native()


def backend() -> str:
    """'rust' when bundles are read in the Rust core, else 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever BFBC2_PURE_PYTHON says) or off for this process; returns
    backend()."""
    global _native
    _native = _with_fbrb(_dbx._import_native()) if on else None
    return backend()


# ------------------------------------------------------------------------------------------------ the API

def entries(path: Path) -> list[Entry]:
    """Every entry in archive order, *deleted* markers included (only the header is read)."""
    if _native is not None:
        return cast('list[Entry]', _native.fbrb_header(os.fspath(path), Entry)[0])
    with open(path, 'rb') as fh:
        return _header(fh)[0]


def names(path: Path) -> dict[str, Entry]:
    return {e.name.lower(): e for e in entries(path) if not e.deleted}


def read(path: Path, wanted: Iterable[str]) -> dict[str, bytes]:
    """{name: bytes} for the entry names given (matched without regard to case; keys as given)."""
    keys = list(dict.fromkeys(wanted))
    if _native is not None:
        return dict(zip(keys, _native.fbrb_read(os.fspath(path), keys, [k.lower() for k in keys])))
    return _py_read(path, keys)


def read_many(items: Sequence[tuple[Path, Iterable[str]]]) -> list[dict[str, bytes] | Exception]:
    """read() of each (bundle, names), in order; a bundle that cannot be read gives the exception read() raises
    (KeyError, ValueError, OSError) in its place, returned. With the Rust core the bundles are read on all cores."""
    jobs = [(path, list(dict.fromkeys(wanted))) for path, wanted in items]
    if _native is not None:
        got = _native.fbrb_read_many([(os.fspath(p), keys, [k.lower() for k in keys]) for p, keys in jobs])
        return [r if isinstance(r, Exception) else dict(zip(keys, r)) for r, (_, keys) in zip(got, jobs)]
    out: list[dict[str, bytes] | Exception] = []
    for p, keys in jobs:
        try:
            out.append(_py_read(p, keys))
        except (KeyError, ValueError, OSError) as e:
            out.append(e)
    return out


# ------------------------------------------------------------------------------------------------ the Python reader

# what the standard-library readers raise for damaged data, raised as ValueError (as the Rust core does): a field past
# the header's end, a name without its zero byte, a gzip stream cut short, damaged or not gzip at all
_DAMAGED: tuple[type[Exception], ...] = (struct.error, IndexError, EOFError, zlib.error, gzip.BadGzipFile)


@contextlib.contextmanager
def _damaged(name: object) -> Iterator[None]:
    try:
        yield
    except _DAMAGED as e:
        raise ValueError(f'{name}: damaged bundle ({type(e).__name__}: {e})') from e


def _cstring(b: bytes, o: int) -> str:
    e = b.index(b'\0', o)
    return b[o:e].decode('ascii', errors='replace')


def _header(fh: BinaryIO) -> tuple[list[Entry], bool, int]:
    name = getattr(fh, 'name', 'file')
    head = fh.read(8)
    if len(head) < 8 or head[:4] != b'FbRB':
        raise ValueError(f'{name} is not an FbRB bundle')
    cut = struct.unpack('>I', head[4:8])[0]
    with _damaged(name):
        h = gzip.decompress(fh.read(cut))
        strlen = struct.unpack_from('>I', h, 4)[0]
        base = 8
        count = struct.unpack_from('>I', h, base + strlen)[0]
        out = []
        for i in range(count):
            no, flags, off, size, size2, to = struct.unpack_from('>6I', h, base + strlen + 4 + 24 * i)
            out.append(Entry(_cstring(h, base + no), _cstring(h, base + to), flags, off, size, size2))
        zipped = h[base + strlen + 4 + 24 * count] != 0
    return out, zipped, 8 + cut


def _py_read(path: Path, wanted: Iterable[str]) -> dict[str, bytes]:
    """read() in Python: the reference the Rust core is checked against."""
    with open(path, 'rb') as fh:
        ents, zipped, start = _header(fh)
        by = {e.name.lower(): e for e in ents if not e.deleted}
        want: dict[str, Entry] = {}
        for n in wanted:
            if n.lower() not in by:
                raise KeyError(f'{n} is not an entry of {path}')
            want[n] = by[n.lower()]
        if not want:
            return {}
        end = max(e.offset + e.size for e in want.values())
        out = {n: bytearray() for n in want}
        fh.seek(start)
        d = zlib.decompressobj(16 + zlib.MAX_WBITS) if zipped else None
        pos = 0                                       # payload offset of the next byte to come
        while pos < end:                              # streamed: only the wanted entries' bytes are kept
            chunk = fh.read(1 << 20)
            if not chunk:
                break
            with _damaged(path):
                data = d.decompress(chunk) if d else chunk
            for n, e in want.items():
                lo, hi = max(e.offset, pos), min(e.offset + e.size, pos + len(data))
                if lo < hi:
                    out[n] += data[lo - pos:hi - pos]
            pos += len(data)
        if any(len(out[n]) != e.size for n, e in want.items()):
            raise ValueError(f'{path}: the payload ends before entry data (damaged bundle)')
        return {n: bytes(b) for n, b in out.items()}


# ------------------------------------------------------------------------------------------------ packing

def _native_pack() -> ModuleType | None:
    """The Rust core when it is on and has the packer (a module built before it was added does not)."""
    return _native if _native is not None and hasattr(_native, 'fbrb_pack') else None


def fbrbtool() -> str:
    """FbrbTool.exe where the suite's build puts it (bfbc2-fbrb-tool), whether it is built or not."""
    from . import paths
    return paths.suite_tool('bfbc2-fbrb-tool', 'FbrbTool.exe')


def _tool_pack(folder: Path, out: Path) -> tuple[int, int]:
    """pack() by FbrbTool.exe: its 'packed N entries (X bytes) into ...' line read back."""
    import re
    import subprocess
    tool = fbrbtool()
    if not os.path.isfile(tool):
        raise OSError(f'{tool} is not built (run bfbc2-fbrb-tool\\build.bat) and the Rust core is not built either')
    r = subprocess.run([tool, 'pack', os.fspath(folder), os.fspath(out)], capture_output=True, text=True,
                       errors='replace')
    m = re.search(r'packed (\d+) entries \((\d+) bytes\)', r.stdout)
    if r.returncode != 0 or m is None:
        text = (r.stderr.strip() or r.stdout.strip())[-2000:]
        io = any(k in text for k in ('is missing', 'could not be written or read', 'in use', 'out of memory'))
        raise (OSError if io else ValueError)(f'FbrbTool pack {os.fspath(folder)}: {text}')
    return int(m.group(1)), int(m.group(2))


def pack(folder: Path, out: Path) -> tuple[int, int]:
    """FbrbTool's pack: the bundle of folder (unpacked by FbrbTool: its files and fbrb.manifest) written to out
    (through a temporary file beside it, so a pack cut short leaves the old bundle). Returns (entries, payload
    bytes). In the Rust core when it is built, else by FbrbTool.exe."""
    native = _native_pack()
    if native is not None:
        n, size = native.fbrb_pack(os.fspath(folder), os.fspath(out))
        return int(n), int(size)
    return _tool_pack(folder, out)


def pack_many(items: Sequence[tuple[Path, Path]]) -> list[tuple[int, int] | Exception]:
    """pack() of each (folder, out), in order; a folder that cannot be packed gives the exception pack() raises
    (ValueError, OSError) in its place, returned. With the Rust core the bundles are packed on all cores."""
    native = _native_pack()
    if native is not None:
        got = native.fbrb_pack_many([(os.fspath(f), os.fspath(o)) for f, o in items])
        return [r if isinstance(r, Exception) else (int(r[0]), int(r[1])) for r in got]
    out: list[tuple[int, int] | Exception] = []
    for f, o in items:
        try:
            out.append(_tool_pack(f, o))
        except (ValueError, OSError) as e:
            out.append(e)
    return out


def build(items: list[tuple[str, str, bytes]], zipped: bool = True) -> bytes:
    """A bundle from [(name, type, data)] (flags 0x00010000, size2 = size) - for tests; FbrbTool packs real ones."""
    offs: dict[str, int]
    strings, offs = bytearray(), {}

    def s(t: str) -> int:
        if t not in offs:
            offs[t] = len(strings)
            strings.extend(t.encode('ascii') + b'\0')
        return offs[t]
    table, payload = bytearray(), bytearray()
    for name, typ, data in items:
        table += struct.pack('>6I', s(name), 0x00010000, len(payload), len(data), len(data), s(typ))
        payload += data
    h = struct.pack('>II', 2, len(strings)) + bytes(strings) + struct.pack('>I', len(items)) + bytes(table) + bytes([1 if zipped else 0]) + struct.pack('>I', len(payload))
    hz = gzip.compress(h)
    return b'FbRB' + struct.pack('>I', len(hz)) + hz + (gzip.compress(bytes(payload)) if zipped else bytes(payload))
