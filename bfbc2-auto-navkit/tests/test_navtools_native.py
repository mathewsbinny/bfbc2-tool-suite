r"""NavKit's tools around the build in the Rust core (round E4e, rust/BENCHMARKS.md "E4e"): each ported piece against
its Python, which stays the reference and the fallback this round (BFBC2_PURE_PYTHON=1 or the module's use_native
switch runs it), and the tools' whole outputs against the answers recorded from the Python before the port
(navtools_golden.json beside this file).

  python test_navtools_native.py            quick: every ported piece against its Python reference on seeded random
                                            inputs, the Rust side strict (a refusal fails instead of running the
                                            Python)
  python test_navtools_native.py --game [--only vehicle,water,domains,measures,offline]
                                            + the tools on Project OPFOR BFBC2's data (read only; temporary folders,
                                            and navwater's navdata cache in work\navwater\cache as its own game test
                                            writes it): ONS_MP_002's vehicle measures and path data (the build clock
                                            fixed), ONS_MP_004's boat mesh and its domains world, navparity's
                                            measures of sp_009 and sp_004_b (builds with the clock fixed), the
                                            offline sampler on sp_009 - each digest against the recording, its inputs'
                                            SHA-1 too (a changed input is reported as such, not as a regression)
  python test_navtools_native.py --record --game [--only ...]
                                            write the recording from the current code
"""
import dataclasses
import hashlib
import json
import os
import struct
import sys
import tempfile
import time
from collections.abc import Callable
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools', 'navkit')

GOLDEN = os.path.join(HERE, 'navtools_golden.json')
FIXED_CLOCK = time.struct_time((2026, 11, 17, 12, 0, 0, 1, 321, 0))   # as test_navbuild_native's recordings
FAILS: list[str] = []


def check(ok: bool, what: str) -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)
    if not ok:
        FAILS.append(what)


def canon(x: Any, h: Any) -> None:
    """Feeds x's exact value (types, order, float bits) into the hash h (rust/parity_e3o_nav.py's canon)."""
    if isinstance(x, np.ndarray):
        h.update(b'A' + x.dtype.str.encode() + repr(x.shape).encode())
        h.update(np.ascontiguousarray(x).tobytes())
    elif isinstance(x, np.generic):
        h.update(b'G' + type(x).__name__.encode() + x.tobytes())
    elif isinstance(x, bool) or x is None:
        h.update(repr(x).encode())
    elif isinstance(x, float):
        h.update(b'F' + struct.pack('<d', x))
    elif isinstance(x, int):
        h.update(b'I' + repr(x).encode() + b';')
    elif isinstance(x, (str, bytes)):
        h.update(type(x).__name__.encode() + repr(len(x)).encode() + b':' + (x if isinstance(x, bytes) else x.encode()))
    elif isinstance(x, (list, tuple)):
        h.update(type(x).__name__.encode() + b'[' + repr(len(x)).encode())
        for v in x:
            canon(v, h)
        h.update(b']')
    elif isinstance(x, dict):
        h.update(b'D' + repr(len(x)).encode())
        for k, v in x.items():
            canon(k, h)
            canon(v, h)
    elif dataclasses.is_dataclass(x):
        h.update(b'C' + type(x).__name__.encode())
        canon(x.__dict__, h)
    else:
        raise TypeError(f'canon: {type(x).__name__}')


def digest(x: Any) -> str:
    h = hashlib.sha1()
    canon(x, h)
    return h.hexdigest()


def file_sha1(p: str) -> str:
    h = hashlib.sha1()
    with open(p, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 22), b''):
            h.update(chunk)
    return h.hexdigest()


def load() -> dict[str, Any]:
    if os.path.isfile(GOLDEN):
        with open(GOLDEN, encoding='utf-8') as f:
            return json.load(f)
    return {}


# ------------------------------------------------------------------------------------------------ quick

def _strict(name: str) -> Any:
    """The core's function, or a failure (the test runs the Rust side strictly: no silent fallback)."""
    import navnative
    navnative.use_native(True)
    m = navnative.core(name)
    check(m is not None, f'the Rust core has {name}')
    return m


def random_samples(rng: np.random.Generator, nx: int, nz: int, pitch: float, layers: int = 3) -> Any:
    """A multi-layer sample run (navsample.Samples) of random heights, normals, flags."""
    import navsample
    counts = rng.integers(0, layers + 1, nx * nz)
    start = np.zeros(nx * nz + 1, np.int64)
    np.cumsum(counts, out=start[1:])
    n = int(start[-1])
    base = rng.uniform(0, 3, nx * nz)
    y = np.empty(n, np.float32)
    for c in range(nx * nz):
        k0, k1 = start[c], start[c + 1]
        # top down, storeys 1-4 m apart, small steps between columns (the rise bands hit and miss)
        y[k0:k1] = base[c] - np.cumsum(rng.uniform(0.3, 4.0, k1 - k0)) + 4.0
    nrm = rng.normal(0, 0.25, (n, 3)).astype(np.float32)
    nrm[:, 1] = np.abs(nrm[:, 1]) + 0.6
    nrm /= np.linalg.norm(nrm, axis=1, keepdims=True)
    nrm = (np.rint(nrm * 127) / 127.0).astype(np.float32)
    flags = rng.integers(0, 3, n).astype(np.uint8)
    hit = np.zeros((n, 3), np.uint32)
    return navsample.Samples(pitch, -3.2, 7.6, nx, nz, 50.0, -50.0, 0.4, 0.28, 1.9, 0.1, start, y, nrm, flags, hit)


def quick_vehicle(rng: np.random.Generator) -> int:
    """navvehicle.obstacle_distance against obstacle_distance_reference: random runs, both pitches, every option."""
    import navvehicle as nv
    m = _strict('navkit_obstacle_distance')
    calls = 0
    for case in range(24):
        pitch = [0.8, 1.0, 0.2][case % 3]
        s = random_samples(rng, int(rng.integers(5, 40)), int(rng.integers(5, 140)), pitch)
        n = len(s.y)
        cand = rng.random(n) < 0.6
        solid = rng.random(n) < 0.7
        extra = None
        if case % 2:
            extra = np.where(rng.random(n) < 0.3, s.y + rng.uniform(0.2, 2.0, n), np.nan).astype(np.float32)
        gx, gz = nv.gradients(s)
        lo, hi = [(nv.CLEAR_LOW, nv.CLEAR_HIGH), (nv.TERRAIN_RISE, nv.CLEAR_HIGH), (0.1, 1.3)][case % 3]
        radius = [nv.CLEARANCE + 1.0, 1.5, 4.2][case % 4 % 3]
        want = nv.obstacle_distance_reference(s, cand, solid, extra, gx, gz, lo, hi, radius)
        got = nv.obstacle_distance(s, cand, solid, extra, gx, gz, lo, hi, radius)
        calls += 1
        if not np.array_equal(want.view(np.uint32), got.view(np.uint32)) or got.dtype != want.dtype:
            check(False, f'obstacle_distance case {case}: {int((want != got).sum())} of {n} differ')
            return calls
    del m
    check(True, f'navvehicle.obstacle_distance: {calls} random runs bit for bit as the reference')
    return calls


def quick_domains(rng: np.random.Generator) -> int:
    """navdomains' searches (leg_dijkstra, tables) against dijkstra's Python on random graphs with equal costs."""
    import navdomains as nd
    import navnative
    _strict('navkit_domain_graph')
    calls = 0
    for case in range(30):
        n = int(rng.integers(2, 300))
        e = int(rng.integers(0, n * 3))
        edges = rng.integers(0, n, (e, 2))
        costs = rng.choice([1.0, 2.0, 0.5, 0.1, 0.0, 3.3], e) if case % 2 else rng.uniform(0, 50, e)
        d = nd.graph_domain('x', rng.uniform(0, 100, (n, 3)), edges, 1.7, costs)
        W = nd.World([d])
        for t in range(int(rng.integers(1, 8))):
            a, b = rng.integers(0, n, 2)
            W.add('enter', ('x', int(a)), ('x', int(b)), float(t))
        queries = [(int(rng.integers(0, n)), [int(x) for x in rng.integers(0, n, int(rng.integers(0, 4)))])
                   for _ in range(6)]
        got: list[Any] = []
        for mode in (True, False):
            navnative.use_native(mode)
            got.append([W.tables()] + [W.leg_dijkstra('x', src, tg or None) for src, tg in queries])
            calls += 7
        navnative.use_native(True)
        if digest(got[0]) != digest(got[1]):
            check(False, f'navdomains case {case}: different')
            return calls
    check(True, f'navdomains: {calls // 2} tables and searches on 30 random graphs as dijkstra\'s Python (types, '
                f'values, order)')
    # the same sources both ways
    d = nd.graph_domain('x', rng.uniform(0, 100, (500, 3)), rng.integers(0, 500, (1500, 2)), 2.0)
    W = nd.World([d])
    ok = True
    for src in range(0, 500, 7):
        navnative.use_native(True)
        a = W.leg_dijkstra('x', src, [3, 9] if src % 2 else None)
        navnative.use_native(False)
        b = W.leg_dijkstra('x', src, [3, 9] if src % 2 else None)
        ok &= digest(a) == digest(b)
        calls += 1
    navnative.use_native(True)
    check(ok, 'navdomains.leg_dijkstra: 72 searches (with and without targets) as the Python')
    return calls


def quick_levelscan() -> int:
    """levelobjects' scan of a built folder tree against its Python (blueprint_flags, mesh_aliases, havok_index)."""
    import levelobjects as lo
    _strict('navkit_level_scan')
    with tempfile.TemporaryDirectory() as tmp:
        texts: dict[str, str | bytes] = {
            'objects/a/Crate.xml': '<instance type="GameSharedResources.DynamicModelEntityData"><field name="Mesh" '
                                   'ref="objects/a/crate_mesh/x" /></instance>',
            'objects/b/Rock.XML': '<field name="ForbidPathfinding">true</field>\r\n<field ref="objects/rock_Mesh/y" '
                                  'name="Mesh"/>\r\n<field name="DisablePathfinding">PdtWhenAlive</field>',
            'objects/b/Fence.xml': '<field name="ExcludeFromPathfinding">true</field><field name="ForbidPathfinding">'
                                   'false</field><field name="DisablePathfinding">Pdt9</field><field '
                                   'name="DisablePathfinding">PdtNever</field>',
            'objects/c/odd.xml': b'<field name="Mesh" ref="caf\xe9/b_mesh/z"/>\xff\xfe',
            'objects/c/none.xml': '<field name="Mesh" ref="null"/>',
            'objects/c/plain.xml': '<instance/>',
            '.hidden/x.xml': '<field name="ForbidPathfinding">true</field>',
            'objects/.h.xml': '<field name="ForbidPathfinding">true</field>',
            'objects/d/sub/deep.xml': '<fields name="Mesh" ref="no"/><field name="Mesh" ref="yes/m/q"/>',
        }
        files = ['objects/a/Crate_Havok.res', 'objects/b/Rock_havok.RES', 'objects/e/door_Havok_Havok.res',
                 'objects/e/door_Havok.res', 'objects/.g/x_Havok.res', 'objects/a/Crate.xml/../q_havok.res']
        bases = [os.path.join(tmp, b) for b in ('one', 'two', 'missing')]
        for k, base in enumerate(bases[:2]):
            for rel, t in texts.items():
                p = os.path.join(base, rel.replace('Crate', 'Crate' if k == 0 else 'Box'))
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with open(p, 'wb') as f:
                    f.write(t if isinstance(t, bytes) else t.encode('utf-8'))
            for rel in files:
                p = os.path.normpath(os.path.join(base, rel))
                os.makedirs(os.path.dirname(p), exist_ok=True)
                open(p, 'wb').close()
        sc = lo.scan(bases)
        check(sc is not None, 'levelobjects.scan read the folders')
        x, y = lo.blueprint_flags(bases, sc), lo.blueprint_flags(bases)
        check(list(x.items()) == list(y.items()) and len(x) > 0,
              f'levelobjects.blueprint_flags: {len(x)} entries, the same items in the same order as its Python')
        # the scan's havok files are the collision index's packfiles (round F1a's Rust walk), in its order
        havok = lo.havok_index(bases)
        mine: dict[str, str] = {}
        for n, base in enumerate(bases):
            for r in sc[n][1]:
                rel = r.replace(os.sep, '/').lower()[:-len('_havok.res')]
                mine.setdefault(rel, os.path.join(base, r))
        check(all(os.path.normcase(havok.get(k, '')) == os.path.normcase(v) for k, v in mine.items()),
              f'levelobjects.scan: its {len(mine)} packfiles are the collision index\'s')
    return 2


def _f32(v: float) -> float:
    return struct.unpack('<f', struct.pack('<f', v))[0]


def _grid_polygon(rng: np.random.Generator, pitch: float, cx: int, cz: int) -> list[tuple[float, float, float]]:
    """A closed outline on the half-pitch lines (as NavKit and DICE write them): a rectangle with a 45-degree cut."""
    w, h = int(rng.integers(1, 9)), int(rng.integers(1, 9))
    y = _f32(float(rng.choice([0.95, 1.2, 3.95, 6.45])))
    x0, z0 = (cx - 0.5) * pitch, (cz - 0.5) * pitch
    x1, z1 = x0 + w * pitch, z0 + h * pitch
    pts = [(x0, z0), (x1, z0), (x1, z1 - pitch if h > 1 and w > 1 and rng.random() < 0.5 else z1)]
    if pts[-1][1] != z1:
        pts.append((x1 - pitch, z1))
    pts += [(x0, z1), (x0, z0)]
    return [(_f32(x), y, _f32(z)) for x, z in pts]


def random_file(rng: np.random.Generator, path: str, base: Any = None) -> Any:
    """A path-data file of 1 - 4 sectors (areas on the half-pitch lines, portals, border portals, route nodes and
    edges); with base, a candidate made from it: areas kept, moved, split or dropped, nodes moved a little, edges and
    costs changed, a sector dropped or added."""
    import hpdtool
    import kymesh
    sectors = []
    src = base.sectors if base is not None else [None] * int(rng.integers(1, 5))
    for s, old in enumerate(src):
        if base is not None and rng.random() < 0.1:
            continue                                   # a sector only the reference has
        sid = old.id if old is not None else 0x0100 + s
        if old is not None:
            mesh = kymesh.parse_mesh(old.mesh)
            areas = []
            for a in mesh.areas:
                r = rng.random()
                if r < 0.55:
                    areas.append(a)
                elif r < 0.8:
                    b = kymesh.Area(**{**a.__dict__})
                    b.outline = [[(_f32(p[0] + float(rng.choice([0.0, a.pitch]))), p[1], p[2]) for p in line]
                                 for line in a.outline]
                    areas.append(b)
                elif r < 0.9:
                    b = kymesh.Area(**{**a.__dict__})
                    b.min_y, b.max_y = _f32(a.min_y + 1.5), _f32(a.max_y + 1.5)
                    areas.append(b)
            if rng.random() < 0.5:
                areas = areas[::-1]
            for a in areas:
                a.portals = [kymesh.Portal(p.p0, p.p1, int(rng.integers(0, max(len(areas), 1))) if rng.random() < 0.2
                                           else min(p.target, len(areas) - 1)) for p in a.portals]
            nodes = [hpdtool.Node(n.id, (_f32(n.pos[0] + float(rng.choice([0.0, 0.0, 0.03, 0.2]))), n.pos[1],
                                         _f32(n.pos[2] + float(rng.choice([0.0, -0.04])))), n.flags, n.value, 0, 0, 0)
                     for n in old.nodes if rng.random() < 0.9]
            nodes = nodes[::-1] if rng.random() < 0.3 else nodes
            edges = [hpdtool.Edge(e.tag if rng.random() < 0.8 else e.tag ^ 1, e.src % max(len(nodes), 1),
                                  e.dst % max(len(nodes), 1)) for e in old.edges if nodes]
            pitch = mesh.pitch
        else:
            pitch = float(_f32(float(rng.choice([0.8, 0.2]))))
            areas = []
            n_areas = int(rng.integers(1, 9))
            for k in range(n_areas):
                cx, cz = int(rng.integers(-20, 20)), int(rng.integers(-20, 20))
                outline = [_grid_polygon(rng, pitch, cx, cz)]
                y = outline[0][0][1]
                if rng.random() < 0.3:                 # a second storey over the same ground
                    outline = [[(p[0], _f32(p[1] + 3.0), p[2]) for p in outline[0]]]
                a = kymesh.Area(sid, 0, 0, 0, _f32(0.4), pitch, _f32(outline[0][0][1] - 0.95 + 0.5),
                                _f32(outline[0][0][1] - 0.95 - 0.1 * k))
                a.outline = outline
                pts = outline[0]
                a.portals = [kymesh.Portal(pts[0], pts[1], int(rng.integers(0, n_areas)))
                             for _ in range(int(rng.integers(0, 3)))]
                a.border_portals = [kymesh.BorderPortal(pts[1], pts[2], (0.0, -0.0, float(rng.choice([1.0, -1.0, 0.0005]))))
                                    for _ in range(int(rng.integers(0, 2)))]
                # D lines (runs along a grid line, a diagonal, a zero-length piece) and F cells (pitch squares)
                a.d_lines = []
                for _ in range(int(rng.integers(0, 3))):
                    x0, z0 = (cx + int(rng.integers(-2, 3))) * pitch, (cz + int(rng.integers(-2, 3))) * pitch + pitch / 2
                    run = int(rng.integers(1, 5)) * float(rng.choice([1.0, -1.0]))
                    yy = _f32(float(rng.choice([0.95, 1.2, 1.95, 3.95])))
                    line = [(_f32(x0), yy, _f32(z0)), (_f32(x0 + run * pitch), _f32(yy + 0.1), _f32(z0))]
                    if rng.random() < 0.3:
                        line.append(line[-1])
                    if rng.random() < 0.3:
                        line.append((_f32(line[-1][0] + pitch), yy, _f32(line[-1][2] + pitch)))
                    a.d_lines.append(line if rng.random() < 0.5 else [(p[2], p[1], p[0]) for p in line])
                a.cells = []
                for _ in range(int(rng.integers(0, 3))):
                    x0, z0 = (cx + int(rng.integers(-3, 3)) - 0.5) * pitch, (cz + int(rng.integers(-3, 3)) - 0.5) * pitch
                    yy = _f32(float(rng.choice([0.95, 1.2, 3.95])))
                    sq = [(_f32(x0), yy, _f32(z0)), (_f32(x0 + pitch), yy, _f32(z0)), (_f32(x0 + pitch), yy, _f32(z0 + 2 * pitch)),
                          (_f32(x0), yy, _f32(z0 + 2 * pitch))]
                    d = [(1.0, -0.0, -0.0), (-1.0, -0.0, -0.0), (-0.0, -0.0, 1.0), (-0.0, -0.0, -1.0)][int(rng.integers(0, 4))]
                    a.cells.append(kymesh.Cell(d, sq))
                areas.append(a)
                del y
            nodes = []
            for k in range(int(rng.integers(0, 40))):
                p = (rng.uniform(-16, 16), float(rng.choice([1.9, 4.9])), rng.uniform(-16, 16))
                nodes.append(hpdtool.Node(1000 * s + k, (_f32(p[0]), _f32(p[1]), _f32(p[2])), 0x80000000, 0.0, 0, 0, 0))
            edges = [hpdtool.Edge(0x80000000 | int(rng.integers(0, 1 << 12)), int(rng.integers(0, len(nodes))),
                                  int(rng.integers(0, len(nodes)))) for _ in range(int(rng.integers(0, 3 * len(nodes) + 1)))
                     ] if nodes else []
        tiles = [[kymesh.TileItem(k, 0.0, 0.0) for k in range(len(areas))]]
        mesh = kymesh.Mesh(3, 1, 0, sid, 0.02, pitch, (0.0, 100.0, 0.0, 100.0), 1, 1, 0.4, tiles, areas, [], b'\0\0\0\0')
        sectors.append(hpdtool.Sector(sid, (0.0,) * 6, 0, 100007, 0, nodes, edges, kymesh.write_mesh(mesh)))
    if base is not None and rng.random() < 0.3:
        sectors.append(hpdtool.Sector(0x0200, (0.0,) * 6, 0, 100007, 0, [], [], base.sectors[0].mesh))
    f = hpdtool.HpdFile((0,) * hpdtool.HEADER_WORDS, sectors)
    with open(path, 'wb') as fh:
        fh.write(hpdtool.write(f))
    return f


def quick_measures(rng: np.random.Generator) -> int:
    """navdiff.diff (compare_sector, the cells) and ordercheck.PathData.on_area against their Python on random pairs
    of built path-data files."""
    import navdf
    import navdiff
    import navnative
    import ordercheck
    _strict('navkit_diff')
    _strict('navkit_df')
    d_sides = f_cells = groups = 0
    _strict('navkit_order_areas')
    calls = rows_n = on = 0
    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        for case in range(40):
            a, b = os.path.join(tmp, f'a{case}.hpd'), os.path.join(tmp, f'b{case}.hpd')
            f = random_file(rng, a)
            random_file(rng, b, f)
            clip = None if case % 3 else (-6.0, -8.0, 9.0, 7.5)
            got = []
            for mode in (True, False):
                navnative.use_native(mode)
                res, rows, R, C = navdiff.diff(a, b, log=lambda *x: None, clip=clip)
                df = navdf.compare_sectors(R, C, clip)
                got.append((res, rows, df, [[R[s].cells(), C[s].cells() if s in C else None] for s in sorted(R)]))
                d_sides += df['df_d_sides_reference'] if mode else 0
                f_cells += df['df_f_cells_reference'] if mode else 0
            navnative.use_native(True)
            calls += 1
            rows_n += len(got[0][1])
            if digest(got[0]) != digest(got[1]):
                ok = False
                print(f'  diff case {case}: different', {k: (got[0][0][k], got[1][0][k]) for k in got[0][0]
                                                         if got[0][0][k] != got[1][0][k]})
                break
            pts = [(float(x), float(y), float(z)) for x, y, z in zip(rng.uniform(-17, 17, 60), rng.choice([0.0, 0.3, 3.1, 9.0], 60),
                                                                  rng.uniform(-17, 17, 60))]
            pts += [(-0.4 * k, 0.0, 0.4 * k) for k in range(10)]           # on the half-pitch lines themselves
            for path in (a, b):
                # the route groups (the union-find in the core), clipped and with dropped nodes too
                drop = {n.id for s in f.sectors for n in s.nodes[::7]}
                for args in ((), ((-9.0, -9.0, 9.0, 9.0), drop)):
                    navnative.use_native(False)
                    want = ordercheck.PathData(path, *args)
                    navnative.use_native(True)
                    have = ordercheck.PathData(path, *args)
                    groups += len(have.group_size)
                    ok &= digest([list(have.group.items()), list(have.group_size.items()), have.nodes]) ==                         digest([list(want.group.items()), list(want.group_size.items()), want.nodes])
                pd_r, pd_p = ordercheck.PathData(path), ordercheck.PathData(path)
                pd_p.areas                                                  # noqa: B018 - made in Python: the reference
                for p in pts:
                    for ytol in (2.5, 0.2):
                        x, y = pd_r.on_area(p, ytol), pd_p.on_area(p, ytol)
                        on += x
                        ok &= x == y
                ok &= pd_r.__dict__.get('_core') not in (None, False)       # the core answered
    check(ok, f'navdiff.diff and navdf.compare_sectors ({d_sides} D sides, {f_cells} F cells) on {calls} random file '
              f'pairs ({rows_n} sector rows, cells clipped on a third), ordercheck route groups ({groups}) and '
              f'ordercheck on_area at 140 points each (inside {on} times): the same as their Python')
    return calls


def quick_offline(rng: np.random.Generator) -> int:
    """navsample_offline.terrain_blocks against terrain_blocks_reference: random height fields (float32), surfaces
    inside, on the edge of and outside the field, both capsule bottoms."""
    from types import SimpleNamespace

    import navsample_offline as so
    _strict('navkit_terrain_blocks')
    ok, calls, blocked = True, 0, 0
    for case in range(12):
        n = int(rng.integers(2, 60))
        size_m = float(rng.choice([1.0, 2.0, 0.75]))
        lo = np.array([float(rng.uniform(-50, 0)), 0.0, float(rng.uniform(-50, 0))])
        hi = lo + np.array([n * size_m, 0.0, n * size_m])
        heights = (np.cumsum(rng.normal(0, 0.3, (n, n)), axis=1) + rng.uniform(-1, 1, (n, n))).astype(np.float32)
        hf = SimpleNamespace(min=lo, max=hi, size=n, heights=heights, cell=((hi[0] - lo[0]) / n, (hi[2] - lo[2]) / n))
        m = 4000
        x = rng.uniform(lo[0] - 3, lo[0] + n * size_m + 3, m)
        z = rng.uniform(lo[2] - 3, lo[2] + n * size_m + 3, m)
        x[:50] = lo[0] + rng.integers(0, n, 50) * size_m             # on the samples themselves
        y = (rng.uniform(-3, 3, m) + float(heights.mean())).astype(np.float64 if case % 2 else np.float32)
        for bottom in (0.28, 0.6):
            want = so.terrain_blocks_reference(hf, x, z, y, bottom)
            got = so.terrain_blocks(hf, x, z, y, bottom)
            calls += 1
            blocked += int(got.sum())
            ok &= np.array_equal(want, got)
    check(ok, f'navsample_offline.terrain_blocks: {calls} random runs ({blocked} of {calls * 4000} surfaces blocked) as '
              f'the reference')
    return calls


def quick() -> None:
    """Every ported piece against its Python reference on seeded random inputs."""
    rng = np.random.default_rng(20261118)
    calls = quick_vehicle(rng) + quick_domains(rng) + quick_levelscan() + quick_measures(rng) + quick_offline(rng)
    print(f'quick: {calls} calls compared')


# ------------------------------------------------------------------------------------------------ game

def _fixed_clock() -> Callable[[], None]:
    clock = time.localtime
    time.localtime = lambda *a: FIXED_CLOCK

    def restore() -> None:
        time.localtime = clock
    return restore


def _inputs(root: str, paths: list[str]) -> dict[str, str]:
    return {os.path.relpath(p, root).replace(os.sep, '/'): file_sha1(p) for p in sorted(set(paths)) if os.path.isfile(p)}


def game_vehicle(root: str, tmp: str) -> tuple[dict[str, Any], dict[str, str]]:
    import navregress
    import navvehicle
    lv = 'ons_mp_002'
    L = os.path.join(root, 'work', 'levels', lv)
    sp = navvehicle.samples_of(root, lv)
    assert sp is not None
    ins = _inputs(root, [sp, navvehicle.reference_of(root, lv) or ''] +
                  [os.path.join(L, n) for n in ('forbid_tris.npy', 'exclude_tris.npy', 'dynamic_tris.npy')])
    import navsample
    s8, _ = navsample.load(sp)

    def tris(name: str) -> Any:
        p = os.path.join(L, name)
        return np.load(p) if os.path.isfile(p) else None
    t = time.perf_counter()
    s1, m = navvehicle.vehicle_grid(s8, tris('forbid_tris.npy'), tris('exclude_tris.npy'), tris('dynamic_tris.npy'),
                                    navregress.level_water(root, lv), log=lambda *a: None)
    grid_s = time.perf_counter() - t
    out = os.path.join(tmp, 'vehicle', 'pathdata.hpd')
    restore = _fixed_clock()
    try:
        t = time.perf_counter()
        navvehicle.build_level(root, lv, out, 'reference', log=lambda *a: None)
        build_s = time.perf_counter() - t
    finally:
        restore()
    got = {'grid': digest([s1, m]), 'pathdata': file_sha1(out), 'bytes': os.path.getsize(out)}
    print(f'  vehicle {lv}: grid {grid_s:.1f} s, build {build_s:.1f} s, {got["bytes"]:,} bytes')
    return got, ins


def game_water(root: str, tmp: str) -> tuple[dict[str, Any], dict[str, str]]:
    import navdomains as nd
    import navparity
    import navwater_mesh as nm
    lv = 'ons_mp_004'
    out = os.path.join(tmp, 'water')
    t = time.perf_counter()
    nm.level_run(root, lv, nm.PROFILES['pbl'], out_dir=out, log=lambda *a: None)
    water_s = time.perf_counter() - t
    base = os.path.join(out, f'{lv}.pbl')
    mesh = json.load(open(base + '.watermesh.json'))
    report = json.load(open(base + '.report.json'))
    for d in (mesh['stats'], report['stats']):
        d.pop('seconds', None)
    t = time.perf_counter()
    W = nd.level_world(root, lv, watermesh=base + '.watermesh.json', log=lambda *a: None)
    tables = W.tables()
    world = nd.to_json(W, lv, tables)
    for dom in world['domains'].values():      # the temporary boat mesh's file carries its build time (stats.seconds)
        if dom.get('file') == os.path.basename(base + '.watermesh.json'):
            dom.pop('sha1', None)
    inf = W.domains['infantry']
    rng = np.random.default_rng(3)
    plans = []
    for _ in range(40):
        a, b = rng.integers(0, inf.n, 2)
        c, legs = W.plan(('infantry', int(a)), ('infantry', int(b)), tables)
        plans.append([c, legs, W.plan_flat(('infantry', int(a)), ('infantry', int(b)))])
    world_s = time.perf_counter() - t
    print(f'  water {lv}: mesh {water_s:.1f} s, world {world_s:.1f} s')
    ins = _inputs(root, [navparity.reference_of(root, lv) or ''])
    return {'watermesh': digest(mesh), 'report': digest(report), 'world': digest(world), 'plans': digest(plans)}, ins


def game_measures(root: str, tmp: str) -> tuple[dict[str, Any], dict[str, str]]:
    import navparity
    import navregress
    got: dict[str, Any] = {}
    ins: dict[str, str] = {}
    for lv in ('sp_009', 'sp_004_b'):
        p = os.path.join(tmp, f'{lv}.hpd')
        restore = _fixed_clock()
        try:
            navregress.build_level(root, lv, p)
        finally:
            restore()
        t = time.perf_counter()
        row = navparity.run_level(root, lv, p, lambda *a: None)
        took = time.perf_counter() - t
        row.pop('candidate')
        got[lv] = {'candidate': file_sha1(p), 'row': digest(row), 'ground': row['ground'], 'connectivity': row['connectivity']}
        ins.update(_inputs(root, [navparity.need_reference(root, lv)]))
        print(f'  measures {lv}: {took:.1f} s (ground {row["ground"]}, connect {row["connectivity"]})')
    return got, ins


def game_offline(root: str, tmp: str) -> tuple[dict[str, Any], dict[str, str]]:
    import navsample_offline as so
    lv = 'sp_009'
    names = {'': os.path.join(tmp, 'off.bin'), '_hi': os.path.join(tmp, 'off_hi.bin')}
    t = time.perf_counter()
    so.run_level(root, lv, 0.8, log=lambda *a: None, names=names)
    took = time.perf_counter() - t
    print(f'  offline {lv} 0.8: {took:.1f} s')
    ins = _inputs(root, [os.path.join(root, 'work', 'navkit', lv, 'navkit.json')])
    return {k or 'normal': file_sha1(p) for k, p in names.items()}, ins


GAME: dict[str, Callable[[str, str], tuple[dict[str, Any], dict[str, str]]]] = {
    'vehicle': game_vehicle, 'water': game_water, 'measures': game_measures, 'offline': game_offline}


def game(golden: dict[str, Any], record: bool, only: list[str]) -> None:
    import navkit
    root = navkit.find_opfor(None)
    with tempfile.TemporaryDirectory() as tmp:
        for name in only:
            got, ins = GAME[name](root, tmp)
            if record:
                golden.setdefault('game', {})[name] = {'got': got, 'inputs': ins}
                print(f'{name}: recorded')
                continue
            want = golden.get('game', {}).get(name)
            if want is None:
                check(False, f'{name}: not recorded (--record --game --only {name})')
                continue
            changed = sorted(k for k in set(ins) | set(want['inputs']) if ins.get(k) != want['inputs'].get(k))
            if changed:
                check(False, f'{name}: its inputs are not the recorded ones ({", ".join(changed)}): not compared')
                continue
            for k in sorted(set(got) | set(want['got'])):
                check(got.get(k) == want['got'].get(k), f'{name}: {k} as recorded')


def main(argv: list[str]) -> int:
    record = '--record' in argv
    golden = load()
    if not record:
        quick()
    if '--game' in argv:
        only = argv[argv.index('--only') + 1].split(',') if '--only' in argv else list(GAME)
        game(golden, record, only)
    if record and not FAILS:
        golden['recorded'] = time.strftime('%Y-%m-%d')
        with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(golden, f, indent=1, sort_keys=True)
    print('all passed' if not FAILS else f'{len(FAILS)} FAILED')
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
