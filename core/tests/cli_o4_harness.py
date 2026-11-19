r"""Round O4's harness group: the command lines test_cli_o4.py replays for the map and bot test harness's harness.py.

cases(): the quick tier's (synthetic saved runs and opfor.log texts made in the case folder; --runs / --opfor point
into it; `run`, which tests path data, is handed to the Python and is replayed only where it stops before testing).
opfor_calls(opfor): the --game tier's (Project OPFOR's saved runs copied into the case folder; import-log reads the real
opfor.log in place and writes only the copied run). The harness never starts the game.
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

STATUSES = ['pass', 'info', 'warn', 'fail', 'skip', 'hold', 'error']


def _result(rng: random.Random, status: str | None = None, **metrics: Any) -> dict[str, Any]:
    st = status or rng.choice(STATUSES)
    r: dict[str, Any] = {'status': st, 'summary': f'summary {rng.randrange(1000)} é', 'metrics': dict(metrics),
                         'details': [f'detail {k}' for k in range(rng.choice([0, 1, 3, 5]))],
                         'seconds': round(rng.uniform(0, 30), 1)}
    return r


def _run(name: str, seed: int, levels: list[str], tweak: Any = None) -> dict[str, Any]:
    rng = random.Random(seed)
    run: dict[str, Any] = {'format': 'bfbc2-harness-run', 'version': 1, 'name': name,
                           'date': f'2026-10-{seed % 28 + 1:02d} 2{seed % 4}:{seed % 60:02d}', 'host': 'PC',
                           'suite': {'commit': 'abc1234', 'changed': bool(seed % 2)},
                           'options': {'levels': levels, 'pairs': 200}, 'levels': {}, 'seconds': 312 + seed}
    for lv in levels:
        L: dict[str, Any] = {'name': lv.upper(), 'notes': ['a note'] if seed % 3 == 0 else [],
                             'points': {'sources': 107, 'targets': 64, 'orders': 179, 'infantry_sources': 100,
                                        'infantry_targets': 60, 'infantry_orders': 170},
                             'excluded': {'sources': {'vehicle': 4, 'boat': 3}, 'targets': {}, 'orders': {'air': 9}},
                             'files': {}, 'level': {}, 'ingame': {}}
        for key in ('dice', 'navkit'):
            F: dict[str, Any] = {'path': f'C:\\x\\{lv}\\{key}.hpd', 'sha1': f'{seed:02d}{key}' + 'f' * 34, 'size': 9,
                                 'tests': {}}
            T = F['tests']
            T['structure'] = _result(rng, 'pass', problems=0)
            T['reachability'] = _result(rng, None, reach_pct=round(rng.uniform(80, 100), 2), reach_over_pct=1.5,
                                        targets_lost=rng.randrange(3), islands=rng.randrange(1, 9),
                                        main_island_nodes_pct=77.0)
            T['battery'] = _result(rng, None, found_pct=round(rng.uniform(80, 100), 2), extra_pct=0.5,
                                   length_ratio_median=round(rng.uniform(0.95, 1.05), 3),
                                   length_ratio_p90=round(rng.uniform(0.95, 1.2), 3), pairs=200)
            T['battery']['data'] = {'random': {'points': [[1, 2], [3, 4]], 'bits': ''.join(
                rng.choice('01') for _ in range(40))}, 'level': {'points': 7, 'bits': '0101'}}
            T['orders'] = _result(rng, None, on_area_pct=round(rng.uniform(50, 100), 2), orders=179)
            T['offsets'] = _result(rng, 'pass', matches=1)
            if key == 'navkit':
                T['parity'] = _result(rng, 'info', ground=round(rng.uniform(80, 99), 2), extra_ground=3.25,
                                      nodes_same=88.5, df_d_recall=50.0, sectors_candidate=40)
            L['files'][key] = F
        L['level'] = {'has-ai': _result(rng, 'pass', has_ai=1),
                      'offsets-asset': {'status': 'pass', 'summary': 'x', 'metrics': {'x': -17, 'z': -9, 'vx': -17,
                                                                                       'vz': -8}, 'details': [],
                                        'seconds': 0.0}}
        L['ingame'] = {'loads-ai-init': _result(rng, 'hold')}
        run['levels'][lv] = L
    if tweak:
        tweak(run)
    return run


def _dump(run: Any) -> bytes:
    return json.dumps(run, indent=1).replace('\n', '\r\n').encode('ascii')


def _new(run: dict[str, Any]) -> None:
    """Changes for compare: worse / better / count metrics, a test now failing, one fixed, lost pairs, a test only one
    side has, a level only one side has (one of each: Python lists set differences in hash order)."""
    nk = run['levels']['sp_009']['files']['navkit']['tests']
    nk['reachability']['metrics']['reach_pct'] = 50.0
    nk['battery']['metrics']['found_pct'] = 99.9
    nk['battery']['metrics']['length_ratio_median'] = 1.5
    nk['battery']['metrics']['pairs'] = 201
    nk['orders']['status'] = 'fail'
    nk['parity']['metrics']['ground'] = 1
    nk['parity']['metrics']['extra_ground'] = 3.3
    nk['parity']['metrics']['sectors_candidate'] = 41
    nk['battery']['data']['random']['bits'] = '0' * 40
    dk = run['levels']['sp_009']['files']['dice']['tests']
    dk['structure']['status'] = 'error'
    del dk['offsets']
    run['levels']['sp_009']['files']['dice']['sha1'] = 'beef' * 10
    run['levels']['sp_009']['level']['new-check'] = {'status': 'fail', 'summary': 'n', 'metrics': {}, 'details': []}
    run['levels']['mp_001'] = run['levels'].pop('sp_004_b')


def _odd(run: dict[str, Any]) -> None:
    run['suite'] = None
    run['seconds'] = 12.5
    run['host'] = None
    L = run['levels']['sp_009']
    del L['excluded']
    L['name'] = ''
    L['files']['dice']['sha1'] = None
    L['files']['dice']['tests']['structure']['status'] = 'strange'
    L['files']['navkit']['tests']['battery']['details'] = [f'd{k}' for k in range(30)]
    L['files']['navkit']['tests']['battery']['status'] = 'fail'
    L['ingame'] = {}


LOG = '\n'.join([
    '[10:00:00.000] ---- opfor.dll loaded into PID 4242 ----',
    '[10:00:01.000] queued hostMultiplayerLocal request: level=\'Levels/SP_009/SP_009\'',
    '[10:00:02.000] Session::updateState arg1=00000002',
    '[10:00:03.000] Session::updateState arg1=00000003',
    '[10:00:03.500] AI: init',
    '[10:00:04.000] sector offsets load handle 1 X(+1Ch)=-17 Z(+18h)=-9',
    '[10:00:04.100] sector offsets load handle 2 X(+1Ch)=-17 Z(+18h)=-8',
    '[10:00:05.000] unit counters (interval): 0 pathfinders',
    '[10:00:06.000] unit counters (interval): 12 pathfinders',
    '[10:00:07.000] exit(0) called',
    '[11:00:00.000] ---- opfor.dll loaded into PID 5151 ----',
    '[11:00:01.000] dev trigger: level=\'Levels/SP_009/SP_009\'',
    '[11:00:02.000] sector offsets load handle 1 X(+1Ch)=-16 Z(+18h)=-9',
    '[11:00:02.100] sector offsets: on - X=-16 Z=-9',
    '[11:00:03.000] wrote crashreport.xml',
    '[12:00:00.000] ---- opfor.dll loaded into PID 6161 ----',
    '[12:00:01.000] dev trigger: level=\'Levels/MP_001/MP_001\'',
    '[12:00:02.000] Session::updateState arg1=00000003',
    '[12:00:03.000] AI: init',
] + [f'[12:00:{10 + k}.000] unit counters (interval): {k * 3} pathfinders' for k in range(11)]) + '\n'


def cases() -> list[Case]:
    lv2 = ['sp_009', 'sp_004_b']
    base = _run('base', 1, lv2)
    new = _run('new', 1, lv2, _new)
    odd = _run('odd', 6, ['sp_009'], _odd)
    other = {'format': 'something-else', 'name': 'x'}
    newer = dict(base, version=2)
    runs: dict[str, Any] = {'runs\\base.json': _dump(base), 'runs\\new.json': _dump(new), 'runs\\odd.json': _dump(odd),
                            'runs\\other.json': _dump(other), 'runs\\newer.json': _dump(newer),
                            'runs\\notes.txt': b'not a run', 'runs\\my run.json': _dump(_run('my run', 9, ['sp_01']))}
    bad = dict(runs, **{'runs\\broken.json': b'{"format": '})
    log = {'opfor.log': LOG.encode('utf-8')}
    fake = {'opfor\\game': None, 'opfor\\game\\logs\\opfor.log': LOG.encode('utf-8')}
    r = ['--runs', '{d}\\runs']
    env_fake = {'OPFOR_ROOT': '{d}\\opfor'}
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('the usage', {}, [], {}),
        ('an unknown command (the usage)', {}, ['what'], {}),
        ('list', runs, ['list'] + r, {}),
        ('list, a damaged run (handed to the Python)', bad, ['list'] + r, {}),
        ('list, an empty folder', {'runs': None}, ['list'] + r, {}),
        ('list, a missing folder', {}, ['list', '--runs', '{d}\\none'], {}),
        ('list, the runs folder of OPFOR_ROOT (made)', dict(fake, **{'opfor\\work\\harness\\runs\\base.json':
                                                                     _dump(base)}), ['list'], env_fake),
        ('list, --runs without a value', runs, ['list', '--runs'], {}),
        ('show', runs, ['show', 'base'] + r, {}),
        ('show --details 1', runs, ['show', 'new', '--details', '1'] + r, {}),
        ('show --details -1', runs, ['show', 'odd', '--details', '-1'] + r, {}),
        ('show --details 99', runs, ['show', 'odd', '--details', '99'] + r, {}),
        ('show --details x', runs, ['show', 'odd', '--details', 'x'] + r, {}),
        ('show a run with odd values', runs, ['show', 'odd'] + r, {}),
        ('show a name with a space', runs, ['show', 'my run'] + r, {}),
        ('show a .json path', runs, ['show', '{d}\\runs\\new.json'], {}),
        ('show without a run', runs, ['show'] + r, {}),
        ('show a missing run', runs, ['show', 'nothere'] + r, {}),
        ('show a file of another format', runs, ['show', 'other'] + r, {}),
        ('show a run of a newer harness', runs, ['show', 'newer'] + r, {}),
        ('show a damaged run (handed to the Python)', bad, ['show', 'broken'] + r, {}),
        ('compare', runs, ['compare', 'base', 'new'] + r, {}),
        ('compare the other way', runs, ['compare', 'new', 'base'] + r, {}),
        ('compare --scale 10', runs, ['compare', 'base', 'new', '--scale', '10'] + r, {}),
        ('compare --scale 0', runs, ['compare', 'base', 'new', '--scale', '0'] + r, {}),
        ('compare --scale x', runs, ['compare', 'base', 'new', '--scale', 'x'] + r, {}),
        ('compare a run with itself', runs, ['compare', 'base', 'base'] + r, {}),
        ('compare, two levels missing (hash order: handed to the Python)', runs,
         ['compare', '{d}\\runs\\my run.json', 'base'] + r, {'PYTHONHASHSEED': '0'}),
        ('compare with a run of one level', runs, ['compare', 'base', 'odd'] + r, {}),
        ('compare with one run', runs, ['compare', 'base'] + r, {}),
        ('compare with a missing run', runs, ['compare', 'base', 'none'] + r, {}),
        ('ingame', {}, ['ingame', 'sp_009'], {}),
        ('ingame without a level', {}, ['ingame'], {}),
        ('import-log', dict(runs, **log), ['import-log', 'base', '--level', 'sp_009', '--log', '{d}\\opfor.log'] + r, {}),
        ('import-log, the first session', dict(runs, **log),
         ['import-log', 'base', '--level', 'Levels/SP_009', '--log', '{d}\\opfor.log', '--session', '0'] + r, {}),
        ('import-log, a level the run lacks', dict(runs, **log),
         ['import-log', 'base', '--level', 'mp_001', '--log', '{d}\\opfor.log'] + r, {}),
        ('import-log, a level with no session', dict(runs, **log),
         ['import-log', 'base', '--level', 'sp_01', '--log', '{d}\\opfor.log'] + r, {}),
        ('import-log, session out of range', dict(runs, **log),
         ['import-log', 'base', '--level', 'sp_009', '--log', '{d}\\opfor.log', '--session', '5'] + r, {}),
        ('import-log, a missing log', runs, ['import-log', 'base', '--level', 'sp_009', '--log', '{d}\\none.log'] + r,
         {}),
        ('import-log, OPFOR_ROOT\'s log', dict(fake, **runs), ['import-log', 'base', '--level', 'sp_009'] + r,
         env_fake),
        ('import-log without --level', runs, ['import-log', 'base'] + r, {}),
        ('import-log without a run', runs, ['import-log', '--level', 'sp_009'] + r, {}),
        ('import-log, --level without a value', runs, ['import-log', 'base', '--level'], {}),
        ('import-log, a missing run', dict(runs, **log),
         ['import-log', 'none', '--level', 'sp_009', '--log', '{d}\\opfor.log'] + r, {}),
        ('run --reference to a missing run (handed to the Python)', dict(fake, **runs),
         ['run', 'x', '--reference', 'none'] + r, env_fake),
    ]
    out: list[Case] = [(f'harness: {n}', 'harness', f, a, e, False) for n, f, a, e in rows]
    return with_variants(out, ['harness: show', 'harness: compare', 'harness: import-log', 'harness: list'])


def opfor_calls(opfor: str) -> list[Call]:
    """Project OPFOR's saved harness runs (work\\harness\\runs), copied into the case folder: list, show, compare of
    neighbours, and import-log of the real opfor.log (read in place) into a copy."""
    calls: list[Call] = []
    folder = os.path.join(opfor, 'work', 'harness', 'runs')
    if not os.path.isdir(folder):
        return calls
    names = sorted(f for f in os.listdir(folder) if f.endswith('.json'))
    files: dict[str, Any] = {f'runs\\{f}': os.path.join(folder, f) for f in names}
    r = ['--runs', '{d}\\runs']
    calls.append(('harness list', 'harness', ['list'] + r, files, {}, True))
    for f in names:
        calls.append((f'harness show {f}', 'harness', ['show', f[:-5]] + r, files, {}, True))
        calls.append((f'harness show {f} --details 25', 'harness', ['show', f[:-5], '--details', '25'] + r, files, {},
                      True))
    for a, b in zip(names, names[1:]):
        # PYTHONHASHSEED: a set difference of two or more findings is listed in Python's str hash order (bfbc2.exe
        # hands those to the Python); a fixed seed makes the two Python runs list them alike
        calls.append((f'harness compare {a} {b}', 'harness', ['compare', a[:-5], b[:-5]] + r, files,
                      {'PYTHONHASHSEED': '0'}, False))
    log = os.path.join(opfor, 'game', 'logs', 'opfor.log')
    if os.path.isfile(log):
        for f in names[:3]:
            for lv in ('sp_009', 'mp_001', 'mp_005'):
                calls.append((f'harness import-log {f} {lv} (the real opfor.log, read in place)', 'harness',
                              ['import-log', f[:-5], '--level', lv, '--log', log] + r, files, {}, False))
    return calls
