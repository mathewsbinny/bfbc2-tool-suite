r"""Round NK3's parity on real levels: NavKit's detail features (navdetail.detail_features: the broken sides, the D
lines and the F cells of one area), the old Python (a suite checkout from before NK3, 8433e3a or earlier: its
navdetail.py) against the new Rust (this checkout: navdetail.detail_features through bc2_native), on every NavKit
regression level (navregress.build_level: Project OPFOR BFBC2's in-game samples, read only).

  python rust\parity_nk3_detail.py capture <suite> <opfor> <scratch> [level ...]
      build each level with `suite` (the build clock fixed), recording every detail_features call - its inputs
      (the area's cells in their dict order, the half cells, what eligible() / breakable() answered) and its answer -
      into <scratch>\<level>.nk3.pickle, the file as <scratch>\<level>.<suite folder>.hpd; prints a JSON line per level (the file's SHA-1, the calls, the seconds in
      detail_pass and detail_features, the build's seconds and peak)
  python rust\parity_nk3_detail.py compare <old suite> <new suite> <scratch> <out.json> [level ...]
      replay every recorded call through the old navdetail and the new one (each in its own process, timed, its
      peak above the loaded calls), and compare the answers value for value (floats bit for bit, -0.0 apart from 0.0)
      with each other and with the recording
  python rust\parity_nk3_detail.py order <old suite> <scratch> <out.json> [level ...]
      what Python's set order does: replay every call through the old navdetail as it is, with its sets filled in
      another order (the cells' dict reversed: CPython's set layout, and so its iteration order, follows insertion
      order where hashes collide), and with the spec's order-free rules (navdetail's docstring: the loops' start edge
      and the nearest F cell's ties); counts the places where the order can decide (a loop starting at a pinch, a tie
      for the nearest F cell) and compares the answers

  python rust\parity_nk3_detail.py filediff <suite> <old.hpd> <new.hpd>
      two builds of a level (capture's files): everything equal but F record points' heights, which are listed

  python rust\parity_nk3_detail.py dice <suite> <opfor>
      DICE's own F records on every infantry path-data file of the game copy (read only): the shapes the order
      rules decide never occur there (KYM-DF-2)

Levels: the 15 regression builds of work\navkit\regress (navregress's levels) when none are named.
"""
import ctypes
import ctypes.wintypes
import gc
import hashlib
import importlib.util
import json
import os
import pickle
import struct
import subprocess
import sys
import time
from collections import defaultdict
from typing import Any

LEVELS = ['ons_mp_002', 'ons_mp_004', 'sp_003_b', 'sp_004_b', 'sp_005', 'sp_005_b', 'sp_006', 'sp_007', 'sp_008',
          'sp_008_b', 'sp_009', 'sp_01', 'sp_02', 'sp_03', 'sp_04']


class _Counters(ctypes.Structure):
    _fields_ = [('cb', ctypes.wintypes.DWORD), ('PageFaultCount', ctypes.wintypes.DWORD),
                ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]


def memory():
    """(working set, peak working set) of this process, MB."""
    c = _Counters()
    c.cb = ctypes.sizeof(c)
    k32 = ctypes.WinDLL('kernel32')
    k32.GetCurrentProcess.restype = ctypes.wintypes.HANDLE
    psapi = ctypes.WinDLL('psapi')
    psapi.GetProcessMemoryInfo.argtypes = [ctypes.wintypes.HANDLE, ctypes.POINTER(_Counters), ctypes.wintypes.DWORD]
    psapi.GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb)
    return c.WorkingSetSize / 2 ** 20, c.PeakWorkingSetSize / 2 ** 20


def use_suite(suite):
    sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core', 'navkit-tools')


def load_navdetail(suite, name):
    """navdetail.py of a checkout as module `name` (kymesh: the one already imported)."""
    spec = importlib.util.spec_from_file_location(name, os.path.join(suite, 'bfbc2-auto-navkit', 'tools',
                                                                      'navdetail.py'))
    assert spec is not None and spec.loader is not None
    m = importlib.util.module_from_spec(spec)
    sys.modules[name] = m
    spec.loader.exec_module(m)
    return m


def bits(v):
    """A value with every float as its bits (so -0.0 != 0.0 and NaN == NaN), Cells as (direction, points)."""
    if isinstance(v, float):
        return ('f', struct.pack('<d', v))
    if isinstance(v, (list, tuple)):
        return tuple(bits(x) for x in v)
    if isinstance(v, (set, frozenset)):
        return ('set', tuple(sorted(bits(x) for x in v)))
    if hasattr(v, 'direction') and hasattr(v, 'points'):
        return ('F', bits(v.direction), bits(v.points))
    return v


def answer(got):
    """detail_features's answer, comparable: (broken sides as a set, D lines, F cells)."""
    broken, lines, fcells = got
    return bits((set(broken), lines, fcells))


class Recorder:
    """A callable's answers, recorded (capture) or replayed (compare / order)."""

    def __init__(self, fn=None, answers=None):
        self.fn = fn
        self.answers = {} if answers is None else answers

    def __call__(self, *a):
        if self.fn is None:
            return self.answers[a]
        got = self.fn(*a)
        self.answers[a] = got
        return got


# ------------------------------------------------------------------------------------------------ capture

def capture_child(suite, opfor, lv, scratch):
    use_suite(suite)
    import navbuild
    import navdetail
    import navregress
    sys.path.insert(0, os.path.join(suite, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    calls: list[dict[str, Any]] = []
    spent = {'detail_pass': 0.0, 'detail_features': 0.0}
    # the old navbuild calls detail_features, the new one features (its answer also holds the D sides)
    name = 'features' if hasattr(navdetail, 'features') else 'detail_features'
    real_features = getattr(navdetail, name)

    def features(cells, pitch, ox, oz, step, eligible=None, half=None, breakable=None, with_f=True):
        el = None if eligible is None else Recorder(eligible)
        br = None if breakable is None else Recorder(breakable)
        t = time.perf_counter()
        got = real_features(cells, pitch, ox, oz, step, el, half, br, with_f=with_f)
        spent['detail_features'] += time.perf_counter() - t
        triple = (got.broken, got.lines, got.cells) if name == 'features' else got
        calls.append({'cells': cells, 'pitch': pitch, 'ox': ox, 'oz': oz, 'step': step,
                      'eligible': None if el is None else el.answers, 'half': half,
                      'breakable': None if br is None else br.answers, 'with_f': with_f,
                      'answer': answer(triple)})
        return got
    setattr(navdetail, name, features)
    real_pass = navbuild.detail_pass

    def detail_pass(*a, **k):
        t = time.perf_counter()
        try:
            return real_pass(*a, **k)
        finally:
            spent['detail_pass'] += time.perf_counter() - t
    navbuild.detail_pass = detail_pass
    clock = time.localtime
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    out = os.path.join(scratch, f'{lv}.{os.path.basename(os.path.normpath(suite))}.hpd')
    t = time.perf_counter()
    try:
        navregress.build_level(opfor, lv, out)
    finally:
        time.localtime = clock
    took = time.perf_counter() - t
    _ws, peak = memory()
    data = open(out, 'rb').read()                # kept for the file comparison (filediff)
    with open(os.path.join(scratch, f'{lv}.nk3.pickle'), 'wb') as f:
        pickle.dump(calls, f, protocol=pickle.HIGHEST_PROTOCOL)
    res = {'level': lv, 'sha1': hashlib.sha1(data).hexdigest(), 'bytes': len(data), 'calls': len(calls),
           'cells': sum(len(c['cells']) for c in calls), 'build_s': took, 'peak_mb': peak,
           'detail_pass_s': spent['detail_pass'], 'detail_features_s': spent['detail_features'],
           'd_lines': sum(len(c['answer'][1]) for c in calls), 'f_cells': sum(len(c['answer'][2]) for c in calls)}
    g = golden_test.load().get('levels', {}).get(lv)
    if g:
        inputs = {os.path.relpath(q, opfor).replace(os.sep, '/'): golden_test.file_sha1(q)
                  for q in golden_test.input_files(opfor, lv, navregress)}
        res['golden'] = ('inputs changed' if inputs != g['inputs'] else
                         'as recorded' if g['pathdata'] == res['sha1'] else 'DIFFERENT')
    print(json.dumps(res, sort_keys=True), flush=True)


# ------------------------------------------------------------------------------------------------ replay

def replay(nd, call, cells=None):
    """One recorded call through navdetail module nd; cells: another dict for the area (the order probe)."""
    el = None if call['eligible'] is None else Recorder(answers=call['eligible'])
    br = None if call['breakable'] is None else Recorder(answers=call['breakable'])
    return nd.detail_features(call['cells'] if cells is None else cells, call['pitch'], call['ox'], call['oz'],
                              call['step'], el, call['half'], br, with_f=call['with_f'])


def run_child(suite, scratch, lv, out, spec=''):
    """Every recorded call of a level through `suite`'s navdetail; the answers to out (pickle); prints a JSON line.
    spec: the old Python with the spec's two order rules in place of its sets' order (loops_spec, f_point_spec)."""
    use_suite(suite)
    import navdetail
    if spec:
        navdetail._loops = lambda q: loops_spec(q, navdetail._turn)
        navdetail._f_point = lambda *a: f_point_spec(navdetail, *a)
    with open(os.path.join(scratch, f'{lv}.nk3.pickle'), 'rb') as f:
        calls = pickle.load(f)
    gc.collect()
    ws0, _peak0 = memory()
    got = []
    t = time.perf_counter()
    for call in calls:
        got.append(replay(navdetail, call))
    took = time.perf_counter() - t
    _ws1, peak1 = memory()
    answers = [answer(g) for g in got]
    with open(out, 'wb') as f:
        pickle.dump(answers, f, protocol=pickle.HIGHEST_PROTOCOL)
    print(json.dumps({'s': took, 'loaded_mb': ws0, 'peak_mb': peak1, 'peak_above_mb': peak1 - ws0,
                      'same_as_recorded': sum(a == c['answer'] for a, c in zip(answers, calls))}, sort_keys=True), flush=True)


def child_json(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    lines = [ln for ln in p.stdout.splitlines() if ln.startswith('{')]
    if p.returncode or not lines:
        return {'error': (p.stderr or p.stdout)[-2000:]}
    return json.loads(lines[-1])


# ------------------------------------------------------------------------------------------------ the order probe

def order_child(old, scratch, lv):
    """What Python's set order decides on one level (see the module's docstring); prints a JSON line."""
    use_suite(old)
    nd = load_navdetail(old, 'navdetail_old')
    with open(os.path.join(scratch, f'{lv}.nk3.pickle'), 'rb') as f:
        calls = pickle.load(f)
    ev: dict[str, Any] = {'calls': len(calls), 'loops': 0, 'start_pinch': 0, 'start_pinch_decides': 0, 'fallback': 0,
          'fallback_min': 0, 'min_tie': 0, 'min_tie_decides': 0, 'min_tie_spec_differs': 0, 'fallback_sum_gt2': 0,
          'as_is_same': 0, 'reversed_same': 0, 'rules_same': 0, 'rules_same_reversed': 0}
    examples: dict[str, list[Any]] = defaultdict(list)
    real_loops, real_point = nd._loops, nd._f_point

    def loops_probe(quarters):
        # the loops' starts: a pinch there (two edges out of the start vertex) leaves the first loop's edge to the
        # order the quarters' set gave the edges
        ev['loops'] += 1
        got = real_loops(quarters)
        edges: defaultdict[Any, int] = defaultdict(int)
        for (i, j), (sx, sz) in quarters:
            c = (2 * i, 2 * j)
            if sx:
                b0, b1 = (2 * i + sx, 2 * j - 1), (2 * i + sx, 2 * j + 1)
            else:
                b0, b1 = (2 * i - 1, 2 * j + sz), (2 * i + 1, 2 * j + sz)
            if (b0[0] - c[0]) * (b1[1] - c[1]) - (b0[1] - c[1]) * (b1[0] - c[0]) < 0:
                b0, b1 = b1, b0
            for p, r in ((c, b0), (b0, b1), (b1, c)):
                if edges[(r, p)]:
                    edges[(r, p)] -= 1
                else:
                    edges[(p, r)] += 1
        out_of: defaultdict[Any, int] = defaultdict(int)
        for (p, _r), n in edges.items():
            out_of[p] += n
        for loop in got:
            if out_of[loop[0]] > 1:
                ev['start_pinch'] += 1
                if len(examples['start_pinch']) < 5:
                    examples['start_pinch'].append(sorted(quarters))
        if any(out_of[loop[0]] > 1 for loop in got) and got != loops_rule(quarters):
            ev['start_pinch_decides'] += 1
        return got

    def point_probe(p, axis, sign, cells, fset, pitch, ox, oz):
        vx, vz = p
        if axis == 'x':
            ci = [(vx + sign) // 2] if vx % 2 else [vx // 2 + sign]
            cj = [(vz - 1) // 2, (vz + 1) // 2] if vz % 2 else [vz // 2]
        else:
            cj = [(vz + sign) // 2] if vz % 2 else [vz // 2 + sign]
            ci = [(vx - 1) // 2, (vx + 1) // 2] if vx % 2 else [vx // 2]
        if not [1 for a in ci for b in cj if (a, b) in cells]:
            ev['fallback'] += 1
            near = [c for c in fset if abs(2 * c[0] - vx) <= 1 and abs(2 * c[1] - vz) <= 1 and c in cells]
            ev['fallback_sum_gt2'] += len(near) > 2
            if not near:
                ev['fallback_min'] += 1
                dist = {c: abs(2 * c[0] - vx) + abs(2 * c[1] - vz) for c in fset}
                best = min(dist.values())
                tied = [c for c in fset if dist[c] == best]
                if len(tied) > 1:
                    ev['min_tie'] += 1
                    if len({cells[c] for c in tied}) > 1:
                        ev['min_tie_decides'] += 1
                        py = min(fset, key=lambda c: dist[c])
                        spec = nearest_f_cells(fset, vx, vz, axis, sign)
                        spec_y = sum(cells[c] for c in spec) / len(spec)
                        ev['min_tie_spec_differs'] += cells[py] != spec_y
                        examples['min_tie'].append({'vertex': p, 'axis': axis, 'sign': sign, 'tied': sorted(tied),
                                                    'heights': [cells[c] for c in sorted(tied)], 'python': py,
                                                    'spec': spec, 'old_y': cells[py] + nd.Y_OFFSET,
                                                    'new_y': spec_y + nd.Y_OFFSET})
        return real_point(p, axis, sign, cells, fset, pitch, ox, oz)

    def loops_rule(quarters):
        return loops_spec(quarters, nd._turn)

    nd._loops, nd._f_point = loops_probe, point_probe
    for call in calls:
        if answer(replay(nd, call)) == call['answer']:
            ev['as_is_same'] += 1
    nd._loops, nd._f_point = real_loops, real_point
    for call in calls:
        rev = dict(reversed(list(call['cells'].items())))
        if answer(replay(nd, call, rev)) == call['answer']:
            ev['reversed_same'] += 1
    nd._loops, nd._f_point = loops_rule, lambda *a: f_point_spec(nd, *a)
    for call in calls:
        if answer(replay(nd, call)) == call['answer']:
            ev['rules_same'] += 1
        rev = dict(reversed(list(call['cells'].items())))
        if answer(replay(nd, call, rev)) == call['answer']:
            ev['rules_same_reversed'] += 1
    nd._loops, nd._f_point = real_loops, real_point
    ev['level'] = lv
    ev['examples'] = examples
    print(json.dumps(ev, sort_keys=True), flush=True)


def loops_spec(quarters, turn):
    """navdetail._loops with the spec's order-free start: at a loop's start vertex, the edge whose end is least
    ((x, z) order); elsewhere as the old Python (the sharpest right turn at a pinch)."""
    edges: defaultdict[Any, int] = defaultdict(int)
    for (i, j), (sx, sz) in sorted(quarters):
        c = (2 * i, 2 * j)
        if sx:
            b0, b1 = (2 * i + sx, 2 * j - 1), (2 * i + sx, 2 * j + 1)
        else:
            b0, b1 = (2 * i - 1, 2 * j + sz), (2 * i + 1, 2 * j + sz)
        if (b0[0] - c[0]) * (b1[1] - c[1]) - (b0[1] - c[1]) * (b1[0] - c[0]) < 0:
            b0, b1 = b1, b0
        for p, r in ((c, b0), (b0, b1), (b1, c)):
            if edges[(r, p)]:
                edges[(r, p)] -= 1
            else:
                edges[(p, r)] += 1
    nxt: defaultdict[Any, list[Any]] = defaultdict(list)
    for (p, r), n in sorted(edges.items()):
        for _ in range(n):
            nxt[p].append(r)
    loops = []
    while any(nxt.values()):
        p0 = min(p for p, v in nxt.items() if v)
        loop, p, prev = [p0], p0, None
        while True:
            cand = nxt[p]
            if prev is None:
                cand.sort()
            elif len(cand) > 1:
                din = (p[0] - prev[0], p[1] - prev[1])
                cand.sort(key=lambda r: turn(din, (r[0] - p[0], r[1] - p[1])))
            r = cand.pop(0)
            prev, p = p, r
            if p == p0:
                break
            loop.append(p)
        loops.append(loop)
    return loops


def nearest_f_cells(fset, vx, vz, axis, sign):
    """The spec's nearest F cells to vertex (vx, vz) (half units) for a record of axis and sign, in (i, j) order: the
    F cells at the least |dx| + |dz|, of them those furthest toward the record's side (its heights come from that
    side, as DICE's do); the vertex takes their mean height."""
    dist = {c: abs(2 * c[0] - vx) + abs(2 * c[1] - vz) for c in fset}
    best = min(dist.values())
    near = [c for c in sorted(fset) if dist[c] == best]
    far = max(sign * (c[0] if axis == 'x' else c[1]) for c in near)
    return [c for c in near if sign * (c[0] if axis == 'x' else c[1]) == far]


def f_point_spec(nd, p, axis, sign, cells, fset, pitch, ox, oz):
    """navdetail._f_point with the spec's order-free ties: the F cells touching the vertex summed in (i, j) order,
    and, with none of those, the mean of nearest_f_cells."""
    vx, vz = p
    if axis == 'x':
        ci = [(vx + sign) // 2] if vx % 2 else [vx // 2 + sign]
        cj = [(vz - 1) // 2, (vz + 1) // 2] if vz % 2 else [vz // 2]
    else:
        cj = [(vz + sign) // 2] if vz % 2 else [vz // 2 + sign]
        ci = [(vx - 1) // 2, (vx + 1) // 2] if vx % 2 else [vx // 2]
    hs = [cells[(a, b)] for a in ci for b in cj if (a, b) in cells]
    if not hs:
        fs = sorted(fset)
        hs = [cells[c] for c in fs if abs(2 * c[0] - vx) <= 1 and abs(2 * c[1] - vz) <= 1 and c in cells] or \
             [cells[c] for c in nearest_f_cells(fset, vx, vz, axis, sign)]
    return (ox + vx * pitch / 2.0, sum(hs) / len(hs) + nd.Y_OFFSET, oz + vz * pitch / 2.0)


# ------------------------------------------------------------------------------------------------ the files

def file_diff(suite, old_path, new_path):
    """Every difference between two path-data files, explained: the header and every sector's fields, nodes and
    edges equal; per differing mesh, every field of every area equal but F record points' heights, listed
    (sector, area, record, point, x, z, old y, new y). Returns (rest_equal, changes, bytes differing)."""
    use_suite(suite)
    import dataclasses

    import hpdtool
    import kymesh
    a, b = hpdtool.read(old_path), hpdtool.read(new_path)
    ra, rb = open(old_path, 'rb').read(), open(new_path, 'rb').read()
    nbytes = sum(x != y for x, y in zip(ra, rb)) + abs(len(ra) - len(rb))
    ok = a.header == b.header and len(a.sectors) == len(b.sectors)
    changes = []
    for sa, sb in zip(a.sectors, b.sectors):
        ok &= dataclasses.replace(sa, mesh=b'') == dataclasses.replace(sb, mesh=b'')
        if sa.mesh == sb.mesh:
            continue
        ma, mb = kymesh.parse_mesh(sa.mesh), kymesh.parse_mesh(sb.mesh)
        ok &= dataclasses.replace(ma, areas=[]) == dataclasses.replace(mb, areas=[]) and len(ma.areas) == len(mb.areas)
        for ai, (xa, xb) in enumerate(zip(ma.areas, mb.areas)):
            ok &= dataclasses.replace(xa, cells=[]) == dataclasses.replace(xb, cells=[]) and len(xa.cells) == len(xb.cells)
            for ci, (fa, fb) in enumerate(zip(xa.cells, xb.cells)):
                ok &= fa.direction == fb.direction and len(fa.points) == len(fb.points)
                for pi, (pa, pb) in enumerate(zip(fa.points, fb.points)):
                    ok &= (pa[0], pa[2]) == (pb[0], pb[2])
                    if pa[1] != pb[1]:
                        changes.append({'sector': f'0x{sa.id:04X}', 'area': ai, 'record': ci,
                                        'direction': fa.direction, 'point': pi, 'x': pa[0], 'z': pa[2],
                                        'old_y': pa[1], 'new_y': pb[1]})
    return ok, changes, nbytes


# ------------------------------------------------------------------------------------------------ DICE's shapes

def dice_shapes(suite, opfor):
    """What DICE's own F records hold (KYM-DF-2), on every infantry path-data file of the game copy (read only):
    records, closed, counter-clockwise, with a repeated vertex, records of one direction in an area sharing a vertex;
    normal-pitch vertices at cell corners / elsewhere."""
    use_suite(suite)
    import hpdtool
    import kymesh
    root = os.path.join(opfor, 'game', 'Output', 'win32', 'ai', 'levels')
    c: dict[str, int] = defaultdict(int)
    files = 0
    for lv in sorted(os.listdir(root)):
        p = os.path.join(root, lv, 'infantry', 'pathdata.hpd')
        if not os.path.isfile(p):
            continue
        files += 1
        for s in hpdtool.read(p).sectors:
            for a in kymesh.parse_mesh(s.mesh).areas:
                h = a.pitch / 2.0
                by_dir = defaultdict(list)
                for r in a.cells:
                    pts = r.points
                    vs = [(round(q[0] / h), round(q[2] / h)) for q in pts[:-1]]
                    c['records'] += 1
                    c['closed'] += pts[0] == pts[-1]
                    c['ccw'] += sum(x[0] * y[2] - y[0] * x[2] for x, y in zip(pts, pts[1:])) > 0
                    c['repeated_vertex'] += len(set(vs)) != len(vs)
                    by_dir[tuple(r.direction)].append(set(vs))
                    if a.pitch > 0.5:
                        for q in pts[:-1]:
                            corner = round(q[0] % 0.8 / 0.4) % 2 == 1 and round(q[2] % 0.8 / 0.4) % 2 == 1
                            c['vertices_corner' if corner else 'vertices_other'] += 1
                for sets in by_dir.values():
                    c['records_touching'] += sum(bool(x & y) for n, x in enumerate(sets) for y in sets[n + 1:])
    return {'files': files, **c}


# ------------------------------------------------------------------------------------------------ main

def main():
    cmd = sys.argv[1]
    if cmd == '--capture':
        capture_child(*sys.argv[2:6])
        return 0
    if cmd == '--run':
        run_child(*sys.argv[2:7])
        return 0
    if cmd == 'dice':
        # python rust\parity_nk3_detail.py dice <suite> <opfor>
        print(json.dumps(dice_shapes(*sys.argv[2:4]), sort_keys=True))
        return 0
    if cmd == 'filediff':
        # python rust\parity_nk3_detail.py filediff <suite> <old.hpd> <new.hpd>
        ok, changes, nbytes = file_diff(*sys.argv[2:5])
        print(json.dumps({'rest_equal': ok, 'changes': changes, 'bytes_differ': nbytes}, sort_keys=True))
        return 0 if ok else 1
    if cmd == '--order':
        order_child(*sys.argv[2:5])
        return 0
    me = os.path.abspath(__file__)
    if cmd == 'capture':
        suite, opfor, scratch = sys.argv[2:5]
        os.makedirs(scratch, exist_ok=True)
        for lv in sys.argv[5:] or LEVELS:
            r = child_json([sys.executable, me, '--capture', suite, opfor, lv, scratch])
            r.setdefault('level', lv)
            print(json.dumps(r, sort_keys=True), flush=True)
        return 0
    if cmd == 'order':
        old, scratch, out_json = sys.argv[2:5]
        results = []
        for lv in sys.argv[5:] or LEVELS:
            r = child_json([sys.executable, me, '--order', old, scratch, lv])
            r.setdefault('level', lv)
            results.append(r)
            print(json.dumps({k: v for k, v in r.items() if k != 'examples'}, sort_keys=True), flush=True)
            with open(out_json, 'w', encoding='utf-8', newline='\n') as f:
                json.dump(results, f, indent=1, sort_keys=True)
        return 0
    if cmd == 'compare':
        old, new, scratch, out_json = sys.argv[2:6]
        results = []
        for lv in sys.argv[6:] or LEVELS:
            r = {'level': lv}
            outs = {}
            for kind, suite in (('old', old), ('new', new), ('spec', old)):
                outs[kind] = os.path.join(scratch, f'{lv}.{kind}.answers')
                r[kind] = child_json([sys.executable, me, '--run', suite, scratch, lv, outs[kind]]
                                     + (['spec'] if kind == 'spec' else []))
            with open(os.path.join(scratch, f'{lv}.nk3.pickle'), 'rb') as f:
                calls = pickle.load(f)
            got = {}
            for kind in ('old', 'new', 'spec'):
                if os.path.isfile(outs[kind]):
                    with open(outs[kind], 'rb') as f:
                        got[kind] = pickle.load(f)
                    os.remove(outs[kind])
            r['calls'] = len(calls)
            if len(got) == 3:
                # the old Python against the Rust: equal but where the sets' order decided; the Rust against the old
                # Python with the spec's order rules: equal everywhere
                differ = [n for n, (a, b) in enumerate(zip(got['old'], got['new'])) if a != b]
                r['differ'] = len(differ)
                r['differ_calls'] = differ
                r['old_as_recorded'] = sum(a == c['answer'] for a, c in zip(got['old'], calls))
                r['new_vs_spec_differ'] = sum(a != b for a, b in zip(got['new'], got['spec']))
                r['order_decided'] = [n for n, (a, b) in enumerate(zip(got['old'], got['spec'])) if a != b]
            results.append(r)
            print(json.dumps(r, sort_keys=True), flush=True)
            with open(out_json, 'w', encoding='utf-8', newline='\n') as f:
                json.dump(results, f, indent=1, sort_keys=True)
        bad = [r['level'] for r in results if r.get('new_vs_spec_differ') != 0 or r.get('old_as_recorded') != r['calls']
               or r.get('differ_calls') != r.get('order_decided')]
        n = sum(r['calls'] for r in results)
        d = sum(len(r.get('differ_calls', ())) for r in results)
        print(f'{len(results) - len(bad)} of {len(results)} levels as expected: the Rust equals the old Python on '
              f'{n - d:,} of {n:,} calls, the other {d} the calls the order of its sets decided; the Rust equals the old '
              f'Python with the order rules of the spec on all' + (f'; not as expected: {bad}' if bad else ''))
        return 1 if bad else 0
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main())
