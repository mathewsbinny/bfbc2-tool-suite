"""hpdtool.py - reader/writer for BFBC2 Kynapse navigation data (pathdata.hpd).

Part of BC2 NavKit (see ..\\README.md; written from ..\\research\\KYNAPSE_SPEC.md).

Usage:
    python hpdtool.py info <pathdata.hpd>                 file summary
    python hpdtool.py sector <pathdata.hpd> <index>       one sector: nodes, edges, mesh header
    python hpdtool.py roundtrip <file-or-folder>...       parse + write back, compare byte for byte
    python hpdtool.py coverage <file-or-folder>...        check every byte of every runtime mesh is explained
    python hpdtool.py meshtrip <file-or-folder>...        structured mesh model (kymesh.py) round trip

Sector.mesh holds the raw runtime-mesh record; kymesh.parse_mesh / write_mesh convert it to and
from the structured model (areas, portals, outlines, cells, neighbour links).

parse runs in the suite's Rust core only (round F0: bc2_native.hpd_parse; round RT3 ported the reference's errors -
its ValueError and struct.error, in its words - and its reading of NaNs, then retired parse_reference against its
recorded answers, bfbc2-auto-navkit/tests/f0_pathdata_replay*.json). write runs in the Rust core when it is built
(bc2_native.hpd_write, the same bytes; where the reference would raise, or a value is a NaN, the Python runs, so the
error stays this module's): write_reference is the Python, kept as the reference and the fallback for the objects
NavKit's Python build assembles; BFBC2_PURE_PYTHON=1 or use_native(False) runs it only. mesh_layout and mesh_coverage run in the Rust core
only (round F3: bc2_native.hpd_mesh_layout / hpd_mesh_coverage, which raise the reference's ValueError and
struct.error with its messages since round RT2); their Python was retired in round RT2 against its recorded answers
(tests/f3_golden_hpd.json).
"""
import contextlib
import os
import struct
import sys
from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from typing import Any, cast

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools')

MAGIC = b'Kynogon HPD\0'
HEADER_WORDS = 36           # u32 words after the magic (0x0C..0x9B)
HEADER_SIZE = 12 + HEADER_WORDS * 4
TABLE_ENTRY = struct.Struct('<I6f5I')  # 48 bytes
NODE = struct.Struct('<I3fIf3I')       # 36 bytes
EDGE = struct.Struct('<3I')            # 12 bytes
MESH_MARKER = b'KynogonRuntimeMesh'


@dataclass
class Node:
    id: int                  # node id; high bit = border node shared with a neighbouring sector
                             # (same id and position appear in that sector too)
    pos: tuple[float, float, float]  # world position, Y up
    flags: int               # always 0x80000000
    value: float             # DistEdgeMax on the first node of a sector (infantry 15, vehicle 50), else 0
    a: int                   # index of this node's first outgoing edge + 1 (edges are sorted by source)
    b: int                   # low u16: entry of the node list sorted by x; high u16: sorted by y
    c: int                   # low u16: entry of the node list sorted by z; bit 16: border node


@dataclass
class Edge:
    tag: int                 # cost: 0x80000000 | floor(3-D distance between the nodes * 16)
    src: int
    dst: int


@dataclass
class Sector:
    id: int                  # grid coordinates, 0xXXYY
    bounds: tuple[float, ...]  # minX, maxX, minY, maxY, minZ, maxZ
    reserved: int            # always 0 so far
    date: int                # build date YYMMDD, zero-based month (100007 = 2010-01-07)
    time: int                # build time HHMMSScc
    nodes: list[Node] = field(default_factory=list)
    edges: list[Edge] = field(default_factory=list)
    mesh: bytes = b''        # runtime mesh record (starts with "KynogonRuntimeMesh"), raw for now


@dataclass
class HpdFile:
    header: tuple[int, ...]  # the 36 header words (word 2 = sector count, rewritten on save)
    sectors: list[Sector]


_NATIVE: list[Any] = []


def _core() -> Any:
    """The Rust core (bfbc2core.native.require): NativeMissing, saying how to get it, when it is not there."""
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('path data (bfbc2-auto-navkit/tools/hpdtool.py)', 'hpd_mesh_layout')


def _native() -> Any:
    """The Rust core with hpd_parse, or None: not built, or BFBC2_PURE_PYTHON set."""
    if not _NATIVE:
        m = None
        try:
            suite_paths.use('core')
            from bfbc2core import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        _NATIVE.append(m if m is not None and hasattr(m, 'hpd_parse') else None)
    return _NATIVE[0]


def use_native(on: bool) -> bool:
    """The Rust reader / writer on (when built, whatever BFBC2_PURE_PYTHON says) or off for this process; True when
    it runs."""
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


def parse(data: bytes) -> HpdFile:
    """The file as an HpdFile, in the Rust core (round F0: bc2_native.hpd_parse; its only reader since round RT3,
    which retired parse_reference against its recorded answers: a file the reference refused raises its ValueError or
    struct.error with its message, a NaN reads as struct reads it). A bytearray or memoryview is read as its bytes
    (Sector.mesh is then bytes, where the reference gave a slice of the caller's type); another type raises Refused."""
    if type(data) is not bytes:
        if not isinstance(data, bytes | bytearray | memoryview):
            suite_paths.use('core')
            from bfbc2core import native
            raise native.refused(f'path data: a bytes-like object is needed, not {type(data).__name__}')
        data = bytes(data)
    return cast(HpdFile, _core().hpd_parse(data, Node, Edge, Sector, HpdFile))


_shared: dict[tuple[str, int, int], HpdFile] | None = None


@contextlib.contextmanager
def shared_reads() -> Iterator[None]:
    """Inside it, read() parses each file once and hands every caller the same HpdFile - for readers that do not change
    it (round E4e: navparity's measures parsed the same two files ten times). The files are forgotten on leaving."""
    global _shared
    outer = _shared is not None
    if not outer:
        _shared = {}
    try:
        yield
    finally:
        if not outer:
            _shared = None


def read(path: str) -> HpdFile:
    """parse(the file's bytes); inside shared_reads(), parsed once per file (size and time) and shared: read only."""
    if _shared is None:
        with open(path, 'rb') as fh:
            return parse(fh.read())
    st = os.stat(path)
    key = (os.path.normcase(os.path.abspath(path)), st.st_size, st.st_mtime_ns)
    f = _shared.get(key)
    if f is None:
        with open(path, 'rb') as fh:
            f = _shared[key] = parse(fh.read())
    return f


MESH_OBJECT_HEADER = 0x50


MeshLayout = tuple[list[tuple[int, int, str]], list[tuple[int, list[tuple[Any, ...]], int | None]],
                   dict[int, int], list[tuple[int, int, int]], list[int]]


def mesh_layout(m: bytes) -> MeshLayout:
    """Walk a runtime mesh record (offsets relative to its start) and return a list of
    (start, end, label) regions plus the parsed structure: the tile table (one 4-byte slot past the last tile, never
    read by the loader, kept for exact round trips), each tile's items and end offset, each object they name (its
    fixed header, its three tables), the trailer (the last 8 bytes: their own offset and 0), the link table and its
    lists. Raises ValueError on inconsistencies and struct.error on a read past the end (the reference's words). In
    the Rust core (round F3: bc2_native.hpd_mesh_layout)."""
    return cast(MeshLayout, _core().hpd_mesh_layout(bytes(m)))


Coverage = tuple[list[tuple[int, int]], list[tuple[int, int, str]]]


def mesh_coverage(m: bytes) -> Coverage:
    """The gaps between the sorted regions and their overlaps (mesh_layout's errors), in the Rust core (round F3)."""
    return cast(Coverage, _core().hpd_mesh_coverage(bytes(m)))


def _block(s: Sector) -> bytes:
    out = bytearray(struct.pack('<2I', len(s.nodes), len(s.edges)))
    for n in s.nodes:
        out += NODE.pack(n.id, *n.pos, n.flags, n.value, n.a, n.b, n.c)
    for e in s.edges:
        out += EDGE.pack(e.tag, e.src, e.dst)
    out += s.mesh
    return bytes(out)


def write(f: HpdFile) -> bytes:
    """The file's bytes: in the Rust core when it is built (round F0), else write_reference."""
    nat = _native()
    if nat is not None:
        got = nat.hpd_write(f)
        if got is not None:
            return cast(bytes, got)
    return write_reference(f)


def write_reference(f: HpdFile) -> bytes:
    """The Python writer (the reference)."""
    header = list(f.header)
    header[2] = len(f.sectors)
    out = bytearray(MAGIC + struct.pack(f'<{HEADER_WORDS}I', *header))
    blocks = [_block(s) for s in f.sectors]
    offset = 0
    for s, b in zip(f.sectors, blocks):
        out += TABLE_ENTRY.pack(s.id, *s.bounds, s.reserved, len(b), offset, s.date, s.time)
        offset += len(b)
    for b in blocks:
        out += b
    return bytes(out)


def _files(args: Iterable[str]) -> Iterator[str]:
    for a in args:
        if os.path.isdir(a):
            for dp, _, fs in os.walk(a):
                for fn in fs:
                    if fn.lower() == 'pathdata.hpd':
                        yield os.path.join(dp, fn)
        else:
            yield a


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print(__doc__)
        return 1
    cmd = argv[1]
    if cmd == 'info':
        f = parse(open(argv[2], 'rb').read())
        nodes = sum(len(s.nodes) for s in f.sectors)
        edges = sum(len(s.edges) for s in f.sectors)
        mesh = sum(len(s.mesh) for s in f.sectors)
        dates = sorted({s.date for s in f.sectors})
        print(f'{len(f.sectors)} sectors, {nodes} graph nodes, {edges} edges, {mesh} bytes of runtime mesh; '
              f'build dates {dates[0]}..{dates[-1]}')
        return 0
    if cmd == 'sector':
        f = parse(open(argv[2], 'rb').read())
        s = f.sectors[int(argv[3])]
        print(f'sector {s.id:#06x} bounds {[round(b, 2) for b in s.bounds]} built {s.date} {s.time:08d}')
        for n in s.nodes:
            print(f'  node {n.id:#010x} pos ({n.pos[0]:.2f}, {n.pos[1]:.2f}, {n.pos[2]:.2f}) flags {n.flags:#x} '
                  f'value {n.value:g} a {n.a} b {n.b & 0xFFFF},{n.b >> 16} c {n.c}')
        for e in s.edges:
            print(f'  edge {e.tag:#010x} {e.src} -> {e.dst}')
        print(f'  mesh {len(s.mesh)} bytes: {s.mesh[:48].hex(" ")}')
        return 0
    if cmd == 'coverage':
        total = clean = 0
        gap_kinds: dict[tuple[tuple[int, ...], int], int] = {}
        for p in _files(argv[2:]):
            f = parse(open(p, 'rb').read())
            for s in f.sectors:
                total += 1
                try:
                    gaps, overlaps = mesh_coverage(s.mesh)
                except (ValueError, struct.error) as ex:
                    print(f'ERROR {p} sector {s.id:#x}: {ex}')
                    continue
                if not gaps and not overlaps:
                    clean += 1
                else:
                    key = (tuple((e - b) for b, e in gaps[:3]), len(overlaps))
                    gap_kinds[key] = gap_kinds.get(key, 0) + 1
                    if gap_kinds[key] <= 2:
                        desc = [f'{b:#x}:{s.mesh[b:e].hex(" ")}' for b, e in gaps[:4]]
                        print(f'{os.path.basename(os.path.dirname(os.path.dirname(p)))}/{os.path.basename(os.path.dirname(p))} '
                              f'sector {s.id:#x} len {len(s.mesh):#x} last-tile-entry '
                              f'{struct.unpack_from("<I", s.mesh, 0x50 + struct.unpack_from("<I", s.mesh, 0x38)[0] * struct.unpack_from("<I", s.mesh, 0x3C)[0] * 4)[0]:#x}: '
                              f'gaps {desc} overlaps {overlaps[:2]}')
        print(f'coverage: {clean}/{total} meshes fully explained; gap patterns {len(gap_kinds)}')
        return 0
    if cmd == 'meshtrip':
        import kymesh
        total = same = 0
        reasons: dict[str, int] = {}
        for p in _files(argv[2:]):
            for s in parse(open(p, 'rb').read()).sectors:
                total += 1
                try:
                    out = kymesh.write_mesh(kymesh.parse_mesh(s.mesh))
                except Exception as ex:
                    reasons[type(ex).__name__ + ': ' + str(ex)[:60]] = reasons.get(type(ex).__name__ + ': ' + str(ex)[:60], 0) + 1
                    continue
                if out == s.mesh:
                    same += 1
                else:
                    common = min(len(out), len(s.mesh))
                    first = next((i for i in range(common) if out[i] != s.mesh[i]), common)
                    diff = f'len {len(out) - len(s.mesh):+d}, first diff at word {first // 4 * 4:#x}'
                    if not reasons:
                        print(f'first mismatch: {p} sector {s.id:#x}: {diff}; '
                              f'orig {s.mesh[first // 4 * 4:first // 4 * 4 + 16].hex(" ")} new {out[first // 4 * 4:first // 4 * 4 + 16].hex(" ")}')
                    reasons[diff] = reasons.get(diff, 0) + 1
        print(f'mesh round trip: {same}/{total} identical')
        for k, v in sorted(reasons.items(), key=lambda kv: -kv[1])[:8]:
            print(f'  {v:6} x {k}')
        return 0 if same == total else 2
    if cmd == 'roundtrip':
        ok = bad = 0
        for p in _files(argv[2:]):
            data = open(p, 'rb').read()
            try:
                same = write(parse(data)) == data
            except (ValueError, struct.error) as ex:      # a truncated file: one failure, not the end of the batch
                same = False
                print(f'ERROR {p}: {ex}')
            ok += same
            bad += not same
            if not same:
                print(f'MISMATCH {p}')
        print(f'round trip: {ok} identical, {bad} different')
        return 0 if bad == 0 else 2
    print(__doc__)
    return 1


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('hpdtool', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
