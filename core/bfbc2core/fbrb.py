"""Read-only access to .fbrb bundles without unpacking them (the layout FbrbTool reads: rust/bc2-formats/src/fbrb).

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
u32 payload length; then the payload (gzip when zipped). Unpacking and packing, FbrbTool's commands:

    unpack(bundle, out)              FbrbTool's `unpack <bundle> <out>`: the same files and fbrb.manifest, in the Rust
                                     core (rounds E3j, E3k)
    pack(folder, out)                FbrbTool's `pack <folder> <out>`: the bundle of an unpacked folder (its files and
                                     fbrb.manifest), the same bytes; returns (entries, payload bytes)
    pack_many([(folder, out), ...])  pack() of many folders on all cores: each result is the tuple, or the exception
                                     pack() raised

pack and unpack run in the Rust core, which they require (bfbc2core.native.require; FbrbTool.exe, their fallback
before, was retired 2026-11-18): bc2_formats::fbrb::pack_path (zlib's deflate ported byte for byte, DICE's padded
layouts, the size2 rules) writes the bytes FbrbTool.exe wrote (core/tests/test_fbrb_pack_native.py: every bundle of
the game copy against the recorded ones). A folder that cannot be packed raises ValueError (a malformed manifest, an
unsafe entry name, an old manifest whose size2 cannot be kept) or OSError (a missing or unreadable file, a bundle
that cannot be written).

entries / names / read / read_many run in the suite's Rust core (rust/, module bc2_native: bc2_formats::fbrb) when it
is built, else in Python here; both give the same entries and bytes and refuse the same bundles
(core/tests/test_fbrb_native.py: every bundle of the game copy). BFBC2_PURE_PYTHON=1 (or use_native(False)) forces
the Python reader; backend() says which one runs.

The bundle cache (round E3e, the Rust core only; bc2_formats::fbrb::cache): a zipped payload is one gzip stream, so
an entry costs decompressing the payload up to it, and a tool that reads entries one call at a time decompressed the
same bundle again and again. With the Rust core, read / read_many keep each bundle's header and the payload
decompressed so far in the process (BFBC2_FBRB_MEMORY_MB, default 128), and the whole decompressed payload of a large
bundle (BFBC2_FBRB_CACHE_MIN_MB of compressed payload, default 8) read a second time - by this process or any other -
on disk, in %LOCALAPPDATA%\\bfbc2-tool-suite\\cache\\fbrb (BFBC2_FBRB_CACHE names another folder, 0 turns it off;
BFBC2_FBRB_CACHE_GB caps it, default 16, the least recently used deleted first). Everything is keyed by the bundle's
path, size and times, so a changed bundle is never answered from the cache; the results and errors are read()'s
(core/tests/test_fbrb_cache.py). Stored payloads are read where each entry is.

    read_uncached(path, [name, ...]) read() without the cache (the Rust core's reader as before E3e)
    cache_config() / cache_configure(memory=, folder=, disk_cap=, disk_min=, no_disk=, env=)
    cache_stats(reset=False)         what the cache did in this process
    cache_clear()                    forget what the process keeps in memory
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


# ------------------------------------------------------------------------------------------------ the bundle cache

def _cache_native() -> ModuleType | None:
    """The Rust core when it is on and has the bundle cache's controls (a module built before E3e does not)."""
    return _native if _native is not None and hasattr(_native, 'fbrb_cache_stats') else None


def read_uncached(path: Path, wanted: Iterable[str]) -> dict[str, bytes]:
    """read() without the bundle cache: the payload decompressed from its start (the Python reader without the Rust
    core, which has no cache)."""
    keys = list(dict.fromkeys(wanted))
    native = _cache_native()
    if native is not None:
        return dict(zip(keys, native.fbrb_read_uncached(os.fspath(path), keys, [k.lower() for k in keys])))
    return _py_read(path, keys)


def cache_config() -> dict[str, object] | None:
    """{'memory', 'folder', 'disk_cap', 'disk_min'} of the bundle cache (bytes; folder None: no disk cache), or None
    without the Rust core."""
    native = _cache_native()
    return dict(native.fbrb_cache_config()) if native is not None else None


def cache_configure(**kw: object) -> None:
    """Changes the bundle cache's configuration (memory, folder, disk_cap, disk_min in bytes; no_disk=True turns the
    disk cache off; env=True starts from the environment's) and forgets what the process kept. Nothing without the
    Rust core."""
    native = _cache_native()
    if native is not None:
        native.fbrb_cache_configure(**kw)


def cache_stats(reset: bool = False) -> dict[str, int]:
    """What the bundle cache did in this process (reads, disk_hits, memory_hits, decompressed_reads,
    decompressed_bytes, written, written_bytes, header_hits); empty without the Rust core."""
    native = _cache_native()
    return dict(native.fbrb_cache_stats(reset)) if native is not None else {}


def cache_clear() -> None:
    """Forgets the headers and payloads the process keeps in memory (the disk cache stays)."""
    native = _cache_native()
    if native is not None:
        native.fbrb_cache_clear()


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

def _core(attr: str) -> ModuleType:
    """The Rust core for FbrbTool's pack / unpack (no Python twin: they always need it, whatever use_native says),
    or NativeMissing saying how to get it."""
    from . import native
    return native.require('FbrbTool\'s pack and unpack', attr)


def pack(folder: Path, out: Path) -> tuple[int, int]:
    """FbrbTool's pack: the bundle of folder (unpacked by FbrbTool: its files and fbrb.manifest) written to out
    (through a temporary file beside it, so a pack cut short leaves the old bundle). Returns (entries, payload
    bytes). In the Rust core (required)."""
    n, size = _core('fbrb_pack').fbrb_pack(os.fspath(folder), os.fspath(out))
    return int(n), int(size)


def pack_many(items: Sequence[tuple[Path, Path]]) -> list[tuple[int, int] | Exception]:
    """pack() of each (folder, out), in order, on all cores; a folder that cannot be packed gives the exception
    pack() raises (ValueError, OSError) in its place, returned."""
    got = _core('fbrb_pack_many').fbrb_pack_many([(os.fspath(f), os.fspath(o)) for f, o in items])
    return [r if isinstance(r, Exception) else (int(r[0]), int(r[1])) for r in got]


def unpack_in_process(bundle: Path, out: Path) -> bool:
    """unpack() that answers False instead of raising when the bundle cannot be unpacked (out may then hold part of
    it, as FbrbTool left it): for a caller with a quicker fallback (a copy of an earlier unpack)."""
    try:
        _core('fbrb_unpack').fbrb_unpack(os.fspath(bundle), os.fspath(out))
    except (ValueError, OSError):
        return False
    return True


def unpack(bundle: Path, out: Path) -> str:
    """FbrbTool's `unpack <bundle> <out>`: every entry written under out by its archive name, *deleted* markers
    listed in out\\fbrb.manifest only, the manifest FbrbTool writes - the same files and bytes. Returns 'rust' (in
    process, bc2_formats::fbrb::unpack_path; the Rust core is required).

    The Rust core unpacks every bundle as FbrbTool.exe did (one engine with FbrbTool's command line,
    bfbc2core.fbrbtool: .NET's gzip reader, path rules and errors); a bundle it cannot unpack raises ValueError
    (OSError for files), as FbrbTool.exe refused it."""
    _core('fbrb_unpack').fbrb_unpack(os.fspath(bundle), os.fspath(out))
    return 'rust'


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
