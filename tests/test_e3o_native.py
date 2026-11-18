r"""Round E3o (rust/BENCHMARKS.md, "E3o"): each faster path against the code it replaced, compared exactly (every float
by its bits, every array by dtype, shape and bytes; or the same exception).

  - kymesh.parse_mesh in Rust (bc2_native.kymesh_parse) against kymesh.parse_mesh_reference (the format's Python
    reader: kept for good), run both ways;
  - navdata.load (the Rust core's areas, cells and union-finds), navdiff.area_cells and navpath.route's A* against
    the answers recorded from their retired Python (round R1, 2026-11-18: tests/navdata_golden.json - navdata.load of
    40 built files, 300 union-finds, 10,000 routes; on the game data every path-data file's NavData, area cells and
    pieces, and 400 routes);
  - havok.verify's packfile part from the Rust walk's record (havok_verify_packfile) against Packfile + ignorable;
  - texqa's analysis in worker processes against one process.

  python test_e3o_native.py           quick: built inputs - 600 seeded runtime meshes (each also cut short, with
                                      bytes changed and a NaN written), 40 built path-data files, 300 union-finds,
                                      400 seeded route graphs (ties, one-way edges, islands; 25 routes each), 500
                                      seeded packfiles with the classes verify knows (each against 4 changed PC
                                      copies), 9 built textures with 1 and 3 workers
  python test_e3o_native.py --game    + every sector of the game copy's path data (rust\parity_e3o_nav.py mesh),
                                      every path-data file of the game copy and DICE's of every prepared level
                                      against the recording, 400 routes on sp_009 / sp_004_b (bench_sweep's),
                                      verify-havok's Onslaught files both ways, the WW2 vehicles' textures (texqa,
                                      cold, 1 worker and all)
  python test_e3o_native.py --record [--game]
                                      write navdata_golden.json's answers from the current code: only after a
                                      deliberate change to navdata / navdiff / navpath

The module is required: without it (or built from other sources) this test fails, saying how to build it.
"""
import json
import os
import random
import struct
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
sys.path.insert(0, os.path.join(SUITE, 'rust'))
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools', 'console-convert')
import hpdtool  # noqa: E402
import kymesh  # noqa: E402
import numpy as np  # noqa: E402
import parity_e3o_nav  # noqa: E402
from bfbc2core import native, navdata, navpath  # noqa: E402

FAILS: list[str] = []
NULL = kymesh.NULL
GOLDEN = os.path.join(HERE, 'navdata_golden.json')
GOLD: dict = {}                     # navdata_golden.json: the retired Python's answers (round R1)
RECORD = {'on': False}


def load_golden() -> None:
    try:
        with open(GOLDEN, encoding='utf-8') as fh:
            GOLD.update(json.load(fh))
    except FileNotFoundError:
        pass


def save_golden() -> None:
    GOLD['recorded'] = time.strftime('%Y-%m-%d')
    with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(GOLD, fh, indent=1, sort_keys=True)
        fh.write('\n')
    print(f'wrote {GOLDEN}')


def nav_digest(nav) -> str:
    """A NavData's exact value (parity_e3o_nav.digest), without its path (a temporary or another checkout's)."""
    d = dict(nav.__dict__)
    d.pop('_adj', None)
    d.pop('path', None)
    return parity_e3o_nav.digest((type(nav).__name__, d))


def nav_outcome(load, p: str) -> list:
    try:
        return ['ok', nav_digest(load(p))]
    except Exception as e:                                       # noqa: BLE001 - compared, not handled
        return ['error', type(e).__name__, str(e).replace(p, '<path>')]


def check(ok: bool, what: str, detail: str = '') -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def outcome(f, *a, **kw):
    """f's result as a digest of its exact value, or its exception as (type name, message)."""
    try:
        return ('ok', parity_e3o_nav.digest(f(*a, **kw)))
    except Exception as e:                                       # noqa: BLE001 - compared, not handled
        return ('error', type(e).__name__, str(e))


def f32(rnd: random.Random, special: bool = True) -> float:
    """A float32 value (as stored): mostly coordinates, now and then -0.0, a denormal, an infinity."""
    r = rnd.random()
    if special and r < 0.03:
        x = rnd.choice([-0.0, 0.0, 1e-42, -3e-39, float('inf'), float('-inf'), 3.4e38])
    else:
        x = rnd.uniform(-600, 600)
    return struct.unpack('<f', struct.pack('<f', x))[0]


def v3(rnd: random.Random, special: bool = True) -> tuple[float, float, float]:
    return (f32(rnd, special), f32(rnd, special), f32(rnd, special))


# ------------------------------------------------------------------------------------------------ kymesh

def polygon(rnd: random.Random, cx: float, cz: float, y: float) -> list[tuple[float, float, float]]:
    n = rnd.randint(3, 9)
    r = rnd.uniform(0.5, 9.0)
    pts = []
    for k in range(n):
        a = 2 * np.pi * k / n + rnd.uniform(-0.2, 0.2)
        p = (cx + r * rnd.uniform(0.4, 1.0) * float(np.cos(a)), y, cz + r * rnd.uniform(0.4, 1.0) * float(np.sin(a)))
        pts.append(tuple(struct.unpack('<3f', struct.pack('<3f', *p))))
    return pts + [pts[0]]


def random_mesh(rnd: random.Random, sector: int = 0x0102, finite: bool = False) -> kymesh.Mesh:
    """A runtime mesh of 1 - 8 areas: portals between them, border portals, D lines, outlines (closed polygons and
    stray lines of 0 - 3 points), F cells, tiles naming the areas in a shuffled order, neighbour links."""
    n = rnd.randint(1, 8)
    areas = []
    for _ in range(n):
        cx, cz, y = rnd.uniform(-50, 50), rnd.uniform(-50, 50), rnd.uniform(0, 20)
        outline = [polygon(rnd, cx, cz, y)] if rnd.random() < 0.9 else []
        # boundary points near the area (a file's cells stay few) for navdata, anywhere (specials too) otherwise
        stray = (lambda cx=cx, y=y, cz=cz: (cx + rnd.uniform(-9, 9), y, cz + rnd.uniform(-9, 9))) if finite else \
            (lambda: v3(rnd))
        outline += [[stray() for _ in range(rnd.randint(0, 3))] for _ in range(rnd.randint(0, 2))]
        a = kymesh.Area(sector, rnd.randrange(4), rnd.randrange(4), rnd.randrange(3), f32(rnd, False), 0.8,
                        f32(rnd, False), f32(rnd, False),
                        [kymesh.Portal(stray(), stray(), rnd.randrange(n)) for _ in range(rnd.randint(0, 3))],
                        [kymesh.BorderPortal(stray(), stray(), v3(rnd),
                                             rnd.choice([(NULL, NULL), (rnd.getrandbits(32), rnd.getrandbits(32))]))
                         for _ in range(rnd.randint(0, 2))],
                        [[v3(rnd) for _ in range(rnd.randint(0, 4))] for _ in range(rnd.randint(0, 2))],
                        outline,
                        [kymesh.Cell(v3(rnd), [v3(rnd) for _ in range(rnd.randint(0, 3))]) for _ in range(rnd.randint(0, 2))],
                        v3(rnd), {key: rnd.choice([NULL, 0, 0x50, rnd.getrandbits(32)]) for key in (0x20, 0x28, 0x30, 0x38, 0x40, 0x48)})
        if not a.cells:
            a.end_row = (0.0, 0.0, 0.0)
        areas.append(a)
    tx, tz = rnd.randint(1, 3), rnd.randint(1, 3)
    tiles: list[list[kymesh.TileItem]] = [[] for _ in range(tx * tz)]
    order = list(range(n))
    rnd.shuffle(order)
    for k in order:
        for t in rnd.sample(range(tx * tz), rnd.randint(1, min(2, tx * tz))):
            tiles[t].append(kymesh.TileItem(k, f32(rnd), f32(rnd)))
    links = [kymesh.NeighbourLink(rnd.getrandbits(16), [
        kymesh.LinkObject((rnd.getrandbits(32), rnd.getrandbits(32), rnd.getrandbits(32)),
                          [(rnd.randrange(n), rnd.randrange(0, 0x60, 4)) for _ in range(rnd.randint(0, 3))])
        for _ in range(rnd.randint(0, 3))]) for _ in range(rnd.choice([0, 0, 1, 2]))]
    return kymesh.Mesh(3, 1, 0, sector, 0.8, 0.8, (f32(rnd), f32(rnd), f32(rnd), f32(rnd)), tx, tz, f32(rnd),
                       tiles, areas, links, bytes(rnd.getrandbits(8) for _ in range(4)),
                       rnd.choice([NULL, 0, rnd.getrandbits(32)]))


def variants(rnd: random.Random, b: bytes) -> list[bytes]:
    """b; cut short twice; with 1 - 6 bytes changed twice; a NaN written over a word; a huge count."""
    out = [b, b[:rnd.randrange(len(b))], b[:rnd.randrange(0x50, len(b))] if len(b) > 0x51 else b[:20]]
    for _ in range(2):
        m = bytearray(b)
        for _ in range(rnd.randint(1, 6)):
            m[rnd.randrange(len(m))] = rnd.getrandbits(8)
        out.append(bytes(m))
    m = bytearray(b)
    i = rnd.randrange(0x20, len(m) - 4) & ~3
    m[i:i + 4] = struct.pack('<I', 0x7FC00001)
    out.append(bytes(m))
    m = bytearray(b)
    i = rnd.randrange(0x50, len(m) - 4) & ~3
    m[i:i + 4] = struct.pack('<I', rnd.choice([0x7FFFFFFF, 0xFFFFFFFF, 0x10000]))
    out.append(bytes(m))
    return out


def quick_kymesh() -> None:
    rnd = random.Random(1117)
    n = same = parsed = rust = built = 0
    nat = kymesh._native()
    for _ in range(600):
        b = kymesh.write_mesh(random_mesh(rnd))
        built += nat.kymesh_parse(b) is not None
        for v in variants(rnd, b):
            kymesh.use_native(True)
            got = outcome(kymesh.parse_mesh, v)
            ref = outcome(kymesh.parse_mesh_reference, v)
            n += 1
            same += got == ref
            if ref[0] == 'ok':
                parsed += 1
                rust += nat.kymesh_parse(v) is not None
    check(same == n, f'runtime meshes: {same} of {n} the same Mesh (value for value) or the same error both ways '
          f'({parsed} parsed)')
    check(built == 600, f'the Rust reader read {built} of the 600 built meshes itself, and {rust} of the {parsed} '
          'the reference reads (the rest hold a NaN, or changed bytes made one: left to the reference)')
    kymesh.use_native(False)
    check(kymesh._native() is None and kymesh.parse_mesh(b).areas == kymesh.parse_mesh_reference(b).areas,
          'use_native(False): the reference reads')
    kymesh.use_native(True)


# ------------------------------------------------------------------------------------------------ navdata

def random_hpd(rnd: random.Random, path: str) -> None:
    """A path-data file of 1 - 4 sectors: finite meshes, nodes (some shared border ids), edges with costs, a few
    ladder-steep ones."""
    sectors = []
    next_id = 1
    for s in range(rnd.randint(1, 4)):
        sid = 0x0100 + s
        nodes = []
        for _ in range(rnd.randint(0, 30)):
            nid = rnd.choice([next_id, 0x80000000 | rnd.randrange(4)]) if rnd.random() < 0.2 else next_id
            next_id += 1
            p = (rnd.uniform(-60, 60), rnd.uniform(0, 30) if rnd.random() < 0.2 else rnd.uniform(0, 3), rnd.uniform(-60, 60))
            nodes.append(hpdtool.Node(nid, tuple(struct.unpack('<3f', struct.pack('<3f', *p))), 0x80000000, 0.0, 0, 0, 0))
        edges = [hpdtool.Edge(0x80000000 | rnd.randrange(1 << 12), rnd.randrange(len(nodes)), rnd.randrange(len(nodes)))
                 for _ in range(rnd.randint(0, 3 * len(nodes)))] if nodes else []
        mesh = kymesh.write_mesh(random_mesh(rnd, sid, finite=True))
        sectors.append(hpdtool.Sector(sid, (0.0,) * 6, 0, 100007, 0, nodes, edges, mesh))
    with open(path, 'wb') as fh:
        fh.write(hpdtool.write(hpdtool.HpdFile((0,) * hpdtool.HEADER_WORDS, sectors)))


def quick_navdata() -> None:
    rnd = random.Random(2026)
    n = same = cells = 0
    rec = GOLD.setdefault('quick', {})
    want = rec.get('navdata', [])
    got_all = []
    with tempfile.TemporaryDirectory() as tmp:
        for k in range(40):
            p = os.path.join(tmp, f'{k}.hpd')
            random_hpd(rnd, p)
            a = nav_outcome(navdata.load, p)
            got_all.append(a)
            n += 1
            same += k < len(want) and want[k] == a
            cells += len(navdata.load(p).cell_area) if a[0] == 'ok' else 0
    if RECORD['on']:
        rec['navdata'] = got_all
        same = n
    check(same == n and cells > 1000, f'built path data: {same} of {n} files load to the NavData recorded from the '
          f'retired Python ({cells:,} cells)')
    rnd = random.Random(5)
    n = same = 0
    for _ in range(300):
        m = rnd.randint(1, 400)
        e = np.array([(rnd.randrange(m), rnd.randrange(m)) for _ in range(rnd.randint(0, 2 * m))], dtype=np.int64).reshape(-1, 2)
        skip = np.array([rnd.random() < 0.2 for _ in range(len(e))], dtype=bool) if rnd.random() < 0.5 else None
        a = parity_e3o_nav.digest(navdata._union_roots(m, e, skip))
        if RECORD['on']:
            rec.setdefault('union_roots', [])[n:] = [a]
        want_u = rec.get('union_roots', [])
        same += n < len(want_u) and want_u[n] == a
        n += 1
    check(same == n, f'union-find: {same} of {n} random graphs the roots recorded from the retired Python (with and '
          f'without skipped edges)')


# ------------------------------------------------------------------------------------------------ navpath

def random_nav(rnd: random.Random) -> navdata.NavData:
    """A NavData with only its route graph: nodes on a coarse grid (equal estimates and costs: ties), directed edges
    (one-way ones), several islands."""
    m = rnd.randint(2, 300)
    grid = rnd.random() < 0.5
    pos = np.array([(rnd.randrange(20) * 2.0, rnd.randrange(3) * 1.0, rnd.randrange(20) * 2.0) if grid else
                    (rnd.uniform(-90, 90), rnd.uniform(0, 8), rnd.uniform(-90, 90)) for _ in range(m)], dtype=np.float64)
    pos[:, 1] += navdata.Y_OFFSET
    pairs = []
    for a in range(m):
        for _ in range(rnd.randint(0, 3)):
            b = rnd.randrange(m)
            d = float(np.linalg.norm(pos[a] - pos[b]))
            if d < 30:
                pairs.append((a, b, int(d * 16) / 16.0))
                if rnd.random() < 0.8:
                    pairs.append((b, a, int(d * 16) / 16.0))
    edges = np.array([(a, b) for a, b, _ in pairs], dtype=np.int64).reshape(-1, 2)
    cost = np.array([c for _, _, c in pairs], dtype=np.float64)
    roots = navdata._union_roots(m, edges)
    order = {r: k for k, (r, _) in enumerate(sorted(((int(r), int((roots == r).sum())) for r in set(roots.tolist())),
                                                     key=lambda t: -t[1]))}
    island = np.array([order[int(r)] for r in roots], dtype=np.int32)
    return navdata.NavData('built', [], np.zeros((0, 2)), np.zeros(0), np.zeros(0, np.int32), pos,
                           np.zeros(m, np.int64), edges, cost, island, [], None)


def quick_navpath() -> None:
    rnd = random.Random(99)
    n = same = found = gaps = 0
    v = np.random.default_rng(20261117).normal(size=(20000, 3))
    fused = np.frombuffer(navpath._native().PathGraph.norms(np.ascontiguousarray(v, '<f8').tobytes(), navpath.NORM),
                          '<f8')
    here = fused.tobytes() == np.array([np.linalg.norm(x) for x in v]).tobytes()
    print(f"     (the estimate {navpath.NORM!r}: np.linalg.norm's bits here too: {here} - the routes do not depend on it)")
    rec = GOLD.setdefault('quick', {})
    want_r = rec.get('routes', [])
    for q in range(400):
        nav = random_nav(rnd)
        g = navpath.ground_nodes(nav)
        r1 = []
        for _ in range(25):
            a, b = g[rnd.randrange(len(g))], g[rnd.randrange(len(g))]
            if rnd.random() < 0.1:
                b = b + np.array([rnd.uniform(-20, 20), 0, rnd.uniform(-20, 20)])
            r1.append(outcome(navpath.route, nav, a, b))
            r = navpath.route(nav, a, b)
            n += 1
            found += r.found
            gaps += r.gap is not None
        d1 = parity_e3o_nav.digest(r1)
        if RECORD['on']:
            rec.setdefault('routes', [])[q:] = [d1]
            want_r = rec['routes']
        same += 25 * (q < len(want_r) and want_r[q] == d1)
    check(same == n, f'routes: {same} of {n} the Route recorded from the retired Python (nodes, cost, length, '
          f'expanded, gap, reason; 25 per graph, digested; {found} found, {gaps} across islands)')
    nav = random_nav(random.Random(3))
    nav.edge_cost = nav.edge_cost.copy()
    if len(nav.edge_cost):
        nav.edge_cost[0] = float('nan')
    g = navpath.ground_nodes(nav)
    got = outcome(navpath.route, nav, g[0], g[-1])
    check(got[0] == 'error' and got[1] == 'ValueError', 'a NaN cost: a ValueError saying the graph cannot be '
          'searched (the retired Python searched it)', str(got))


# ------------------------------------------------------------------------------------------------ havok.verify

def verify_classes(rnd: random.Random) -> dict:
    """The classes ignorable knows (members at their offsets), plus seeded random ones."""
    import havok
    import test_ports_native as tp
    k = tp.random_classes(rnd)
    M, K = havok.Member, havok.Klass
    k['hkpStorageExtendedMeshShapeMeshSubpartStorage'] = K('hkpStorageExtendedMeshShapeMeshSubpartStorage', None, 64, [
        M('vertices', None, havok.ARRAY, tp.VECTOR4, 0, 0, 0), M('indices16', None, havok.ARRAY, tp.UINT16, 0, 0, 12)])
    k['hkpCollidableBoundingVolumeData'] = K('hkpCollidableBoundingVolumeData', None, 64, [
        M('min', None, tp.UINT32, 0, 3, 0, 0), M('expansionMin', None, tp.UINT8, 0, 3, 0, 12),
        M('max', None, tp.UINT32, 0, 3, 0, 0x10), M('pad', None, tp.UINT8, 0, 4, 0, 0x1C)])
    k['hkpCollidable'] = K('hkpCollidable', None, 96, [
        M('shape', 'hkC0', havok.POINTER, havok.STRUCT, 0, 0, 0),
        M('boundingVolumeData', 'hkpCollidableBoundingVolumeData', havok.STRUCT, 0, 0, 0, 0x10)])
    k['hkpExtendedMeshShapeTrianglesSubpart'] = K('hkpExtendedMeshShapeTrianglesSubpart', None, 64, [
        M('numTriangleShapes', None, tp.INT32, 0, 0, 0, 0), M('triangleOffset', None, tp.INT32, 0, 0, 0, 8)])
    for name in ('hkpMaxSizeMotion', 'hkpKeyframedRigidMotion'):
        k[name] = K(name, None, 0x120, [
            M('type', None, tp.UINT8, 0, 0, 0, 0), M('savedMotion', name, havok.POINTER, havok.STRUCT, 0, 0, 0x118),
            M('savedQualityTypeIndex', None, tp.UINT16, 0, 0, 0, 0x11C)])
    return k


def verify_reference(console: bytes, start: int, k: dict, pc: bytes) -> list:
    """The packfile part of havok.verify_reference: (offset, reason) of each differing byte."""
    import havok
    pf = havok.Packfile(console, start, k)
    out = pf.convert()
    return [(i, havok.ignorable(pf, out, pc, i)) for i in range(start, len(pc)) if out[i] != pc[i]]


def quick_havok() -> None:
    import havok
    suite_paths.use('bf1943-import')
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-bf1943-import', 'tests'))
    import test_ports_native as tp
    rnd = random.Random(17)
    n = same = answered = reasons = ref_ok = 0
    seen_reasons: set = set()
    havok.use_native(True)
    special = ['hkpStorageExtendedMeshShapeMeshSubpartStorage', 'hkpCollidable', 'hkpCollidableBoundingVolumeData',
               'hkpExtendedMeshShapeTrianglesSubpart', 'hkpMaxSizeMotion', 'hkpKeyframedRigidMotion']
    for _ in range(500):
        k = verify_classes(rnd)
        # the objects: mostly the classes verify knows
        b = None
        for _ in range(20):
            kk = {c: k[c] for c in rnd.sample(special, rnd.randint(1, 4))}
            kk.update({c: v for c, v in k.items() if c.startswith('hkC')} if rnd.random() < 0.3 else {})
            try:
                b = bytes(16) + tp.random_packfile(rnd, kk)
                break
            except KeyError:
                continue
        if b is None:
            continue
        try:
            conv = havok.Packfile(b, 16, k).convert()
        except Exception:                                        # noqa: BLE001 - refused: both raise (checked below)
            conv = b
        for _ in range(4):
            changed = bytearray(conv)
            for _ in range(rnd.randint(0, 40)):
                changed[rnd.randrange(16, len(changed))] = rnd.getrandbits(8)
            pc = bytes(changed[:len(changed) - rnd.choice([0, 0, 0, 3])])
            if rnd.random() < 0.05:
                pc += b'\0\0'                                    # longer than the console file: the reference's IndexError
            ref = outcome(verify_reference, b, 16, k, pc)
            got = havok._native.havok_verify_packfile(b, 16, havok._rust_classes(k), pc)
            n += 1
            ref_ok += ref[0] == 'ok' and len(pc) <= len(b)
            if got is None:
                same += 1                                        # left to the reference: its answer, by construction
                continue
            answered += 1
            same += ref == ('ok', parity_e3o_nav.digest(got))
            reasons += sum(1 for _, r in got if r)
            seen_reasons |= {r for _, r in got if r}
    # triangle padding: an indices16 array of 8 (the random layouts' arrays hold 3 at most)
    k = verify_classes(rnd)
    content = bytearray(128)
    struct.pack_into('>I', content, 16, 8)                       # indices16's count; its data at 64
    content[64:80] = bytes(range(1, 17))
    b = bytes(16) + tp.packfile(['hkpStorageExtendedMeshShapeMeshSubpartStorage'], bytes(content), [(12, 64)], [],
                                [(0, 'hkpStorageExtendedMeshShapeMeshSubpartStorage')])
    conv = havok.Packfile(b, 16, k).convert()
    data = 16 + conv.find(bytes([2, 1, 4, 3]))                   # the indices, swapped to little endian
    for flip in ([6, 7], [0, 14], [6, 15]):
        flipped = bytearray(conv)
        for j in flip:
            flipped[data - 16 + j] ^= 0xFF
        ref = outcome(verify_reference, b, 16, k, bytes(flipped))
        got = havok._native.havok_verify_packfile(b, 16, havok._rust_classes(k), bytes(flipped))
        n += 1
        ref_ok += 1
        answered += got is not None
        same += got is not None and ref == ('ok', parity_e3o_nav.digest(got))
        seen_reasons |= {r for _, r in got or [] if r}
    check(same == n and answered > 0.9 * ref_ok, f'packfile verification: {same} of {n} the same (offset, reason) '
          f'lists as Packfile + ignorable ({answered} of the {ref_ok} the reference answers answered by the Rust '
          f'record, the rest read past the data; {reasons:,} ignorable bytes)')
    check(len(seen_reasons) == 4, f'every reason met: {sorted(seen_reasons)}')


# ------------------------------------------------------------------------------------------------ texqa

def quick_texqa() -> None:
    suite_paths.use('model-tools')
    import texqa
    import texwrite
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-console-convert', 'tests'))
    import test_texqa as tt
    with tempfile.TemporaryDirectory() as tmp:
        build = os.path.join(tmp, 'build', 'objects')
        xbox = os.path.join(tmp, 'xbox', 'objects')
        os.makedirs(build)
        os.makedirs(xbox)
        for k in range(9):
            img = tt.picture(32 << (k % 3), 32 << ((k + 1) % 3), seed=k)
            pc = texwrite.texture(img if k % 4 else img[::-1].copy())
            open(os.path.join(build, f't{k}_d.res'), 'wb').write(pc if k != 5 else pc[:50])
            if k % 3:
                open(os.path.join(xbox, f't{k}_d.res'), 'wb').write(tt.to_xbox(texwrite.texture(img)))
        src = texqa.LooseSource(os.path.join(tmp, 'xbox'))
        outs = []
        for jobs in (1, 3):
            logs: list[str] = []
            rows = texqa.run([os.path.join(tmp, 'build')], sources=[src], cache=os.path.join(tmp, f'c{jobs}'),
                             log=logs.append, jobs=jobs)
            outs.append((json.dumps(texqa._num(rows), sort_keys=False), texqa.text_report(rows), logs,
                         open(os.path.join(tmp, f'c{jobs}', f'rows_v{texqa.VERSION}.json'), 'rb').read()))
    check(outs[0] == outs[1], 'texqa: 9 built textures (one unreadable, one flipped) give the same rows, report, log '
          'and cache with 3 worker processes as with one')


# ------------------------------------------------------------------------------------------------ game

def game() -> None:
    from bfbc2core import paths
    root = paths.find_opfor(None, start=SUITE)
    levels = os.path.join(root, 'game', 'Output', 'win32', 'ai', 'levels')
    if not os.path.isdir(levels):
        print('skipped the game checks: no Project OPFOR BFBC2 game copy')
        return
    t = time.perf_counter()
    check(parity_e3o_nav.mesh(parity_e3o_nav.files([levels])) == 0,
          f'every sector of the game copy\'s path data: the same Mesh both ways ({time.perf_counter() - t:.0f} s)')
    game_nav(root, levels)
    game_havok(root)
    game_texqa(root)


def file_sha1(p: str) -> str:
    import hashlib
    h = hashlib.sha1()
    with open(p, 'rb') as fh:
        for chunk in iter(lambda: fh.read(1 << 22), b''):
            h.update(chunk)
    return h.hexdigest()


def cells_and_pieces(p: str) -> tuple[str, str, int]:
    """(digest of navdiff.area_cells of every area of every sector, in order; digest of navdiff.pieces of every
    area's boundary, sorted; areas)."""
    import hashlib

    import navdiff
    hc, hp = hashlib.sha1(), hashlib.sha1()
    areas = 0
    for sec in hpdtool.parse(open(p, 'rb').read()).sectors:
        mesh = kymesh.parse_mesh(sec.mesh)
        cells = navdiff.area_cells_many(mesh.areas)
        for a, c in zip(mesh.areas, cells):
            parity_e3o_nav.canon(list(c.items()), hc)
            segs = navdiff.segments(a)
            parity_e3o_nav.canon(sorted(navdiff.pieces(segs, a.pitch)), hp)
        areas += len(mesh.areas)
    return hc.hexdigest(), hp.hexdigest(), areas


def game_nav(root: str, levels: str) -> None:
    """Every path-data file of the game copy and DICE's infantry file of every prepared level: navdata.load (the
    NavData), navdiff.area_cells of every area and navdiff.pieces of every boundary, against the digests recorded
    from the retired Python (navdata_golden.json, 'game'; a file whose SHA-1 is not the recorded one is reported, not
    compared); bench_sweep's 400 routes on sp_009 / sp_004_b against the recorded Routes."""
    suite_paths.use('navigation-studio')
    import navparity
    refs = [r for lv in sorted(os.listdir(os.path.join(root, 'work', 'levels')))
            if (r := navparity.reference_of(root, lv)) and os.path.isfile(r)]
    paths: dict[str, str] = {}
    for p in parity_e3o_nav.files([levels]) + refs:
        paths.setdefault(os.path.normcase(os.path.realpath(p)), p)
    rec = GOLD.setdefault('game', {})
    files = rec.setdefault('files', {})
    n = same = changed = areas = 0
    t = time.perf_counter()
    for p in sorted(paths.values()):
        key = os.path.relpath(p, root).replace(os.sep, '/')
        sha = file_sha1(p)
        got: dict = {'sha1': sha, 'navdata': nav_outcome(navdata.load, p)}
        got['cells'], got['pieces'], na = cells_and_pieces(p)
        areas += na
        if RECORD['on']:
            files[key] = got
        rec_f = files.get(key)
        n += 1
        if rec_f is None or rec_f['sha1'] != sha:
            changed += 1
            print(f'  {key}: {"not recorded" if rec_f is None else "changed since the recording"}: not compared')
            continue
        ok = got == rec_f
        same += ok
        print(f'  {"same" if ok else "DIFFERENT"} {key} ({na:,} areas)', flush=True)
    check(same == n - changed and n >= 35 and not changed,
          f'path data of the game copy and DICE\'s prepared levels: {same} of {n} files the NavData, area cells and '
          f'pieces recorded from the retired Python ({areas:,} areas; {changed} not compared; '
          f'{time.perf_counter() - t:.0f} s)')
    routes = rec.setdefault('routes', {})
    n = same = 0
    for lv in ('sp_009', 'sp_004_b'):
        p = navparity.reference_of(root, lv)
        nav = navdata.load(p)
        g = navpath.ground_nodes(nav)
        rnd = random.Random(1)
        pairs = [(nav.node_pos[rnd.randrange(len(g))], nav.node_pos[rnd.randrange(len(g))]) for _ in range(200)]
        d = parity_e3o_nav.digest([outcome(navpath.route, nav, a, b) for a, b in pairs])
        if RECORD['on']:
            routes[lv] = {'sha1': file_sha1(p), 'routes': d}
        n += 200
        same += 200 * (routes.get(lv, {}).get('routes') == d)
    check(same == n == 400, f'bench_sweep\'s 400 routes on sp_009 / sp_004_b: {same} the Routes recorded from the '
          f'retired Python')


def game_havok(root: str) -> None:
    import glob

    import convert
    import havok
    exe = os.path.join(root, 'game', 'BFBC2Game.exe')
    pc_dir = os.path.join(root, 'work', 'audit', 'mp_common')
    consoles = sorted(glob.glob(os.path.join(root, 'work', 'onslaught', 'build_*', 'con_*')))
    if not (os.path.isfile(exe) and os.path.isfile(os.path.join(pc_dir, 'fbrb.manifest')) and consoles):
        print('skipped verify-havok: no console Onslaught builds / unpacked PC mp_common')
        return
    k = havok.classes(exe)
    pc = convert.manifest(pc_dir, 'HavokPhysicsData')
    con: dict[str, str] = {}
    for f in consoles:
        for rel, p in convert.manifest(f, 'HavokPhysicsData').items():
            con.setdefault(rel, p)
    n = same = answered = 0
    t_new = t_ref = 0.0
    for rel in sorted(set(pc) & set(con)):
        c, p = open(con[rel], 'rb').read(), open(pc[rel], 'rb').read()
        if len(c) != len(p):
            continue
        t = time.perf_counter()
        a = outcome(havok.verify, c, p, k)
        t_new += time.perf_counter() - t
        t = time.perf_counter()
        b = outcome(havok.verify_reference, c, p, k)
        t_ref += time.perf_counter() - t
        n += 1
        same += a == b
        answered += havok._native.havok_verify_packfile(c, havok.wrapper_end(c), havok._rust_classes(k), p) is not None
    check(same == n and n >= 300, f'verify-havok: {same} of {n} Onslaught files the same (exact, reasons, unexplained '
          f'offsets) both ways, {answered} answered by the Rust record (Rust {t_new:.1f} s, reference {t_ref:.1f} s)')


def game_texqa(root: str) -> None:
    suite_paths.use('bf1943-import')
    import texqa
    folder = os.path.join(root, 'work', 'ww2', 'bf1943_vehicles')
    if not os.path.isdir(folder):
        print('skipped texqa: no work\\ww2\\bf1943_vehicles')
        return
    outs = []
    with tempfile.TemporaryDirectory() as tmp:
        for jobs in ('1', '0'):
            out = os.path.join(tmp, f'out{jobs}')
            t = time.perf_counter()
            code = texqa.main([folder, '-o', out, '--cache', os.path.join(tmp, f'cache{jobs}'), '--worst', '4'] +
                              (['--workers', jobs] if jobs != '0' else []))
            dt = time.perf_counter() - t
            rows = json.load(open(os.path.join(out, 'texqa.json')))
            outs.append((code, rows, open(os.path.join(out, 'texqa.txt'), 'rb').read(), dt))
    a, b = outs
    check(a[:3] == b[:3] and len(a[1]['rows']) > 10, f'texqa on the WW2 vehicles ({len(a[1]["rows"])} textures, cold): '
          f'the same texqa.json and texqa.txt with one process ({a[3]:.1f} s) and all ({b[3]:.1f} s)')


def main(argv: list[str]) -> int:
    try:
        native.require('Round E3o\'s checks (tests/test_e3o_native.py)', 'PathGraph')
    except Exception as e:                                      # noqa: BLE001 - NativeMissing, with how to get it
        print(f'FAIL the Rust core: {e}')
        return 1
    RECORD['on'] = '--record' in argv
    load_golden()
    quick_kymesh()
    quick_navdata()
    quick_navpath()
    quick_havok()
    quick_texqa()
    if '--game' in argv:
        game()
    if RECORD['on'] and not FAILS:
        save_golden()
    print('all passed' if not FAILS else f'{len(FAILS)} check(s) failed')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
