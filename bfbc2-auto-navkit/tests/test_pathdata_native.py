r"""Round F0: path data (hpdtool.parse / write, bc2_native.hpd_parse / hpd_write) and the runtime-mesh writer
(kymesh.write_mesh, bc2_native.kymesh_write) in the Rust core against their Python, the reference that stays until the
port passes a full test tier (hpdtool.parse_reference / write_reference, kymesh.write_mesh_reference).

Compared: the parsed objects (every type, attribute order, value, float bit), the written bytes, and for damaged or
changed inputs the same result or the same error. The port answers None where it leaves a case to the reference;
those are counted, and a case the reference reads or writes must never be one of them unless it holds a NaN.

  python test_pathdata_native.py          quick: files and meshes built here (random sectors, nodes, edges, areas
                                          with every table kind, links), 1,500 damaged copies, 600 changed objects
                                          written both ways, the first 4 path data files of the game copy's ai folder
  python test_pathdata_native.py --game   + every pathdata.hpd and kept original of OPFOR's game copy and work
                                          folder (run_tests.pathdata_files): both ways, every sector's mesh
                                          written both ways, 3 damaged copies of each file; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import copy
import os
import random
import struct
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import hpdtool  # noqa: E402
import kymesh  # noqa: E402
from bfbc2core import native  # noqa: E402
from f0_parity import Checks, canon, outcome, stdout_utf8  # noqa: E402

CLASSES = (hpdtool.Node, hpdtool.Edge, hpdtool.Sector, hpdtool.HpdFile)


# ------------------------------------------------------------------------------------------------ built inputs

def f32(rng: random.Random) -> float:
    """A float32 value (as a file holds it), now and then a special one."""
    r = rng.random()
    if r < 0.05:
        return rng.choice([0.0, -0.0, 1.0, float('inf'), -float('inf'), 1e-45, 3.4028234663852886e38])
    return struct.unpack('<f', struct.pack('<f', rng.uniform(-5000, 5000)))[0]


def v3(rng: random.Random) -> tuple[float, float, float]:
    return (f32(rng), f32(rng), f32(rng))


def build_mesh(rng: random.Random) -> kymesh.Mesh:
    """A random runtime mesh: every area placed by some tile, every table kind, links into areas."""
    tx, tz = rng.randint(1, 3), rng.randint(1, 3)
    n_areas = rng.randint(1, 6)
    areas = []
    for _ in range(n_areas):
        a = kymesh.Area(rng.randrange(1 << 16), rng.randrange(8), rng.randrange(8), rng.randrange(3), f32(rng),
                        f32(rng), f32(rng), f32(rng))
        a.portals = [kymesh.Portal(v3(rng), v3(rng), rng.randrange(n_areas)) for _ in range(rng.randint(0, 3))]
        a.border_portals = [kymesh.BorderPortal(v3(rng), v3(rng), v3(rng),
                                                rng.choice([(kymesh.NULL, kymesh.NULL), (rng.randrange(1 << 32), 7)]))
                            for _ in range(rng.randint(0, 2))]
        a.d_lines = [[v3(rng) for _ in range(rng.randint(0, 4))] for _ in range(rng.randint(0, 2))]
        a.outline = [[v3(rng) for _ in range(rng.randint(1, 5))] for _ in range(rng.randint(0, 3))]
        a.cells = [kymesh.Cell(v3(rng), [v3(rng) for _ in range(rng.randint(0, 3))]) for _ in range(rng.randint(0, 2))]
        if a.cells:
            a.end_row = v3(rng)
        for key, lst in ((0x20, None), (0x28, a.portals), (0x30, a.border_portals), (0x38, a.d_lines),
                         (0x40, a.outline), (0x48, a.cells)):
            if not lst and rng.random() < 0.7:
                a.null_offsets[key] = rng.choice([kymesh.NULL, 0, 0x50, rng.randrange(1 << 32)])
        areas.append(a)
    order = list(range(n_areas))
    rng.shuffle(order)
    tiles: list[list[kymesh.TileItem]] = [[] for _ in range(tx * tz)]
    for i in order:                                  # each area in some tile, now and then in two
        for _ in range(rng.choice([1, 1, 2])):
            tiles[rng.randrange(tx * tz)].append(kymesh.TileItem(i, f32(rng), f32(rng)))
    # areas are numbered as the tiles first name them (the reader's numbering), so a written mesh reads back equal
    first: list[int] = []
    for t in tiles:
        for it in t:
            if it.area not in first:
                first.append(it.area)
    renum = {old: new for new, old in enumerate(first)}
    areas = [areas[old] for old in first]
    for t in tiles:
        for it in t:
            it.area = renum[it.area]
    for a in areas:
        for p in a.portals:
            p.target = renum[p.target]
    links = []
    for _ in range(rng.choice([0, 0, 1, 2])):
        objs = [kymesh.LinkObject((rng.randrange(1 << 32), rng.randrange(9), 0),
                                  [(rng.randrange(len(areas)), rng.randrange(0, 0x50, 4)) for _ in range(rng.randint(0, 3))])
                for _ in range(rng.randint(0, 3))]
        links.append(kymesh.NeighbourLink(rng.randrange(1 << 16), objs))
    return kymesh.Mesh(rng.randrange(1 << 16), rng.randrange(1 << 16), 0, rng.randrange(1 << 16), f32(rng), f32(rng),
                       (f32(rng), f32(rng), f32(rng), f32(rng)), tx, tz, f32(rng), tiles, areas, links,
                       bytes(rng.randrange(256) for _ in range(4)), rng.choice([kymesh.NULL, 0, 1234]))


def build_file(rng: random.Random) -> bytes:
    """A random path data file, written by the reference."""
    sectors = []
    for _ in range(rng.randint(0, 4)):
        n = rng.randint(0, 12)
        nodes = [hpdtool.Node(rng.randrange(1 << 32), v3(rng), 0x80000000, f32(rng), rng.randrange(1 << 32),
                              rng.randrange(1 << 32), rng.randrange(1 << 32)) for _ in range(n)]
        edges = [hpdtool.Edge(rng.randrange(1 << 32), rng.randrange(max(n, 1)), rng.randrange(max(n, 1)))
                 for _ in range(rng.randint(0, 2 * n))]
        bounds = tuple(f32(rng) for _ in range(6))
        mesh = kymesh.write_mesh_reference(build_mesh(rng))
        sectors.append(hpdtool.Sector(rng.randrange(1 << 16), bounds, 0, 100007, rng.randrange(1 << 32), nodes, edges,
                                      mesh))
    header = tuple(rng.randrange(1 << 32) for _ in range(hpdtool.HEADER_WORDS))
    return hpdtool.write_reference(hpdtool.HpdFile(header, sectors))


def damaged(data: bytes, rng: random.Random) -> bytes:
    """data cut short, with bytes changed, or both."""
    b = bytearray(data)
    kind = rng.randrange(4)
    if kind in (0, 3) and b:
        del b[rng.randrange(len(b)):]
    if kind in (1, 3) and b:
        for _ in range(rng.randint(1, 4)):
            # mostly the header, the table and the counts, where a change moves everything after it
            at = rng.randrange(min(len(b), 400)) if rng.random() < 0.6 else rng.randrange(len(b))
            b[at] = rng.randrange(256)
    if kind == 2:
        b += bytes(rng.randrange(256) for _ in range(rng.randint(1, 9)))
    return bytes(b)


# ------------------------------------------------------------------------------------------------ comparisons

class Tally:
    """Counts of one comparison. strict: a case the reference answers must be answered in Rust too (unless it holds
    a NaN); changed objects are not strict (a memoryview, a generator, a list of another length: the port leaves
    what it does not read exactly as struct does to the reference)."""

    def __init__(self, strict: bool = True) -> None:
        self.strict = strict
        self.same = 0
        self.ndiff = 0
        self.differ: list[str] = []
        self.native = 0          # answers the Rust core gave
        self.fallback = 0        # cases it left to the reference
        self.fallback_ok: list[str] = []   # ... that the reference answered (allowed only with a NaN)

    def add(self, same: bool, label: str) -> None:
        if same:
            self.same += 1
            return
        self.ndiff += 1
        if len(self.differ) < 20:
            self.differ.append(label)


def has_nan(v: Any, depth: int = 0) -> bool:
    """A NaN anywhere in v (a value the port leaves to the reference)."""
    if isinstance(v, float):
        return v != v
    if depth > 50 or isinstance(v, str | bytes | bytearray | memoryview):
        return False
    if isinstance(v, dict):
        return any(has_nan(x, depth + 1) for x in v.values())
    if isinstance(v, list | tuple):
        return any(has_nan(x, depth + 1) for x in v)
    if hasattr(v, '__dict__'):
        return any(has_nan(x, depth + 1) for x in vars(v).values())
    return False


def parse_both(data: bytes, t: Tally, label: str) -> Any:
    """hpd_parse against parse_reference; the reference's file (or None)."""
    nat = native.require('hpdtool', 'hpd_parse')
    ref = outcome(hpdtool.parse_reference, data)
    got = nat.hpd_parse(data, *CLASSES)
    if got is None:
        t.fallback += 1
        if ref[0] == 'ok' and t.strict and not has_nan(hpdtool.parse_reference(data)):
            t.fallback_ok.append(label)
        t.add(outcome(hpdtool.parse, data) == ref, label)
    else:
        t.native += 1
        t.add(ref == ('ok', canon(got)), label)
    return hpdtool.parse_reference(data) if ref[0] == 'ok' else None


def write_both(f: Any, t: Tally, label: str, want: bytes | None = None) -> None:
    nat = native.require('hpdtool', 'hpd_write')
    ref = outcome(hpdtool.write_reference, f)
    got = nat.hpd_write(f)
    if got is None:
        t.fallback += 1
        if ref[0] == 'ok' and t.strict and not has_nan(f):
            t.fallback_ok.append(label)
        t.add(outcome(hpdtool.write, f) == ref, label)
    else:
        t.native += 1
        t.add(ref == ('ok', canon(got)) and (want is None or got == want), label)


def mesh_both(m: Any, t: Tally, label: str, want: bytes | None = None) -> None:
    nat = native.require('kymesh', 'kymesh_write')
    ref = outcome(kymesh.write_mesh_reference, m)
    got = nat.kymesh_write(m)
    if got is None:
        t.fallback += 1
        if ref[0] == 'ok' and t.strict and not has_nan(m):
            t.fallback_ok.append(label)
        t.add(outcome(kymesh.write_mesh, m) == ref, label)
    else:
        t.native += 1
        t.add(ref == ('ok', canon(got)) and (want is None or got == want), label)


def report(c: Checks, t: Tally, what: str) -> None:
    c.check(not t.ndiff and not t.fallback_ok,
            f'{what}: {t.same} identical ({t.native} answered in Rust, {t.fallback} left to the reference)',
            f'{t.ndiff} differ, e.g. {t.differ[:5]}; reference answered but Rust did not: {t.fallback_ok[:5]}')


# ------------------------------------------------------------------------------------------------ changed objects

BAD_FLOATS = [1e39, -1e39, float('nan'), float('inf'), -0.0, 7, True, '1.0', None, 2 ** 200]
BAD_INTS = [-1, 1 << 32, 1.0, True, None, '3', (1 << 32) - 1, 0]


def change_file(f: Any, rng: random.Random) -> Any:
    """A copy of a parsed file with one value changed to something the writers may or may not take."""
    g: Any = copy.deepcopy(f)
    secs = g.sectors
    k = rng.randrange(9)
    if k == 0:
        h = list(g.header)
        r = rng.random()
        if r < 0.3:
            h = h[:rng.randrange(len(h))]
        elif r < 0.6:
            h[rng.randrange(len(h))] = rng.choice(BAD_INTS)
        else:
            h[2] = rng.choice(BAD_INTS)                  # word 2 is replaced: anything goes there
        g.header = tuple(h) if rng.random() < 0.5 else h
    elif k == 1:
        g.sectors = tuple(secs) if rng.random() < 0.5 else (s for s in secs)
    elif secs:
        s = rng.choice(secs)
        if k == 2:
            s.mesh = rng.choice([bytearray(s.mesh), memoryview(s.mesh), s.mesh.decode('latin-1'), b''])
        elif k == 3:
            b = list(s.bounds)
            if rng.random() < 0.3:
                b = b[:5]
            else:
                b[rng.randrange(6)] = rng.choice(BAD_FLOATS)
            s.bounds = tuple(b) if rng.random() < 0.7 else b
        elif k == 4:
            setattr(s, rng.choice(['id', 'reserved', 'date', 'time']), rng.choice(BAD_INTS))
        elif k == 5 and s.nodes:
            n = rng.choice(s.nodes)
            name = rng.choice(['id', 'flags', 'a', 'b', 'c', 'value', 'pos'])
            if name == 'value':
                n.value = rng.choice(BAD_FLOATS)
            elif name == 'pos':
                n.pos = rng.choice([(1.0, 2.0), [1.0, 2.0, 3.0], (1.0, rng.choice(BAD_FLOATS), 0.0)])
            else:
                setattr(n, name, rng.choice(BAD_INTS))
        elif k == 6 and s.edges:
            setattr(rng.choice(s.edges), rng.choice(['tag', 'src', 'dst']), rng.choice(BAD_INTS))
        elif k == 7:
            s.nodes = tuple(s.nodes)
        else:
            s.edges = s.edges + [hpdtool.Edge(1, 2, 3)]
    return g


def change_mesh(m: kymesh.Mesh, rng: random.Random) -> Any:
    """A copy of a mesh with one thing changed (to values of any type: what the writers are given)."""
    g: Any = copy.deepcopy(m)
    k = rng.randrange(14)
    areas = g.areas
    if k == 0:
        setattr(g, rng.choice(['tiles_x', 'tiles_z', 'loaded', 'sector_id', 'links_null_offset']), rng.choice(BAD_INTS + [2, 3]))
    elif k == 1:
        setattr(g, rng.choice(['version', 'flags']), rng.choice([65535, 65536, -1, 0, True]))
    elif k == 2:
        setattr(g, rng.choice(['resolution', 'pitch', 'radius']), rng.choice(BAD_FLOATS))
    elif k == 3:
        g.end_slot = rng.choice([b'', b'12345678', bytearray(b'abcd'), 'abcd'])
    elif k == 4 and any(g.tiles):
        t = rng.choice([t for t in g.tiles if t])
        it = rng.choice(t)
        r = rng.random()
        if r < 0.5:
            it.area = rng.choice([-1, len(areas), len(areas) + 5, True, 1.0])
        else:
            setattr(it, rng.choice(['a', 'b']), rng.choice(BAD_FLOATS))
    elif k == 5 and any(g.tiles):
        t = rng.choice([t for t in g.tiles if t])
        t.pop(rng.randrange(len(t)))                    # an area may now be placed by no tile
    elif k == 6 and areas:
        a = rng.choice(areas)
        if a.portals:
            rng.choice(a.portals).target = rng.choice([-1, len(areas), 0, True])
        else:
            a.portals.append(kymesh.Portal((0.0, 0.0, 0.0), (1.0, 1.0, 1.0), rng.randrange(len(areas) + 1)))
    elif k == 7 and areas:
        a = rng.choice(areas)
        a.null_offsets = rng.choice([{}, {0x28: -1}, {0x20: 1 << 32}, {0x48: 5, 0x99: 'x'}, [(0x28, 1)], {40.0: 3}])
    elif k == 8 and areas:
        a = rng.choice(areas)
        setattr(a, rng.choice(['sector_id', 'tile_x', 'tile_z', 'layer']), rng.choice(BAD_INTS))
    elif k == 9 and areas:
        a = rng.choice(areas)
        setattr(a, rng.choice(['radius', 'pitch', 'max_y', 'min_y']), rng.choice(BAD_FLOATS))
    elif k == 10 and areas:
        a = rng.choice(areas)
        a.end_row = rng.choice([(1.0, 2.0), (float('nan'), 0.0, 0.0), (1e39, 0.0, 0.0), [1.0, 2.0, 3.0]])
    elif k == 11 and g.links:
        lo = rng.choice(g.links)
        if lo.objects:
            o = rng.choice(lo.objects)
            o.refs = o.refs + [rng.choice([(0, -4), (len(areas), 0), (0, 1 << 40), [0, 4]])]
        else:
            lo.sector_id = rng.choice(BAD_INTS)
    elif k == 12 and areas:
        a = rng.choice(areas)
        a.outline = a.outline + [[(1.0, 2.0, 3.0), rng.choice([(1.0,), (1e39, 0.0, 0.0), (0.0, 0.0, 0.0)])]]
    else:
        g.tiles = g.tiles[:-1] if g.tiles and rng.random() < 0.5 else g.tiles + [[]]
    return g


# ------------------------------------------------------------------------------------------------ corpora

def corpus(game: bool) -> list[str]:
    try:
        sys.path.insert(0, SUITE)
        import run_tests
        root = run_tests.opfor()
    except Exception as ex:          # noqa: BLE001 - no OPFOR: the built inputs only
        print(f'skip the game files ({ex})')
        return []
    if game:
        return list(run_tests.pathdata_files(root))
    out = []
    for dp, _, fs in os.walk(os.path.join(root, 'game', 'Output', 'win32', 'ai')):
        out += [os.path.join(dp, f) for f in fs if f.lower() == 'pathdata.hpd']
        if len(out) >= 4:
            break
    return sorted(out)[:4]


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    native.require('hpdtool', 'hpd_parse')
    c.check(hpdtool.use_native(True) and kymesh.use_native(True), 'the Rust core runs hpdtool and kymesh')
    rng = random.Random(0xF0)

    # built files and meshes
    tp, tw, tm = Tally(), Tally(), Tally()
    files = [build_file(rng) for _ in range(60)]
    for i, data in enumerate(files):
        f = parse_both(data, tp, f'built {i}')
        if f is not None:
            write_both(f, tw, f'built {i}', data)
            for j, s in enumerate(f.sectors):
                m = kymesh.parse_mesh_reference(s.mesh)
                mesh_both(m, tm, f'built {i} sector {j}', s.mesh)
    report(c, tp, 'built files parsed both ways')
    report(c, tw, 'built files written both ways, each the file it was read from')
    report(c, tm, 'built meshes written both ways, each the record it was read from')

    td = Tally()
    for i in range(1500):
        parse_both(damaged(rng.choice(files), rng), td, f'damaged {i}')
    report(c, td, 'damaged files: the same objects or the same error')

    tc, tcm = Tally(strict=False), Tally(strict=False)
    parsed = [hpdtool.parse_reference(d) for d in files]
    for i in range(300):
        write_both(change_file(rng.choice(parsed), rng), tc, f'changed file {i}')
    meshes = [kymesh.parse_mesh_reference(s.mesh) for f in parsed for s in f.sectors]
    for i in range(300):
        mesh_both(change_mesh(rng.choice(meshes), rng), tcm, f'changed mesh {i}')
    report(c, tc, 'changed files written: the same bytes or the same error')
    report(c, tcm, 'changed meshes written: the same bytes or the same error')

    # the dispatching functions
    data = files[1]
    c.check(canon(hpdtool.parse(data)) == canon(hpdtool.parse_reference(data))
            and hpdtool.write(hpdtool.parse(data)) == data, 'hpdtool.parse / write dispatch to the Rust core')
    c.check(outcome(hpdtool.parse, b'nope') == outcome(hpdtool.parse_reference, b'nope')
            and outcome(hpdtool.parse, b'nope')[0] == 'error', "a file that is not path data: the reference's error")
    hpdtool.use_native(False)
    c.check(hpdtool._native() is None and hpdtool.write(hpdtool.parse(data)) == data, 'use_native(False): the Python')
    hpdtool.use_native(True)

    # the game's files
    paths = corpus(game)
    if paths:
        gp, gw, gm, gd = Tally(), Tally(), Tally(), Tally()
        t_nat = t_ref = tw_nat = tw_ref = tm_nat = tm_ref = 0.0
        nbytes = nsectors = 0
        nat = native.require('hpdtool', 'hpd_parse')
        for n, p in enumerate(paths):
            if n and n % 20 == 0:
                print(f'  {n} of {len(paths)} files', flush=True)
            data = open(p, 'rb').read()
            nbytes += len(data)
            label = os.path.relpath(p, os.path.dirname(os.path.dirname(os.path.dirname(p))))
            t0 = time.perf_counter()
            nat.hpd_parse(data, *CLASSES)
            t1 = time.perf_counter()
            f = parse_both(data, gp, label)
            t_ref += time.perf_counter() - t1
            t_nat += t1 - t0
            if f is None:
                continue
            t0 = time.perf_counter()
            nat.hpd_write(f)
            t1 = time.perf_counter()
            hpdtool.write_reference(f)
            tw_nat += t1 - t0
            tw_ref += time.perf_counter() - t1
            write_both(f, gw, label, data)
            for s in f.sectors:
                nsectors += 1
                try:
                    m = kymesh.parse_mesh(s.mesh)
                except Exception as ex:          # noqa: BLE001 - a mesh the reader refuses is counted
                    gm.add(False, f'{label} sector {s.id:#x}: {ex}')
                    continue
                t0 = time.perf_counter()
                nat.kymesh_write(m)
                t1 = time.perf_counter()
                kymesh.write_mesh_reference(m)
                tm_nat += t1 - t0
                tm_ref += time.perf_counter() - t1
                mesh_both(m, gm, f'{label} sector {s.id:#x}', s.mesh)
            frng = random.Random(len(data))
            for k in range(3 if game else 5):
                parse_both(damaged(data, frng), gd, f'{label} damaged {k}')
        report(c, gp, f'{len(paths)} game path data files ({nbytes / 1e6:.0f} MB) parsed both ways')
        report(c, gw, f'{len(paths)} game path data files written both ways, each byte for byte the file')
        report(c, gm, f'{nsectors} game runtime meshes written both ways, each byte for byte the record')
        report(c, gd, 'damaged game files: the same objects or the same error')
        print(f'timing: parse {t_ref:.2f} s Python, {t_nat:.2f} s Rust; write {tw_ref:.2f} s / {tw_nat:.2f} s; '
              f'mesh write {tm_ref:.2f} s / {tm_nat:.2f} s')
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
