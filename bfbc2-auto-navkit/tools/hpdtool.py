"""hpdtool.py - reader/writer for BFBC2 Kynapse navigation data (pathdata.hpd).

Part of BC2 NavKit (see ..\\README.md; written from ..\\research\\KYNAPSE_SPEC.md).

Usage:
    python hpdtool.py info <pathdata.hpd>                 file summary
    python hpdtool.py sector <pathdata.hpd> <index>       one sector: nodes, edges, mesh header
    python hpdtool.py roundtrip <file-or-folder>...       parse + write back, compare byte for byte

Decoded so far: header, sector table, per-sector graph (nodes, edges). The runtime mesh is kept as
raw bytes until its layout is decoded (the round trip then still proves everything else).
"""
import os
import struct
import sys
from dataclasses import dataclass, field
from typing import List, Tuple

MAGIC = b'Kynogon HPD\0'
HEADER_WORDS = 36           # u32 words after the magic (0x0C..0x9B)
HEADER_SIZE = 12 + HEADER_WORDS * 4
TABLE_ENTRY = struct.Struct('<I6f5I')  # 48 bytes
NODE = struct.Struct('<I3fIf3I')       # 36 bytes
EDGE = struct.Struct('<3I')            # 12 bytes
MESH_MARKER = b'KynogonRuntimeMesh'


@dataclass
class Node:
    id: int                  # high bit set on some nodes (apparently linked to other sectors)
    pos: Tuple[float, float, float]  # world position, Y up
    flags: int
    value: float
    a: int
    b: int                   # two packed u16
    c: int


@dataclass
class Edge:
    tag: int                 # high bit set; low bits vary per edge
    src: int
    dst: int


@dataclass
class Sector:
    id: int                  # grid coordinates, 0xXXYY
    bounds: Tuple[float, ...]  # minX, maxX, minY, maxY, minZ, maxZ
    reserved: int            # always 0 so far
    date: int                # build date YYMMDD, zero-based month (100007 = 2010-01-07)
    time: int                # build time HHMMSScc
    nodes: List[Node] = field(default_factory=list)
    edges: List[Edge] = field(default_factory=list)
    mesh: bytes = b''        # runtime mesh record (starts with "KynogonRuntimeMesh"), raw for now


@dataclass
class HpdFile:
    header: Tuple[int, ...]  # the 36 header words (word 2 = sector count, rewritten on save)
    sectors: List[Sector]


def parse(data: bytes) -> HpdFile:
    if data[:12] != MAGIC:
        raise ValueError('not a Kynogon HPD file')
    header = struct.unpack_from(f'<{HEADER_WORDS}I', data, 12)
    count = header[2]
    blocks = HEADER_SIZE + count * TABLE_ENTRY.size
    sectors = []
    for i in range(count):
        e = TABLE_ENTRY.unpack_from(data, HEADER_SIZE + i * TABLE_ENTRY.size)
        sid, bounds, reserved, size, offset, date, time = e[0], e[1:7], e[7], e[8], e[9], e[10], e[11]
        start = blocks + offset
        block = data[start:start + size]
        n_nodes, n_edges = struct.unpack_from('<2I', block, 0)
        pos = 8
        nodes = []
        for _ in range(n_nodes):
            v = NODE.unpack_from(block, pos)
            nodes.append(Node(v[0], (v[1], v[2], v[3]), v[4], v[5], v[6], v[7], v[8]))
            pos += NODE.size
        edges = []
        for _ in range(n_edges):
            edges.append(Edge(*EDGE.unpack_from(block, pos)))
            pos += EDGE.size
        mesh = block[pos:]
        if not mesh.startswith(MESH_MARKER):
            raise ValueError(f'sector {i} ({sid:#x}): runtime mesh not where expected')
        sectors.append(Sector(sid, bounds, reserved, date, time, nodes, edges, mesh))
    if blocks + sum(len(_block(s)) for s in sectors) != len(data):
        raise ValueError('sector blocks do not cover the file')
    return HpdFile(header, sectors)


def _block(s: Sector) -> bytes:
    out = bytearray(struct.pack('<2I', len(s.nodes), len(s.edges)))
    for n in s.nodes:
        out += NODE.pack(n.id, *n.pos, n.flags, n.value, n.a, n.b, n.c)
    for e in s.edges:
        out += EDGE.pack(e.tag, e.src, e.dst)
    out += s.mesh
    return bytes(out)


def write(f: HpdFile) -> bytes:
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


def _files(args):
    for a in args:
        if os.path.isdir(a):
            for dp, _, fs in os.walk(a):
                for fn in fs:
                    if fn.lower() == 'pathdata.hpd':
                        yield os.path.join(dp, fn)
        else:
            yield a


def main(argv):
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
    if cmd == 'roundtrip':
        ok = bad = 0
        for p in _files(argv[2:]):
            data = open(p, 'rb').read()
            try:
                same = write(parse(data)) == data
            except ValueError as ex:
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
    sys.exit(main(sys.argv))
