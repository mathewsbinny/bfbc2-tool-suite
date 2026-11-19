"""Read-only access to .fbrb bundles without unpacking them (the layout FbrbTool reads: rust/bc2-formats/src/fbrb),
and FbrbTool's unpack and pack, in the suite's Rust core (rust/, module bc2_native: bc2_formats::fbrb).

    entries(path)                    [Entry]: name, type, flags, offset, size, size2 - reads only the header (fast)
    names(path)                      {lower-case entry name: Entry}, *deleted* markers left out
    read(path, [name, ...])          {name: bytes} of the entries asked for (the payload is decompressed once, up to
                                     the last one asked for; a missing name raises KeyError)
    read_many([(path, [name, ...]), ...])
                                     read() of many bundles on all cores: each result is the dict, or the exception
                                     read() raised

A damaged bundle raises ValueError ("... is not an FbRB bundle" for a file that is not one), a missing entry KeyError,
a file that cannot be read OSError.

Layout: "FbRB", u32 big-endian header length, the header (gzip): u32, u32 string-table length, the strings, u32 entry
count, per entry six u32 (name offset, flags, payload offset, size, size2, type offset), one byte (payload zipped),
u32 payload length; then the payload (gzip when zipped). Unpacking and packing, FbrbTool's commands:

    unpack(bundle, out)              FbrbTool's `unpack <bundle> <out>`: the same files and fbrb.manifest (rounds E3j,
                                     E3k: one engine with FbrbTool's command line, bfbc2core.fbrbtool)
    pack(folder, out)                FbrbTool's `pack <folder> <out>`: the bundle of an unpacked folder (its files and
                                     fbrb.manifest), the same bytes; returns (entries, payload bytes)
    pack_many([(folder, out), ...])  pack() of many folders on all cores: each result is the tuple, or the exception
                                     pack() raised

pack is bc2_formats::fbrb::pack_path (zlib's deflate ported byte for byte, DICE's padded layouts, the size2 rules):
FbrbTool's bytes on every bundle of the game copy (core/tests/test_fbrb_pack_native.py, against the bundles
FbrbTool.exe packed: core/tests/fbrbtool_golden_game.json; the exe was retired 2026-11-18, round R4). A folder that cannot be
packed raises ValueError (a malformed manifest, an unsafe entry name, an old manifest whose size2 cannot be kept) or
OSError (a missing or unreadable file, a bundle that cannot be written).

The Python reader and the fallbacks to FbrbTool.exe without the Rust core were retired on 2026-11-18 (round F1a,
rust/BENCHMARKS.md "Retired Python"); the Python reader's answers on every bundle of the game copy are kept as
recorded digests (core/tests/fbrb_golden.json, compared by test_fbrb_native.py). Without the module (python
rust/build_native.py, or --download) these functions raise bfbc2core.native.NativeMissing saying how to get it.

The bundle cache (round E3e; bc2_formats::fbrb::cache): a zipped payload is one gzip stream, so an entry costs
decompressing the payload up to it, and a tool that reads entries one call at a time decompressed the same bundle
again and again. read / read_many keep each bundle's header and the payload decompressed so far in the process
(BFBC2_FBRB_MEMORY_MB, default 128), and the whole decompressed payload of a large bundle (BFBC2_FBRB_CACHE_MIN_MB of
compressed payload, default 8) read a second time - by this process or any other - on disk, in
%LOCALAPPDATA%\\bfbc2-tool-suite\\cache\\fbrb (BFBC2_FBRB_CACHE names another folder, 0 turns it off;
BFBC2_FBRB_CACHE_GB caps it, default 16, the least recently used deleted first). Everything is keyed by the bundle's
path, size and times, so a changed bundle is never answered from the cache; the results and errors are read()'s
(core/tests/test_fbrb_cache.py). Stored payloads are read where each entry is.

    read_uncached(path, [name, ...]) read() without the cache
    cache_config() / cache_configure(memory=, folder=, disk_cap=, disk_min=, no_disk=, env=)
    cache_stats(reset=False)         what the cache did in this process
    cache_clear()                    forget what the process keeps in memory
"""
import gzip
import os
import struct
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from os import PathLike
from types import ModuleType
from typing import cast

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


# ------------------------------------------------------------------------------------------------ the Rust core

_native: ModuleType | None = None


def _rust() -> ModuleType:
    """The Rust core, or NativeMissing saying how to get it."""
    global _native
    if _native is None:
        from . import native
        _native = native.require('the bundle reader and packer (bfbc2core.fbrb)', 'fbrb_unpack')
    return _native


# ------------------------------------------------------------------------------------------------ the API

def entries(path: Path) -> list[Entry]:
    """Every entry in archive order, *deleted* markers included (only the header is read)."""
    return cast('list[Entry]', _rust().fbrb_header(os.fspath(path), Entry)[0])


def names(path: Path) -> dict[str, Entry]:
    return {e.name.lower(): e for e in entries(path) if not e.deleted}


def read(path: Path, wanted: Iterable[str]) -> dict[str, bytes]:
    """{name: bytes} for the entry names given (matched without regard to case; keys as given)."""
    keys = list(dict.fromkeys(wanted))
    return dict(zip(keys, _rust().fbrb_read(os.fspath(path), keys, [k.lower() for k in keys])))


def read_many(items: Sequence[tuple[Path, Iterable[str]]]) -> list[dict[str, bytes] | Exception]:
    """read() of each (bundle, names), in order, on all cores; a bundle that cannot be read gives the exception read()
    raises (KeyError, ValueError, OSError) in its place, returned."""
    jobs = [(path, list(dict.fromkeys(wanted))) for path, wanted in items]
    got = _rust().fbrb_read_many([(os.fspath(p), keys, [k.lower() for k in keys]) for p, keys in jobs])
    return [r if isinstance(r, Exception) else dict(zip(keys, r)) for r, (_, keys) in zip(got, jobs)]


# ------------------------------------------------------------------------------------------------ the bundle cache

def read_uncached(path: Path, wanted: Iterable[str]) -> dict[str, bytes]:
    """read() without the bundle cache: the payload decompressed from its start."""
    keys = list(dict.fromkeys(wanted))
    return dict(zip(keys, _rust().fbrb_read_uncached(os.fspath(path), keys, [k.lower() for k in keys])))


def cache_config() -> dict[str, object] | None:
    """{'memory', 'folder', 'disk_cap', 'disk_min'} of the bundle cache (bytes; folder None: no disk cache)."""
    return dict(_rust().fbrb_cache_config())


def cache_configure(**kw: object) -> None:
    """Changes the bundle cache's configuration (memory, folder, disk_cap, disk_min in bytes; no_disk=True turns the
    disk cache off; env=True starts from the environment's) and forgets what the process kept."""
    _rust().fbrb_cache_configure(**kw)


def cache_stats(reset: bool = False) -> dict[str, int]:
    """What the bundle cache did in this process (reads, disk_hits, memory_hits, decompressed_reads,
    decompressed_bytes, written, written_bytes, header_hits)."""
    return dict(_rust().fbrb_cache_stats(reset))


def cache_clear() -> None:
    """Forgets the headers and payloads the process keeps in memory (the disk cache stays)."""
    _rust().fbrb_cache_clear()


# ------------------------------------------------------------------------------------------------ packing

def pack(folder: Path, out: Path) -> tuple[int, int]:
    """FbrbTool's pack: the bundle of folder (unpacked by FbrbTool: its files and fbrb.manifest) written to out
    (through a temporary file beside it, so a pack cut short leaves the old bundle). Returns (entries, payload
    bytes). Streamed (round S1): each file is read and deflated as the bundle is written - about 10 MB held per
    bundle, never its payload (a WW2 level's 650 MB and its compressed copy were held before)."""
    n, size = _rust().fbrb_pack(os.fspath(folder), os.fspath(out))
    return int(n), int(size)


def pack_many(items: Sequence[tuple[Path, Path]]) -> list[tuple[int, int] | Exception]:
    """pack() of each (folder, out), in order, on all cores (each streamed: a few bundles at once hold tens of MB,
    not their payloads); a folder that cannot be packed gives the exception pack() raises (ValueError, OSError) in
    its place, returned."""
    got = _rust().fbrb_pack_many([(os.fspath(f), os.fspath(o)) for f, o in items])
    return [r if isinstance(r, Exception) else (int(r[0]), int(r[1])) for r in got]


def unpack_in_process(bundle: Path, out: Path) -> bool:
    """unpack(): True when it unpacked the bundle (FbrbTool's files and manifest), False when the bundle cannot be
    unpacked (out may then hold part of it, as FbrbTool leaves it). For a caller with a fallback of its own (a copy of
    an earlier unpack)."""
    m = _rust()
    try:
        m.fbrb_unpack(os.fspath(bundle), os.fspath(out))
    except (ValueError, OSError):
        return False
    return True


def unpack(bundle: Path, out: Path) -> str:
    """FbrbTool's `unpack <bundle> <out>`: every entry written under out by its archive name, *deleted* markers
    listed in out\\fbrb.manifest only, the manifest FbrbTool writes - the same files and bytes. Returns 'rust' (how it
    ran: in process, bc2_formats::fbrb::unpack_path, one engine with FbrbTool's command line since round E3k:
    .NET's gzip reader, path rules and errors). A bundle that cannot be unpacked raises ValueError (OSError for
    files), as FbrbTool reports it."""
    _rust().fbrb_unpack(os.fspath(bundle), os.fspath(out))
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
    hz = gzip.compress(h, mtime=0)                     # the same bytes every run (the tests' recorded cases)
    return b'FbRB' + struct.pack('>I', len(hz)) + hz + (gzip.compress(bytes(payload), mtime=0) if zipped else bytes(payload))
