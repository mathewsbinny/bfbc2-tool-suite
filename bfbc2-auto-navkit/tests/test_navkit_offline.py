r"""navkit.py's orchestration without the game: a stand-in Project OPFOR BFBC2 folder built in a temp folder (a game
folder with BFBC2Game.exe, level bundles, path data, opfor.ini and its log; tools\FbrbTool.exe / DbxEdit.exe as empty
files) and every step driven with its heavy parts stood in (FbrbTool / DbxEdit runs, the build, the game launch, the
running-game check). Every file the steps write must land under the temp folder - checked by a guard on open,
os.remove, os.makedirs, shutil.move and shutil.rmtree while the steps run.

Covered: find_opfor / find_tool / Project and their error messages, the level list and its filters, the path-data
lookups (original, kept original, no-original marker), settings / samples lookups, unpack's stamp, run's errors,
levels / level_names / status output, ini_set / recover_ini, sampling_box (objects, path data, nothing), prepare
(SectorOffsets, the offsets rule, a given box), ensure_settings, detailed_rects / detailed_box,
dice_detailed_sectors, build_inputs (detailed zones, missing samples), build, check, compare, install / restore
(backup once, the no-original marker, traversal annotations), apply (DICE base, NavKit base, every error, the report
on failure, --install), sample / sample_run / sample_level (the ini changed and restored, each result: done, the game
exited, timed out, the level ended), close_game / game_running, and main's argument parsing, help and errors.

  python test_navkit_offline.py
"""
import builtins
import contextlib
import hashlib
import io
import json
import os
import shutil
import sys
import tempfile
from types import SimpleNamespace
from typing import Any, cast

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402
suite_paths.use('core', 'navkit', 'navkit-tools')
import hpdtool  # noqa: E402
import kymesh  # noqa: E402
import ladders  # noqa: E402
import levelobjects  # noqa: E402
import navbuild  # noqa: E402
import navcompare  # noqa: E402
import navkit  # noqa: E402
import navlinks  # noqa: E402,F401  (imported by apply; loaded before the write guard)
import navregress  # noqa: E402
import navsample  # noqa: E402
import navtraversal  # noqa: E402,F401
from bfbc2core import navtune  # noqa: E402

hpdtool_write = hpdtool.write
EMPTY_HPD = hpdtool.write(hpdtool.HpdFile(tuple([0] * hpdtool.HEADER_WORDS), []))   # reads back exactly


class Patch:
    """Attribute replacements, undone in reverse order."""

    def __init__(self):
        self.saved = []

    def set(self, obj, name, value):
        self.saved.append((obj, name, getattr(obj, name)))
        setattr(obj, name, value)

    def undo(self):
        while self.saved:
            obj, name, value = self.saved.pop()
            setattr(obj, name, value)


@contextlib.contextmanager
def writes_only_under(root, outside):
    """Records (in outside) every write, removal or folder creation the code under test makes outside root."""
    base = os.path.normcase(os.path.realpath(root)) + os.sep
    real: dict[str, Any] = {'open': builtins.open, 'remove': os.remove, 'makedirs': os.makedirs, 'move': shutil.move,
            'rmtree': shutil.rmtree, 'replace': os.replace}

    def note(path, what):
        if isinstance(path, int):
            return
        p = os.path.normcase(os.path.realpath(os.path.abspath(os.fspath(path))))
        if not (p + os.sep).startswith(base):
            outside.append(f'{what} {path}')

    def g_open(file, mode='r', *a, **k):
        if any(c in mode for c in 'wax+'):
            note(file, 'open ' + mode)
        return real['open'](file, mode, *a, **k)

    def g_remove(path, *a, **k):
        note(path, 'remove')
        return real['remove'](path, *a, **k)

    def g_makedirs(path, *a, **k):
        note(path, 'makedirs')
        return real['makedirs'](path, *a, **k)

    def g_move(src, dst, *a, **k):
        note(src, 'move from')
        note(dst, 'move to')
        return real['move'](src, dst, *a, **k)

    def g_rmtree(path, *a, **k):
        note(path, 'rmtree')
        return real['rmtree'](path, *a, **k)

    def g_replace(src, dst, *a, **k):
        note(dst, 'replace')
        return real['replace'](src, dst, *a, **k)
    builtins.open, os.remove, os.makedirs, shutil.move, shutil.rmtree, os.replace = \
        cast(Any, (g_open, g_remove, g_makedirs, g_move, g_rmtree, g_replace))
    try:
        yield
    finally:
        builtins.open, os.remove, os.makedirs, shutil.move, shutil.rmtree, os.replace = \
            real['open'], real['remove'], real['makedirs'], real['move'], real['rmtree'], real['replace']


def appended(items: list[Any], item: Any, result: Any) -> Any:
    """items.append(item), then result (for stand-ins that record their call)."""
    items.append(item)
    return result


def write(path, data=b''):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(data if isinstance(data, bytes) else data.encode('utf-8'))
    return path


def read(path, mode='r'):
    with open(path, mode) as f:
        return f.read()


def make_opfor(root):
    """A stand-in OPFOR folder: game\\BFBC2Game.exe, opfor.ini, its log, the tools and four level folders."""
    game = os.path.join(root, 'game')
    write(os.path.join(game, 'BFBC2Game.exe'), b'MZ')
    write(os.path.join(game, 'opfor.ini'), '[dev]\r\nlevel=Levels/MP_005\r\nnavsample=0\r\n\r\n[debug]\r\nprobes=0\r\n')
    write(os.path.join(game, 'logs', 'opfor.log'), 'old log line\n')
    for t in ('FbrbTool.exe', 'DbxEdit.exe'):
        write(os.path.join(root, 'tools', t), b'MZ')
    levels = os.path.join(game, 'Dist', 'win32', 'levels')
    for d in ('SP_01', 'mp_005', 'sp_common', 'opfor_fill', 'MP_009'):
        if d != 'MP_009':
            write(os.path.join(levels, d, 'level-00.fbrb'), f'bundle {d}')
        else:
            os.makedirs(os.path.join(levels, d))       # no level-00.fbrb: not a level
    return root


def main(argv):
    failed = 0

    def check(ok, what, detail=''):
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''))

    def raises(fn, *a, **k):
        """The NavKitError message fn raises, or None."""
        try:
            fn(*a, **k)
        except navkit.NavKitError as e:
            return str(e)
        return None

    import logging
    logging.getLogger('bfbc2').addHandler(logging.NullHandler())       # main's error log: not on the console
    logging.getLogger('bfbc2').propagate = False
    tmp = tempfile.mkdtemp(prefix='navkit_offline_')
    P = Patch()
    outside: list[str] = []
    try:
        root = make_opfor(os.path.join(tmp, 'opfor'))
        fake_here = os.path.join(tmp, 'suite', 'bfbc2-auto-navkit')      # no toolkit tools next to it
        os.makedirs(fake_here)
        P.set(navkit, 'HERE', fake_here)
        running = {'on': False}
        P.set(navkit, 'game_running', lambda: running['on'])
        with writes_only_under(tmp, outside):
            # --- find_opfor / find_tool / Project ------------------------------------------------------
            check(navkit.find_opfor(root) == os.path.abspath(root), 'find_opfor: the given folder (it has game\\)')
            old_env = os.environ.get('OPFOR_ROOT')
            os.environ['OPFOR_ROOT'] = root
            try:
                env_found = navkit.find_opfor(os.path.join(tmp, 'nowhere'))
                os.environ['OPFOR_ROOT'] = os.path.join(tmp, 'also nowhere')
                msg = raises(navkit.find_opfor, os.path.join(tmp, 'nowhere'))
            finally:
                if old_env is None:
                    os.environ.pop('OPFOR_ROOT', None)
                else:
                    os.environ['OPFOR_ROOT'] = old_env
            check(env_found == os.path.abspath(root), 'find_opfor: OPFOR_ROOT when the given folder has no game\\')
            check(msg is not None and 'not found' in msg and 'nowhere' in msg and 'also nowhere' in msg
                  and 'project-opfor-bfbc2' in msg, 'find_opfor: nothing found - the error names every folder looked in', str(msg))
            check(navkit.find_tool(root, 'FbrbTool.exe', 'bfbc2-fbrb-tool') == os.path.join(root, 'tools', 'FbrbTool.exe'),
                  'find_tool: OPFOR\'s tools\\ folder when the toolkit has no build')
            write(os.path.join(tmp, 'suite', 'bfbc2-fbrb-tool', 'FbrbTool.exe'), b'MZ')
            check(navkit.find_tool(root, 'FbrbTool.exe', 'bfbc2-fbrb-tool') == os.path.join(tmp, 'suite', 'bfbc2-fbrb-tool', 'FbrbTool.exe'),
                  'find_tool: the toolkit\'s own build first')
            msg = raises(navkit.find_tool, root, 'Nothing.exe', 'bfbc2-nothing')
            check(msg is not None and 'Nothing.exe not found' in msg and 'build.bat' in msg and 'bfbc2-nothing' in msg,
                  'find_tool: a missing tool - run build.bat in its folder', str(msg))
            bare = os.path.join(tmp, 'bare')
            os.makedirs(os.path.join(bare, 'game'))
            msg = raises(navkit.Project, bare)
            check(msg is not None and 'no game copy' in msg and 'setup_game.bat' in msg, 'Project: a game folder without BFBC2Game.exe', str(msg))
            pr: Any = navkit.Project(root)
            check(pr.game == os.path.join(root, 'game') and pr.ini == os.path.join(root, 'game', 'opfor.ini')
                  and pr.dbxedit == os.path.join(root, 'tools', 'DbxEdit.exe'), 'Project: game, ini and tool paths')

            # --- levels and path-data lookups ----------------------------------------------------------
            check(sorted(pr.level_dirs()) == ['mp_005', 'sp_01'],
                  'level_dirs: folders with a level-00 bundle, not *_common or opfor_*', str(sorted(pr.level_dirs())))
            check(pr.level('Levels/SP_01') == 'sp_01' and pr.level('levels\\MP_005') == 'mp_005', 'level: a partition path names the level')
            msg = raises(pr.level, 'sp_99')
            check(msg is not None and "unknown level 'sp_99'" in msg, 'level: an unknown level', str(msg))
            pd = pr.pathdata('mp_005')
            check(pr.dice_pathdata('mp_005') is None, 'dice_pathdata: no file - None')
            write(pd, b'dice')
            check(pr.dice_pathdata('mp_005') == pd, 'dice_pathdata: the installed file is DICE\'s while nothing is kept')
            write(pd + navkit.NEW_SUFFIX)
            check(pr.dice_pathdata('mp_005') is None, 'dice_pathdata: a no-original marker - the file there is NavKit\'s')
            os.remove(pd + navkit.NEW_SUFFIX)
            write(pd + navkit.BACKUP_SUFFIX, b'dice')
            check(pr.dice_pathdata('mp_005') == pd + navkit.BACKUP_SUFFIX, 'dice_pathdata: the kept original once NavKit\'s is installed')
            os.remove(pd + navkit.BACKUP_SUFFIX)
            msg = raises(pr.settings, 'sp_01')
            check(msg is not None and 'not prepared yet' in msg and 'prepare SP_01' in msg, 'settings: not prepared - says which command', str(msg))
            check(pr.samples08('sp_01') is None, 'samples08: none')
            full = write(os.path.join(root, 'work', 'navsample', 'sp_01_full.bin'), b's')
            check(pr.samples08('sp_01') == full, 'samples08: an older campaign run (_full)')
            own = write(pr.samples('sp_01'), b's')
            check(pr.samples08('sp_01') == own, 'samples08: navkit.py sample\'s own file first')
            check(pr.samples('sp_01', '_hi_p02').endswith('sp_01_navkit_hi_p02.bin'), 'samples: the suffix goes before .bin')

            # --- run / unpack --------------------------------------------------------------------------
            out = pr.run([sys.executable, '-c', 'print("one"); print("two")'], print)
            check(out == ['one', 'two'], 'run: the output lines', str(out))
            msg = raises(pr.run, [sys.executable, '-c', 'import sys; print("a"); print("b"); print("c"); print("last"); sys.exit(3)'], print)
            check(msg is not None and 'failed' in msg and 'b | c | last' in msg and ' a ' not in msg,
                  'run: a failing tool - its name and last three lines', str(msg))
            runs = []

            def fake_run(args, log):
                runs.append(os.path.basename(args[0]) + ' ' + args[1])
                if args[1] == 'unpack':
                    write(os.path.join(args[3], 'fbrb.manifest'), 'x')
                return ['converted 3 file(s)']
            pr_run = pr.run
            pr.run = fake_run
            logged: list[str] = []
            bundle = os.path.join(pr.game, 'Dist', 'win32', 'levels', 'SP_01', 'level-00.fbrb')
            dest = os.path.join(root, 'work', 'levels', 'sp_01', 'dist')
            write(os.path.join(dest, 'stale.txt'), 'stale')
            pr.unpack(bundle, dest, logged.append)
            first = list(runs)
            pr.unpack(bundle, dest, logged.append)
            check(first == ['FbrbTool.exe unpack', 'DbxEdit.exe xml'] and runs == first and not os.path.isfile(os.path.join(dest, 'stale.txt'))
                  and read(os.path.join(dest, '.source.sha256')) == hashlib.sha256(read(bundle, 'rb')).hexdigest()
                  and any('already unpacked' in m for m in logged),
                  'unpack: FbrbTool then DbxEdit into a clean folder, stamped; the same bundle again is not unpacked', f'{runs} {logged}')
            write(bundle, 'bundle SP_01 patched')
            pr.unpack(bundle, dest, logged.append)
            check(len(runs) == 4, 'unpack: a changed bundle is unpacked again')
            pr.run = pr_run

            # --- small helpers -------------------------------------------------------------------------
            check(navkit.fmt([1.0, -2.5, 300]) == '1,-2.5,300' and navkit.pitch_suffix(0.8) == '' and navkit.pitch_suffix(0.2) == '_p02',
                  'fmt and pitch_suffix')
            t = '[dev]\r\nlevel=Levels/MP_005\r\nnavsample=0\r\n\r\n[debug]\r\nprobes=0\r\n'
            t1 = navkit.ini_set(t, 'dev', 'navsample', '1')
            t2 = navkit.ini_set(t1, 'DEV', 'autostart', '1')
            t3 = navkit.ini_set(t2, 'debug', 'probes', '1')
            t4 = navkit.ini_set(t3, 'new', 'key', 'v')
            check('navsample=1\r\n' in t1 and 'navsample=0' not in t1, 'ini_set: a key in its section replaced, CRLF kept', repr(t1))
            check(t2.split('\n')[1] == 'autostart=1\r' and t2.count('[dev]') == 1, 'ini_set: a missing key added at the top of its section', repr(t2))
            check('probes=1\r\n' in t3 and 'level=Levels/MP_005' in t3, 'ini_set: the last section too')
            check(t4.endswith('\r\n\r\n[new]\r\nkey=v\r\n'), 'ini_set: a missing section appended', repr(t4[-30:]))
            check(navkit.ini_set('[a]\nx=1\n', 'a', 'x', '2') == '[a]\nx=2\n', 'ini_set: LF files stay LF')
            ini_before = read(pr.ini, 'rb')
            navkit.recover_ini(pr, logged.append)
            write(navkit.ini_backup(pr), b'[dev]\nsaved=1\n')
            write(pr.ini, b'changed by a killed run')
            navkit.recover_ini(pr, logged.append)
            check(read(pr.ini, 'rb') == b'[dev]\nsaved=1\n' and not os.path.isfile(navkit.ini_backup(pr)) and 'interrupted' in logged[-1],
                  'recover_ini: a copy left by a killed sampling run is put back (and nothing happens without one)')
            write(pr.ini, ini_before)

            # --- sampling_box / level_partition_name -----------------------------------------------------
            L = pr.level_work('sp_01')
            write(os.path.join(L, 'objects.json'), json.dumps([{'m': [0] * 12 + [10.0, 5.0, 20.0, 1]}]))
            write(os.path.join(L, 'group_instances.json'), json.dumps([{'m': [0] * 12 + [110.0, 25.0, -40.0, 1]}]))
            write(os.path.join(L, 'soldier_spawns.json'), json.dumps([[50.0, 0.0, 0.0]]))
            box, yr, source = navkit.sampling_box(pr, 'sp_01', L)
            check(box == [-40, -90, 160, 70] and yr == [-30, 180] and 'objects + 50 m' in source,
                  'sampling_box: the objects\' and spawns\' extent + 50 m, on a 10 m grid; y 30 m below to 150 m above', f'{box} {yr}')
            empty = os.path.join(tmp, 'empty_level')
            for n in ('objects.json', 'group_instances.json', 'soldier_spawns.json'):
                write(os.path.join(empty, n), '[]')
            msg = raises(navkit.sampling_box, pr, 'sp_01', empty)
            check(msg is not None and '--box and --y' in msg, 'sampling_box: nothing placed - pass --box and --y', str(msg))
            write(pd, b'dice')
            outline = [[(0.0, 1.0, 0.0), (100.0, 3.0, 0.0)], [(-30.0, 2.0, 210.0)]]
            P.set(hpdtool, 'parse', lambda data: SimpleNamespace(sectors=[SimpleNamespace(mesh=b'm', id=0x0102),
                                                                          SimpleNamespace(mesh=b'd', id=0x0001)]))
            P.set(kymesh, 'parse_mesh', lambda m: SimpleNamespace(areas=[SimpleNamespace(outline=outline)],
                                                                  pitch=0.2 if m == b'd' else 0.8))
            box, yr, source = navkit.sampling_box(pr, 'mp_005', L)
            check(box == [-50, -20, 120, 230] and yr == [-30, 160] and 'path data + 20 m' in source,
                  'sampling_box: the original path data\'s extent + 20 m', f'{box} {yr}')
            check(navkit.dice_detailed_sectors(pr, 'mp_005') == [(0, 1)] and navkit.dice_detailed_sectors(pr, 'sp_01') == [],
                  'dice_detailed_sectors: DICE\'s pitch-0.2 sectors as (x, z) ids; none without path data')
            P.undo()
            P.set(navkit, 'HERE', fake_here)
            P.set(navkit, 'game_running', lambda: running['on'])
            os.remove(pd)
            msg = raises(navkit.level_partition_name, L, 'sp_01')
            check(msg is not None and 'levels/sp_01.xml not found' in msg, 'level_partition_name: no partition in the bundle', str(msg))
            write(os.path.join(L, 'dist', 'levels', 'SP_01.xml'), '<partition/>')
            check(navkit.level_partition_name(L, 'sp_01') == 'SP_01', 'level_partition_name: the partition\'s own spelling')

            # --- prepare ------------------------------------------------------------------------------
            unpacked, lo_calls = [], []
            pr.unpack = lambda bundle, dest, log: unpacked.append(os.path.relpath(dest, root))
            P.set(levelobjects, 'main', lambda args: lo_calls.append(args[1:3]))
            P.set(ladders, 'extract', lambda L_, common: [{'bottom': [0, 0, 0], 'top': [0, 5, 0]}])
            P.set(navregress, 'reference_bounds', lambda r, lv: None)
            write(os.path.join(L, 'dist', 'levels', 'sp_01', 'Precomputed', 'Infantry', 'SectorOffsets.xml'),
                  '<field name="XOffset">-3</field><field name="ZOffset">7</field>')
            plog: list[str] = []
            navkit.prepare(pr, 'sp_01', log=plog.append)
            s = pr.settings('sp_01')
            check(s == {'level': 'SP_01', 'box': [-40, -90, 160, 70], 'y': [-30, 180], 'box_source': 'extent of the level\'s objects + 50 m',
                        'offsets_source': 'SectorOffsets', 'xoff': -3, 'zoff': 7},
                  'prepare: settings from the objects\' extent and the SectorOffsets asset', str(s))
            check(unpacked[:2] == [os.path.join('work', 'navkit', 'common', 'sp_common'), os.path.join('work', 'navkit', 'common', 'mp_common')]
                  and unpacked[2] == os.path.join('work', 'levels', 'sp_01', 'dist') and len(unpacked) == 3
                  and os.path.isdir(os.path.join(L, 'pkg')) and lo_calls == [[L, 'SP_01']]
                  and json.load(open(os.path.join(L, 'ladders.json'))) == [{'bottom': [0, 0, 0], 'top': [0, 5, 0]}],
                  'prepare: the common bundles and the level unpacked (no Package copy: an empty pkg folder), objects and ladders extracted',
                  str(unpacked))
            # a Package copy, no SectorOffsets, a given box: offsets by the rule from the box
            write(os.path.join(pr.game, 'Package', 'levels', 'mp_005', 'level-00.fbrb'), 'pkg')
            L5 = pr.level_work('mp_005')
            write(os.path.join(L5, 'dist', 'levels', 'MP_005.xml'), '<partition/>')
            unpacked.clear()
            try:                                    # no objects yet (a new map): a given box and y need none
                navkit.prepare(pr, 'mp_005', log=plog.append, box=[-250.0, 120.0, 300.0, 400.0], y=[0.0, 90.0])
            except navkit.NavKitError as ex:
                check(False, f'prepare: a given box and y on a level without objects ({ex})')
            for n in ('objects.json', 'group_instances.json', 'soldier_spawns.json'):     # for the runs below
                write(os.path.join(L5, n), '[[1, 2, 3]]' if n == 'soldier_spawns.json' else '[]')
            s5 = pr.settings('mp_005')
            check(s5['box_source'] == 'given' and s5['box'] == [-250.0, 120.0, 300.0, 400.0] and s5['y'] == [0.0, 90.0]
                  and (s5['xoff'], s5['zoff']) == (-4, 0) and s5['offsets_source'] == 'rule: box'
                  and unpacked[-1] == os.path.join('work', 'levels', 'mp_005', 'pkg'),
                  'prepare: a given box; no SectorOffsets - one sector below the box (the rule); the Package bundle unpacked too', str(s5))
            P.set(navregress, 'reference_bounds', lambda r, lv: (-120.0, 30.0, 10.0, 50.0))
            navkit.prepare(pr, 'mp_005', log=plog.append)
            s5 = pr.settings('mp_005')
            check((s5['xoff'], s5['zoff']) == (-3, -1) and s5['offsets_source'] == 'rule: path data extent',
                  'prepare: no SectorOffsets but path data - one sector below its extent', str(s5))
            del pr.unpack

            # --- ensure_settings / detailed boxes ----------------------------------------------------------
            check(navkit.ensure_settings(pr, 'sp_01') == s, 'ensure_settings: navkit.json when there is one')
            os.remove(pr.settings_path('sp_01'))
            P.set(navsample, 'load', lambda p: (SimpleNamespace(min_x=-10.0, min_z=20.0, nx=11, nz=6, pitch=0.8,
                                                                y_bottom=-5.0, y_top=95.04), 0))
            st = navkit.ensure_settings(pr, 'sp_01')
            check(st == {'level': 'SP_01', 'box': [-10.0, 20.0, -2.0, 24.0], 'y': [-5.0, 95.04], 'box_source': 'its 0.8 m samples',
                         'offsets_source': 'its SectorOffsets', 'xoff': -3, 'zoff': 7} and os.path.isfile(pr.settings_path('sp_01')),
                  'ensure_settings: a level sampled before navkit.json - made from its samples and SectorOffsets', str(st))
            msg = raises(navkit.ensure_settings, pr, 'mp_005x')
            check(msg is not None and 'not prepared yet' in msg, 'ensure_settings: no samples - not prepared', str(msg))
            P.set(navkit, 'detailed_sectors', lambda pr_, lv: [(4, 1), (5, 2)])
            rects = navkit.detailed_rects(pr, 'sp_01')
            dbox = navkit.detailed_box(pr, 'sp_01')
            check(rects == [[99.0, 799.0, 201.0, 901.0], [199.0, 899.0, 301.0, 1001.0]] and dbox == [99.0, 799.0, 301.0, 1001.0],
                  'detailed_rects / detailed_box: the sectors\' 100 m squares (offsets applied) plus a 1 m margin', f'{rects} {dbox}')
            P.set(navkit, 'detailed_sectors', lambda pr_, lv: [])
            msg = raises(navkit.detailed_box, pr, 'sp_01')
            check(msg is not None and 'no detailed sectors' in msg, 'detailed_box: no detailed sectors - nothing to sample at 0.2', str(msg))

            # --- build_inputs / build -----------------------------------------------------------------
            for name, val in (('level_water', lambda r, lv: None), ('reference_bounds', lambda r, lv: None),
                              ('order_points_of', lambda r, lv: [[1, 2, 3]]), ('area_orders_of', lambda r, lv: None)):
                P.set(navregress, name, val)
            os.remove(full)
            msg = raises(navkit.build_inputs, pr, 'sp_01')
            check(msg is not None and 'samples missing' in msg and 'run sample first' in msg, 'build_inputs: no high-capsule run - samples missing', str(msg))
            write(pr.samples('sp_01', '_hi'), b's')
            P.set(navkit, 'detailed_sectors', lambda pr_, lv: [(4, 1)])
            blog: list[str] = []
            samples, forbid, xoff, zoff, kw = navkit.build_inputs(pr, 'sp_01', blog.append)
            check(samples == own and forbid == os.path.join(L, 'forbid_tris.npy') and (xoff, zoff) == (-3, 7) and kw['detailed'] is None
                  and kw['high_path'] == pr.samples('sp_01', '_hi') and kw['seeds'] == [[50.0, 0.0, 0.0]]
                  and kw['ladders'] == [{'bottom': [0, 0, 0], 'top': [0, 5, 0]}] and kw['order_points'] == [[1, 2, 3]],
                  'build_inputs: samples, forbidden triangles, offsets, spawns as seeds, ladders; no 0.2 m samples - no detailed zones')
            write(pr.samples('sp_01', '_p02'), b's')
            kw = navkit.build_inputs(pr, 'sp_01', blog.append)[4]
            check(kw['detailed'] == {'samples': pr.samples('sp_01', '_p02'), 'high': None, 'sectors': {(4, 1)}}
                  and 'no high-capsule run at 0.2' in blog[-1], 'build_inputs: 0.2 m samples without their high-capsule run - said', str(blog[-1:]))
            write(pr.samples('sp_01', '_hi_p02'), b's')
            kw = navkit.build_inputs(pr, 'sp_01', blog.append)[4]
            kw['log']('a', 1)
            check(kw['detailed']['high'] == pr.samples('sp_01', '_hi_p02') and blog[-1] == '  a 1'
                  and blog[-2] == '  detailed zones: 1 sectors at pitch 0.2',
                  'build_inputs: both 0.2 m runs - the detailed zones; the build\'s log indented', str(blog[-2:]))
            built = []

            def fake_build(samples, forbid, xoff, zoff, out, edits=None, cfg=None, **kw_):
                built.append((out, edits, cfg))
                write(out, EMPTY_HPD)
            P.set(navbuild, 'build', fake_build)
            stale_trav = write(navkit.traversal_path(pr.out_dir('sp_01'), 'sp_01'), '{}')
            navkit.build(pr, 'sp_01', log=blog.append)
            ours = os.path.join(pr.out_dir('sp_01'), 'pathdata.hpd')
            check(built[-1] == (ours, None, navbuild.DEFAULTS) and not os.path.isfile(stale_trav) and 'built in' in blog[-1],
                  'build: into work\\navkit\\<lv>\\pathdata.hpd with the default settings; the old traversal annotations removed')

            # --- check / compare ------------------------------------------------------------------------
            msg = raises(navkit.check, pr, 'mp_005')
            check(msg is not None and 'not built yet' in msg, 'check: not built yet', str(msg))
            seen_args = []

            def fake_tool_run(args, log):
                seen_args.append(args)
                return ['positions: 10 of 10', '  detail line', 'route check: ok', 'other']
            pr.run = fake_tool_run
            clog: list[str] = []
            navkit.check(pr, 'sp_01', log=clog.append)
            report = os.path.join(pr.out_dir('sp_01'), 'check.txt')
            check(seen_args[-1][2:] == [os.path.join(L, 'dist', 'levels', 'SP_01.xml'), ours] and 'self-check' in clog[0]
                  and clog[1:3] == ['  positions: 10 of 10', '  route check: ok'] and read(report).startswith('positions: 10 of 10\n'),
                  'check: ordercheck as a self-check without original path data; the summary lines logged, the report kept', str(clog))
            write(pd, b'dice')
            compared: list[tuple] = []
            P.set(navcompare, 'compare', lambda xml, a, b: appended(compared, (a, b), 'result'))
            P.set(navcompare, 'report', lambda res, log: log(f'report of {res}'))
            write(os.path.join(pr.out_dir('mp_005'), 'pathdata.hpd'), EMPTY_HPD)
            clog.clear()
            navkit.check(pr, 'mp_005', log=clog.append)
            check(seen_args[-1][3] == pd and 'against the original' in clog[0] and '    report of result' in clog
                  and compared[-1] == (pd, os.path.join(pr.out_dir('mp_005'), 'pathdata.hpd')),
                  'check: against the original path data, with navcompare\'s summary', str(clog))
            del pr.run
            clog.clear()
            navkit.compare(pr, 'mp_005', log=clog.append)
            check(compared[-1] == (pd, os.path.join(pr.out_dir('mp_005'), 'pathdata.hpd')) and clog[-1] == '  report of result',
                  'compare: the original against NavKit\'s build by default')
            msg = raises(navkit.compare, pr, 'mp_005', candidate=os.path.join(tmp, 'none.hpd'))
            check(msg is not None and 'no candidate file' in msg, 'compare: a missing candidate', str(msg))
            msg = raises(navkit.compare, pr, 'sp_01')
            check(msg is not None and 'no reference file' in msg and '--reference' in msg, 'compare: a level without original path data', str(msg))

            # --- install / restore -------------------------------------------------------------------------
            ilog: list[str] = []
            msg = raises(navkit.install, pr, 'sp_01', src=os.path.join(tmp, 'none.hpd'))
            check(msg is not None and 'not built yet' in msg, 'install: nothing built', str(msg))
            running['on'] = True
            msg = raises(navkit.install, pr, 'sp_01')
            write(pd + navkit.BACKUP_SUFFIX, b'dice')
            msg2 = raises(navkit.restore, pr, 'mp_005')
            os.remove(pd + navkit.BACKUP_SUFFIX)
            running['on'] = False
            check(msg is not None and 'game is running' in msg and msg2 is not None and 'game is running' in msg2,
                  'install / restore: refused while the game runs', f'{msg} {msg2}')
            dest1 = pr.pathdata('sp_01')
            trav_src = navkit.traversal_path(pr.out_dir('sp_01'), 'sp_01')
            write(trav_src, json.dumps({'pathdata_sha1': hashlib.sha1(EMPTY_HPD).hexdigest()}))
            navkit.install(pr, 'sp_01', log=ilog.append)
            trav_dest = navkit.traversal_path(os.path.dirname(dest1), 'sp_01')
            check(read(dest1, 'rb') == EMPTY_HPD and os.path.isfile(dest1 + navkit.NEW_SUFFIX) and not os.path.isfile(dest1 + navkit.BACKUP_SUFFIX)
                  and os.path.isfile(trav_dest) and any('had no original' in m for m in ilog),
                  'install: a level without path data - the no-original marker; traversal annotations made for this file installed')
            check(pr.dice_pathdata('sp_01') is None, 'install: NavKit\'s file on a level without one is not taken for DICE\'s')
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                navkit.levels(pr)
            rows = {l.split()[0]: l for l in buf.getvalue().splitlines()[1:]}
            check('INSTALLED' in rows['sp_01'] and 'prepared' in rows['sp_01'] and 'built' in rows['sp_01'] and ' no ' in rows['sp_01']
                  and 'yes (samplable)' in rows['mp_005'] and 'INSTALLED' not in rows['mp_005'],
                  'levels: path data, prepared / built / INSTALLED per level', buf.getvalue())
            check(sorted(navkit.level_names(pr)) == [('mp_005', True), ('sp_01', False)], 'level_names: (level, samplable) for the GUI',
                  str(navkit.level_names(pr)))
            navkit.restore(pr, 'sp_01', log=ilog.append)
            check(not os.path.isfile(dest1) and not os.path.isfile(dest1 + navkit.NEW_SUFFIX) and not os.path.isfile(trav_dest)
                  and 'had no path data before' in ilog[-1], 'restore: NavKit\'s file, its marker and annotations removed (no original)')
            navkit.restore(pr, 'sp_01', log=ilog.append)
            check('nothing to restore' in ilog[-1], 'restore: nothing installed - nothing to do')
            # a level with DICE's file: the original kept once
            write(navkit.traversal_path(pr.out_dir('mp_005'), 'mp_005'), json.dumps({'pathdata_sha1': 'another file'}))
            trav5 = write(navkit.traversal_path(os.path.dirname(pd), 'mp_005'), '{"stale": 1}')
            ilog.clear()
            navkit.install(pr, 'mp_005', log=ilog.append)
            navkit.install(pr, 'mp_005', log=ilog.append)
            check(read(pd + navkit.BACKUP_SUFFIX, 'rb') == b'dice' and read(pd, 'rb') == EMPTY_HPD and not os.path.isfile(trav5)
                  and sum('original kept' in m for m in ilog) == 1 and any('made for another path-data file' in m for m in ilog),
                  'install: DICE\'s file kept once (a second install keeps it); annotations for another file not installed, stale ones removed',
                  str(ilog))
            write(trav5, '{}')
            write(pd + navkit.NEW_SUFFIX)                   # a stale marker next to a kept original
            navkit.restore(pr, 'mp_005', log=ilog.append)
            check(read(pd, 'rb') == b'dice' and not os.path.isfile(pd + navkit.BACKUP_SUFFIX) and not os.path.isfile(pd + navkit.NEW_SUFFIX)
                  and not os.path.isfile(trav5) and 'restored the original' in ilog[-1],
                  'restore: the original back, a stale marker and the annotations removed')

            # --- apply -------------------------------------------------------------------------------
            alog: list[str] = []
            msg = raises(navkit.apply, pr, 'mp_005', log=alog.append)
            check(msg is not None and 'no side file' in msg and 'Navigation Studio' in msg, 'apply: no side file', str(msg))
            side = navtune.default_path(root, 'mp_005')
            write(side, '{"format": "something else"}')
            msg = raises(navkit.apply, pr, 'mp_005', log=alog.append)
            check(msg is not None and 'not a navtune side file' in msg, 'apply: a file that is not a side file - its reader\'s words', str(msg))
            tune = navtune.NavTune.new('Levels/MP_005', pd, kind='dice')
            tune.add(navtune.op('note', at=[1, 2, 3], text='look here'))
            tune.save(side)
            stale5 = write(navkit.traversal_path(pr.out_dir('mp_005'), 'mp_005'), '{}')
            write(pd, EMPTY_HPD)
            out5 = navkit.apply(pr, 'mp_005', log=alog.append)
            check(out5 == os.path.join(pr.out_dir('mp_005'), 'pathdata_dice_edited.hpd') and read(out5, 'rb') == EMPTY_HPD
                  and not os.path.isfile(stale5) and os.path.isfile(out5 + '.report.txt')
                  and '1 note(s)' in read(out5 + '.report.txt') and 'read back exactly' in read(out5 + '.report.txt')
                  and any('not the one the edits were made on' in m for m in alog),
                  'apply on a DICE base: a copy of DICE\'s file, notes kept in the side file, old annotations removed, the report written',
                  str(alog[-4:]))
            tune2 = navtune.NavTune.new('Levels/MP_005', pd, kind='dice')
            tune2.operations.append(navtune.op('seed', at=[1, 2, 3]))    # not through add, which refuses it too
            tune2.save(side)
            msg = raises(navkit.apply, pr, 'mp_005', log=alog.append)
            check(msg is not None and 'not allowed on a DICE base' in msg, 'apply: a DICE-base side file with a region edit is refused', str(msg))
            tune3 = navtune.NavTune.new('Levels/SP_01', pd, kind='dice')
            tune3.add(navtune.op('note', at=[1, 2, 3], text='x'))
            tune3.save(navtune.default_path(root, 'sp_01'))
            msg = raises(navkit.apply, pr, 'sp_01', log=alog.append)
            check(msg is not None and 'the level has none' in msg, 'apply: a DICE base on a level without DICE\'s data', str(msg))
            # a NavKit base: a build with the walkability edits, then traversal annotations
            tune4 = navtune.NavTune.new('Levels/SP_01', ours, kind='navkit')
            tune4.add(navtune.op('seed', at=[1, 2, 3]))
            tune4.add(navtune.op('traversal', kind='ladder', bottom=[0, 0, 0], top=[0, 5, 0]))
            tune4.save(navtune.default_path(root, 'sp_01'))
            written = []
            P.set(navtraversal, 'write', lambda f, out, ops, level, path, log=None: written.append((out, len(ops), level, path)))
            alog.clear()
            out1 = navkit.apply(pr, 'sp_01', log=alog.append, install_after=True)
            check(out1 == ours and built[-1][1] == [tune4.operations[0]] and written == [(ours, 1, 'Levels/SP_01', navkit.traversal_path(os.path.dirname(ours), 'sp_01'))]
                  and read(pr.pathdata('sp_01'), 'rb') == EMPTY_HPD and any('1 walkability edit(s) applied' in m for m in alog),
                  'apply on a NavKit base: built with the walkability edits, traversals written, installed (--install)', str(alog[-3:]))
            navkit.restore(pr, 'sp_01', log=alog.append)
            # a failing build: the report has what was done and the error
            P.set(navbuild, 'build', lambda *a, **k: (_ for _ in ()).throw(RuntimeError('the build broke')))
            other = os.path.join(tmp, 'elsewhere', 'out.hpd')
            os.makedirs(os.path.dirname(other))
            try:
                navkit.apply(pr, 'sp_01', out=other, log=alog.append)
                err = None
            except RuntimeError as e:
                err = str(e)
            rep = read(other + '.report.txt') if os.path.isfile(other + '.report.txt') else ''
            check(err == 'the build broke' and '== apply sp_01' in rep and rep.rstrip().endswith('error: the build broke'),
                  'apply: a failure leaves the report of what was done and the error (and is raised)', rep)
            # a result that does not read back
            P.set(navbuild, 'build', fake_build)
            P.set(hpdtool, 'write', lambda f: b'not the same bytes')
            msg = raises(navkit.apply, pr, 'sp_01', log=alog.append)
            check(msg is not None and 'does not read back exactly' in msg, 'apply: a file that does not read back exactly is not usable', str(msg))
            P.set(hpdtool, 'write', hpdtool_write)

            # --- status ------------------------------------------------------------------------------
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                navkit.status(pr, 'sp_01')
            text = buf.getvalue()
            check('prepared           : yes' in text and 'offsets -3,7' in text and 'samples_hi' in text and 'built              : no' not in text
                  and 'installed in game  : no' in text and 'original path data : none' in text, 'status: what has been done for a level', text)

            # --- sample / sample_run / sample_level ----------------------------------------------------
            clock = {'t': 0.0}
            P.set(navkit, 'time', SimpleNamespace(time=lambda: clock['t'], sleep=lambda s: clock.__setitem__('t', clock['t'] + s),
                                                  ctime=lambda *a: 'then'))
            launches = []
            script: dict[str, Any] = {'lines': ['navsample: sampling 10 %', 'navsample: sampling 90 %', 'navsample: done 12345 columns'],
                      'write_out': True, 'running': False}

            def fake_popen(args, cwd=None):
                ini = read(pr.ini)
                launches.append((args, cwd, ini))
                name = next(l.split('=', 1)[1].strip() for l in ini.splitlines() if l.startswith('navsample_name='))
                if script['write_out']:
                    write(os.path.join(root, 'work', 'navsample', name + '.bin'), b'samples')
                with open(pr.log_path, 'a') as f:
                    f.write('\n'.join(script['lines']) + '\n')
                running['on'] = script['running']
            P.set(navkit.subprocess, 'Popen', fake_popen)
            P.set(navkit, 'close_game', lambda: running.__setitem__('on', False))
            ini0 = read(pr.ini, 'rb')
            slog: list[str] = []
            running['on'] = True
            msg = raises(navkit.sample, pr, 'sp_01', log=slog.append)
            running['on'] = False
            check(msg is not None and 'game is running' in msg, 'sample: refused while the game runs', str(msg))
            check(navkit.sample(pr, 'sp_01', log=slog.append, confirm=lambda q: False) is False and slog[-1] == '  cancelled' and not launches,
                  'sample: a declined confirmation starts nothing')
            ok = navkit.sample(pr, 'sp_01', log=slog.append, confirm=lambda q: 'twice' in q)
            ini_run = launches[0][2]
            check(ok is True and len(launches) == 2 and launches[0][0] == [os.path.join(root, 'OPFOR BFBC2.exe')] and launches[0][1] == root,
                  'sample: two game launches (normal and high capsule) of OPFOR BFBC2.exe')
            check('level=Levels/SP_01' in ini_run and 'navsample=1' in ini_run and 'navsample_box=-10,20,-2,24' in ini_run
                  and 'navsample_agent=0.4,0.28,1.9' in ini_run and 'navsample_pitch=0.8' in ini_run and 'probes=1' in ini_run
                  and 'navsample_budget_ms' not in ini_run and 'navsample_agent=0.4,0.6,1.9' in launches[1][2]
                  and 'navsample_name=sp_01_navkit_hi' in launches[1][2],
                  'sample: the run\'s settings in opfor.ini (level, box, pitch, agent, probes; no budget at 0.8 m)', ini_run)
            check(read(pr.ini, 'rb') == ini0 and not os.path.isfile(navkit.ini_backup(pr)) and '    done 12345 columns' in slog
                  and any('no AI path data' in m for m in slog),
                  'sample: opfor.ini put back byte for byte after the runs; the newest progress line logged; a level without path data warned',
                  str(slog))
            # pitch 0.2: the detailed box and rects, the larger budget, its own files; the level's settings untouched
            P.set(navkit, 'detailed_sectors', lambda pr_, lv: [(4, 1)])
            launches.clear()
            settings_before = read(pr.settings_path('sp_01'))
            navkit.sample(pr, 'sp_01', log=slog.append, pitch=0.2)
            ini_run = launches[0][2]
            check('navsample_box=99,799,201,901' in ini_run and 'navsample_rects=99,799,201,901' in ini_run
                  and f'navsample_budget_ms={navkit.DETAILED_BUDGET_MS}' in ini_run and 'navsample_name=sp_01_navkit_p02' in ini_run
                  and read(pr.settings_path('sp_01')) == settings_before,
                  'sample --pitch 0.2: the detailed sectors\' box and squares, the detailed budget, _p02 files; settings unchanged', ini_run)
            launches.clear()
            navkit.sample(pr, 'sp_01', log=slog.append, pitch=0.2, box=[0.0, 0.0, 100.0, 100.0], budget_ms=900)
            check('navsample_box=0,0,100,100' in launches[0][2] and 'navsample_rects=\n' in launches[0][2].replace('\r', '')
                  and 'navsample_budget_ms=900' in launches[0][2], 'sample --pitch 0.2 --box: that box, no squares; budget_ms overrides')
            # a new box on a level whose offsets follow its box
            navkit.sample(pr, 'mp_005', log=slog.append, box=[-310.0, 50.0, 100.0, 300.0])
            s5 = pr.settings('mp_005')
            s5['offsets_source'] = 'rule: box'
            json.dump(s5, open(pr.settings_path('mp_005'), 'w'))
            navkit.sample(pr, 'mp_005', log=slog.append, box=[-310.0, 50.0, 100.0, 300.0], y=[1.0, 2.0])
            s5 = pr.settings('mp_005')
            check(s5['box'] == [-310.0, 50.0, 100.0, 300.0] and s5['y'] == [1.0, 2.0] and (s5['xoff'], s5['zoff']) == (-5, -1)
                  and any('sector offsets follow the box' in m for m in slog),
                  'sample --box: saved in the settings; offsets taken from a box follow the new box', str(s5))
            # the results of a run
            sp = pr.settings('sp_01')
            script.update(lines=['navsample: armed'], running=True)

            def result(lines, running_after):
                script.update(lines=lines, running=running_after)
                clock['t'] = 0.0
                return navkit.sample_run(pr, sp, '0.4,0.28,1.9', 'probe', slog.append, 60)
            check(result(['navsample: no physics'], True) == 'no physics', 'sample_run: the sampler\'s own end (no physics)')
            check(result(['navsample: armed'], False) == 'the game exited', 'sample_run: the game gone without a result - the game exited')
            check(result(['navsample: armed'], True) == 'timed out', 'sample_run: no result in time - timed out')
            r = result(['navsample: sampling 40 %', 'UI cmd [main menu]'], True)
            check(r.startswith('the level ended') and '--budget' in r, 'sample_run: back at the main menu after sampling began - the level ended', r)
            check(result(['UI cmd [main menu]', 'navsample: sampling 1 %'], True) == 'timed out',
                  'sample_run: the main menu before sampling (loading) does not end the run')
            check(read(pr.ini, 'rb') == ini0 and not os.path.isfile(navkit.ini_backup(pr)), 'sample_run: opfor.ini restored after every result')
            script.update(lines=['navsample: done'], running=False, write_out=False)
            msg = raises(navkit.sample, pr, 'sp_01', log=slog.append)
            check(msg is not None and 'did not finish' in msg, 'sample: "done" in the log but no samples file - did not finish', str(msg))
            script.update(lines=['navsample: armed'], write_out=True)
            msg = raises(navkit.sample, pr, 'sp_01', log=slog.append)
            check(msg is not None and 'did not finish: the game exited' in msg, 'sample: a run without a result - says why', str(msg))
            # sample_level: the 0.8 m runs, then the 0.2 m runs when there are detailed sectors
            calls: list[tuple] = []
            P.set(navkit, 'sample', lambda pr_, lv, log=print, box=None, y=None, pitch=0.8, **k: appended(calls, (pitch, box), True))
            P.set(navkit, 'detailed_sectors', lambda pr_, lv: [])
            check(navkit.sample_level(pr, 'sp_01', log=slog.append, box=[1.0, 2.0, 3.0, 4.0]) is True and calls == [(0.8, [1.0, 2.0, 3.0, 4.0])]
                  and 'no 0.2 m runs' in slog[-1], 'sample_level: no detailed sectors - the 0.8 m runs only')
            calls.clear()
            P.set(navkit, 'detailed_sectors', lambda pr_, lv: [(1, 1), (2, 2)])
            check(navkit.sample_level(pr, 'sp_01', log=slog.append, confirm=lambda q: 'four times' in q) is True
                  and calls == [(0.8, None), (0.2, None)], 'sample_level: one confirmation, then 0.8 m and 0.2 m runs')
            calls.clear()
            check(navkit.sample_level(pr, 'sp_01', log=slog.append, confirm=lambda q: False) is False and not calls,
                  'sample_level: declined - nothing runs')
            P.set(navkit, 'sample', lambda *a, **k: False)
            check(navkit.sample_level(pr, 'sp_01', log=slog.append) is False, 'sample_level: the 0.8 m runs did not start - stops')

            # --- read_log / close_game / game_running ----------------------------------------------------
            write(pr.log_path, 'one\ntwo\nthree\n')
            check(navkit.read_log(pr, 4) == ['two', 'three'], 'read_log: the lines after the start offset')
            P.undo()
            P.set(navkit, 'HERE', fake_here)
            procs: list[str] = []
            states = iter([True, True, False])
            fake_sub = SimpleNamespace(run=lambda args, **k: appended(procs, args[0], SimpleNamespace(stdout='', returncode=0)))
            P.set(navkit, 'subprocess', fake_sub)
            P.set(navkit, 'game_running', lambda: next(states))
            P.set(navkit, 'time', SimpleNamespace(sleep=lambda s: None, time=lambda: 0.0))
            navkit.close_game()
            check(procs == ['taskkill'], 'close_game: taskkill, then waits until the game is gone (no forced kill)', str(procs))
            procs.clear()
            P.set(navkit, 'game_running', lambda: True)
            navkit.close_game()
            check(procs == ['taskkill', 'taskkill'], 'close_game: a game that does not close is killed (/F)', str(procs))
            P.undo()
            P.set(navkit, 'HERE', fake_here)
            listing = {'out': 'BFBC2Game.exe   1234 Console'}
            P.set(navkit, 'subprocess', SimpleNamespace(run=lambda args, **k: SimpleNamespace(stdout=listing['out']), CREATE_NO_WINDOW=0))
            yes = navkit.game_running()
            listing['out'] = 'INFO: No tasks are running which match the specified criteria.'
            check(yes is True and navkit.game_running() is False, 'game_running: from tasklist\'s output')
            P.undo()

            # --- main -----------------------------------------------------------------------------------
            P.set(navkit, 'HERE', fake_here)
            seen: list[tuple] = []
            for name in ('levels', 'prepare', 'sample', 'sample_level', 'build', 'compare', 'check', 'apply', 'install', 'restore', 'status'):
                P.set(navkit, name, (lambda n: lambda *a, **k: appended(seen, (n, a[1:], k), True))(name))

            def run_main(*args):
                b = io.StringIO()
                with contextlib.redirect_stdout(b):
                    code = navkit.main(['navkit.py', *args])
                return code, b.getvalue()
            for h in ([], ['-h'], ['--help'], ['help']):
                code, text = run_main(*h)
                check(code == 0 and 'NavKit driver' in text and 'navkit.py restore' in text, f'main {" ".join(h) or "(no arguments)"}: the help', text[:80])
            code, text = run_main('levels', '--opfor', root)
            check(code == 0 and seen[-1][0] == 'levels', 'main levels --opfor <folder>')
            code, text = run_main('build', '--root', root)
            check(code == 1 and 'which level?' in text and 'navkit.py build SP_01' in text, 'main: a step without a level - which level?', text)
            code, text = run_main('build', 'sp_99', '--opfor', root)
            check(code == 1 and "unknown level 'sp_99'" in text, 'main: an unknown level', text)
            code, text = run_main('frobnicate', 'sp_01', '--opfor', root)
            check(code == 1 and "unknown command 'frobnicate'" in text, 'main: an unknown command', text)
            code, text = run_main('status', 'sp_01', '--opfor', os.path.join(tmp, 'nowhere'))
            check(code == 1 and text.startswith('error: Project OPFOR BFBC2 not found'), 'main: no OPFOR folder - the error, exit code 1', text)
            seen.clear()
            run_main('prepare', 'SP_01', '--box', '-10,-20,30,40', '--y', '0,100', '--opfor', root)
            run_main('sample', 'sp_01', '--yes', '--opfor', root)
            run_main('sample', 'sp_01', '--pitch', '0.2', '--opfor', root)
            run_main('compare', 'mp_005', '--reference', 'a.hpd', '--candidate', 'b.hpd', '--opfor', root)
            run_main('apply', 'sp_01', '--navtune', 'n.json', '--out', 'o.hpd', '--install', '--opfor', root)
            run_main('install', 'sp_01', '--from', 'f.hpd', '--opfor', root)
            for cmd in ('check', 'restore', 'status', 'build'):
                run_main(cmd, 'sp_01', '--opfor', root)
            got = [(n, a, {k: v for k, v in kw.items() if k != 'confirm'}) for n, a, kw in seen]
            want = [('prepare', ('sp_01',), {'box': [-10.0, -20.0, 30.0, 40.0], 'y': [0.0, 100.0]}),
                    ('sample_level', ('sp_01',), {'box': None, 'y': None}),
                    ('sample', ('sp_01',), {'box': None, 'y': None, 'pitch': 0.2}),
                    ('compare', ('mp_005',), {'reference': 'a.hpd', 'candidate': 'b.hpd'}),
                    ('apply', ('sp_01', 'n.json', 'o.hpd'), {'install_after': True, 'cfg': None}),
                    ('install', ('sp_01',), {'src': 'f.hpd'}),
                    ('check', ('sp_01',), {}), ('restore', ('sp_01',), {}), ('status', ('sp_01',), {}), ('build', ('sp_01',), {'cfg': None})]
            check(got == want, 'main: each command gets its options (--box / --y parsed, --pitch, --reference, --navtune, --from ...)', str(got))
            check(seen[1][2]['confirm'] is None and seen[2][2]['confirm'] is navkit.ask, 'main: --yes - no confirmation; without it, ask')
            seen.clear()
            P.set(navkit, 'sample_level', lambda *a, **k: appended(seen, ('sample_level',), False))
            run_main('all', 'sp_01', '--yes', '--opfor', root)
            stopped = [s[0] for s in seen]
            P.set(navkit, 'sample_level', lambda *a, **k: appended(seen, ('sample_level',), True))
            seen.clear()
            run_main('all', 'sp_01', '--yes', '--opfor', root)
            check(stopped == ['prepare', 'sample_level'] and [s[0] for s in seen] == ['prepare', 'sample_level', 'build', 'check'],
                  'main all: prepare, sample, then build and check only when sampling ran', f'{stopped} {seen}')
            P.set(builtins, 'input', lambda q: ' Y ')
            said_yes = navkit.ask('Continue?')
            P.set(builtins, 'input', lambda q: '')
            said_no = navkit.ask('Continue?')
            check(said_yes is True and said_no is False, 'ask: y / yes, anything else is no')
            P.undo()
        check(not outside, 'every file written, removed or made by the steps is under the temp folder', '; '.join(outside[:5]))
    finally:
        P.undo()
        shutil.rmtree(tmp, ignore_errors=True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
