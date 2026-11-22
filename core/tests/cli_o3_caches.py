r"""Round O3's caches group: the command lines test_cli_o3.py replays for `python -m bfbc2core.caches`
(core\bfbc2core\caches.py) and for the log folder's start-up pruning every guarded tool does (errors.setup:
caches.prune_logs with its archive), which bfbc2.exe now does too.

cases(): the quick tier's: caches, project caches, logs and Rust build folders made in the case folder and the run's
own local app data ({lad}); every variable that names a cache, a cap, Project OPFOR's folder or the build root is set
by the case, so nothing outside the case's folders is read or touched. opfor_calls(opfor): the --game tier's, read-only:
--list / --report / --prune-targets over the real caches, project caches and build folders (listed, never pruned), and
--list / --prune / --clear of logs on a copy of the real log folder in the run's local app data.
"""
from __future__ import annotations

import os
import time
from typing import Any

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}

CACHE_ENVS = ('BFBC2_HLSL_CACHE', 'BFBC2_FBRB_CACHE', 'BFBC2_COLLISION_CACHE', 'BFBC2_EFFECTPORT_CACHE',
              'BFBC2_CULLTREE_CACHE', 'BF1943_SOURCE_INDEX', 'BFBC2_SHADERCOVER_CACHE', 'BFBC2_RENDERQA_CACHE')
CAP_ENVS = ('BFBC2_HLSL_CACHE_MB', 'BFBC2_FBRB_CACHE_GB', 'BFBC2_COLLISION_CACHE_MB', 'BFBC2_EFFECTPORT_CACHE_MB',
            'BFBC2_CULLTREE_CACHE_MB', 'BF1943_SOURCE_INDEX_MB', 'BFBC2_SHADERCOVER_CACHE_MB', 'BFBC2_RENDERQA_CACHE_MB',
            'BFBC2_NAVSTUDIO_CACHE_MB', 'BFBC2_HARNESS_CACHE_MB', 'BFBC2_NAVWATER_CACHE_MB', 'BFBC2_EDITOR_CACHE_MB', 'BFBC2_MESHVIEW_CACHE_MB',
            'BFBC2_CACHE_TOTAL_GB', 'BFBC2_LOGS_MB', 'BFBC2_LOG_TOOL_MB', 'BFBC2_LOG_FILE_MB', 'BFBC2_LOG_ARCHIVE_MB')
# every case: caches at their defaults under the run's local app data, no Project OPFOR (OPFOR_ROOT names a folder
# without game\; the reference copy's folders above hold none), the build root in the case folder
BASE = {**dict.fromkeys(CACHE_ENVS + CAP_ENVS, ''), 'OPFOR_ROOT': '{d}\\no-opfor', 'CARGO_TARGET_DIR': 'tgt',
        'BFBC2_RUST_TARGETS': '{d}\\targets', 'COLUMNS': ''}
CACHE = '{lad}/bfbc2-tool-suite/cache'
LOGS = '{lad}/bfbc2-tool-suite/logs'
DAY = 86400.0
T0 = 1_767_268_800.0            # 2026-01-01 12:00 UTC: the old files' times (fixed: the archive keeps them)


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


def env(**kw: str) -> dict[str, str]:
    e = dict(BASE)
    e.update(kw)
    if e.get('COLUMNS') == '':
        del e['COLUMNS']                    # argparse's width: the pipe's 80 (an empty COLUMNS is not unset)
    return e


def blob(n: int, k: int = 0) -> bytes:
    return bytes((i * 7 + k) & 0xFF for i in range(n))


def caches_files(now: float) -> dict[str, Any]:
    """Every suite cache with entries of known ages and sizes, temporary files (stale and fresh), markers, files of
    no kind and dot files."""
    f: dict[str, Any] = {}
    for i in range(5):                                          # oldest first
        f[f'{CACHE}/fbrb/{i:02x}/payload{i}.payload'] = (blob(3000 + 100 * i, i), T0 + i * DAY)
    f[f'{CACHE}/fbrb/aa.seen'] = (b'', T0)                       # an old marker (30 days)
    f[f'{CACHE}/fbrb/bb.seen'] = (b'', now - 60)                 # a fresh one
    f[f'{CACHE}/fbrb/x.payload.tmp-123'] = (blob(500), now - 7200)   # a dead writer's
    f[f'{CACHE}/fbrb/y.payload.tmp-456'] = (blob(400), now - 30)     # a running writer's
    f[f'{CACHE}/fbrb/readme.txt'] = (b'not a cache entry', T0)
    f[f'{CACHE}/fbrb/.pruned'] = (b'', T0)
    f[f'{CACHE}/hlsl/dll1/a.bin'] = (blob(2048), T0 + 10 * DAY)
    f[f'{CACHE}/hlsl/dll1/b.bin'] = (blob(1024), T0 + 11 * DAY)
    f[f'{CACHE}/hlsl/dll2/c.bin.tmp-9-1'] = (blob(100), T0)
    f[f'{CACHE}/collision/m.bin'] = (blob(1500), T0 + 2.5 * DAY)
    f[f'{CACHE}/effectport/e.pkl'] = (blob(700), T0 + 3.5 * DAY)
    f[f'{CACHE}/culltree/r1.pkl'] = (blob(5000), T0 + 1.5 * DAY)
    f[f'{CACHE}/culltree/sub/r2.pkl'] = (blob(6000), T0 + 4.5 * DAY)
    f[f'{CACHE}/culltree/w.pkl.x.tmp'] = (blob(10), now)
    f[f'{CACHE}/bf1943source/s.pkl'] = (blob(1200), T0 + 20 * DAY)
    f[f'{CACHE}/shadercover/c.pkl'] = (blob(64), T0 + 21 * DAY)
    return f


def opfor_files(now: float) -> dict[str, Any]:
    """A fake Project OPFOR (game\\ and the project caches' folders) and its work\\ folders of every kind."""
    o = '{d}/opfor'
    f: dict[str, Any] = {f'{o}/game': None}
    f[f'{o}/work/navstudio/cache/a.navdata.pkl'] = (blob(4000), T0 + 5 * DAY)
    f[f'{o}/work/navstudio/cache/a.npz'] = (blob(3000), T0 + 6 * DAY)
    f[f'{o}/work/navstudio/cache/a.123.tmp'] = (blob(50), T0)
    f[f'{o}/work/harness/cache/h.npz'] = (blob(2500), T0 + 7 * DAY)
    f[f'{o}/work/harness/bundles/keep.fbrb'] = (blob(900), T0)
    f[f'{o}/work/navwater/cache/w.navdata.pkl'] = (blob(1800), T0 + 0.5 * DAY)
    f[f'{o}/work/editor/levels/mp_001/x.dbx'] = (blob(2200), T0 + 8 * DAY)
    f[f'{o}/work/editor/levels/mp_001/editor-level.json'] = (b'{}', T0 + 8 * DAY)
    f[f'{o}/work/levels/mp_005/a.dbx'] = blob(3000)
    f[f'{o}/work/ww2/out.txt'] = blob(1500)
    f[f'{o}/work/console_cull/s.bin'] = blob(2048 * 1024 + 5)
    f[f'{o}/work/mystery/z'] = blob(10)
    f[f'{o}/work/backups/b.zip'] = blob(77)
    f[f'{o}/work/note.txt'] = b'top-level file'
    f[f'{o}/work/empty_dir'] = None
    return f


def targets_files(now: float) -> dict[str, Any]:
    """A build root: a folder whose checkout is gone and old, one gone but written today, a folder that is no build
    folder. None is named after a checkout: which checkouts sit beside the suite depends on where it is, as this
    checkout's default build folder does, so the cases name CARGO_TARGET_DIR (BASE: tgt, under the suite's rust\\)."""
    t = '{d}/targets'
    f: dict[str, Any] = {}
    f[f'{t}/gone-old/.rustc_info.json'] = (b'{}', T0)
    f[f'{t}/gone-old/debug/deps/x.rlib'] = (blob(3000), T0)
    f[f'{t}/gone-old/debug/deps'] = (None, T0)
    f[f'{t}/gone-old/debug'] = (None, T0)
    f[f'{t}/gone-old'] = (None, T0)
    f[f'{t}/gone-new/CACHEDIR.TAG'] = (b'Signature', now)
    f[f'{t}/notcargo/file.txt'] = (b'x', T0)
    f[f'{t}/notcargo'] = (None, T0)
    return f


def log_name(tool: str, day: int, part: int = 0) -> str:
    date = time.strftime('%Y-%m-%d', time.gmtime(T0 + day * DAY))
    return f'{LOGS}/{tool}-{date}.log' if part == 0 else f'{LOGS}/{tool}-{date}.{part}.log'


def logs_files(now: float, days: int = 22, size: int = 3000) -> dict[str, Any]:
    """Old logs: a tool with more than 20 days (two parts on one day), two more tools, files that are not logs, a
    folder below; a fresh log (written in the last 10 minutes)."""
    f: dict[str, Any] = {}
    for d in range(days):
        f[log_name('navkit', d)] = (blob(size, d), T0 + d * DAY)
    f[log_name('navkit', 3, 1)] = (blob(500), T0 + 3 * DAY + 60)
    for d in range(4):
        f[log_name('aptcheck', d)] = (blob(4000, d), T0 + d * DAY + 30)
        f[log_name('NavKit', d + 30)] = (blob(1000, d), T0 + (d + 30) * DAY)
    f[f'{LOGS}/notes.txt'] = (b'not a log', T0)
    f[f'{LOGS}/sub/navkit-2026-01-01.log'] = (b'below the folder', T0)
    f[log_name('fresh', 50)] = (blob(800), now - 30)
    return f


def cases() -> list[Case]:
    now = time.time()
    caches = caches_files(now)
    opfor = opfor_files(now)
    targets = targets_files(now)
    logs = logs_files(now)
    rows: list[Case] = []

    def add(key: str, files: dict[str, Any], args: list[str], e: dict[str, str] | None = None,
            tool: str = 'caches') -> None:
        rows.append((key, tool, files, args, e if e is not None else env(), False))

    # ---- --list (the default)
    add('list empty', {}, [])
    add('list', caches, [])
    add('list --list', caches, ['--list'])
    add('list names', caches, ['fbrb', 'logs', 'hlsl', 'hlsl'])
    add('list logs', logs, ['logs'])
    add('list logs caps', logs, ['logs'], env(BFBC2_LOGS_MB='1.5', BFBC2_LOG_TOOL_MB='0.25', BFBC2_LOG_FILE_MB='x'))
    add('list off', caches, [], env(BFBC2_HLSL_CACHE='0', BFBC2_FBRB_CACHE='OFF', BFBC2_CULLTREE_CACHE='off'))
    add('list moved', {**caches, '{d}/elsewhere/q.payload': blob(999)}, ['fbrb', 'collision'],
        env(BFBC2_FBRB_CACHE='{d}\\elsewhere', BFBC2_COLLISION_CACHE='{d}\\missing'))
    add('list caps', caches, [], env(BFBC2_FBRB_CACHE_GB='0.5', BFBC2_HLSL_CACHE_MB='1e3', BFBC2_CULLTREE_CACHE_MB='-1',
                                     BFBC2_EFFECTPORT_CACHE_MB='inf', BFBC2_CACHE_TOTAL_GB='1_000',
                                     BFBC2_SHADERCOVER_CACHE_MB=' 12.5 '))
    add('list big sizes', {f'{CACHE}/fbrb/big.payload': blob(3 << 20), f'{CACHE}/hlsl/k.bin': blob(1023),
                           f'{CACHE}/collision/k.bin': blob(1024)}, [])
    add('list project', {**caches, **opfor}, [], env(OPFOR_ROOT='{d}\\opfor'))
    add('list --opfor', {**caches, **opfor}, ['--opfor', '{d}\\opfor', 'navstudio', 'harness', 'editor'])
    add('list --opfor= form', opfor, ['--opfor={d}\\opfor', 'navwater'])
    add('list --opfor empty', opfor, ['--opfor', '', 'navwater'])
    add('list --opfor bad', opfor, ['--opfor', '{d}\\opfor\\work'])
    add('list no project', opfor, ['navstudio', 'editor'])
    add('list unicode folder', {'{d}/ĉache/a.payload': blob(10)}, ['fbrb'], env(BFBC2_FBRB_CACHE='{d}\\ĉache'))
    add('list cp1252 folder', {'{d}/café/a.payload': blob(10)}, ['fbrb'], env(BFBC2_FBRB_CACHE='{d}\\café'))
    add('unknown cache', caches, ['fbrb', 'nope', 'logz'])
    add('unknown cache with --prune', caches, ['--prune', 'nope'])
    # ---- --prune
    add('prune nothing to do', caches, ['--prune'])
    add('prune all tight', {**caches, **logs}, ['--prune'],
        env(BFBC2_FBRB_CACHE_GB='0.000005', BFBC2_HLSL_CACHE_MB='0.002', BFBC2_CULLTREE_CACHE_MB='0.001'))
    add('prune all budget', {**caches, **opfor}, ['--prune'],
        env(OPFOR_ROOT='{d}\\opfor', BFBC2_CACHE_TOTAL_GB='0.00001'))
    add('prune budget zero', {**caches, **opfor}, ['--prune'], env(OPFOR_ROOT='{d}\\opfor', BFBC2_CACHE_TOTAL_GB='0'))
    add('prune fbrb', caches, ['--prune', 'fbrb'], env(BFBC2_FBRB_CACHE_GB='0.00001'))
    add('prune names', caches, ['culltree', 'hlsl', '--prune'], env(BFBC2_CULLTREE_CACHE_MB='0.005'))
    add('prune project', {**caches, **opfor}, ['--prune', 'navstudio', 'editor', '--opfor', '{d}\\opfor'],
        env(BFBC2_NAVSTUDIO_CACHE_MB='0.003', BFBC2_EDITOR_CACHE_MB='0'))
    add('prune project none', caches, ['--prune', 'harness'])
    add('prune off', caches, ['--prune', 'hlsl'], env(BFBC2_HLSL_CACHE='0'))
    add('prune logs', logs, ['--prune', 'logs'])
    add('prune logs caps', logs, ['--prune', 'logs'], env(BFBC2_LOGS_MB='0.02', BFBC2_LOG_TOOL_MB='0.015'))
    add('prune logs archive cap', {**logs, '{lad}/bfbc2-tool-suite/logs-archive/logs-2026-01-01_120000.zip':
                                   (blob(30000), T0), '{lad}/bfbc2-tool-suite/logs-archive/old.zip': (blob(20000), T0 - DAY)},
        ['--prune', 'logs'], env(BFBC2_LOG_ARCHIVE_MB='0.04'))
    add('prune logs none', {}, ['--prune', 'logs'])
    add('prune logs nothing old', logs_files(now, days=5), ['--prune', 'logs'])
    # ---- --clear
    add('clear', {**caches, **logs, **opfor}, ['--clear'], env(OPFOR_ROOT='{d}\\opfor'))
    add('clear named', {**caches, **logs, **opfor}, ['--clear', 'navstudio', 'logs', 'fbrb'], env(OPFOR_ROOT='{d}\\opfor'))
    add('clear empty', {}, ['--clear'])
    # ---- --report
    add('report', {**caches, **logs, **opfor, **targets}, ['--report'], env(OPFOR_ROOT='{d}\\opfor'))
    add('report no work', {**caches, **targets}, ['--report', '--no-work'])
    add('report no project', {**caches, **targets}, ['--report'])
    add('report no targets', caches, ['--report', '--no-work'], env(BFBC2_RUST_TARGETS='{d}\\none'))
    add('report target dir', targets, ['--report', '--no-work'], env(CARGO_TARGET_DIR=' tgt\\..\\t3/sub '))
    add('report target dir rooted', targets, ['--report', '--no-work'], env(CARGO_TARGET_DIR='\\rooted\\t'))
    add('report target dir abs', targets, ['--report', '--no-work'], env(CARGO_TARGET_DIR='{d}\\t2'))
    # ---- --prune-targets
    add('prune-targets', targets, ['--prune-targets'])
    add('prune-targets yes', targets, ['--prune-targets', '--yes'])
    add('prune-targets none', {}, ['--prune-targets'])
    add('prune-targets yes nothing', {'{d}/targets/plain/x.txt': b'x'}, ['--yes', '--prune-targets'])
    # ---- argparse
    add('help', {}, ['-h'])
    add('help long', {}, ['--list', '--help'])
    add('error two modes', {}, ['--list', '--prune'])
    add('error two modes later', {}, ['--report', '--yes', '--clear'])
    add('same mode twice', caches, ['--list', '--list', 'fbrb'])
    add('error --opfor missing', {}, ['--opfor'])
    add('error --opfor option', {}, ['--opfor', '--list'])
    add('error unknown option', {}, ['--bogus', 'fbrb'])
    add('error names twice', {}, ['fbrb', '--list', 'hlsl'])
    add('error single dash', {}, ['-x'])
    add('negative number name', {}, ['-1'])
    add('dash name', {}, ['-'])
    add('abbreviation', caches, ['--rep', '--no-w'])
    add('ambiguous', {}, ['--pr'])
    add('flag with value', {}, ['--yes=1'])
    add('double dash', caches, ['--', 'fbrb'])
    add('help with COLUMNS', {}, ['-h'], env(COLUMNS='60'))
    add('error with COLUMNS', {}, ['--list', '--clear'], env(COLUMNS='120'))
    # ---- a guarded tool's start-up pruning of the log folder (errors.setup: every Python tool, and bfbc2.exe now)
    tool_case = ('culltree', ['info'])
    add('startup prune', logs, tool_case[1], tool=tool_case[0])
    add('startup prune caps', logs, tool_case[1], env(BFBC2_LOGS_MB='0.02', BFBC2_LOG_TOOL_MB='0.012'),
        tool=tool_case[0])
    add('startup prune own day', {**logs, log_name('culltree', 0): (blob(5000), T0)}, tool_case[1],
        env(BFBC2_LOGS_MB='0.001'), tool=tool_case[0])
    add('startup prune aptgraft', logs, [], env(BFBC2_LOG_TOOL_MB='0.004'), tool='aptgraft')
    add('startup prune stockmp', logs, [], tool='stockmp')
    return with_variants(rows, ['list', 'prune all tight', 'report', 'list unicode folder', 'list cp1252 folder',
                                'help', 'prune logs'])


def opfor_calls(opfor: str) -> list[Call]:
    """The real caches, project caches, build folders and log folder, read-only (listed; logs pruned on a copy)."""
    lad = os.environ.get('LOCALAPPDATA') or os.path.expanduser('~')
    real = os.path.join(lad, 'bfbc2-tool-suite')
    names = ['hlsl', 'fbrb', 'collision', 'effectport', 'culltree', 'bf1943source', 'shadercover', 'renderqa']
    real_caches = {e: os.path.join(real, 'cache', n) for e, n in zip(CACHE_ENVS, names)}
    roots = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))),
                         'rust-targets')
    listing = dict(BASE, **real_caches, OPFOR_ROOT=opfor, BFBC2_RUST_TARGETS=roots)
    # --report sums the build folders' sizes, which other checkouts' builds change between the two sides' runs: its
    # lines list the real caches and work\ with no build root (the real root: --prune-targets, names and states only)
    report = dict(listing, BFBC2_RUST_TARGETS='{d}\\no-targets')
    logs = os.path.join(real, 'logs')
    copy = {f'{LOGS}/{n}': os.path.join(logs, n) for n in sorted(os.listdir(logs))
            if os.path.isfile(os.path.join(logs, n))} if os.path.isdir(logs) else {}
    quiet = dict(BASE)                      # the copy's prune: no real cache, no project folder
    return [
        ('list (real caches, read-only)', 'caches', [], {}, listing, True),
        ('list project caches --opfor', 'caches', ['--opfor', opfor, 'navstudio', 'harness', 'navwater', 'editor'], {},
         listing, True),
        ('report --no-work (real caches)', 'caches', ['--report', '--no-work'], {}, report, True),
        ('report (real work\\ by kind)', 'caches', ['--report'], {}, report, True),
        ('prune-targets (listed, no --yes)', 'caches', ['--prune-targets'], {}, listing, True),
        ('list logs (a copy of the real log folder)', 'caches', ['logs'], copy, quiet, True),
        ('prune logs (the copy)', 'caches', ['--prune', 'logs'], copy, quiet, True),
        ('prune logs tight (the copy)', 'caches', ['--prune', 'logs'], copy,
         dict(quiet, BFBC2_LOGS_MB='50', BFBC2_LOG_TOOL_MB='10'), True),
        ('clear logs (the copy)', 'caches', ['--clear', 'logs'], copy, quiet, True),
        ('culltree start-up prune (the copy)', 'culltree', ['info'], copy, dict(quiet, BFBC2_LOGS_MB='50'), False),
    ]
