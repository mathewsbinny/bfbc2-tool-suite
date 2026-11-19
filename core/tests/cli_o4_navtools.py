r"""Round O4's navtools group: the command lines test_cli_o4.py replays for NavKit's graphstats, hpdcheck, hpdview,
detailed, navparity, navregress and sample_all.

cases(): the quick tier's (synthetic inputs made in the case folder: path data written by NavKit's reference
writers, saved parity runs, fake Project OPFOR folders with no game exe - sample_all never reaches a game launch).
opfor_calls(opfor): the --game tier's (Project OPFOR's real path data read in place, its saved parity runs copied into
a stand-in OPFOR folder, navregress's no-reference answer on the real folder, which it only reads).
"""
from __future__ import annotations

import json
import os
import random
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o2_nav as o2nav  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]
with_variants = o2nav.with_variants

LEVELS = ['sp_01', 'sp_02', 'sp_03', 'sp_04', 'sp_003_b', 'sp_004_b', 'sp_005', 'sp_005_b', 'sp_006', 'sp_007',
          'sp_008', 'sp_008_b', 'sp_009']


# ------------------------------------------------------------------------------------------- path data

def _clean_file(seed: int, fault: str = '') -> bytes:
    """A path data file that passes every hpdcheck check (closed square outlines, sorted nodes, valid edges, no D or
    F records), or with one fault put in."""
    hpdtool, kymesh = o2nav._navkit()
    rng = random.Random(seed)
    sectors = []
    for si in range(3):
        mesh = o2nav._mesh(rng)
        for ai, a in enumerate(mesh.areas):
            x, z = float(ai * 10), float(si * 10)
            sq = [(x, 1.0, z), (x + 4.0, 1.0, z), (x + 4.0, 1.5, z + 4.0), (x, 1.0, z + 4.0)]
            a.outline = [sq + [sq[0]]]
            a.portals, a.border_portals, a.d_lines, a.cells = [], [], [], []
            a.null_offsets = {}
        if si == 1 and fault == 'open':
            mesh.areas[0].outline = [mesh.areas[0].outline[0][:-1]]
        if si == 1 and fault == 'dline':
            mesh.areas[0].d_lines = [[(1.0, 2.0, 3.0)]]
        if si == 1 and fault == 'fdir':
            mesh.areas[0].cells = [kymesh.Cell((0.6, 0.0, 0.6), [(0.0, 0.0, 0.0)] * 4)]
            mesh.areas[0].end_row = (0.0, 0.0, 0.0)
        if si == 1 and fault == 'fopen':
            mesh.areas[0].cells = [kymesh.Cell((1.0, 0.0, 0.0), [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (1.0, 0.0, 1.0),
                                                                 (0.0, 0.0, 1.5)])]
            mesh.areas[0].end_row = (0.0, 0.0, 0.0)
        if si == 1 and fault == 'portal':
            mesh.areas[0].portals = [kymesh.Portal((-0.004, 0.0, 0.0), (7.0, 0.0, -0.0), 0)]
        n = 6 + si
        ids = sorted(rng.sample(range(1, 1 << 20), n)) + [0x80000000 | (si * 7 + k) for k in range(2)]
        if si == 2 and fault == 'order':
            ids[0], ids[1] = ids[1], ids[0]
        nodes = [hpdtool.Node(i, (float(k) * 3.5 + si, 0.95, float(si) * 2.25), 0x80000000, 0.0, 0, 0, 0)
                 for k, i in enumerate(ids)]
        edges = [hpdtool.Edge(0x80000010, k, (k + 1) % len(nodes)) for k in range(len(nodes))]
        if si == 0 and fault == 'edge':
            edges.append(hpdtool.Edge(0x80000010, 3, 99))
        if si == 0 and fault == 'zero':
            nodes[2] = hpdtool.Node(nodes[2].id, nodes[1].pos, 0x80000000, 0.0, 0, 0, 0)
        if si == 0 and fault == 'isolated':
            edges = edges[:2]
        bounds = (0.0, 100.0, -5.0, 50.0, 0.0, 100.0)
        sectors.append(hpdtool.Sector(0x0100 * (si + 1) + si, bounds, 0, 100007, 12345678, nodes, edges,
                                      kymesh.write_mesh_reference(mesh)))
    return bytes(hpdtool.write_reference(hpdtool.HpdFile(tuple(range(hpdtool.HEADER_WORDS)), sectors)))


def hpdcheck_cases() -> list[Case]:
    good = _clean_file(1)
    faults = {f: _clean_file(1, f) for f in ('open', 'dline', 'fdir', 'fopen', 'portal', 'order', 'edge', 'zero')}
    rnd = [o2nav._hpd_file(s) for s in (1, 2, 5, 11)]
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('a clean file', {'a.hpd': good}, ['{d}\\a.hpd']),
        ('random files (every check fails somewhere)', {f'r{i}.hpd': b for i, b in enumerate(rnd)},
         [f'{{d}}\\r{i}.hpd' for i in range(len(rnd))]),
        ('an empty file list after a good one, then a missing file', {'a.hpd': good}, ['{d}\\a.hpd', '{d}\\none.hpd']),
        ('a missing file', {}, ['{d}\\none.hpd']),
        ('a folder', {'f': None}, ['{d}\\f']),
        ('not path data', {'a.hpd': b'nope' * 40}, ['{d}\\a.hpd']),
        ('a cut file', {'a.hpd': good[:200]}, ['{d}\\a.hpd']),
        ('a file with no sectors', {'a.hpd': o2nav._hpd_file(9, (0, 0))}, ['{d}\\a.hpd']),
        ('a path in cp1252', {'café.hpd': good}, ['{d}\\café.hpd']),
        ('a path outside cp1252', {'Ж.hpd': faults['open']}, ['{d}\\Ж.hpd']),
    ]
    rows += [(f'one fault: {f}', {'a.hpd': b}, ['{d}\\a.hpd']) for f, b in faults.items()]
    rows.append(('every fault in one run', {f'{f}.hpd': b for f, b in faults.items()},
                 [f'{{d}}\\{f}.hpd' for f in faults]))
    out: list[Case] = [(f'hpdcheck: {n}', 'hpdcheck', f, a, {}, False) for n, f, a in rows]
    return with_variants(out, ['hpdcheck: a path outside cp1252', 'hpdcheck: every fault in one run',
                               'hpdcheck: a path in cp1252'])


def graphstats_cases() -> list[Case]:
    good = _clean_file(1)
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('no files', {}, []),
        ('a clean file', {'a.hpd': good}, ['{d}\\a.hpd']),
        ('isolated nodes', {'a.hpd': _clean_file(3, 'isolated')}, ['{d}\\a.hpd']),
        ('random files', {f'r{i}.hpd': o2nav._hpd_file(s, (1, 3), 30) for i, s in enumerate((4, 6, 8))},
         ['{d}\\r0.hpd', '{d}\\r1.hpd', '{d}\\r2.hpd']),
        ('a file with no nodes (IndexError)', {'a.hpd': o2nav._hpd_file(9, (0, 0))}, ['{d}\\a.hpd']),
        ('an edge past the nodes (IndexError)', {'a.hpd': _clean_file(1, 'edge')}, ['{d}\\a.hpd']),
        ('an Output path with slashes', {'game\\Output\\win32\\ai\\pathdata.hpd': good},
         ['{d}/Output/win32/ai/pathdata.hpd']),
        ('an Output path with backslashes (IndexError)', {'Output\\pathdata.hpd': good}, ['{d}\\Output\\pathdata.hpd']),
        ('a good file, then a missing one', {'a.hpd': good}, ['{d}\\a.hpd', '{d}\\none.hpd']),
        ('not path data', {'a.hpd': b'nope' * 40}, ['{d}\\a.hpd']),
    ]
    out: list[Case] = [(f'graphstats: {n}', 'graphstats', f, a, {}, False) for n, f, a in rows]
    return with_variants(out, ['graphstats: random files', 'graphstats: a good file, then a missing one'])


def hpdview_cases() -> list[Case]:
    good = _clean_file(1)
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('no arguments (IndexError)', {}, []),
        ('two arguments (IndexError)', {'a.hpd': good}, ['{d}\\a.hpd', '0']),
        ('a missing file', {}, ['{d}\\none.hpd', '0', '{d}\\o.png']),
        ('a picture (matplotlib: handed to the Python)', {'a.hpd': good}, ['{d}\\a.hpd', '0', '{d}\\o.png']),
    ]
    out: list[Case] = [(f'hpdview: {n}', 'hpdview', f, a, {}, False) for n, f, a in rows]
    return with_variants(out, ['hpdview: a missing file'])


# ------------------------------------------------------------------------------------------- fake OPFOR folders

def _opfor(extra: dict[str, Any] | None = None, exe: bool = False) -> dict[str, Any]:
    """A stand-in Project OPFOR folder in the case folder (opfor\\game, no game exe unless asked - never used where a
    command line could start the game)."""
    files: dict[str, Any] = {'opfor\\game': None}
    if exe:
        files['opfor\\game\\BFBC2Game.exe'] = b'MZ not a game'
    files.update(extra or {})
    return files


def _row(lv: str, rng: random.Random, **kw: Any) -> dict[str, Any]:
    r: dict[str, Any] = {'level': lv, 'candidate': f'C:\\x\\{lv}.hpd', 'candidate_sha1': 'ab' * 20}
    for k in ('ground', 'extra_ground', 'areas_same', 'portals', 'nodes_same', 'edges_same', 'connectivity',
              'over_connection', 'ai_coverage', 'ai_pairwise', 'ladder_ground', 'df_d_precision', 'df_d_recall',
              'df_f_precision', 'df_f_recall'):
        r[k] = round(rng.uniform(0, 100), rng.choice([1, 2, 4, 12]))
    for k in ('detailed_reference', 'detailed_candidate', 'islands_reference', 'islands_candidate'):
        r[k] = rng.randrange(0, 400)
    r.update(kw)
    return r


def _parity_run(name: str, seed: int, levels: list[str], **over: Any) -> bytes:
    rng = random.Random(seed)
    rows = [_row(lv, rng, **over.get(lv, {})) for lv in levels]
    return json.dumps({'name': name, 'date': f'2026-10-{seed % 28 + 1:02d} 1{seed % 10}:4{seed % 6}', 'levels': rows},
                      indent=1).encode('ascii')


def navparity_cases() -> list[Case]:
    p = 'opfor\\work\\navkit\\parity\\'
    levels = ['sp_009', 'sp_01', 'ons_mp_002', 'sp_004_b', 'mp_001', 'ons_mp_004']
    a = _parity_run('base', 1, levels)
    b = _parity_run('new', 2, levels[:4] + ['zz_new'], sp_01={'areas_same': 0},
                    sp_009={'ground': 12, 'detailed_candidate': 3.5})
    c = _parity_run('same', 3, ['sp_009'], sp_009={'areas_same': 0.0})
    dup = json.dumps({'name': 'dup', 'date': 'x', 'levels': [{'level': 'sp_009', 'areas_same': 1, 'ground': 1},
                                                              {'level': 'sp_009', 'areas_same': 2, 'ground': 3.5},
                                                              {'level': 'sp_01', 'areas_same': 0}]}).encode()
    nonum = json.dumps({'name': 'n', 'date': 'd', 'levels': [{'level': 'sp_01', 'areas_same': 1, 'ground': 'x'}]}).encode()
    empty = json.dumps({'name': 'e', 'date': 'd', 'levels': []}).encode()
    # sum()'s compensated float addition (CPython 3.12+): many levels, values a plain left-to-right sum rounds off
    srng = random.Random(7)
    many = json.dumps({'name': 'many', 'date': 'd', 'levels': [
        {'level': f'lv_{k:02d}', 'areas_same': srng.choice([0, 0.0, 1e-17, 33.3]),
         'ground': srng.choice([0.1, 99.1, 1e16, -1e16, 3, 2.675, 1e-300, srng.uniform(0, 100)]),
         'extra_ground': srng.choice([1, 2, 0.1, 0.7]), 'nodes_same': round(srng.uniform(0, 100), 2)}
        for k in range(40)]}).encode()
    files = _opfor({p + 'base.json': a, p + 'new.json': b, p + 'same_run.json': c, p + 'dup.json': dup,
                    p + 'nonum.json': nonum, p + 'empty.json': empty, p + 'many.json': many, p + 'bad.json': b'{"name": '})
    o = ['--opfor', '{d}\\opfor']
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('show without a name (the usage)', files, ['show'] + o),
        ('an unknown command (the usage)', files, ['what', 'x'] + o),
        ('--opfor without a value', {}, ['show', 'x', '--opfor']),
        ('show one run', files, ['show', 'base'] + o),
        ('show two runs', files, ['show', 'base', 'new'] + o),
        ('show two runs, the other way', files, ['show', 'new', 'base'] + o),
        ('show, extra names ignored', files, ['show', 'base', 'new', 'dup'] + o),
        ('show a name with a space', files, ['show', 'same run'] + o),
        ('show, --opfor first', files, o + ['show', 'dup']),
        ('show a run with repeated levels', files, ['show', 'dup', 'base'] + o),
        ('show a run whose measure is text (handed to the Python)', files, ['show', 'nonum'] + o),
        ('show a run with no levels (ZeroDivisionError)', files, ['show', 'empty'] + o),
        ('show 40 levels (sum()\'s compensated means)', files, ['show', 'many'] + o),
        ('show 40 levels against another run', files, ['show', 'base', 'many'] + o),
        ('show a damaged run (handed to the Python)', files, ['show', 'bad'] + o),
        ('show a missing run', files, ['show', 'nothere'] + o),
        ('show, OPFOR_ROOT', files, ['show', 'base'], ),
        ('run (measures: handed to the Python)', files, ['run', 'x', 'sp_009={d}\\none.hpd'] + o),
    ]
    out: list[Case] = []
    for n, f, a2 in rows:
        env = {'OPFOR_ROOT': '{d}\\opfor'} if 'OPFOR_ROOT' in n else {}
        if not f and n != 'the usage':
            f = _opfor()
        out.append((f'navparity: {n}', 'navparity', f, a2, env, False))
    return with_variants(out, ['navparity: show two runs', 'navparity: show a missing run'])


def navregress_cases() -> list[Case]:
    hpd = 'opfor\\game\\Output\\win32\\ai\\levels\\{}\\infantry\\pathdata.hpd'
    g = _opfor({hpd.format('sp_009'): b'x', hpd.format('sp_01'): b'x',
                hpd.format('sp_02') + '.opfor-orig': b'y', hpd.format('sp_02'): b'x',
                hpd.format('sp_03') + '.opfor-new': b'', hpd.format('sp_03'): b'x',
                hpd.format('ons_mp_002') + '.opfor-stockmp-applied': b'', hpd.format('ons_mp_002'): b'x'})
    o = ['--opfor', '{d}\\opfor']
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('no reference run', g, o),
        ('no reference run, named', g, o + ['--reference', 'my run']),
        ('no reference run, --levels', g, o + ['--levels', 'sp_009,sp_01']),
        ('no reference run, an empty --levels', g, o + ['--levels', '']),
        ('a tolerance', g, o + ['--tolerance', ' 0.1 ']),
        ('a tolerance Python cannot read', g, o + ['--tolerance', 'x']),
        ('--tolerance without a value', g, o + ['--tolerance']),
        ('--opfor without a value', g, ['--opfor']),
        ('--levels without a value', g, o + ['--levels']),
        ('OPFOR_ROOT', g, []),
    ]
    out: list[Case] = []
    for n, f, a in rows:
        env = {'OPFOR_ROOT': '{d}\\opfor'} if n == 'OPFOR_ROOT' else {}
        out.append((f'navregress: {n}', 'navregress', f, a, env, False))
    return with_variants(out, ['navregress: no reference run, named'])


def detailed_cases() -> list[Case]:
    g = _opfor()
    env = {'OPFOR_ROOT': '{d}\\opfor'}
    rows: list[tuple[str, list[str]]] = [
        ('the usage', []),
        ('no --offsets (KeyError)', ['sp_009']),
        ('--offsets with one value', ['sp_009', '--offsets', '5']),
        ('--offsets with three values', ['sp_009', '--offsets', '5,6,7']),
        ('--offsets, a third value that is no int', ['sp_009', '--offsets', '5,6,x']),
        ('--offsets, no int', ['sp_009', '--offsets', 'a,6']),
        ('--offsets, spaces and signs', ['sp_009', '--offsets', ' -5 ,+6']),
        ('--offsets as the last argument (KeyError)', ['sp_009', '--samples', 'x', '--offsets']),
        ('a level not prepared (handed to the Python)', ['sp_009', '--offsets', '5,6']),
    ]
    out: list[Case] = [(f'detailed: {n}', 'detailed', g, a, env, False) for n, a in rows]
    return with_variants(out, ['detailed: no --offsets (KeyError)'])


def sample_all_cases() -> list[Case]:
    g = _opfor()                      # no BFBC2Game.exe: navkit.Project refuses before anything is sampled
    env = {'OPFOR_ROOT': '{d}\\opfor'}
    rows: list[tuple[str, list[str]]] = [
        ('no game copy', []),
        ('no game copy, a level list', ['--levels', 'sp_009,sp_01', '--pitch', '0.8']),
        ('a pitch Python cannot read', ['--pitch', 'fast']),
        ('a pitch with spaces', ['--pitch', ' 0.2 ']),
        ('a budget Python cannot read', ['--budget', '1.5']),
        ('a budget, then no game copy', ['--budget', ' 400 ']),
    ]
    out: list[Case] = [(f'sample_all: {n}', 'sample_all', g, a, env, False) for n, a in rows]
    return with_variants(out, ['sample_all: no game copy'])


def cases() -> list[Case]:
    return (hpdcheck_cases() + graphstats_cases() + hpdview_cases() + navparity_cases() + navregress_cases()
            + detailed_cases() + sample_all_cases())


# ------------------------------------------------------------------------------------------- the game

def opfor_calls(opfor: str) -> list[Call]:
    """Project OPFOR's real inputs: DICE's 15 path data files and NavKit's regression builds (read in place),
    its saved parity runs (copied into a stand-in folder), navregress's no-reference answer on the real folder
    (which only reads it: its parity folder is there already)."""
    calls: list[Call] = []
    lv_dir = os.path.join(opfor, 'game', 'Output', 'win32', 'ai', 'levels')
    dice: list[str] = []
    for lv in LEVELS + ['ons_mp_002', 'ons_mp_004']:
        p = os.path.join(lv_dir, lv, 'infantry', 'pathdata.hpd')
        for cand in (p + '.opfor-orig', p):
            if os.path.isfile(cand):
                dice.append(cand)
                break
    if dice:
        calls.append(('hpdcheck DICE\'s files (run_tests\' line)', 'hpdcheck', dice, {}, {}, True))
        # its own IndexError: graphstats names a path with Output by its '/' parts, and these have backslashes
        calls.append(('graphstats DICE\'s files (backslashes: IndexError)', 'graphstats', dice, {}, {}, False))
        calls.append(('graphstats DICE\'s files, paths with slashes', 'graphstats', [d.replace('\\', '/') for d in dice],
                      {}, {}, True))
    regress = os.path.join(opfor, 'work', 'navkit', 'regress')
    if os.path.isdir(regress):
        built = sorted(os.path.join(regress, f) for f in os.listdir(regress) if f.endswith('.hpd'))
        for p in built[:12]:
            calls.append((f'hpdcheck {os.path.basename(p)}', 'hpdcheck', [p], {}, {}, False))
        calls.append(('graphstats NavKit\'s regression builds', 'graphstats', built[:12], {}, {}, True))
    parity = os.path.join(opfor, 'work', 'navkit', 'parity')
    if os.path.isdir(parity):
        runs = sorted(f for f in os.listdir(parity) if f.endswith('.json'))
        files: dict[str, Any] = {'opfor\\game': None}
        for f in runs:
            files['opfor\\work\\navkit\\parity\\' + f] = os.path.join(parity, f)
        o = ['--opfor', '{d}\\opfor']
        for f in runs:
            calls.append((f'navparity show {f}', 'navparity', ['show', f[:-5]] + o, files, {}, False))
        for a, b in zip(runs, runs[1:]):
            calls.append((f'navparity show {a} {b}', 'navparity', ['show', a[:-5], b[:-5]] + o, files, {}, False))
    calls.append(('navregress, no such reference run (the real folder, read only)', 'navregress',
                  ['--opfor', opfor, '--reference', 'no_such_run_o4'], {}, {}, False))
    calls.append(('navregress --levels sp_004_b,sp_009, no such reference run', 'navregress',
                  ['--opfor', opfor, '--levels', 'sp_004_b,sp_009', '--reference', 'no_such_run_o4'], {}, {}, False))
    calls.append(('detailed sp_009 (the choice: handed to the Python)', 'detailed', ['sp_009', '--offsets', '-17,-9'],
                  {}, {'OPFOR_ROOT': opfor}, True))
    return calls
