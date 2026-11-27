r"""NavKit in the Rust core (rust/bc2-navbuild through bc2_native).

The generator's logic had a Python twin that was its reference until it passed the full test tier, then retired
(rust/BENCHMARKS.md, "Retired Python"): the grid walks (round E1d: Level.neighbour's linking, components, trace_edges'
side neighbours, line_walkable, build_graph's route-group search; navwalk's flood, islands_of and matched_flags) on
2026-11-17; the walkability rules, build_areas and the cell picking (E2a), the tracing, the roof rule, the cover
nodes, Gabriel pruning, the route edges and group joining, near_characters and the seed picks (E3c), whole stages
and sector graphs, the floods' unseeds and navdiff.area_cells (E3h) on 2026-11-18 (round R1). Their answers were
recorded first, from the Python code itself, in navbuild_golden.json beside this file: a SHA-1 of every walk's answers
on seeded random levels ('quick'), of every E2a / E3c / E3h comparison's answer ('retired': 331 + 456 + 39), and of
whole levels' path data (the build clock fixed). They are the oracle now; navregress (DICE's own path data) stays the
independent check of what NavKit builds. navsample.load's records keep their Python reader (a file format's reader:
kept for good), compared with the Rust core's here.

Round NK5-rest retired navbuild.py: its thin calls of the Rust core live here now ("the Rust core's calls"), its Python
drop_one_way and detail pass are the Rust stage's (NavLevel.drop_one_way / detail_pass: the same recorded answers), and
the whole stages are the Rust stage's (navkit_stage_answer) against the 'stage' answers, recorded from navbuild.py's
stage (the retired Python's recorded answers' own path) on the same levels in a canonical form (dicts sorted, the
counts as numbers) before navbuild.py went, with the diagonal-neighbour links off as that stage built them
(rust/BENCHMARKS.md "NK5-rest").

  python test_navbuild_native.py            quick: the grid walks on 24 + 16 seeded random multi-layer levels and
                                            the E2a / E3c / E3h answers against the recording; arrays and numbers of
                                            other types cast as the core takes them; CPython's float functions the
                                            core reproduces (math.hypot, x ** 2, round(v, 3))
  python test_navbuild_native.py --game [--levels sp_009,sp_004_b]
                                            + whole builds (navregress.build_level, Project OPFOR BFBC2's in-game
                                            samples, read only) into a temporary folder: the path data's SHA-1
                                            against the recording (and the inputs' SHA-1: a changed input is
                                            reported as such, not as a regression)
  python test_navbuild_native.py --record [--game --levels ...]
                                            write navbuild_golden.json from the current code: only after a
                                            deliberate change to NavKit's walks or builds, with navregress showing
                                            what it did to the parity with DICE's path data

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import contextlib
import hashlib
import json
import math
import os
import random
import struct
import sys
import tempfile
import time
from collections.abc import Iterator
from dataclasses import dataclass, replace
from types import SimpleNamespace
from typing import Any, cast

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
import kymesh  # noqa: E402
import navwalk  # noqa: E402

GOLDEN = os.path.join(HERE, 'navbuild_golden.json')
# the moment every sector of a recorded build says it was built (navwalk.build: time.localtime())
FIXED_CLOCK = time.struct_time((2026, 11, 17, 12, 0, 0, 1, 321, 0))
FAILS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if not cond:
        FAILS.append(msg)
        print('FAIL', msg)


# ------------------------------------------------------------------------------------------------ the Rust core's calls
# navbuild.py's thin wrappers of the Rust core's NavLevel, kept here for the recorded answers (round NK5-rest retired
# navbuild.py: the build's settings are the Rust defaults, its stage, drop_one_way and detail pass the Rust stage's
# own - NavLevel.drop_one_way / detail_pass and navkit_stage_answer give them to Python for these checks)

LINK_RULE = 'slope'                         # the build's link_rule (bc2_native.navkit_settings()'s switches)
AREA_LINK = 'terrain'                       # ... and area_link
BISECT = {'on': False, 'max_area': 25}      # build_areas' bisection (switch max_area; off in the build)
DETAILED_PITCH = 0.2


@dataclass(frozen=True)
class Cfg:
    """The build settings these checks vary (the Rust defaults: BuildSettings::default(), at_pitch as the Rust's)."""
    pitch: float = 0.8
    window: range = range(-1, 127)
    tile: int = 50
    tiles: int = 3
    link_step: float = 0.6
    y_offset: float = 0.95
    dist_edge_max: float = 15.0
    node_cover: tuple[float | None, float | None] = (None, 4.0)
    order_nodes: bool = True

    def at_pitch(self, p: float) -> 'Cfg':
        n = int(round(100 / p))
        detailed = abs(p - DETAILED_PITCH) < 1e-6
        return replace(self, pitch=p, window=range(-1, n + 2), tiles=6 if detailed else 3,
                       tile=int(round((20 if detailed else 40) / p)), link_step=0.3 if detailed else 0.6)

    def overrides(self) -> dict[str, Any]:
        """These settings as overrides of the Rust defaults."""
        return dict(pitch=float(self.pitch), window=(self.window.start, self.window.stop), tile=int(self.tile),
                    tiles=int(self.tiles), link_step=float(self.link_step), y_offset=float(self.y_offset),
                    dist_edge_max=float(self.dist_edge_max), node_cover=self.node_cover,
                    order_nodes=bool(self.order_nodes))


DEFAULTS = Cfg()


def require_native() -> Any:
    return navwalk.require_native()


class Level:
    """Kept walkable surfaces on the sample grid, in the Rust core (NavLevel), as navbuild.Level held them."""

    def __init__(self, s: Any, keep: np.ndarray, step: float, gx: np.ndarray | None = None,
                 gz: np.ndarray | None = None, stair: np.ndarray | None = None, cfg: Cfg = DEFAULTS) -> None:
        self.s, self.keep, self.step, self.cfg = s, keep, step, cfg
        self.stair = stair if stair is not None else np.zeros(len(s.y), bool)
        self.y = s.y.astype(float)
        ny = np.maximum(s.normal[:, 1], 0.05)
        self.gx = gx if gx is not None else -s.normal[:, 0] / ny
        self.gz = gz if gz is not None else -s.normal[:, 2] / ny
        self.col_of = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
        self.terrain = (s.hit[:, 0] == navwalk.TERRAIN_SHAPE_VTABLE) if s.hit is not None else np.ones(len(s.y), bool)
        self.blocked: set[tuple[int, int]] = set()
        self._native: Any = None

    def native(self) -> Any:
        if self._native is None:
            walk, gx, gz, stair = navwalk.grid_arrays(self.keep, self.gx, self.gz, self.stair)
            self._native = require_native().NavLevel(
                int(self.s.nx), int(self.s.nz), np.ascontiguousarray(self.s.start, '<i8').tobytes(), walk.tobytes(),
                np.ascontiguousarray(self.y, '<f8').tobytes(), gx.tobytes(), gx.dtype == np.float32, gz.tobytes(),
                gz.dtype == np.float32, stair.tobytes(), np.ascontiguousarray(self.terrain, bool).tobytes(),
                float(self.cfg.pitch), float(self.step), float(self.cfg.link_step))
        return self._native

    def neighbour(self, k: int, gi: int, gj: int, di: int, dj: int, rule: str | None = None) -> int | None:
        return cast(int | None, self.native().neighbour(int(k), int(gi), int(gj), int(di), int(dj), rule or LINK_RULE))


def _window(cfg: Cfg) -> tuple[int, int]:
    return int(cfg.window.start), int(cfg.window.stop)


def _max_area() -> int | None:
    return int(BISECT['max_area']) if BISECT['on'] else None


def build_areas(level: Level, gi0: int, gj0: int, start: Any = (-1, -1), tile: int | None = None,
                tiles: int | None = None, cfg: Cfg = DEFAULTS) -> list[dict[str, Any]]:
    got = level.native().build_areas(int(gi0), int(gj0), _window(cfg), (int(start[0]), int(start[1])),
                                     int(cfg.tile if tile is None else tile), int(cfg.tiles if tiles is None else tiles),
                                     _max_area(), AREA_LINK)
    return [{'tile': t, 'cells': c} for t, c in got]


def _build_areas_many(level: Level, specs: Any, cfg: Cfg) -> list[Any]:
    got = level.native().build_areas_many(
        [(int(sp[0]), int(sp[1]), _window(cfg), (int(sp[4][0]), int(sp[4][1])), int(sp[5]), int(sp[6]))
         for sp in specs], _max_area(), AREA_LINK, store=False)
    return [([{'tile': t, 'cells': c} for t, c, _lo, _hi in sector], [(lo, hi) for _t, _c, lo, hi in sector])
            for sector in got]


def components(level: Level, members: dict[int, tuple[int, int]], gi0: int, gj0: int) -> Any:
    return level.native().components([(int(k), int(i), int(j)) for k, (i, j) in members.items()], int(gi0), int(gj0),
                                     AREA_LINK)


def trace_edges(level: Level, areas: Any, area_of: dict[int, int], gi0: int, gj0: int, cfg: Cfg = DEFAULTS) -> Any:
    return level.native().trace_edges([a['cells'] for a in areas], area_of, int(gi0), int(gj0), _window(cfg),
                                      float(cfg.y_offset), LINK_RULE, True)


def _trace_edges_many(level: Level, sectors: Any, cfg: Cfg) -> dict[Any, Any]:
    got = level.native().trace_edges_many(
        [([a['cells'] for a in inf['areas']], int(inf['origin'][0]), int(inf['origin'][1])) for _sec, inf in sectors],
        _window(cfg), float(cfg.y_offset), LINK_RULE, True)
    return {sec: (per_area, one_way) for (sec, _inf), (per_area, one_way) in zip(sectors, got)}


def drop_one_way(level: Level, per_area: Any, one_way: Any, keep: Any) -> Any:
    """The rows with the one-way candidates not in keep made outline (the Rust stage's drop_one_way)."""
    return level.native().drop_one_way(list(per_area), list(one_way), sorted(n for n in range(len(one_way))
                                                                               if n in keep))


def detail_pass(level: Level, areas: Any, per_area: Any, ox: float, oz: float, gi0: int, gj0: int,
                cfg: Cfg) -> tuple[Any, list[Any]]:
    """The Rust stage's detail pass of one sector: (rows, per area (D lines, F records)); level.blocked its D sides."""
    rows, feats, blocked, _counts = level.native().detail_pass(
        [a['cells'] for a in areas], list(per_area), (int(gi0), int(gj0), float(ox), float(oz)),
        {'pitch': float(cfg.pitch)})
    level.blocked = set(blocked)
    return rows, [(lines, [kymesh.Cell(d, pts) for d, pts in fcells]) for lines, fcells in feats]


def _emitted(got: Any) -> list[Any]:
    return [([kymesh.Portal(p0, p1, t) for p0, p1, t in portals],
             [(kymesh.BorderPortal(p0, p1, d), first, last, line) for p0, p1, d, first, last, line in borders],
             outline) for portals, borders, outline in got]


def trace_emit(level: Level, areas: Any, per_area: Any, gi0: int, gj0: int, ox: float, oz: float,
               cfg: Cfg = DEFAULTS) -> list[Any]:
    return _emitted(level.native().trace_emit([a['cells'] for a in areas], list(per_area), int(gi0), int(gj0),
                                              (float(ox), float(oz)), float(cfg.pitch), float(cfg.y_offset), LINK_RULE))


def line_walkable(level: Level, k0: int, a: tuple[int, int], b: tuple[int, int], gi0: int, gj0: int, k1: int) -> bool:
    nat = level.native()
    nat.set_blocked([(int(x), int(y)) for x, y in level.blocked])
    return cast(bool, nat.line_walkable(int(k0), (int(a[0]), int(a[1])), (int(b[0]), int(b[1])), int(gi0), int(gj0),
                                        int(k1), LINK_RULE))


def build_graph(level: Level, areas: Any, area_of: dict[int, int], traced: Any, borders: Any, gi0: int, gj0: int,
                ox: float, oz: float, new_id: Any, orders: Any = (), ladders: Any = (),
                cfg: Cfg = DEFAULTS) -> tuple[list[Any], list[Any]]:
    """A sector's route graph in the Rust core (bc2_navbuild::sector), its new nodes given ids by new_id."""
    detailed = abs(cfg.pitch - DETAILED_PITCH) < 1e-6
    cover = cfg.node_cover[1 if detailed else 0]
    s = level.s
    gcfg = (float(cfg.pitch), float(cfg.y_offset), _window(cfg), float(cfg.dist_edge_max), LINK_RULE,
            float(cover) if cover else None, detailed, bool(cfg.order_nodes), 3.0, (float(s.min_x), float(s.min_z)))
    mids = [[(float((p.p0[0] + p.p1[0]) / 2), float((p.p0[2] + p.p1[2]) / 2)) for p in portals if not p.target < ai]
            for ai, (portals, _b, _o) in enumerate(traced)]
    job = (0, int(gi0), int(gj0), (float(ox), float(oz)), mids,
           [(tuple(float(v) for v in pos), int(k)) for _nid, pos, k in borders],
           [tuple(float(v) for v in p) for p in orders], [(int(a), int(b)) for a, b in ladders],
           [(int(a), int(b)) for a, b in level.blocked])
    nat = level.native()
    nodes, pairs, _order_at = nat.graph_many(nat.areas_from([[a['cells'] for a in areas]]), [job], gcfg)[0]
    pts = [(new_id(), pos, k, i, j) if b < 0 else (borders[b][0], borders[b][1], k, i, j)
           for b, pos, k, i, j in nodes]
    return pts, pairs


def add_cover_nodes(level: Level, areas: Any, pts: list[Any], add: Any, ox: float, oz: float, cover: float,
                    cfg: Cfg = DEFAULTS) -> None:
    level.native().cover_nodes([a['cells'] for a in areas], pts, add, (float(ox), float(oz)), float(cfg.pitch),
                               float(cfg.y_offset), float(cover))


def gabriel_prune(pts: Any, pairs: Any) -> list[Any]:
    pairs = [(int(a), int(b), float(d)) for a, b, d in pairs]
    return [pairs[n] for n in require_native().navkit_gabriel([tuple(float(v) for v in p[1]) for p in pts], pairs)]


def check_defaults() -> None:
    """The settings these checks use are the Rust defaults (bc2_native.navkit_settings())."""
    cfg, sw = require_native().navkit_settings()
    d = DEFAULTS
    check((cfg['pitch'], tuple(cfg['window']), cfg['tile'], cfg['tiles'], cfg['link_step'], cfg['y_offset'],
           cfg['dist_edge_max'], tuple(cfg['node_cover']), cfg['order_nodes']) ==
          (d.pitch, _window(d), d.tile, d.tiles, d.link_step, d.y_offset, d.dist_edge_max, d.node_cover, d.order_nodes),
          f'the checks\' settings are the Rust defaults ({cfg})')
    check((sw['link_rule'], sw['area_link'], sw['max_area'], sw['order_node_min'], sw['gabriel']) ==
          (LINK_RULE, AREA_LINK, None, 3.0, 'detailed'), f'the checks\' switches are the Rust defaults ({sw})')
    dd = d.at_pitch(DETAILED_PITCH)
    check((dd.window, dd.tile, dd.tiles, dd.link_step) == (range(-1, 502), 100, 6, 0.3), 'the detailed grid')



# ------------------------------------------------------------------------------------------------ recorded answers

def canon(v: Any) -> Any:
    """v as plain JSON values, the same for equal answers whatever their Python or NumPy types: dicts as their items
    sorted, sequences as lists, floats by repr (exact), arrays as dtype, shape and the SHA-1 of their bytes."""
    if isinstance(v, np.ndarray):
        return ['ndarray', v.dtype.str, list(v.shape), hashlib.sha1(np.ascontiguousarray(v).tobytes()).hexdigest()]
    if isinstance(v, np.generic):
        v = v.item()
    if isinstance(v, bool) or v is None or isinstance(v, (int, str)):
        return v
    if isinstance(v, float):
        return ['f', repr(v)]
    if isinstance(v, dict):
        items = [[canon(k), canon(x)] for k, x in v.items()]
        return ['dict', sorted(items, key=lambda kv: json.dumps(kv[0]))]
    if isinstance(v, (list, tuple, range)):
        return [canon(x) for x in v]
    if isinstance(v, (set, frozenset)):
        return ['set', sorted((canon(x) for x in v), key=json.dumps)]
    if hasattr(v, '__dataclass_fields__'):
        return [type(v).__name__] + [canon(getattr(v, f)) for f in v.__dataclass_fields__]
    raise TypeError(f'canon: {type(v).__name__}')


def digest(v: Any) -> str:
    return hashlib.sha1(json.dumps(canon(v), separators=(',', ':')).encode()).hexdigest()


def grid(rng: random.Random, nx: int, nz: int, f32: bool, pitch: float = 0.8) -> Level:
    """A random level: 0-3 surfaces per column on a few floors, rough heights, random keep / stair / terrain."""
    start, ys, nrm, hit = [0], [], [], []
    for _j in range(nz):
        for _i in range(nx):
            floors = rng.sample([0.0, 2.5, 5.0], rng.choice([0, 1, 1, 1, 2, 3]))
            for f in sorted(floors):
                ys.append(f + rng.choice([0.0, 0.1, 0.25, 0.3, 0.55, 0.62, 0.9]) * rng.random())
                n = np.array([rng.uniform(-0.6, 0.6), 1.0, rng.uniform(-0.6, 0.6)])
                nrm.append(n / np.linalg.norm(n))
                hit.append((navwalk.TERRAIN_SHAPE_VTABLE if rng.random() < 0.7 else 1, 0, 0))
            start.append(len(ys))
    count = len(ys)
    s: Any = SimpleNamespace(nx=nx, nz=nz, pitch=pitch, min_x=0.0, min_z=0.0, start=np.array(start, np.int64),
                             y=np.array(ys, np.float32), normal=np.array(nrm, np.float32).reshape(-1, 3),
                             hit=np.array(hit, np.uint32).reshape(-1, 3),
                             flags=np.array([rng.random() < 0.1 for _ in range(count)], np.uint8))
    keep = np.array([rng.random() < 0.85 for _ in range(count)], bool)
    stair = np.array([rng.random() < 0.1 for _ in range(count)], bool)
    if f32:
        return Level(s, keep, 0.28, stair=stair, cfg=DEFAULTS.at_pitch(pitch))
    gx = np.array([rng.uniform(-0.8, 0.8) for _ in range(count)], np.float64)
    gz = np.array([rng.uniform(-0.8, 0.8) for _ in range(count)], np.float64)
    return Level(s, keep, 0.28, gx, gz, stair, cfg=DEFAULTS.at_pitch(pitch))


def search(lv: Level, starts: list[int], other: set[int], cells: dict[int, tuple[int, int]]) -> Any:
    """build_graph's route-group search (NavLevel.search) over the level's window."""
    window = lv.cfg.window
    return lv.native().search(starts, other, cells, (window.start, window.stop), 0, 0, LINK_RULE)


def walks(rng: random.Random) -> dict[str, Any]:
    """Every grid walk's answers on the seeded random levels, as digests per level and walk."""
    out: dict[str, Any] = {'levels': [], 'walkable': []}
    dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1), (2, 1), (0, 0)]
    for case in range(24):
        f32 = case % 2 == 0
        pitch = 0.8 if case % 3 else 0.2
        lv = grid(rng, rng.randint(3, 9), rng.randint(3, 9), f32, pitch)
        s = lv.s
        col = np.repeat(np.arange(s.nx * s.nz), np.diff(s.start))
        got: dict[str, Any] = {}
        # neighbour: every surface, direction and rule, the column its own or another
        ns = []
        for k in range(len(s.y)):
            c = int(col[k])
            for gi, gj in ((c % s.nx, c // s.nx), (rng.randint(-1, s.nx), rng.randint(-1, s.nz))):
                for di, dj in dirs:
                    for rule in (None, 'slope', 'raw', 'terrain'):
                        ns.append(lv.neighbour(k, gi, gj, di, dj, rule=rule))
        got['neighbour'] = ns
        # areas (components), traced sides, lines and searches over each sector-like window
        areas = build_areas(lv, 0, 0, cfg=lv.cfg)
        got['build_areas'] = areas
        members = {k: (int(col[k] % s.nx), int(col[k] // s.nx)) for k in range(len(s.y)) if lv.keep[k]}
        got['components'] = components(lv, members, 0, 0)
        area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
        got['trace_edges'] = trace_edges(lv, areas, area_of, 0, 0, lv.cfg)
        kept = [k for k in range(len(s.y)) if lv.keep[k]]
        if kept:
            cell = {k: (int(col[k] % s.nx), int(col[k] // s.nx)) for k in kept}
            lines = []
            for blocked in (set(), {(rng.choice(kept), rng.choice(kept)) for _ in range(len(kept) // 2)}):
                lv.blocked = blocked
                for _ in range(60):
                    k0, k1 = rng.choice(kept), rng.choice(kept)
                    pa, pb = cell[k0], (rng.randint(-1, s.nx), rng.randint(-1, s.nz))
                    lines.append((k0, pa, pb, k1, line_walkable(lv, k0, pa, pb, 0, 0, k1)))
            got['line_walkable'] = lines
            # the search, as build_graph runs it: groups of nodes on surfaces, union-find, cell map
            nodes = rng.sample(kept, min(len(kept), 6))
            node_at = {k: i for i, k in enumerate(nodes)}
            parent = [0, 0, 2, 2, 4, 5][:len(nodes)]

            def find(x: int, parent: list[int] = parent) -> int:
                while parent[x] != x:
                    x = parent[x]
                return x
            found = []
            for root in sorted({find(i) for i in range(len(nodes))}):
                group = [i for i in range(len(nodes)) if find(i) == root]
                other = {k for k, at in node_at.items() if find(at) != root}
                cells = {nodes[m]: cell[nodes[m]] for m in group}
                found.append((search(lv, [nodes[m] for m in group], other, cells), cells))
            got['search'] = found
        out['levels'].append({k: digest(v) for k, v in got.items()})
    # walkability (navwalk.walkable: its flood, roof islands), islands_of and matched_flags, float32 and float64
    for case in range(16):
        lv = grid(rng, rng.randint(4, 12), rng.randint(4, 12), case % 2 == 0)
        s = lv.s
        seeds = [(rng.uniform(0, s.nx * s.pitch), rng.uniform(0, 3), rng.uniform(0, s.nz * s.pitch)) for _ in range(3)]
        # a second (high-capsule) run over the same grid: the same surfaces, heights moved by up to 0.08 m, its flags
        hi: Any = SimpleNamespace(**s.__dict__)
        hi.y = (s.y + np.array([rng.choice([0.0, 0.02, -0.049, 0.06, 0.08]) for _ in s.y], np.float32)).astype(np.float32)
        hi.flags = np.array([rng.randrange(4) for _ in s.y], np.uint8)
        got = {'matched_flags': navwalk.matched_flags(s, hi)}
        a = navwalk.walkable(s, return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1],
                             high_capsule=hi)
        got['walkable'] = a
        got['islands_of'] = [navwalk.islands_of(s, keep, lv.gx, lv.gz, lv.stair, 0.28) for keep in (lv.keep, a[0])]
        out['walkable'].append({k: digest(v) for k, v in got.items()})
    return out


def load() -> dict[str, Any]:
    try:
        with open(GOLDEN, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def compare(name: str, got: Any, want: Any) -> int:
    """Differences between got and the recorded want (lists of {walk: digest}), each reported."""
    bad = 0
    if len(got) != len(want):
        check(False, f'{name}: {len(got)} cases, {len(want)} recorded')
        return 1
    for case, (g, w) in enumerate(zip(got, want)):
        for walk in sorted(set(g) | set(w)):
            if g.get(walk) != w.get(walk):
                bad += 1
                check(False, f'{name} case {case}: {walk} differs from the recording')
    return bad


TWINS: dict[str, list[str]] = {}            # this run's digests per section (E2a / E3c / E3h)
WANT: dict[str, list[str]] = {}             # the recording's
MODE = {'record': False}


@contextlib.contextmanager
def retired_lips() -> Iterator[None]:
    """The walkability the retired Python's answers were recorded with: object lips held to flat floor only above
    STEEP_TERRAIN_MAX (navwalk.LIP_OBJECT_FLAT_ALL off; on since round NKS-deep D by the user's ruling, 2026-11-27).
    The recorded answers test the port, not the rule; the whole builds (--game) run the shipped defaults."""
    old = navwalk.LIP_OBJECT_FLAT_ALL
    navwalk.LIP_OBJECT_FLAT_ALL = False
    try:
        yield
    finally:
        navwalk.LIP_OBJECT_FLAT_ALL = old


def twin(section: str, name: str, got: Any) -> None:
    """got (the Rust core's answer) against the retired Python's, recorded as a digest in navbuild_golden.json
    ('retired', round R1); recording (--record), got's digest is written instead."""
    n = len(TWINS.setdefault(section, []))
    g = digest(got)
    TWINS[section].append(g)
    if MODE['record']:
        return
    rec = WANT.get(section, [])
    check(n < len(rec) and rec[n] == g, f'{name}: differs from the recorded answer of the retired Python'
          if n < len(rec) else f'{name}: not recorded ({section} #{n})')


def quick(golden: dict[str, Any], record: bool) -> None:
    require_native()
    check_defaults()
    MODE['record'] = record
    WANT.update(golden.get('retired', {}))
    rng = random.Random(943)
    got = walks(rng)
    if record:
        golden['quick'] = got
        print(f'{len(got["levels"])} + {len(got["walkable"])} random levels recorded')
    else:
        want = golden.get('quick', {})
        bad = compare('grid walks', got['levels'], want.get('levels', [])) + \
            compare('walkable', got['walkable'], want.get('walkable', []))
        print(f'grid walks on {len(got["levels"])} + {len(got["walkable"])} random levels: {bad} differ from the '
              f'recording')
    others(rng)
    calls = float_ops(rng) + rules(rng) + areas_and_picks(rng)
    print(f'E2a: {calls} calls against the recorded answers: {len(FAILS)} differences')
    before = len(FAILS)
    calls = e3c(rng)
    print(f'E3c: {calls} calls against the recorded answers: {len(FAILS) - before} differences')
    before = len(FAILS)
    calls = e3h(random.Random(7731))
    print(f'E3h: {calls} calls against the recorded answers: {len(FAILS) - before} differences')
    if record:
        golden['retired'] = TWINS
    for sec, answers in TWINS.items():
        check(len(answers) == len(WANT.get(sec, answers)),
              f'{sec}: {len(answers)} answers, {len(WANT.get(sec, []))} recorded')
    print('answers recorded from the retired Python: ' + ', '.join(f'{k} {len(v)}' for k, v in sorted(TWINS.items())))


def others(rng: random.Random) -> None:
    """Arrays of other types are cast as the core takes them: the same answers as the cast arrays give; a surface
    outside the level."""
    lv = grid(rng, 4, 4, True)
    first = int(np.nonzero(lv.keep)[0][0])
    c = int(np.searchsorted(lv.s.start, first, side='right')) - 1          # its column
    try:
        lv.neighbour(10 ** 6, c % lv.s.nx - 1, c // lv.s.nx, 1, 0)
        check(False, 'a surface outside the level must raise')
    except ValueError:
        pass
    check(cast(Any, lv).neighbour(np.int64(0), np.int64(0), np.int64(0), 1, 0) == lv.neighbour(0, 0, 0, 1, 0),
          'numpy integers')
    odd: Any = grid(rng, 5, 5, False)
    ref: Any = Level(odd.s, odd.keep, 0.28, odd.gx.astype(np.float16).astype(np.float64), odd.gz, odd.stair,
                        cfg=odd.cfg)
    odd.gx = odd.gx.astype(np.float16)                                     # widened to float64
    odd.keep = odd.keep.astype(np.uint8)                                   # a value's truth
    check(odd.native() is not None, 'float16 gradients, uint8 keep: a native level')
    s = odd.s
    for k in range(len(s.y)):
        for di, dj in ((1, 0), (1, 1), (0, -1)):
            gi, gj = int(np.searchsorted(s.start, k, side='right')) - 1, 0
            gi, gj = gi % s.nx, gi // s.nx
            check(odd.neighbour(k, gi, gj, di, dj) == ref.neighbour(k, gi, gj, di, dj), f'cast arrays: neighbour {k}')
    a = navwalk.islands_of(s, odd.keep, odd.gx, odd.gz, odd.stair.astype(np.int8), 0.28)
    b = navwalk.islands_of(s, ref.keep, ref.gx, ref.gz, ref.stair, 0.28)
    check(a[1] == b[1] and np.array_equal(a[0], b[0]), 'cast arrays: islands_of')
    hi: Any = SimpleNamespace(**s.__dict__)
    hi.flags = s.flags.astype(np.int32)
    check(np.array_equal(navwalk.matched_flags(s, hi), navwalk.matched_flags(s, s)), 'matched_flags: int32 flags cast')


def same(a: Any, b: Any) -> bool:
    """The same array bit for bit (dtype, shape, bytes), or the same tuple of such arrays."""
    if isinstance(a, tuple):
        return isinstance(b, tuple) and len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    return a.dtype == b.dtype and a.shape == b.shape and a.tobytes() == b.tobytes()


def float_ops(rng: random.Random) -> int:
    """CPython's math.hypot, x ** 2 and degrees(atan(hypot)), and NumPy's np.hypot (E3c's coverage nodes), as the port
    computes them, on gradients like the rules' (height differences over a pitch) and on values over the whole float
    range."""
    import math
    xs, zs = [], []
    for n in range(150_000):
        if n % 3 == 0:
            xs.append(rng.randint(-60, 60) * 0.05 / rng.choice([0.2, 0.4, 0.8, 1.6]))
            zs.append(rng.uniform(-3, 3) / rng.choice([0.2, 0.4]))
        elif n % 3 == 1:
            xs.append(rng.uniform(-1, 1) * 2.0 ** rng.randint(-1074, 1023))
            zs.append(rng.uniform(-1, 1) * 2.0 ** rng.randint(-1074, 1023))
        else:
            xs.append(float(np.float32(rng.uniform(-500, 500))) - rng.uniform(-500, 500))
            zs.append(rng.choice([0.0, -0.0, xs[-1], -xs[-1], rng.uniform(-1e-300, 1e-300)]))
    xs += [0.0, -0.0, math.inf, -math.inf, math.nan, 1e308, 5e-324]
    zs += [0.0, 3.0, math.nan, 1.0, math.inf, 1e308, 5e-324]
    h, s, d, ch = (np.frombuffer(b, '<f8') for b in navwalk.require_native().navkit_float_ops(
        np.array(xs, '<f8').tobytes(), np.array(zs, '<f8').tobytes()))
    np_hyp = np.hypot(np.array(xs, np.float64), np.array(zs, np.float64))     # NumPy's: the C library's hypot
    bad = 0
    for k, (x, z) in enumerate(zip(xs, zs)):
        try:
            sq = x ** 2
        except OverflowError:                                   # CPython raises where pow overflows
            sq = math.inf
        want = (math.hypot(x, z), sq, math.degrees(math.atan(math.hypot(x, z))), float(np_hyp[k]))
        got = (float(h[k]), float(s[k]), float(d[k]), float(ch[k]))
        if any(not (a == b or (a != a and b != b)) for a, b in zip(want, got)):
            bad += 1
            if bad <= 5:
                check(False, f'float ops ({x!r}, {z!r}): {got} != {want}')
    check(bad == 0, f'float ops: {bad} of {len(xs)} differ')
    # gabriel_prune's port squares array differences as x * x: an array's ** 2 is NumPy's square, a multiply
    a = np.array(xs, np.float64)
    with np.errstate(over='ignore', invalid='ignore'):
        check((a ** 2).tobytes() == (a * a).tobytes(), "float ops: an array's ** 2 is not x * x")
    return len(xs)


def runs(rng: random.Random, nx: int, nz: int, pitch: float, f64_normals: bool = False) -> Any:
    """A random sample run: 0-3 surfaces per column on a few floors with steps and steep pieces."""
    lv = grid(rng, nx, nz, True, pitch)
    s: Any = lv.s
    if f64_normals:
        s.normal = s.normal.astype(np.float64)
    # steeper normals on a third of the surfaces (lips, steep terrain), a few exactly at the lip cosines
    for k in range(len(s.y)):
        if rng.random() < 0.33:
            a = rng.uniform(0.3, 1.2)
            n = np.array([a * rng.choice([-1, 1]), 1.0, rng.uniform(-0.3, 0.3)])
            s.normal[k] = n / np.linalg.norm(n)
        if rng.random() < 0.03:
            s.normal[k, 1] = rng.choice([np.cos(np.radians(15.0)), np.float32(np.cos(np.radians(15.0)))])
    return s


def tris_near(rng: random.Random, s: Any, count: int) -> np.ndarray:
    """Random triangles over (and around) the run's extent, at its floors' heights; some degenerate."""
    out = []
    w, d = s.nx * s.pitch, s.nz * s.pitch
    for _ in range(count):
        x, z, y = rng.uniform(-1, w + 1), rng.uniform(-1, d + 1), rng.choice([0.0, 2.5, 5.0]) + rng.uniform(-0.3, 0.6)
        t = [[x + rng.uniform(-1.5, 1.5), y + rng.uniform(-0.4, 0.4), z + rng.uniform(-1.5, 1.5)] for _ in range(3)]
        if rng.random() < 0.1:
            t[2] = t[1]                                         # a degenerate one
        out.append(t)
    return np.array(out, np.float32)


def rules(rng: random.Random) -> int:
    """navwalk's rules (headroom, stair_slope, narrow_lips, near_objects, on_objects) and walkable with every rule
    switched on (pitch 0.2: the lips; stair, dynamic, exclude and forbidden objects), both backends."""
    calls = 0
    for case in range(14):
        pitch = 0.2 if case % 2 == 0 else 0.8
        s = runs(rng, rng.randint(4, 11), rng.randint(4, 11), pitch, f64_normals=case % 4 == 3)
        n = len(s.y)
        check(navwalk.native_samples(s) is not None, f'rules case {case}: a native grid')
        def mask(p: float, n: int = n) -> np.ndarray:
            return np.array([rng.random() < p for _ in range(n)], bool)
        for ign in (None, mask(0.2), mask(0.0)):
            twin('e2a', f'headroom case {case}', navwalk.headroom(s, ign))
            calls += 1
        for r, tol in ((1, 0.7), (2, 1.1), (4, 0.7), (1, 0.25)):
            m = mask(0.5)
            twin('e2a', f'stair_slope case {case} r {r} tol {tol}', navwalk.stair_slope(s, m, r=r, tol=tol))
            calls += 1
        for rise, flat, width, slope in ((None, None, 2, 40.0), (0.3, None, 2, 40.0), (0.3, 15.0, 2, 40.0),
                                         (None, 15.0, 3, 30.0), (0.05, None, 1, 45.0)):
            base, cand = mask(0.6), mask(0.4)
            twin('e2a', f'narrow_lips case {case} rise {rise} flat {flat} width {width}',
                 navwalk.narrow_lips(s, base, cand, slope, width, rise=rise, flat=flat))
            calls += 1
        tris = tris_near(rng, s, 30)
        for grow, below, above in ((0.5, 2.2, 0.3), (0.4, 0.6, 0.6), (0.1, -0.05, 0.0), (1, 1, 0)):
            twin('e2a', f'near_objects case {case}', navwalk.near_objects(s, tris, grow, below, above))
            calls += 1
        for tol in (0.2, 0.08, 0.5):
            twin('e2a', f'on_objects case {case}', navwalk.on_objects(s, tris, tol))
            calls += 1
        seeds = [(rng.uniform(0, s.nx * s.pitch), rng.uniform(0, 3), rng.uniform(0, s.nz * s.pitch)) for _ in range(3)]
        kw: dict[str, Any] = dict(return_gradients=True, return_stairs=True, extra_seeds=seeds, anchors=seeds[:1],
                                  dynamic_tris=tris_near(rng, s, 6), stair_tris=tris_near(rng, s, 6),
                                  exclude_tris=tris_near(rng, s, 4), exclude_footprint_tris=tris_near(rng, s, 3),
                                  forbid_tris=tris_near(rng, s, 5))
        with retired_lips():
            twin('e2a', f'walkable (every rule) case {case}', navwalk.walkable(s, **kw))
        calls += 1
    # numbers of another kind are cast as the core takes them; triangles of another shape raise
    s = runs(rng, 5, 5, 0.2)
    s.pitch = np.float32(0.2)
    check(navwalk.native_samples(s) is not None, 'a float32 pitch: cast to a float')
    s = runs(rng, 5, 5, 0.2)
    try:
        navwalk.near_objects(s, np.zeros((2, 9)))
        check(False, 'triangles not n x 3 x 3 must raise')
    except ValueError:
        pass
    zero = np.zeros((2, 3, 3), np.float32)                                     # two triangles in one point
    twin('e2a', 'near_objects on point triangles', navwalk.near_objects(s, zero))
    nan_tris = tris_near(rng, s, 3)
    nan_tris[1, 0, 0] = np.nan                                                 # the core refuses (the Python raised)
    try:
        navwalk.near_objects(s, nan_tris)
        check(False, 'near_objects with a NaN x must raise')
    except ValueError:
        pass
    twin('e2a', 'on_objects with a NaN x', navwalk.on_objects(s, nan_tris))
    twin('e2a', 'stair_slope with a NumPy r', navwalk.stair_slope(s, np.ones(len(s.y), bool), r=cast(Any, np.int64(1))))
    return calls


def areas_and_picks(rng: random.Random) -> int:
    """build_areas (tiles, components, bisection) against the recording; the sector graph's centre and portal cell
    picks (navkit_centre_cells, navkit_nearest_cells) against the rule written out here."""
    calls = 0
    saved = dict(BISECT)
    try:
        for case in range(12):
            lv = grid(rng, rng.randint(5, 14), rng.randint(5, 14), case % 2 == 0, 0.8 if case % 3 else 0.2)
            BISECT['on'], BISECT['max_area'] = case % 3 != 0, rng.choice([1, 2, 3, 5, 25])
            for start, tile, tiles in (((-1, -1), 3, 3), ((0, 0), 4, 2), ((2, -1), 1, 6), ((0, 3), 100, 1)):
                twin('e2a', f'build_areas case {case} start {start} tile {tile} bisect {BISECT['on']}',
                     build_areas(lv, 0, 0, start, tile, tiles, cfg=lv.cfg))
                calls += 1
            areas = build_areas(lv, 0, 0, cfg=lv.cfg)
            big = [x for x in areas if len(x['cells']) >= 4]
            got = require_native().navkit_centre_cells([x['cells'] for x in big])
            want: list[int] = []
            for x in big:
                ci = sum(c[0] for c in x['cells']) / len(x['cells'])
                cj = sum(c[1] for c in x['cells']) / len(x['cells'])
                keys = [(c[0] - ci) ** 2 + (c[1] - cj) ** 2 for c in x['cells']]
                want.append(keys.index(min(keys)))
            check(got == want, f'centre cells case {case}')
            ox, oz = rng.choice([0.0, -300.0, 1200.0, -910.4000244140625]), rng.choice([0.0, 500.0, -100.0])
            ends: list[tuple[int, list[tuple[float, float]]]] = []
            for ai, x in enumerate(areas):
                i0, j0 = min(c[0] for c in x['cells']), min(c[1] for c in x['cells'])
                mids = [(ox + (i0 + rng.choice([-0.5, 0.5, 1.5, 3.0])) * lv.cfg.pitch,
                         oz + (j0 + rng.choice([-0.5, 0.0, 2.5])) * lv.cfg.pitch) for _ in range(rng.randint(0, 3))]
                ends.append((ai, mids))
            got2 = require_native().navkit_nearest_cells([(areas[ai]['cells'] if mids else [], mids)
                                                             for ai, mids in ends], ox, oz, lv.cfg.pitch)
            want2: list[list[int]] = []
            for ai, mids in ends:
                want2.append([])
                for mx, mz in mids:
                    keys = [(ox + c[0] * lv.cfg.pitch - mx) ** 2 + (oz + c[1] * lv.cfg.pitch - mz) ** 2
                            for c in areas[ai]['cells']]
                    want2[-1].append(keys.index(min(keys)))
            check(got2 == want2, f'portal cells case {case}')
            calls += 2
    finally:
        BISECT.update(saved)
    return calls


# ------------------------------------------------------------------------------------------------ round E3c

def ordered(v: Any) -> Any:
    """v as plain values, dicts as their items in order (insertion order is the order runs come out in), floats by
    repr (exact; a float64 scalar as the float it is), the mesh's portal objects as tuples."""
    if isinstance(v, np.ndarray):
        return ['ndarray', v.dtype.str, list(v.shape), v.tobytes().hex()]
    if isinstance(v, (bool, np.bool_)):
        return bool(v)
    if isinstance(v, (int, np.integer)):
        return int(v)
    if isinstance(v, (float, np.floating)):
        return ['f', repr(float(v))]
    if v is None or isinstance(v, str):
        return v
    if isinstance(v, dict):
        return ['dict', [[ordered(k), ordered(x)] for k, x in v.items()]]
    if isinstance(v, (list, tuple)):
        return [ordered(x) for x in v]
    if isinstance(v, (set, frozenset)):
        return ['set', sorted((ordered(x) for x in v), key=repr)]
    if hasattr(v, '__dataclass_fields__'):
        return [type(v).__name__] + [ordered(getattr(v, f)) for f in v.__dataclass_fields__]
    raise TypeError(f'ordered: {type(v).__name__}')


def ramps(rng: random.Random, nx: int, nz: int, pitch: float) -> Level:
    """A random level for the tracing: ground on ramps (steps up to 0.95 m a cell, normals to match, so the slope rule
    links cells the detail step breaks), holes, thin second layers (one-way candidates) and decks 2.5 m over part of
    it."""
    ax, az = rng.uniform(-0.95, 0.95), rng.uniform(-0.4, 0.4)
    deck = (rng.randint(0, nx), rng.randint(0, nz), rng.randint(1, nx), rng.randint(1, nz))
    start, ys, nrm, hit = [0], [], [], []
    for j in range(nz):
        for i in range(nx):
            gx, gz = (ax if (i // 3) % 2 else 0.1 * ax), az        # rise per cell
            col = []
            if rng.random() > 0.08:
                col.append((gx * i + gz * j + rng.choice([0.0, 0.0, 0.05, 0.3]), gx / pitch, gz / pitch))
                if rng.random() < 0.12:                         # a thin second layer (a stair edge, a split-off sheet)
                    col.append((col[0][0] + rng.choice([0.15, 0.3, 0.45]), gx / pitch, gz / pitch))
            if deck[0] <= i < deck[0] + deck[2] and deck[1] <= j < deck[1] + deck[3] and rng.random() > 0.1:
                col.append((gx * i + gz * j + 2.5, 0.0, 0.0))
            for y, gxm, gzm in sorted(col):
                ys.append(y)
                n = np.array([-gxm, 1.0, -gzm])
                nrm.append(n / np.linalg.norm(n))
                hit.append((navwalk.TERRAIN_SHAPE_VTABLE if rng.random() < 0.8 else 1, 0, 0))
            start.append(len(ys))
    count = len(ys)
    s: Any = SimpleNamespace(nx=nx, nz=nz, pitch=pitch, min_x=0.0, min_z=0.0, start=np.array(start, np.int64),
                             y=np.array(ys, np.float32), normal=np.array(nrm, np.float32).reshape(-1, 3),
                             hit=np.array(hit, np.uint32).reshape(-1, 3), flags=np.zeros(count, np.uint8))
    return Level(s, np.array([rng.random() < 0.95 for _ in range(count)], bool), 0.28,
                    stair=np.array([rng.random() < 0.03 for _ in range(count)], bool), cfg=DEFAULTS.at_pitch(pitch))


def windowed(rng: random.Random, lv: Level) -> Cfg:
    """lv's settings with a window over part of its grid (border portals where it ends)."""
    s = lv.s
    return replace(lv.cfg, window=range(-1, rng.randint(2, max(3, min(s.nx, s.nz) + 1))))


def trace_chain(lv: Level, cfg: Cfg, gi0: int, gj0: int, ox: float, oz: float, keep: set[int],
                many: bool, tiling: tuple[tuple[int, int], int, int]) -> Any:
    """stage's tracing of one sector: areas (tiling: start, samples per tile, tiles), trace_edges (or
    _trace_edges_many), drop_one_way, detail_pass, trace_emit."""
    start, tile, tiles = tiling
    areas = build_areas(lv, gi0, gj0, start, tile, tiles, cfg=cfg)
    areas.sort(key=lambda a: (a['tile'][0], a['tile'][1], min(lv.y[c[2]] for c in a['cells'])))
    area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
    if many:
        got = _trace_edges_many(lv, [((0, 0), {'areas': areas, 'origin': (gi0, gj0, ox, oz)})], cfg)
        check(got is not None, 'trace_edges_many: native')
        per_area, one_way = got[(0, 0)] if got else trace_edges(lv, areas, area_of, gi0, gj0, cfg)
    else:
        per_area, one_way = trace_edges(lv, areas, area_of, gi0, gj0, cfg)
    traced = ordered((per_area, one_way))
    per_area = drop_one_way(lv, per_area, one_way, keep)
    per_area, details = detail_pass(lv, areas, per_area, ox, oz, gi0, gj0, cfg)
    before = ordered(per_area)
    emitted = trace_emit(lv, areas, per_area, gi0, gj0, ox, oz, cfg)
    return traced, before, ordered(details), sorted(lv.blocked), ordered(emitted)


def sector_graph(lv: Level, cfg: Cfg, ox: float, oz: float, tiling: Any, rng_seed: int,
                 ladders: int = 0) -> Any:
    """stage's graph of one sector on whichever backend is switched on: areas, tracing, the D sides, border nodes on
    the window's edge columns, build_graph (cover nodes, pairs, Gabriel, groups joined, order nodes)."""
    rng = random.Random(rng_seed)
    start, tile, tiles = tiling
    areas = build_areas(lv, 0, 0, start, tile, tiles, cfg=cfg)
    areas.sort(key=lambda a: (a['tile'][0], a['tile'][1], min(lv.y[c[2]] for c in a['cells'])))
    for n, a in enumerate(areas):
        a['layer'] = n
    area_of = {c[2]: ai for ai, a in enumerate(areas) for c in a['cells']}
    per_area, one_way = trace_edges(lv, areas, area_of, 0, 0, cfg)
    per_area = drop_one_way(lv, per_area, one_way, set())
    per_area, _details = detail_pass(lv, areas, per_area, ox, oz, 0, 0, cfg)
    traced = trace_emit(lv, areas, per_area, 0, 0, ox, oz, cfg)
    cells = [c for a in areas for c in a['cells']]
    borders = []
    for n in range(rng.randint(0, 6) if cells else 0):
        i, j, k = rng.choice(cells)
        borders.append((0x80000000 | (900 + n), (ox + i * cfg.pitch, float(lv.y[k]) + cfg.y_offset, oz + j * cfg.pitch),
                        k))
    orders = [(ox + i * cfg.pitch, float(lv.y[k]), oz + j * cfg.pitch) for i, j, k in rng.sample(cells, min(3, len(cells)))]
    ids = [0]

    def new_id() -> int:
        ids[0] += 1
        return ids[0]
    lads = [(rng.choice(cells)[2], rng.choice(cells)[2]) for _ in range(ladders if cells else 0)]
    pts, pairs = build_graph(lv, areas, area_of, traced, borders, 0, 0, ox, oz, new_id, orders, lads, cfg)
    return ordered(pts), ordered(pairs)


def e3c(rng: random.Random) -> int:
    """Round E3c's pieces on random levels against the answers recorded from their retired Python twin: trace_edges
    (one sector and the parallel batch), trace_emit after drop_one_way and the detail pass, whole sector graphs,
    build_areas_many, near_characters, roof_islands, add_cover_nodes, gabriel_prune; navsample.load's records against
    its Python reader (a format reader: kept)."""
    calls = 0
    seen = dict(portals=0, borders=0, outline=0, one_way=0, details=0)
    for case in range(60):
        pitch = 0.8 if case % 3 else 0.2
        n1, n2 = rng.randint(4, 16), rng.randint(4, 16)
        lv = ramps(rng, n1, n2, pitch) if case % 2 else grid(rng, n1, n2, case % 4 == 0, pitch)
        cfg = windowed(rng, lv) if case % 4 else lv.cfg
        gi0, gj0 = rng.choice([0, 0, 1, 2]), rng.choice([0, 0, 1])
        ox, oz = rng.choice([0.0, -300.0, 1200.0, -910.4000244140625]), rng.choice([0.0, 500.0, -100.0, 12.8])
        keep = {n for n in range(64) if rng.random() < 0.5}
        tiling = rng.choice([((-1, -1), cfg.tile, cfg.tiles), ((-1, -1), 3, 6), ((0, 0), 4, 4), ((-1, 0), 2, 9)])
        a = trace_chain(lv, cfg, gi0, gj0, ox, oz, keep, case % 3 == 1, tiling)
        seen['one_way'] += len(a[0][1])
        seen['details'] += sum(len(x[0]) + len(x[1]) for x in a[2])
        for kind, n in (('portals', 0), ('borders', 1), ('outline', 2)):
            seen[kind] += sum(len(x[n]) for x in a[4])
        for name, x in zip(('trace_edges', 'drop / detail', 'detail_pass', 'blocked', 'trace_emit'), a):
            twin('e3c', f'{name} case {case} (pitch {pitch}, window {cfg.window})', x)
        calls += 3
    check(all(seen.values()), f'the random levels make every kind of edge: {seen}')
    print(f'E3c tracing: {seen}')
    # whole route graphs (cover nodes on the detailed pitch, Gabriel there, the groups joined, order nodes)
    joined = 0
    for case in range(36):
        pitch = 0.2 if case % 2 else 0.8
        n1, n2 = rng.randint(6, 26), rng.randint(6, 26)
        lv = ramps(rng, n1, n2, pitch) if case % 3 else grid(rng, n1, n2, case % 4 == 0, pitch)
        cfg = windowed(rng, lv) if case % 4 == 1 else lv.cfg
        if case % 5 == 2:
            cfg = replace(cfg, dist_edge_max=rng.choice([3.0, 1.6]))         # shorter edges: more groups to join
        ox, oz = rng.choice([0.0, -910.4000244140625, 300.0]), rng.choice([0.0, 12.8, -100.0])
        tiling = rng.choice([((-1, -1), cfg.tile, cfg.tiles), ((-1, -1), 4, 6), ((0, 0), 7, 4)])
        seed = rng.randrange(1 << 30)
        a = sector_graph(lv, cfg, ox, oz, tiling, seed)
        twin('e3c', f'build_graph case {case} (pitch {pitch}, {len(a[0])} nodes, {len(a[1])} pairs)', a)
        joined += sum(1 for p in a[0] if p[0] < 900)
        calls += 4
    print(f'E3c route graphs: {joined} nodes')
    # build_areas for a stage's sectors at once (with each area's lowest and highest ground) against build_areas
    for case in range(16):
        lv = ramps(rng, rng.randint(6, 20), rng.randint(6, 20), 0.8 if case % 2 else 0.2) if case % 3 else             grid(rng, rng.randint(6, 20), rng.randint(6, 20), case % 2 == 0)
        specs = [(gi, gj, 0.0, 0.0, rng.choice([(-1, -1), (0, 0), (0, -1)]), rng.choice([3, 4, 50]),
                  rng.choice([1, 3, 6])) for gi, gj in ((0, 0), (2, 1), (-1, 3), (5, 0))]
        many = _build_areas_many(lv, specs, lv.cfg)
        twin('e3c', f'build_areas_many case {case}', ordered(many))
        calls += 1
    # soldiers' surroundings (near_characters): random characters, reaches and height bands
    for case in range(16):
        s = runs(rng, rng.randint(3, 12), rng.randint(3, 12), 0.2 if case % 2 else 0.8)
        char = np.array([rng.random() < 0.08 for _ in range(len(s.y))], bool)
        grow, below, above = rng.choice([0.9, 0.5, 1.7, 0.8]), rng.choice([2.2, 0.5]), rng.choice([0.3, 1.0])
        twin('e3c', f'near_characters case {case}', navwalk.near_characters(s, char, grow, below, above))
        calls += 1
    # the sample files' records (versions 1 and 2; a run cut off between columns, a record cut short): read in the
    # Rust core (its Python was retired in round RT2 against its recorded answers: test_f3_native's navsample12 area)
    import navsample
    with tempfile.TemporaryDirectory() as tmp:
        for case in range(12):
            version = 1 + case % 2
            nx, nz = rng.randint(1, 9), rng.randint(1, 9)
            body = bytearray()
            for _c in range(nx * nz if case % 4 else rng.randint(0, nx * nz)):
                n = rng.choice([0, 1, 1, 2, 3, 5])
                body.append(n)
                for _ in range(n):
                    body += struct.pack('<f3bB', rng.uniform(-50, 50), *(rng.randint(-128, 127) for _ in range(3)),
                                        rng.randrange(256))
                    if version == 2:
                        body += struct.pack('<3I', *(rng.randrange(1 << 32) for _ in range(3)))
            if case == 11:
                body = body[:-3]                                   # a record cut short: struct.error, as before
            head = navsample.HEADER.pack(b'ONS1', version, 0.8, -12.0, 4.0, nx, nz, 50.0, -50.0, 0.4, 0.6, 1.8,
                                         0.05, 0)
            path = os.path.join(tmp, f'run{case}.bin')
            with open(path, 'wb') as f:
                f.write(head + bytes(body))

            try:
                smp, c = navsample.load(path)
            except struct.error as e:
                check(case == 11 and 'unpack_from requires' in str(e), f'navsample.load case {case} (version {version}): {e}')
            else:
                n = int(smp.start[-1])
                check(case != 11 and len(smp.start) == nx * nz + 1 and len(smp.y) == n == len(smp.flags)
                      and smp.normal.shape == (n, 3) and smp.hit is not None
                      and smp.hit.shape == ((n if version == 2 else 0), 3)
                      and isinstance(smp.y.base, memoryview) and c <= nx * nz,
                      f'navsample.load case {case} (version {version}): the Rust core\'s arrays')
            calls += 1
    # roof islands: random keep and anchors, the rule's settings varied
    saved = (navwalk.ROOF_RAISE, navwalk.ROOF_SHARE, navwalk.ROOF_NEAR, navwalk.ROOF_NEAR_DY)
    try:
        for case in range(24):
            lv = grid(rng, rng.randint(4, 14), rng.randint(4, 14), case % 2 == 0, 0.8 if case % 3 else 0.2)
            s = lv.s
            navwalk.ROOF_RAISE, navwalk.ROOF_SHARE, navwalk.ROOF_NEAR, navwalk.ROOF_NEAR_DY = (
                rng.choice([2.0, 0.5, 0.1]), rng.choice([0.5, 0.0, 0.2, 1.0]), rng.choice([2.0, 0.8, 5.0, 1]),
                rng.choice([2.5, 0.3]))
            kmask = np.array([rng.random() < 0.7 for _ in range(len(s.y))], bool)
            kept = np.nonzero(kmask)[0].tolist()
            anchors = set(rng.sample(kept, min(len(kept), rng.randint(0, 3))))
            terrain = lv.terrain
            navwalk.ROOF_STATS.clear()
            x = navwalk.roof_islands(s, kmask, lv.gx, lv.gz, lv.stair, 0.28, terrain, anchors)
            xs = dict(navwalk.ROOF_STATS)
            navwalk.ROOF_STATS.clear()
            twin('e3c', f'roof_islands case {case}: {xs}', (x, xs))
            calls += 1
    finally:
        navwalk.ROOF_RAISE, navwalk.ROOF_SHARE, navwalk.ROOF_NEAR, navwalk.ROOF_NEAR_DY = saved
        navwalk.ROOF_STATS.clear()
    # coverage nodes: build_graph's add (one node per surface and rounded position) on random areas and nodes
    for case in range(24):
        pitch = 0.8 if case % 2 else 0.2
        lv = grid(rng, rng.randint(5, 16), rng.randint(5, 16), case % 3 == 0, pitch)
        areas = build_areas(lv, 0, 0, cfg=lv.cfg)
        ox, oz = rng.choice([0.0, -910.4000244140625, 300.0]), rng.choice([0.0, 12.8])
        cells = [c for x in areas for c in x['cells']]
        first: list[Any] = []
        for n in range(rng.randint(0, 8)):
            ci, cj, ck = rng.choice(cells) if cells else (0, 0, 0)
            first.append((n + 1, (ox + ci * pitch + rng.choice([0.0, 0.4]), float(lv.y[ck]) + 0.95, oz + cj * pitch),
                          ck, ci, cj))
        cover = rng.choice([4.0, 1.0, 0.5, 2.3, 3])

        def run(cover: float = cover, first: list[Any] = first, lv: Level = lv, areas: list[Any] = areas,
                ox: float = ox, oz: float = oz) -> Any:
            pts = list(first)
            taken = {p[2] for p in pts}
            taken_pos = {tuple(round(v, 3) for v in p[1]) for p in pts}

            def add(i: int, j: int, k: int) -> None:
                pos = (ox + i * lv.cfg.pitch, float(lv.y[k]) + lv.cfg.y_offset, oz + j * lv.cfg.pitch)
                key = tuple(round(v, 3) for v in pos)
                if k in taken or key in taken_pos:
                    return
                taken.add(k)
                taken_pos.add(key)
                pts.append((1000 + len(pts), pos, k, i, j))
            add_cover_nodes(lv, areas, pts, add, ox, oz, cover, lv.cfg)
            return ordered(pts)
        twin('e3c', f'add_cover_nodes case {case} (cover {cover})', run())
        calls += 1
    # Gabriel pruning: nodes on a few floors (some at one spot), pairs within 15 m, some lengths equal
    for case in range(40):
        n = rng.randint(0, 60)
        pts = [(q, (rng.choice([rng.uniform(0, 30), float(rng.randint(0, 30))]), rng.choice([0.95, 3.45, 5.0]),
                    rng.choice([rng.uniform(0, 30), float(rng.randint(0, 30))])), q, 0, 0) for q in range(n)]
        if n > 3:
            pts[1] = pts[0]
        pairs = [(a_, b_, math.dist(pts[a_][1], pts[b_][1])) for a_ in range(n) for b_ in range(a_ + 1, n)
                 if math.dist(pts[a_][1], pts[b_][1]) <= 15.0 and rng.random() < 0.6]
        twin('e3c', f'gabriel_prune case {case}', gabriel_prune(pts, pairs))
        calls += 1
    return calls


def stage_grid(lv: Level) -> tuple[Any, ...]:
    """lv as a ready walkability grid (its keep, gradients and stairs) for the Rust stage."""
    walk, gx, gz, stair = navwalk.grid_arrays(lv.keep, lv.gx, lv.gz, lv.stair)
    s = lv.s
    return (int(s.nx), int(s.nz), float(s.min_x), float(s.min_z), float(s.pitch),
            np.ascontiguousarray(s.start, '<i8').tobytes(), np.ascontiguousarray(lv.y, '<f8').tobytes(), walk.tobytes(),
            (gx.tobytes(), gx.dtype == np.float32), (gz.tobytes(), gz.dtype == np.float32), stair.tobytes(),
            np.ascontiguousarray(lv.terrain, bool).tobytes())


def stage_answer(lv: Level, cfg: Cfg, orders: list[Any]) -> Any:
    """The Rust stage on lv's grid, in the canonical form its answers are recorded in: per sector (sorted) its mesh's
    SHA-1, its areas (tile, layer, cells) and origin; the graphs; the border nodes of every sector that has some; the
    next id, the dropped carried nodes, the order nodes' ids and count, the D / F counts (d, f, outline moved, one-sided)
    and the last sector's D sides. The diagonal-neighbour links off, as navbuild.py's stage built the recorded ones."""
    a = require_native().navkit_stage_answer(stage_grid(lv), 0, 0, 0.28, cfg.overrides(), {'diagonal_links': False},
                                             [tuple(float(v) for v in p) for p in orders])
    meshes = [(sec, hashlib.sha1(m).hexdigest()) for sec, m in a['meshes']]
    border = [(sec, nodes) for sec, nodes in a['border_nodes'] if nodes]
    return (meshes, a['info'], a['graphs'], border, a['next_id'], sorted(a['dropped']), a['order_ids'],
            a['order_added'], tuple(a['detail']), a['blocked'][-1] if a['blocked'] else [])


def e3h(rng: random.Random) -> int:
    """Round E3h's pieces against the answers recorded from their retired Python twin: whole stages (the Rust
    stage, against the 'stage' answers) on random levels over the corner of four sectors, at the normal and the
    detailed pitch; sector graphs with ladders; walkable's unseeds; navdiff.area_cells; round(v, 3) against
    CPython's."""
    calls = 0
    sizes = dict(sectors=0, areas=0, nodes=0, details=0)
    for case in range(10):
        pitch = 0.2 if case % 2 else 0.8
        n = rng.randint(14, 40)
        lv = ramps(rng, n, n, pitch) if case % 3 else grid(rng, n, n, case % 4 == 0, pitch)
        # the grid straddles the corner of four sectors (x, z = 100 m)
        lv.s.min_x = 100.0 - rng.randint(3, n - 3) * pitch
        lv.s.min_z = 100.0 - rng.randint(3, n - 3) * pitch
        cfg = DEFAULTS.at_pitch(pitch)
        kept = np.nonzero(lv.keep)[0]
        orders = []
        for k in rng.sample(kept.tolist(), min(4, len(kept))):
            c = int(lv.col_of[k])
            orders.append((lv.s.min_x + (c % lv.s.nx) * pitch, float(lv.y[k]), lv.s.min_z + (c // lv.s.nx) * pitch))
        a = stage_answer(lv, cfg, orders)
        sizes['details'] += a[8][0] + a[8][1]
        twin('stage', f'stage case {case} (pitch {pitch}, {n} x {n})', ordered(a))
        sizes['sectors'] += len(a[0])
        sizes['areas'] += sum(len(v[1]) for v in a[1])
        sizes['nodes'] += sum(len(g[2]) for g in a[2])
        calls += 1
    print(f'stages: {sizes}')
    # sector graphs with ladder links (and their new nodes)
    for case in range(12):
        pitch = 0.2 if case % 2 else 0.8
        lv = ramps(rng, rng.randint(6, 20), rng.randint(6, 20), pitch)
        cfg = lv.cfg
        seed = rng.randrange(1 << 30)
        x = sector_graph(lv, cfg, 0.0, -100.0, ((-1, -1), cfg.tile, cfg.tiles), seed, 3)
        twin('e3h', f'build_graph with ladders case {case}', x)
        calls += 1
    # walkable's unseeds (hand edits dropping the island a point is on): the island from the flood's NavLevel
    dropped = 0
    for case in range(16):
        s = runs(rng, rng.randint(6, 18), rng.randint(6, 18), 0.2 if case % 2 else 0.8)
        cols = [c for c in range(s.nx * s.nz) if s.start[c + 1] > s.start[c]]
        seeds = []
        for c in rng.sample(cols, min(len(cols), 6)):
            k = rng.randrange(int(s.start[c]), int(s.start[c + 1]))
            seeds.append((float(s.min_x + (c % s.nx) * s.pitch), float(s.y[k]), float(s.min_z + (c // s.nx) * s.pitch)))
        kw: dict[str, Any] = dict(extra_seeds=seeds[:3], unseeds=seeds[3:] + seeds[:1], return_gradients=True)
        with retired_lips():
            x = navwalk.walkable(s, **kw)
            whole = navwalk.walkable(s, extra_seeds=seeds[:3])
        dropped += int(whole.sum() - x[0].sum())
        twin('e3h', f'walkable with unseeds case {case}', x)
        calls += 1
    check(dropped > 0, 'the unseeds drop ground')
    print(f'E3h unseeds: {dropped} kept surfaces dropped')
    # navdiff.area_cells (the parity measures' cells): random closed outlines on the half-pitch lines and off them,
    # 45-degree bridges, stray segments, portals as kymesh objects - the Rust core against the NumPy reference
    import kymesh
    import navdiff
    areas_ = []
    for case in range(60):
        n = rng.randint(3, 14)
        cx, cz = rng.uniform(-900, 900), rng.uniform(-900, 900)
        step = rng.choice([0.4, 0.1, 0.25, 0.37])
        ring = []
        for q in range(n):
            ang = 2 * math.pi * q / n
            r = rng.uniform(1, 12)
            x, z = cx + r * math.cos(ang), cz + r * math.sin(ang)
            if case % 2:
                x, z = round(x / step) * step, round(z / step) * step
            ring.append((x, rng.choice([0.95, 3.2, rng.uniform(0, 40)]), z))
        outline = [ring + [ring[0]]]
        if case % 3 == 0:
            outline.append([(cx, 1.0, cz), (cx + rng.uniform(-3, 3), 2.0, cz + rng.uniform(-3, 3))])
        portals = [kymesh.Portal(ring[0], ring[1], 0)] if case % 4 == 0 else []
        areas_.append(SimpleNamespace(outline=outline, portals=portals, border_portals=[]))
    areas_.append(SimpleNamespace(outline=[], portals=[], border_portals=[]))
    got_c = navdiff.area_cells_many(areas_)
    twin('e3h', 'navdiff.area_cells', [list(c.items()) for c in got_c])
    print(f'E3h area cells: {len(areas_)} areas, {sum(len(c) for c in got_c)} cells')
    calls += 1
    # round(v, 3): random values, exact ties (multiples of 1/16), negative zero, the build's positions
    vals = [rng.uniform(-2000, 2000) for _ in range(60000)] + [k / 16 for k in range(-40000, 40000)]
    vals += [rng.randint(-2500, 2500) * 0.2 + rng.choice([0.0, 0.95, -0.05]) for _ in range(40000)]
    vals += [-0.0, 0.0, 0.0004, -0.0004, 1e-300, 5e-324, 2.675, 1.0005]
    got = np.frombuffer(require_native().navkit_round3(np.array(vals, '<f8').tobytes()), '<f8')
    want_r = np.array([round(v, 3) for v in vals], '<f8')
    check(np.array_equal(got.view('<u8'), want_r.view('<u8')), 'round(v, 3): bit for bit as CPython')
    return calls + 1


# ------------------------------------------------------------------------------------------------ whole levels

def input_files(root: str, lv: str, navregress: Any) -> list[str]:
    """The files a level's build reads its data from (navregress.build_level): the in-game samples (normal and
    pitch 0.2, each with its high-capsule run), the level's object files and settings."""
    samples, _x, _z, f = navregress.inputs(root, lv)
    out = [samples, samples[:-4] + '_hi.bin', *f.values()]
    p02 = os.path.join(root, 'work', 'navsample', f'{lv}_navkit_p02.bin')
    out += [p02, p02[:-4].replace('_p02', '_hi_p02') + '.bin']
    L = os.path.join(root, 'work', 'levels', lv)
    out += [os.path.join(L, n) for n in ('order_points.json', 'area_orders.json', 'ladders.json')]
    out.append(os.path.join(root, 'work', 'navkit', lv, 'navkit.json'))
    return sorted({p for p in out if isinstance(p, str) and os.path.isfile(p)})


def file_sha1(p: str) -> str:
    h = hashlib.sha1()
    with open(p, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 22), b''):
            h.update(chunk)
    return h.hexdigest()


def game(golden: dict[str, Any], record: bool, levels: list[str]) -> None:
    import navregress
    suite_paths.use('navkit')
    import navkit
    root = navkit.find_opfor(None)
    # the files carry each sector's build date and time (navbuild.write_file: time.localtime()): a fixed moment
    clock = time.localtime
    time.localtime = lambda *a: FIXED_CLOCK
    try:
        with tempfile.TemporaryDirectory() as tmp:
            for lv in levels:
                p = os.path.join(tmp, f'{lv}.hpd')
                t = time.perf_counter()
                navregress.build_level(root, lv, p)
                took = time.perf_counter() - t
                data = open(p, 'rb').read()
                inputs = {os.path.relpath(q, root).replace(os.sep, '/'): file_sha1(q)
                          for q in input_files(root, lv, navregress)}
                got = {'pathdata': hashlib.sha1(data).hexdigest(), 'bytes': len(data), 'inputs': inputs}
                if record:
                    golden.setdefault('levels', {})[lv] = got
                    print(f'{lv}: recorded ({len(data):,} bytes, {took:.0f} s)')
                    continue
                want = golden.get('levels', {}).get(lv)
                if want is None:
                    check(False, f'{lv}: not recorded (--record --game --levels {lv})')
                    continue
                changed = sorted(k for k in set(inputs) | set(want['inputs']) if inputs.get(k) != want['inputs'].get(k))
                if changed:
                    check(False, f'{lv}: its inputs are not the recorded ones ({", ".join(changed)}): not compared '
                                 f'(check the new inputs with navregress, then re-record)')
                    continue
                ok = got['pathdata'] == want['pathdata']
                check(ok, f'{lv}: the path data differs from the recording ({len(data):,} bytes, recorded '
                          f'{want["bytes"]:,})')
                print(f'{lv}: {"as recorded" if ok else "DIFFERENT"} ({len(data):,} bytes), {took:.0f} s')
    finally:
        time.localtime = clock


if __name__ == '__main__':
    record = '--record' in sys.argv
    golden = load()
    quick(golden, record)
    if '--game' in sys.argv:
        levels = ['sp_009']
        if '--levels' in sys.argv:
            levels = sys.argv[sys.argv.index('--levels') + 1].split(',')
        game(golden, record, levels)
    if record and not FAILS:
        golden['recorded'] = time.strftime('%Y-%m-%d')
        with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(golden, f, indent=1, sort_keys=True)
            f.write('\n')
        print(f'wrote {GOLDEN}')
    print('all passed' if not FAILS else f'{len(FAILS)} failures')
    sys.exit(1 if FAILS else 0)
