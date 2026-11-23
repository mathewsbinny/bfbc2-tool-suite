"""NavKit driver: one command per step, from a level name to an installed pathdata.hpd.

  python navkit.py levels                     list the game's levels (and which can be sampled)
  python navkit.py prepare SP_01              unpack + convert the level, extract its objects,
                                              choose the sampling box and sector offsets
  python navkit.py sample  SP_01 [--yes]      in-game collision sampling (launches the game): two runs at 0.8 m,
                                              then two at 0.2 m over the level's detailed zones (if it has any)
  python navkit.py build   SP_01              generate pathdata.hpd
  python navkit.py check   SP_01              score it (against DICE's file if the level has one)
  python navkit.py apply   SP_01 [--navtune file] [--out file] [--install]
                                              build with the edits of the level's navigation side file
                                              (work\\navkit\\<level>\\<level>.navtune.json, made in the
                                              map editor's AI workspace) into work\\navkit\\<level>\\pathdata.hpd,
                                              traversals into <level>.traversal.json, a report next to it;
                                              --install: then install it
  python navkit.py install SP_01 [--from file]  copy it (or another built file) into the game, with its
                                              traversal annotations (DICE's file is backed up once)
  python navkit.py restore SP_01              put the original file back
  python navkit.py all     SP_01 [--yes]      prepare, sample, build, check
  python navkit.py status  SP_01              what has been done for a level
  python navkit.py compare SP_01 [--reference a.hpd] [--candidate b.hpd]
                                              two path-data files on the level's AI positions
                                              (default: the original against NavKit's build)

Options: --box minX,minZ,maxX,maxZ and --y bottom,top (prepare / sample: override the sampling box),
         --pitch 0.8 / 0.2 (sample: that pass only; 0.2 = a detailed-zone run over the level's detailed sectors;
         writes <level>_navkit_p02.bin / _hi_p02.bin, the level's settings and its 0.8 m samples untouched),
         --opfor <Project OPFOR BFBC2 folder> (default: OPFOR_ROOT, else "project-opfor-bfbc2" next to the suite),
         --no-detail (build / apply / all: no D lines or F cells - the build setting detail_features off).

NavKit works on Project OPFOR BFBC2's game copy and uses its in-game sampler. FbrbTool's and DbxEdit's commands run
in process (bfbc2core.fbrbtool / dbxedit: the suite's Rust core, which NavKit needs).

Files (in the OPFOR folder): work\\levels\\<level>\\ (unpacked level, objects), work\\navkit\\<level>\\
(settings, pathdata.hpd, check report), work\\navsample\\<level>_navkit*.bin (samples, written by OPFOR's
[dev] navsample). See QUICKSTART.md.
"""
import glob
import hashlib
import json
import math
import os
import re
import shutil
import subprocess
import sys
import time
from collections.abc import Callable, Iterable
from typing import Any

type Log = Callable[[str], object]

NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)   # no console flashes when run from the GUI
HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
from bfbc2core import atomicio, dbxedit, fbrbtool, gameledger, paths  # noqa: E402

DETAILED_BUDGET_MS = 400   # pitch-0.2 runs: ms of sampling per frame - 10x the columns of a normal run, so it must finish
                           # in less game time than a round or a campaign level allows an idle player (150: ONS_MP_002 at
                           # 95 %, SP_006 and SP_007 back at the menu after ~11 min; 400: all 15 levels, 2026-10-22)
AGENTS = [('normal', '0.4,0.28,1.9', ''), ('high capsule', '0.4,0.6,1.9', '_hi')]   # radius,stepMax,height
SAMPLE_DONE = re.compile(r'navsample: (done|no physics|cannot create|empty box|needs \[debug\])')
BACKUP_SUFFIX = '.opfor-orig'
NEW_SUFFIX = '.opfor-new'       # where NavKit installed onto a level that had no path data to back up
# other OPFOR stages that install game files keep <file>.opfor-<stage>-orig and mark their writes
# <file>.opfor-<stage>-applied (stock-MP AI: .opfor-stockmp-orig / .opfor-stockmp-applied)


def original_file(p: str) -> str | None:
    """DICE's original of game file p, or None. From the game copy's install ledger when an owner holds the file
    there (round DF2: the bottom layer's prior, kept once by its sha256; None where an install put the file where
    there was none). Else by the markers of before the ledger: the earliest backup any OPFOR stage kept
    (<file>.opfor-orig or <file>.opfor-<stage>-orig, the oldest first - a later stage backs up an earlier one's
    write), else p itself unless an OPFOR stage wrote it where there was no original (NavKit's .opfor-new marker, or
    a .opfor-<stage>-applied marker with no backup of that stage): then the file there is OPFOR's, not DICE's."""
    game = gameledger.find_game(p)
    if game is not None:
        led = gameledger.Ledger(game)
        rel = os.path.relpath(p, game)
        if led.holders(rel):
            return led.original(rel)
    d, base = os.path.split(p)
    try:
        names = {f.lower(): f for f in os.listdir(d or '.')}
    except OSError:
        return None
    pre = base.lower() + '.opfor-'
    ours = [f for f in names if f.startswith(pre)]
    if pre + NEW_SUFFIX[len('.opfor-'):] in ours:
        # NavKit installed onto nothing: only its own backup (a stale marker next to it) can be DICE's
        own = pre + BACKUP_SUFFIX[len('.opfor-'):]
        return os.path.join(d, names[own]) if own in ours else None
    if any(f.endswith('-applied') and f[:-len('-applied')] + '-orig' not in ours for f in ours):
        return None                    # another stage installed it onto nothing
    backups = [os.path.join(d, names[f]) for f in ours if f.endswith('-orig')]
    backups = [b for b in backups if os.path.isfile(b)]
    if backups:
        return min(backups, key=lambda b: (os.path.getmtime(b), not b.lower().endswith(BACKUP_SUFFIX)))
    return p if os.path.isfile(p) else None


def opfor_write_note(p: str) -> str:
    """Why original_file(p) is None while p exists: the stage markers next to it (for error messages)."""
    d, base = os.path.split(p)
    try:
        marks = sorted(f for f in os.listdir(d or '.') if f.lower().startswith(base.lower() + '.opfor-'))
    except OSError:
        marks = []
    if os.path.isfile(p) and marks:
        return f" (the installed {base} is an OPFOR write, not DICE's: {', '.join(marks)})"
    return ''


def _sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for b in iter(lambda: f.read(1 << 20), b''):
            h.update(b)
    return h.hexdigest()


class NavKitError(Exception):
    pass


def find_opfor(given: str | None = None) -> str:
    """The Project OPFOR BFBC2 folder: --opfor, else OPFOR_ROOT, else a "project-opfor-bfbc2" folder next to one of the folders
    this tool sits in (e.g. next to the suite). bfbc2core.paths.find_opfor, walking up from the suite."""
    try:
        return paths.find_opfor(given, start=os.path.dirname(HERE))
    except paths.GameCopyNotFound as e:
        raise NavKitError(str(e)) from None


def find_tool(opfor: str, exe: str, toolkit_dir: str) -> str:
    """A toolkit tool: built in the toolkit (..\\<toolkit_dir>\\<exe>), else OPFOR's tools\\ folder."""
    try:
        return paths.find_tool(opfor, exe, toolkit_dir, suite=os.path.dirname(HERE))
    except paths.ToolNotFound as e:
        raise NavKitError(str(e)) from None


class Project:
    def __init__(self, root: str | None = None) -> None:
        self.root = find_opfor(root)
        self.game = paths.game_dir(self.root)
        self.work = os.path.join(self.root, 'work')
        self.ini = os.path.join(self.game, 'opfor.ini')
        self.log_path = os.path.join(self.game, 'logs', 'opfor.log')
        if not os.path.isfile(paths.game_exe(self.root)):
            raise NavKitError(f'no game copy in {self.game} (run OPFOR\'s setup_game.bat first)')
        # FbrbTool's and DbxEdit's commands run in process (bfbc2core.fbrbtool / dbxedit.run_cmd, the Rust core): the
        # command lines name the program, no exe (retired 2026-11-18)
        self.fbrbtool = 'FbrbTool'
        self.dbxedit = 'DbxEdit'

    # --- levels --------------------------------------------------------------------------------------
    def level_dirs(self) -> dict[str, str]:
        base = os.path.join(self.game, 'Dist', 'win32', 'levels')
        return {d.lower(): os.path.join(base, d) for d in sorted(os.listdir(base))
                if os.path.isfile(os.path.join(base, d, 'level-00.fbrb')) and not d.lower().endswith('_common')
                and not d.lower().startswith('opfor_')}   # opfor_*: OPFOR's data-fill bundles, not levels

    def pathdata(self, lv: str) -> str:
        return os.path.join(self.game, 'Output', 'win32', 'ai', 'levels', lv, 'infantry', 'pathdata.hpd')

    def dice_pathdata(self, lv: str) -> str | None:
        """The level's original path data (the earliest OPFOR backup if an OPFOR file is installed - NavKit's or
        stock-MP AI's), or None - also where an OPFOR stage installed onto a level that had none (original_file)."""
        return original_file(self.pathdata(lv))

    def level(self, name: str) -> str:
        lv = name.replace('\\', '/').split('/')[-1].lower()
        dirs = self.level_dirs()
        if lv not in dirs:
            raise NavKitError(f'unknown level {name!r} (see: navkit.py levels)')
        return lv

    def level_work(self, lv: str) -> str:
        return os.path.join(self.work, 'levels', lv)

    def out_dir(self, lv: str) -> str:
        d = os.path.join(self.work, 'navkit', lv)
        os.makedirs(d, exist_ok=True)
        return d

    def settings_path(self, lv: str) -> str:
        return os.path.join(self.out_dir(lv), 'navkit.json')

    def settings(self, lv: str) -> dict[str, Any]:
        p = self.settings_path(lv)
        if not os.path.isfile(p):
            raise NavKitError(f'{lv}: not prepared yet (navkit.py prepare {lv.upper()})')
        try:
            with open(p, encoding='utf-8') as f:
                return json.load(f)
        except ValueError:          # a damaged file: said, rather than a JSON traceback in every later step
            raise NavKitError(f'{lv}: {p} is damaged - prepare the level again (navkit.py prepare {lv.upper()})') from None

    def samples(self, lv: str, suffix: str = '') -> str:
        return os.path.join(self.work, 'navsample', f'{lv}_navkit{suffix}.bin')

    def samples08(self, lv: str) -> str | None:
        """The level's 0.8 m samples: <lv>_navkit.bin (navkit.py sample) or <lv>_full.bin (older campaign runs)."""
        for name in (f'{lv}_navkit', f'{lv}_full'):
            q = os.path.join(self.work, 'navsample', name + '.bin')
            if os.path.isfile(q):
                return q
        return None

    def run(self, args: list[str], log: Log) -> list[str]:
        runner = fbrbtool.run_cmd if fbrbtool.is_fbrbtool(args[0]) else dbxedit.run_cmd
        r = runner(args, capture_output=True, text=True, errors='replace', creationflags=NO_WINDOW)
        out = (r.stdout + r.stderr).strip().splitlines()
        if r.returncode != 0:
            raise NavKitError(f'{" ".join([os.path.basename(args[0]), *args[1:2]])} failed: ' + ' | '.join(out[-3:]))
        return out

    def unpack(self, bundle: str, dest: str, log: Log) -> None:
        """Unpacked again when the bundle's hash changed (round DF2, AU4-DS-14): into <dest>.tmp-<pid>, stamped
        there, then swapped in by renames under <dest>.unpack.lock - a second process never deletes a folder another
        is unpacking, and a file held in the old unpack (the map editor, its DBX inspector) never leaves a mix of two unpacks
        stamped as clean."""
        stamp = os.path.join(dest, '.source.sha256')
        source_hash = _sha256(bundle)

        def fresh() -> bool:
            if os.path.isfile(os.path.join(dest, 'fbrb.manifest')) and os.path.isfile(stamp):
                with open(stamp) as f:
                    return f.read().strip() == source_hash
            return False
        if fresh():
            log(f'  {os.path.relpath(dest, self.root)} already unpacked')
            return
        with atomicio.locked(dest + '.unpack.lock'):
            if fresh():
                log(f'  {os.path.relpath(dest, self.root)} already unpacked')
                return
            log(f'  unpacking {os.path.relpath(bundle, self.game)} ...')
            new = f'{dest}.tmp-{os.getpid()}'
            if os.path.isdir(new):
                shutil.rmtree(new)
            try:
                self.run([self.fbrbtool, 'unpack', bundle, new], log)
                log('  converting binary DBX to XML ...')
                log('  ' + self.run([self.dbxedit, 'xml', new], log)[-1])
            except BaseException:
                shutil.rmtree(new, ignore_errors=True)         # this process's own unfinished unpack
                raise
            atomicio.write_bytes(os.path.join(new, '.source.sha256'), source_hash.encode())
            old = f'{dest}.tmp-{os.getpid()}-0'
            if os.path.isdir(dest):
                try:
                    os.rename(dest, old)
                except OSError as e:
                    raise NavKitError(f'{dest} cannot be replaced by the new unpack: a file in it is held by another '
                                      'program (close the map editor or an Explorer window on it, then run '
                                      f'again) - {e}') from e
            try:
                os.rename(new, dest)
            except OSError:
                if os.path.isdir(old):
                    os.rename(old, dest)
                raise
            if os.path.isdir(old):
                try:
                    shutil.rmtree(old)
                except OSError:
                    log(f'  the old unpack {old} could not be deleted (a file in it is held): delete it later')


# --- steps -----------------------------------------------------------------------------------------------
def levels(pr: Project, log: Log = print) -> None:
    log('level        AI path data   NavKit')
    for lv, _d in pr.level_dirs().items():
        ai = 'yes (samplable)' if pr.dice_pathdata(lv) else 'no'
        done = []
        if os.path.isfile(os.path.join(pr.work, 'navkit', lv, 'navkit.json')):
            done.append('prepared')
        if os.path.isfile(os.path.join(pr.work, 'navkit', lv, 'pathdata.hpd')):
            done.append('built')
        if installed(pr, lv):
            done.append('INSTALLED')
        log(f'{lv:12s} {ai:14s} {", ".join(done)}')


def level_names(pr: Project) -> list[tuple[str, bool]]:
    """[(level, samplable)] for the GUI."""
    return [(lv, pr.dice_pathdata(lv) is not None) for lv in pr.level_dirs()]


def prepare(pr: Project, lv: str, log: Log = print, box: list[float] | None = None,
            y: list[float] | None = None) -> None:
    import levelobjects
    log(f'== prepare {lv}')
    common = []
    for c in ('sp_common', 'mp_common'):
        dest = os.path.join(pr.work, 'navkit', 'common', c)
        pr.unpack(os.path.join(pr.game, 'Dist', 'win32', 'levels', c, 'level-00.fbrb'), dest, log)
        common.append(dest)
    L = pr.level_work(lv)
    pr.unpack(os.path.join(pr.level_dirs()[lv], 'level-00.fbrb'), os.path.join(L, 'dist'), log)
    pkg = os.path.join(pr.game, 'Package', 'levels', os.path.basename(pr.level_dirs()[lv]), 'level-00.fbrb')
    if os.path.isfile(pkg):
        pr.unpack(pkg, os.path.join(L, 'pkg'), log)
    else:
        os.makedirs(os.path.join(L, 'pkg'), exist_ok=True)
    LV = level_partition_name(L, lv)
    log('  extracting objects, collision and spawn points ...')
    levelobjects.main(['levelobjects.py', L, LV] + common)
    import ladders
    lad = ladders.extract(L, common)
    atomicio.write_json(os.path.join(L, 'ladders.json'), lad, indent=1)
    log(f'  ladders: {len(lad)} (ladder links: their far ends seeded and linked in the build)')

    xoff = zoff = None
    so = glob.glob(os.path.join(L, 'dist', 'levels', lv, 'Precomputed', 'Infantry', 'SectorOffsets.xml'))
    if so:
        xoff, zoff = sector_offsets_of(so[0])
    # the objects decide the box only where --box / --y do not (a new map with nothing placed has no objects)
    try:
        auto_box, auto_y, source = sampling_box(pr, lv, L) if box is None or y is None else (None, None, 'given')
    except (NavKitError, OSError) as e:
        if (box is None) == (y is None):
            raise
        # one of the two given: name only the one missing
        raise NavKitError(f'{lv}: no objects to size the sampling ' + ('height range - pass --y as well' if y is None
                          else 'box - pass --box as well')) from e
    s: dict[str, Any] = {'level': LV, 'box': box or auto_box, 'y': y or auto_y, 'box_source': 'given' if box else source}
    if xoff is None:
        # No SectorOffsets asset (Onslaught and other MP levels): DICE's rule, one sector below the path data's
        # lowest extent, so the first sector id is 1,1 - it holds for all 15 shipped SectorOffsets (13 campaign,
        # ons_mp_005, ons_mp_008). A new map has no path data; its box (which holds the ground) stands in.
        import navregress
        ext = navregress.reference_bounds(pr.root, lv)
        lo = ext[:2] if ext else s['box'][:2]
        xoff, zoff = int(math.floor(lo[0] / 100.0)) - 1, int(math.floor(lo[1] / 100.0)) - 1
        s['offsets_source'] = 'rule: path data extent' if ext else 'rule: box'
    else:
        s['offsets_source'] = 'SectorOffsets'
    s['xoff'], s['zoff'] = xoff, zoff
    atomicio.write_json(pr.settings_path(lv), s, indent=1)
    cols = (s['box'][2] - s['box'][0]) / 0.8 * (s['box'][3] - s['box'][1]) / 0.8
    log(f'  box {fmt(s["box"])} ({s["box_source"]}), y {fmt(s["y"])}, sector offsets {s["xoff"]}, {s["zoff"]} '
        f'({s["offsets_source"]}); {cols / 1e6:.1f} M columns per sampling run')
    log(f'  settings: {os.path.relpath(pr.settings_path(lv), pr.root)}')


def level_partition_name(L: str, lv: str) -> str:
    for f in glob.glob(os.path.join(L, 'dist', 'levels', '*.xml')):
        b = os.path.splitext(os.path.basename(f))[0]
        if b.lower() == lv:
            return b
    raise NavKitError(f'{lv}: level partition levels/{lv}.xml not found in the bundle')


def sampling_box(pr: Project, lv: str, L: str) -> tuple[list[int], list[int], str]:
    """Box (minX, minZ, maxX, maxZ) and y (bottom, top): the original path data's extent plus 20 m if the
    level has path data, else the extent of its placed objects and spawn points plus 50 m."""
    pts: list[tuple[float, ...]] = []
    ref = pr.dice_pathdata(lv)
    if ref:
        import hpdtool
        import kymesh
        f = hpdtool.parse(open(ref, 'rb').read())
        for s in f.sectors:
            for a in kymesh.parse_mesh(s.mesh).areas:
                for line in a.outline:
                    pts.extend((p[0], p[1], p[2]) for p in line)
        margin, source = 20.0, 'extent of the level\'s path data + 20 m'
    if not pts:
        for name in ('objects.json', 'group_instances.json'):
            for o in json.load(open(os.path.join(L, name))):
                pts.append((o['m'][12], o['m'][13], o['m'][14]))
        pts += [tuple(p) for p in json.load(open(os.path.join(L, 'soldier_spawns.json')))]
        margin, source = 50.0, 'extent of the level\'s objects + 50 m'
    if not pts:
        raise NavKitError(f'{lv}: no objects to size the sampling box - pass --box and --y')
    xs, ys, zs = zip(*pts)
    box = [math.floor((min(xs) - margin) / 10) * 10, math.floor((min(zs) - margin) / 10) * 10,
           math.ceil((max(xs) + margin) / 10) * 10, math.ceil((max(zs) + margin) / 10) * 10]
    yr = [math.floor((min(ys) - 30) / 10) * 10, math.ceil((max(ys) + 150) / 10) * 10]
    return box, yr, source


def fmt(v: Iterable[float]) -> str:
    return ','.join(f'{a:g}' for a in v)


def game_running() -> bool:
    """tasklist names BFBC2Game.exe. A tasklist that does not answer in 30 s (the service host busy, as during the
    game's crash dump) counts as the game running: a refusal, never a hang (AU4-BUG-20)."""
    try:
        r = subprocess.run(['tasklist', '/FI', 'IMAGENAME eq BFBC2Game.exe', '/NH'], capture_output=True, text=True,
                           creationflags=NO_WINDOW, timeout=30)
    except subprocess.TimeoutExpired:
        return True
    return 'BFBC2Game.exe' in r.stdout


def ini_set(text: str, section: str, key: str, value: str) -> str:
    """Sets key=value in [section] of an ini text (adds the key if missing)."""
    lines = text.split('\n')
    sec: str | None
    start: int | None
    sec, start, end = None, None, len(lines)
    for i, l in enumerate(lines):
        m = re.match(r'\s*\[([^\]]+)\]', l)
        if m:
            if sec == section.lower():
                end = i
                break
            sec = m.group(1).strip().lower()
            if sec == section.lower():
                start = i
    cr = '\r' if '\r\n' in text else ''   # keep the file's line endings
    if start is None:
        return text.rstrip('\r\n') + f'{cr}\n{cr}\n[{section}]{cr}\n{key}={value}{cr}\n'
    for i in range(start + 1, end):
        if re.match(rf'\s*{re.escape(key)}\s*=', lines[i]):
            lines[i] = f'{key}={value}{cr}'
            return '\n'.join(lines)
    lines.insert(start + 1, f'{key}={value}{cr}')
    return '\n'.join(lines)


def ini_backup(pr: Project) -> str:
    return os.path.join(pr.work, 'navkit', 'opfor.ini.backup')


def recover_ini(pr: Project, log: Log = print) -> None:
    """A sampling run that was killed leaves opfor.ini changed: put the saved copy back."""
    b = ini_backup(pr)
    if os.path.isfile(b):
        atomicio.copy_file(b, pr.ini)
        os.remove(b)
        log('  restored opfor.ini from the copy an interrupted sampling run left behind')


def ensure_settings(pr: Project, lv: str) -> dict[str, Any]:
    """The level's settings (navkit.json); a level sampled before navkit.json existed gets one from what is there: its
    name (the converted level partition), box and heights (its 0.8 m samples), sector offsets (its SectorOffsets)."""
    if os.path.isfile(pr.settings_path(lv)):
        return pr.settings(lv)
    import navsample
    L = pr.level_work(lv)
    sp = pr.samples08(lv)
    so = glob.glob(os.path.join(L, 'dist', 'levels', '*', 'Precomputed', 'Infantry', 'SectorOffsets.xml'))
    if not sp or not so:
        raise NavKitError(f'{lv}: not prepared yet (navkit.py prepare {lv.upper()})')
    xoff, zoff = sector_offsets_of(so[0])
    s, _ = navsample.load(sp)
    name = level_partition_name(L, lv)
    st = {'level': name, 'box': [round(s.min_x, 3), round(s.min_z, 3), round(s.min_x + (s.nx - 1) * s.pitch, 3),
                                 round(s.min_z + (s.nz - 1) * s.pitch, 3)],
          'y': [round(s.y_bottom, 3), round(s.y_top, 3)], 'box_source': 'its 0.8 m samples',
          'offsets_source': 'its SectorOffsets', 'xoff': xoff, 'zoff': zoff}
    atomicio.write_json(pr.settings_path(lv), st, indent=1)
    return st


def sector_offsets_of(xml_path: str) -> tuple[int, int]:
    """(XOffset, ZOffset) of a level's SectorOffsets.xml (DbxEdit's XML of the asset)."""
    t = open(xml_path, encoding='utf-8').read()
    x = re.search(r'name="XOffset">(-?\d+)<', t)
    z = re.search(r'name="ZOffset">(-?\d+)<', t)
    if x is None or z is None:                  # a damaged or foreign file: said, rather than an AttributeError
        raise NavKitError(f'{xml_path}: no XOffset / ZOffset value (re-run navkit.py prepare)')
    return int(x.group(1)), int(z.group(1))


def source_stamps(folder: str) -> str:
    """A digest of every .source.sha256 stamp under folder (Project.unpack writes one per unpacked bundle): it changes
    whenever any bundle the folder was unpacked from changes."""
    h = hashlib.sha1()
    for dp, _d, fs in sorted(os.walk(folder)):
        if '.source.sha256' in fs:
            with open(os.path.join(dp, '.source.sha256'), encoding='utf-8') as f:
                h.update(os.path.relpath(dp, folder).encode('utf-8') + b'=' + f.read().strip().encode('ascii') + b';')
    return h.hexdigest()


def detailed_sectors(pr: Project, lv: str) -> list[tuple[int, ...]]:
    """The sectors to build as detailed zones (pitch 0.2), as (grid x, grid z) - chosen from the level alone
    (tools\\detailed.py: flagged objects, stacked floors in the 0.8 m samples); cached in work\\navkit\\<lv>\\
    detailed.json while the samples are the same."""
    import detailed
    s = ensure_settings(pr, lv)
    sp = pr.samples08(lv)
    cache = os.path.join(pr.out_dir(lv), 'detailed.json')
    # the sector ids are relative to the offsets (a new --box changes them); the level's objects come from its unpack,
    # so its source stamps are part of the key (suite deep scan 2026-11-15, C7: a level prepared again from a changed
    # bundle kept the detailed sectors of the old one)
    key = (f'{os.path.basename(sp) if sp else None}|{os.path.getmtime(sp) if sp else 0}|{detailed.MARGIN}|{detailed.STACKED_MIN}'
           f'|{s["xoff"]}|{s["zoff"]}|{source_stamps(pr.level_work(lv))}')
    c = atomicio.read_json(cache, {})           # a damaged cache is a miss (C6)
    if isinstance(c, dict) and c.get('key') == key and isinstance(c.get('sectors'), list):
        return [tuple(x) for x in c['sectors']]
    common = [os.path.join(pr.work, 'navkit', 'common', c) for c in ('mp_common', 'sp_common')]
    chosen, why = detailed.choose(pr.level_work(lv), common, sp, s['xoff'], s['zoff'])
    atomicio.write_json(cache, {'key': key, 'sectors': chosen, 'why': why})
    return [tuple(x) for x in chosen]       # as the cached answer gives them


def detailed_rects(pr: Project, lv: str, margin: float = 1.0) -> list[list[float]]:
    """The detailed sectors' squares plus a margin (the sampling window's overlap), for navsample_rects."""
    s = ensure_settings(pr, lv)
    return [[(i + s['xoff']) * 100 - margin, (j + s['zoff']) * 100 - margin, (i + s['xoff']) * 100 + 100 + margin,
             (j + s['zoff']) * 100 + 100 + margin] for i, j in detailed_sectors(pr, lv)]


def dice_detailed_sectors(pr: Project, lv: str) -> list[tuple[int, int]]:
    """The sectors DICE's path data builds as detailed zones (pitch 0.2), as (grid x, grid z) ids; empty without it.
    A build on a level with DICE path data takes these (build_detailed, KYG-DET-1: DICE's choice is designer zones
    that no level-data rule recovers, so its own list is the record)."""
    import hpdtool
    import kymesh
    p = pr.dice_pathdata(lv)
    if not p:
        return []
    return sorted((sec.id >> 8, sec.id & 0xff) for sec in hpdtool.parse(open(p, 'rb').read()).sectors
                  if abs(kymesh.parse_mesh(sec.mesh).pitch - 0.2) < 1e-3)


def detailed_box(pr: Project, lv: str, margin: float = 1.0) -> list[float]:
    """The box around the detailed sectors (their 100 m squares plus a margin), for a pitch-0.2 sampling run."""
    s = ensure_settings(pr, lv)
    ids = detailed_sectors(pr, lv)
    if not ids:
        raise NavKitError(f'{lv}: no detailed sectors (no flagged objects or stacked floors) - nothing to sample at 0.2')
    xs, zs = [(i + s['xoff']) * 100 for i, _ in ids], [(j + s['zoff']) * 100 for _, j in ids]
    return [min(xs) - margin, min(zs) - margin, max(xs) + 100 + margin, max(zs) + 100 + margin]


def pitch_suffix(pitch: float) -> str:
    """Sample-file suffix of a pitch: '' for the normal 0.8, '_p02' for 0.2."""
    return '' if abs(pitch - 0.8) < 1e-6 else f'_p{int(round(pitch * 10)):02d}'


def sample(pr: Project, lv: str, log: Log = print, confirm: Callable[[str], bool] | None = None,
           box: list[float] | None = None, y: list[float] | None = None, timeout: float = 3600, pitch: float = 0.8,
           budget_ms: int | None = None) -> bool:
    s = ensure_settings(pr, lv)
    if abs(pitch - 0.8) > 1e-6:
        # a detailed-zone run: its own box (the detailed sectors by default) and only their squares sampled
        # (navsample_rects); the level's settings stay as they are
        s = dict(s, box=box or detailed_box(pr, lv), y=y or s['y'], rects=None if box else detailed_rects(pr, lv))
    elif box or y:
        s['box'], s['y'] = box or s['box'], y or s['y']
        if box and s.get('offsets_source') == 'rule: box':
            # offsets from the box (a level without path data): follow the new box, or ground below the old
            # lowest sector would get grid positions below 0 (prepare does the same)
            s['xoff'], s['zoff'] = int(math.floor(box[0] / 100.0)) - 1, int(math.floor(box[1] / 100.0)) - 1
            log(f'  sector offsets follow the box: {s["xoff"]}, {s["zoff"]}')
        atomicio.write_json(pr.settings_path(lv), s, indent=1)
    log(f'== sample {lv}: box {fmt(s["box"])}, y {fmt(s["y"])}, pitch {pitch}')
    if not pr.dice_pathdata(lv):
        log('  warning: this level has no AI path data, so its AI (and the sampler) may not start - see QUICKSTART.md')
    if game_running():
        raise NavKitError('the game is running - close it first')
    if confirm and not confirm(f'NavKit will start the game twice to sample {lv.upper()} '
                               '(about 1-5 minutes each; the screen may stay black). Continue?'):
        log('  cancelled')
        return False
    recover_ini(pr, log)
    os.makedirs(os.path.join(pr.work, 'navsample'), exist_ok=True)
    ps = pitch_suffix(pitch)
    for label, agent, suffix in AGENTS:
        log(f'  run {label} ({agent}) ...')
        out = pr.samples(lv, suffix + ps)
        if os.path.isfile(out):
            os.remove(out)
        result = sample_run(pr, s, agent, os.path.basename(out)[:-4], log, timeout, pitch, budget_ms)
        if not os.path.isfile(out) or 'done' not in result:
            raise NavKitError(f'sampling did not finish: {result or "no result in the log"}')
    log('  samples: ' + ', '.join(os.path.relpath(pr.samples(lv, a[2] + ps), pr.root) for a in AGENTS))
    return True


def sample_level(pr: Project, lv: str, log: Log = print, confirm: Callable[[str], bool] | None = None,
                 box: list[float] | None = None, y: list[float] | None = None) -> bool:
    """The level's sampling: the normal and high-capsule runs at 0.8 m, then - when the level has detailed sectors
    (detailed.py: flagged objects, stacked floors in the 0.8 m samples) - the same two at 0.2 m over those sectors.
    One confirmation for all game launches."""
    if confirm and not confirm(f'NavKit will start the game up to four times to sample {lv.upper()} (two 0.8 m runs, then '
                               'two 0.2 m runs over its detailed zones; about 1-15 minutes each; the screen may stay '
                               'black). Continue?'):
        log('  cancelled')
        return False
    if not sample(pr, lv, log=log, box=box, y=y):
        return False
    ids = detailed_sectors(pr, lv)
    if not ids:
        log(f'  {lv}: no detailed sectors - no 0.2 m runs')
        return True
    log(f'  {lv}: {len(ids)} detailed sectors - 0.2 m runs')
    return sample(pr, lv, log=log, pitch=0.2)


def sample_run(pr: Project, s: dict[str, Any], agent: str, name: str, log: Log, timeout: float, pitch: float = 0.8,
               budget_ms: int | None = None) -> str:
    orig = open(pr.ini, 'rb').read()
    atomicio.copy_file(pr.ini, ini_backup(pr))
    log_start = os.path.getsize(pr.log_path) if os.path.isfile(pr.log_path) else 0
    t = orig.decode('utf-8', errors='replace')
    for sec, k, v in (('dev', 'level', 'Levels/' + s['level']), ('dev', 'autostart', '1'), ('dev', 'navsample', '1'),
                      ('dev', 'navsample_name', name), ('dev', 'navsample_box', fmt(s['box'])),
                      ('dev', 'navsample_y', fmt(s['y'])), ('dev', 'navsample_pitch', fmt([pitch])),
                      ('dev', 'navsample_offset', '0'), ('dev', 'navsample_agent', agent), ('debug', 'probes', '1'),
                      ('dev', 'navsample_rects', ';'.join(fmt(r) for r in (s.get('rects') or ())))) + (
            # a pitch-0.2 run is 10x the columns: a larger time budget per frame, so it finishes before a round with
            # working bots (or a campaign level's idle player: SP_006 back at the menu at 60 %, 2026-10-22) can end;
            # budget_ms overrides it (a retry at a larger budget: less game time for the same columns)
            # working bots can end (ONS_MP_002 at 40 ms: back to the menu at 95 %, 2026-10-21); normal runs keep the ini's
            (('dev', 'navsample_budget_ms', str(budget_ms)),) if budget_ms else
            () if abs(pitch - 0.8) < 1e-6 else (('dev', 'navsample_budget_ms', str(DETAILED_BUDGET_MS)),)):
        t = ini_set(t, sec, k, v)
    result = ''
    try:
        atomicio.write_bytes(pr.ini, t.encode('utf-8'))      # the user's ini: never half written (C2)
        subprocess.Popen([os.path.join(pr.root, 'OPFOR BFBC2.exe')], cwd=pr.root)
        t0 = time.time()
        last = ''
        time.sleep(15)
        while time.time() - t0 < timeout:
            time.sleep(5)
            new = read_log(pr, log_start)
            prog = [l for l in new if 'navsample:' in l]
            if prog and prog[-1] != last:
                last = prog[-1]
                log('    ' + last.split('navsample:', 1)[1].strip())
            found = [SAMPLE_DONE.search(l) for l in new]
            m = [x for x in found if x]
            if m:
                result = m[-1].group(1)
                break
            if not game_running():
                result = 'the game exited'
                break
            # back at the main menu after sampling began: the level ended (a round over, a campaign level failed)
            first = next((i for i, l in enumerate(new) if 'navsample: sampling' in l), None)   # not 'armed': before the menu
            if first is not None and any('UI cmd [main menu]' in l for l in new[first:]):
                result = ('the level ended (back at the main menu) - retry with a larger budget: '
                          'tools\\sample_all.py --levels <level> --budget <ms per frame>')   # navkit.py has no --budget
                break
        else:
            result = 'timed out'
    finally:
        close_game()
        atomicio.write_bytes(pr.ini, orig)
        os.remove(ini_backup(pr))
    return result


def read_log(pr: Project, start: int) -> list[str]:
    with open(pr.log_path, 'rb') as f:
        f.seek(start)
        return f.read().decode('utf-8', errors='replace').splitlines()


def close_game() -> None:
    if not game_running():
        return
    subprocess.run(['taskkill', '/IM', 'BFBC2Game.exe'], capture_output=True, creationflags=NO_WINDOW)
    for _ in range(10):
        time.sleep(1)
        if not game_running():
            return
    subprocess.run(['taskkill', '/F', '/IM', 'BFBC2Game.exe'], capture_output=True, creationflags=NO_WINDOW)
    time.sleep(2)


def sample_grid(path: str) -> tuple[float, float, float, int, int]:
    """(pitch, min_x, min_z, nx, nz) from a navsample file's header."""
    import navsample
    with open(path, 'rb') as f:
        head = f.read(navsample.HEADER.size)
    if len(head) < navsample.HEADER.size:
        raise NavKitError(f'{os.path.basename(path)}: not a navsample file (too short)')
    magic, _v, pitch, min_x, min_z, nx, nz = navsample.HEADER.unpack_from(head, 0)[:7]
    if magic != b'ONS1':
        raise NavKitError(f'{os.path.basename(path)}: not a navsample file')
    return pitch, min_x, min_z, nx, nz


def uncovered(g: tuple[float, float, float, int, int], sectors: Iterable[tuple[int, int]],
              s: dict[str, Any]) -> list[tuple[int, int]]:
    """The sectors whose 100 m square a sample grid g (sample_grid) does not cover, in the order given."""
    pitch, min_x, min_z, nx, nz = g
    max_x, max_z = min_x + (nx - 1) * pitch, min_z + (nz - 1) * pitch
    return [(i, j) for i, j in sectors
            if (i + s['xoff']) * 100 < min_x - 1e-3 or (i + s['xoff']) * 100 + 100 > max_x + 1e-3
            or (j + s['zoff']) * 100 < min_z - 1e-3 or (j + s['zoff']) * 100 + 100 > max_z + 1e-3]


# DICE's own detailed sectors on a level with DICE path data (KYG-DET-1): off. Round NKG2 measured them on the 15
# regression levels (462 of DICE's 502 sampled at 0.2): NavKit's 0.8 m build reproduces DICE's ground and graph less
# well than its 0.2 m build in the 293 sectors NavKit makes detailed and DICE does not (nodes_near -2.1, connectivity
# -0.05 points), so XNAV-BLD-3 keeps the level's own choice until the coarse build closes that gap.
DICE_DETAILED = False


def build_detailed(pr: Project, lv: str, p02: str) -> tuple[set[tuple[int, ...]], str]:
    """The sectors a build makes detailed, and a note for its log line: the level's own choice (detailed_sectors); with
    DICE_DETAILED, on a level with DICE path data, DICE's own detailed sectors (KYG-DET-1) that the pitch-0.2 run p02
    sampled - the level's own choice is what sample --pitch 0.2 samples, inside the run's grid -; the others stay at
    0.8 until sampled (the note counts them)."""
    own = detailed_sectors(pr, lv)
    dice = dice_detailed_sectors(pr, lv) if DICE_DETAILED else []
    if not dice:
        return set(own), ''
    out = set(uncovered(sample_grid(p02), dice, pr.settings(lv))) | (set(dice) - set(own))
    note = f", DICE's ({len(out)} of its {len(dice)} not sampled at 0.2 stay at 0.8)" if out else ", DICE's"
    return {x for x in dice if x not in out}, note


def stale_detailed_samples(pr: Project, lv: str, p02: str, hi02: str | None, sectors: Iterable[tuple[int, int]],
                           s: dict[str, Any]) -> None:
    """Refuses pitch-0.2 samples that do not fit the build: not at pitch 0.2, a high-capsule run on another grid
    than the normal one, or a grid that does not cover every detailed sector's square (left from a sampling run
    before the box, the sector offsets or the detailed sectors changed)."""
    redo = f' - sample them again (navkit.py sample {lv.upper()} --pitch 0.2)'
    g = sample_grid(p02)
    if abs(g[0] - 0.2) > 1e-4:
        raise NavKitError(f'{lv}: {os.path.basename(p02)} is at pitch {g[0]:g}, not 0.2{redo}')
    if hi02 and sample_grid(hi02) != g:
        raise NavKitError(f'{lv}: {os.path.basename(hi02)} and {os.path.basename(p02)} are on different grids '
                          f'(one is from an older box){redo}')
    pitch, min_x, min_z, nx, nz = g
    max_x, max_z = min_x + (nx - 1) * pitch, min_z + (nz - 1) * pitch
    out = uncovered(g, sectors, s)
    if out:
        raise NavKitError(f'{lv}: {os.path.basename(p02)} (x {min_x:.1f}..{max_x:.1f}, z {min_z:.1f}..{max_z:.1f}) '
                          f'does not cover {len(out)} detailed sector(s), e.g. {out[0]} - made for another box or '
                          f'other sector offsets{redo}')


def build_inputs(pr: Project, lv: str, log: Log = print) -> tuple[str, str, int, int, dict[str, Any]]:
    """(samples, forbid, xoff, zoff, keyword arguments) of navwalk.build for a level: one place, so that build and
    apply (a build with walkability edits) always pass the same inputs - detailed zones and ladders included."""
    import navregress
    s = pr.settings(lv)
    L = pr.level_work(lv)
    s08 = pr.samples08(lv)                      # <lv>_navkit.bin, or <lv>_full.bin of older campaign runs
    if not s08 or not os.path.isfile(s08[:-4] + '_hi.bin'):
        raise NavKitError(f'{lv}: samples missing ({os.path.basename(pr.samples(lv))} and its _hi run) - run sample first')
    detailed: dict[str, Any] | None = None
    if os.path.isfile(pr.samples(lv, '_p02')):
        # the detailed-zone pass: DICE's pitch-0.2 sectors built from the level's pitch-0.2 samples (sample --pitch 0.2)
        hi02 = pr.samples(lv, '_hi_p02')
        sectors, note = build_detailed(pr, lv, pr.samples(lv, '_p02'))
        detailed = dict(samples=pr.samples(lv, '_p02'), high=hi02 if os.path.isfile(hi02) else None,
                        sectors=sectors)
        stale_detailed_samples(pr, lv, detailed['samples'], detailed['high'], sorted(detailed['sectors']), s)
        log(f'  detailed zones: {len(detailed["sectors"])} sectors at pitch 0.2{note}'
            + ('' if detailed['high'] else ' (no high-capsule run at 0.2 - stairs use the normal capsule there)'))
    kw = dict(detailed=detailed,
              dynamic_path=os.path.join(L, 'dynamic_tris.npy'), high_path=s08[:-4] + '_hi.bin',
              stairs_path=os.path.join(L, 'stair_tris.npy'),
              seeds=json.load(open(os.path.join(L, 'soldier_spawns.json'))),
              water=navregress.level_water(pr.root, lv), bounds=navregress.build_bounds(pr.root, lv),
              order_points=navregress.order_points_of(pr.root, lv), area_orders=navregress.area_orders_of(pr.root, lv),
              ladders=json.load(open(os.path.join(L, 'ladders.json'))) if os.path.isfile(os.path.join(L, 'ladders.json')) else None,
              log=lambda *a: log('  ' + ' '.join(str(x) for x in a)))
    return s08, os.path.join(L, 'forbid_tris.npy'), s['xoff'], s['zoff'], kw


def build(pr: Project, lv: str, log: Log = print, cfg: dict[str, Any] | None = None) -> None:
    """The level's build in the Rust core (navwalk.build). cfg: build settings over the Rust defaults (e.g.
    {'detail_features': False}; None: the defaults)."""
    import navwalk
    out = os.path.join(pr.out_dir(lv), 'pathdata.hpd')
    log(f'== build {lv} -> {os.path.relpath(out, pr.root)}')
    t0 = time.time()
    samples, forbid, xoff, zoff, kw = build_inputs(pr, lv, log)
    trav = traversal_path(os.path.dirname(out), lv)
    if os.path.isfile(trav):
        os.remove(trav)                          # annotations of the previous file: its node ids no longer hold
    navwalk.build(samples, forbid, xoff, zoff, out, **kw, settings=cfg)
    log(f'  built in {time.time() - t0:.0f} s ({os.path.getsize(out) / 1e6:.1f} MB)')


def check(pr: Project, lv: str, log: Log = print) -> None:
    s = pr.settings(lv)
    ours = os.path.join(pr.out_dir(lv), 'pathdata.hpd')
    if not os.path.isfile(ours):
        raise NavKitError(f'{lv}: not built yet')
    xml = os.path.join(pr.level_work(lv), 'dist', 'levels', s['level'] + '.xml')
    ref = pr.dice_pathdata(lv)
    args = [sys.executable, os.path.join(HERE, 'tools', 'ordercheck.py'), xml] + ([ref] if ref else []) + [ours]
    log(f'== check {lv}' + (' against the original path data' if ref else ' (self-check: no original path data)'))
    out = pr.run(args, log)
    report = os.path.join(pr.out_dir(lv), 'check.txt')
    open(report, 'w', encoding='utf-8').write('\n'.join(out) + '\n')
    for l in out:
        if re.search(r'positions:|on an area|route check|self-check', l) and not l.startswith('  '):
            log('  ' + l)
    if ref:
        import navcompare
        log('  summary against the original:')
        navcompare.report(navcompare.compare(xml, ref, ours), lambda m: log('    ' + m))
    log(f'  full report: {os.path.relpath(report, pr.root)}')


def compare(pr: Project, lv: str, log: Log = print, reference: str | None = None, candidate: str | None = None) -> None:
    """Two path-data files of a level on its AI positions (default: the original against NavKit's build)."""
    import navcompare
    s = pr.settings(lv)
    reference = reference or pr.dice_pathdata(lv)
    candidate = candidate or os.path.join(pr.out_dir(lv), 'pathdata.hpd')
    if not reference or not os.path.isfile(reference):
        raise NavKitError(f'{lv}: no reference file (the level has no original path data; pass --reference)')
    if not os.path.isfile(candidate):
        raise NavKitError(f'{lv}: no candidate file {candidate} (build first, or pass --candidate)')
    xml = os.path.join(pr.level_work(lv), 'dist', 'levels', s['level'] + '.xml')
    log(f'== compare {lv}: reference {os.path.relpath(reference, pr.root)}, candidate {os.path.relpath(candidate, pr.root)}')
    navcompare.report(navcompare.compare(xml, reference, candidate), lambda m: log('  ' + m))


def traversal_path(folder: str, lv: str) -> str:
    """The traversal annotations written next to a level's path data (write_traversal)."""
    return os.path.join(folder, f'{lv}.traversal.json')


# The apply's edit stage runs in the Rust core (round NK1: rust/bc2-navbuild/src/graphedit.rs through bc2_native;
# navlinks.py and navtraversal.py were retired against their recorded answers - core/tests/cli_o4_navkit.py's apply
# cases and rust/BENCHMARKS.md "NK1"). bfbc2.exe answers navkit apply on a DICE base itself; a NavKit base rebuilds
# the level here (navwalk.build: the Rust build) and then calls the same core. The side file's operations go to it as JSON.
GRAPH_OPS = ('link-areas', 'route-edge')


def _edit_core() -> Any:
    """The Rust core for the apply's edits, or bfbc2core.native.NativeMissing saying how to get it."""
    from bfbc2core import native
    return native.require("navkit apply's edits (links, route edges, traversal annotations)", 'navkit_graph_edits')


def _edit_refused() -> Exception:
    from bfbc2core import native
    return native.refused('navkit apply: a path-data file or a side-file value its edits do not take (a NaN or an '
                          'infinity, a position past 2**53, a route edge whose cost does not fit 32 bits)')


def graph_edits(data: bytes, operations: list[dict[str, Any]]) -> tuple[bytes, list[str], str]:
    """The side file's links and route edges (DESIGN.md 5.1) written into the route graph of the path data `data`:
    (the edited file, one line per graph operation - '  ' and its message, or '  NOT APPLIED: ' and why - and the
    line saying how many were applied). A point joins the route node nearest to it (within 15 m horizontally and
    4 m in height); the two nodes are looked for in one sector (the closest pair wins; border nodes let a link cross
    a sector edge); costs are 0x80000000 | floor(3-D distance * 16); edges stay sorted by source node with each
    node's first-edge index recomputed. ValueError / struct.error for a file that is not path data; IndexError for
    an edge whose node index is past its sector's nodes."""
    got = _edit_core().navkit_graph_edits(data, json.dumps(operations))
    if got is None:
        raise _edit_refused()
    return bytes(got[0]), list(got[1]), str(got[2])


def write_traversal(data: bytes, pathdata_path: str, operations: list[dict[str, Any]], level: Any, out_path: str,
                    log: Log = print) -> int:
    """The traversal annotations among operations (ladders, jumps, vaults, drops) written to out_path as
    `<level>.traversal.json` for OPFOR's bot layer (format 'bfbc2-traversal' 1: each annotation's ends, the route
    node each end joins - nearest within 15 m horizontally and 4 m in height - and its island, 0 the largest),
    for the path data `data` written at pathdata_path (its SHA-1 recorded). Not route edges: the game's AI cannot
    climb or jump them. Returns the annotations written."""
    got = _edit_core().navkit_traversal(data, os.path.basename(pathdata_path), json.dumps(operations),
                                        json.dumps(level))
    if got is None:
        raise _edit_refused()
    text, lines = str(got[0]), list(got[1])
    for line in lines:
        log(line)
    tmp = out_path + '.tmp'
    with open(tmp, 'w', encoding='utf-8', newline='\n') as fh:
        fh.write(text)
    os.replace(tmp, out_path)
    return len(lines)


def owner(lv: str) -> str:
    """The owner NavKit installs a level's path data as, in the game copy's install ledger."""
    return f'navkit:{lv.lower()}'


def installed(pr: Project, lv: str) -> bool:
    """NavKit's install of the level is in the game copy's install ledger, or its markers from before the ledger are
    beside the path data."""
    pd = pr.pathdata(lv)
    if os.path.isfile(pd + BACKUP_SUFFIX) or os.path.isfile(pd + NEW_SUFFIX):
        return True
    try:
        return bool(gameledger.Ledger(pr.game).owner_files(owner(lv)))
    except gameledger.LedgerError as e:
        raise NavKitError(str(e)) from e


def _game_rel(pr: Project, p: str) -> str:
    return os.path.relpath(p, pr.game).replace('\\', '/')


def _adopt_legacy(led: gameledger.Ledger, lv: str, rel: str, log: Log) -> None:
    """NavKit's own legacy markers beside the path data (.opfor-orig / .opfor-new, from before the install ledger)
    adopted into the ledger; another tool's refused in words."""
    if gameledger.adopt_legacy(led, owner(lv), [rel], 'navkit') is not None:
        log(f"  NavKit's markers from before the install ledger moved into it ({rel})")


def _traversal_made_for(trav: str) -> str | None:
    if not os.path.isfile(trav):
        return None
    try:
        with open(trav, encoding='utf-8') as f:
            made_for: str | None = json.load(f).get('pathdata_sha1')
            return made_for
    except (OSError, ValueError):
        return None


def install(pr: Project, lv: str, log: Log = print, src: str | None = None) -> None:
    """The built file (and its traversal annotations, when made for it) installed as navkit:<lv> in the game copy's
    install ledger (round DF2): what was there is kept there once, by its sha256, and restore gives it back whatever
    was installed over NavKit since."""
    ours = src or os.path.join(pr.out_dir(lv), 'pathdata.hpd')
    if not os.path.isfile(ours):
        raise NavKitError(f'{lv}: not built yet')
    if game_running():
        raise NavKitError('the game is running - close it first')
    dest = pr.pathdata(lv)
    rel = _game_rel(pr, dest)
    led = gameledger.Ledger(pr.game)
    try:
        _adopt_legacy(led, lv, rel, log)
        me = owner(lv)
        holders = led.holders(rel)
        with open(ours, 'rb') as f:
            data = f.read()
        trav, trav_dest = traversal_path(os.path.dirname(ours), lv), traversal_path(os.path.dirname(dest), lv)
        trav_rel = _game_rel(pr, trav_dest)
        with_trav = _traversal_made_for(trav) == hashlib.sha1(data).hexdigest()
        files = {rel: data}
        if with_trav:
            with open(trav, 'rb') as f:
                files[trav_rel] = f.read()
        if me not in holders:
            if holders:
                log(f"  installed over {holders[-1]}'s file (either can be reverted first: the ledger keeps what each "
                    'one replaced)')
            elif os.path.isfile(dest):
                log(f"  the original {os.path.basename(dest)} kept in the game copy's install ledger (restore puts it "
                    'back)')
            else:
                log(f'  noting {os.path.basename(dest)} had no original (restore will remove it)')
        plan = led.install(me, files, derived=False, note='NavKit path data')
        for n in plan.notes:
            log(f'  {n}')
        log(f'== installed {os.path.relpath(ours, pr.root)} as {os.path.relpath(dest, pr.root)}')
        if with_trav:
            log(f'  traversal annotations installed as {os.path.relpath(trav_dest, pr.root)}')
        else:
            if os.path.isfile(trav):
                log(f'  traversal annotations {os.path.basename(trav)} not installed: made for another path-data file')
            # stale annotations nobody installed through the ledger (from before it): none for another file
            if os.path.isfile(trav_dest) and not led.holders(trav_rel):
                os.remove(trav_dest)
    except gameledger.LedgerError as e:
        raise NavKitError(str(e)) from e


def restore(pr: Project, lv: str, log: Log = print) -> None:
    """NavKit's install taken out of the game copy's install ledger: the file from before NavKit put back, or, when
    another tool installed over NavKit since, NavKit's layer taken out from under it (that tool's revert then gives
    back what was there before NavKit)."""
    dest = pr.pathdata(lv)
    rel = _game_rel(pr, dest)
    me = owner(lv)
    legacy = any(m.lower() in (BACKUP_SUFFIX, NEW_SUFFIX) for m in gameledger.legacy_markers(dest))
    led = gameledger.Ledger(pr.game)
    try:
        if not led.owner_files(me) and not legacy:
            log(f'{lv}: nothing to restore (no NavKit file installed)')
            return
        if game_running():
            raise NavKitError('the game is running - close it first')
        _adopt_legacy(led, lv, rel, log)
        if not led.owner_files(me):
            log(f'{lv}: nothing to restore (no NavKit file installed)')
            return
        holders = led.holders(rel)
        above = holders[holders.index(me) + 1] if me in holders and holders.index(me) + 1 < len(holders) else None
        plan = led.uninstall(me)
        for n in plan.notes:
            if 'taken out from under' not in n:
                log(f'  {n}')
        if above:
            log(f"== NavKit's {os.path.relpath(dest, pr.root)} taken out from under {above}'s install: the file stays "
                f"{above}'s, and its revert now puts back what was there before NavKit")
        elif os.path.isfile(dest):
            log(f'== restored the original {os.path.relpath(dest, pr.root)}')
        else:
            log(f'== removed the installed {os.path.relpath(dest, pr.root)} (the level had no path data before)')
        trav = traversal_path(os.path.dirname(dest), lv)
        if os.path.isfile(trav) and not led.holders(_game_rel(pr, trav)):
            os.remove(trav)
    except gameledger.LedgerError as e:
        raise NavKitError(str(e)) from e


def apply(pr: Project, lv: str, navtune_path: str | None = None, out: str | None = None, log: Log = print,
          install_after: bool = False, cfg: dict[str, Any] | None = None) -> str:
    """Builds the level with the edits of its navigation side file (DESIGN.md sections 5.1-5.2): walkability
    edits (regions, seeds, unseeds) go into the build, then links and route edges into its route graph
    (graph_edits); on a DICE base only the graph edits, on a copy of DICE's file. Traversal annotations go to
    <level>.traversal.json next to it (write_traversal). The result is read back to check it, the whole log is
    kept as <output>.report.txt, and with install_after the result is installed. cfg: build settings over the Rust
    defaults for a NavKit-base build (None: the defaults). Returns the output path."""
    lines: list[str] = []
    target: dict[str, str | None] = {'out': out}               # the output path, as soon as _apply knows it (its default depends on the base)

    def say(msg: str) -> None:
        lines.append(msg)
        log(msg)
    try:
        out = _apply(pr, lv, navtune_path, out, say, target, cfg)
    except Exception as e:              # any failure: the report of what was done, and the error (then re-raised)
        if target['out']:
            open(target['out'] + '.report.txt', 'w', encoding='utf-8').write('\n'.join(lines + [f'error: {e}']) + '\n')
        raise
    open(out + '.report.txt', 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    if install_after:
        install(pr, lv, log=log, src=out)
    return out


def _apply(pr: Project, lv: str, navtune_path: str | None, out: str | None, log: Log,
           target: dict[str, str | None] | None = None, cfg: dict[str, Any] | None = None) -> str:
    target = target if target is not None else {}
    import hpdtool
    import navwalk
    from bfbc2core import navtune
    navtune_path = navtune_path or navtune.default_path(pr.root, lv)
    if not os.path.isfile(navtune_path):
        raise NavKitError(f'{lv}: no side file {navtune_path} (make edits in the map editor\'s AI workspace first)')
    try:
        tune = navtune.load(navtune_path)
    except ValueError as e:
        raise NavKitError(str(e)) from e
    walk = [o for o in tune.operations if o['op'] in ('walkable', 'unwalkable', 'seed', 'unseed')]
    graph = [o for o in tune.operations if o['op'] in GRAPH_OPS]
    later = [o for o in tune.operations if o['op'] == 'traversal']
    t0 = time.time()
    if tune.kind == 'navkit':
        out = target['out'] = out or os.path.join(pr.out_dir(lv), 'pathdata.hpd')
        log(f'== apply {lv}: {len(tune.operations)} edit(s) from {os.path.relpath(navtune_path, pr.root)} '
            f'-> {os.path.relpath(out, pr.root)}')
        # the same inputs as build (detailed zones, ladders ...), plus the edits
        samples, forbid, xoff, zoff, kw = build_inputs(pr, lv, log)
        navwalk.build(samples, forbid, xoff, zoff, out, edits=walk, **kw, settings=cfg)
        log(f'  built in {time.time() - t0:.0f} s; {len(walk)} walkability edit(s) applied')
    else:
        # a DICE base: no rebuild - the graph edits go into a copy of DICE's file, for this PC only (never
        # shared: it contains DICE's data; the side file is what can be shared)
        if walk:
            raise NavKitError(f'{lv}: region edits need a NavKit base (DESIGN.md 5.1)')
        src = pr.dice_pathdata(lv)
        if not src:
            raise NavKitError(f'{lv}: the side file\'s base is DICE\'s data, but the level has none')
        out = target['out'] = out or os.path.join(pr.out_dir(lv), 'pathdata_dice_edited.hpd')
        log(f'== apply {lv}: {len(tune.operations)} edit(s) on a copy of DICE\'s file -> {os.path.relpath(out, pr.root)} '
            f'(for this PC only)')
        if tune.base_changed(src):
            log('  note: DICE\'s file is not the one the edits were made on (another game version?); the edits are '
                'by position and are applied, but check them')
        shutil.copyfile(src, out)
    if graph:
        edited, lines, applied = graph_edits(open(out, 'rb').read(), graph)
        for line in lines:
            log(line)
        open(out, 'wb').write(edited)
        log(applied)
    data = open(out, 'rb').read()
    f = hpdtool.parse(data)
    if hpdtool.write(f) != data:
        raise NavKitError(f'{out}: does not read back exactly - not usable')
    trav = traversal_path(os.path.dirname(out), lv)
    if later:
        write_traversal(data, out, later, tune.level, trav, log=log)
        log(f'  {len(later)} traversal annotation(s) written to {os.path.relpath(trav, pr.root)} (for OPFOR\'s bot layer; '
            f'not route edges - the game\'s AI cannot climb or jump them)')
    elif os.path.isfile(trav):
        os.remove(trav)                          # an earlier run's annotations no longer apply
    notes = sum(o['op'] == 'note' for o in tune.operations)
    if notes:
        log(f'  {notes} note(s): kept in the side file only')
    log(f'  read back exactly ({time.time() - t0:.0f} s in all)')
    return out


def status(pr: Project, lv: str, log: Log = print) -> None:
    log(f'{lv}:')
    L = pr.level_work(lv)
    log(f'  original path data : {pr.dice_pathdata(lv) or "none"}')
    log(f'  prepared           : {"yes" if os.path.isfile(os.path.join(pr.work, "navkit", lv, "navkit.json")) else "no"}')
    if os.path.isfile(os.path.join(pr.work, 'navkit', lv, 'navkit.json')):
        s = pr.settings(lv)
        log(f'    box {fmt(s["box"])}, y {fmt(s["y"])}, offsets {s["xoff"]},{s["zoff"]}')
    for _, _, suf in AGENTS:
        p = pr.samples(lv, suf)
        log(f'  samples{suf or "":12s}: {time.ctime(os.path.getmtime(p)) if os.path.isfile(p) else "no"}')
    p = os.path.join(pr.work, 'navkit', lv, 'pathdata.hpd')
    log(f'  built              : {time.ctime(os.path.getmtime(p)) if os.path.isfile(p) else "no"}')
    log(f'  installed in game  : {"yes" if installed(pr, lv) else "no"}')
    del L


def ask(question: str) -> bool:
    return input(question + ' [y/N] ').strip().lower() in ('y', 'yes')


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:]]
    opts: dict[str, str] = {}
    for k in ('--box', '--y', '--opfor', '--root', '--reference', '--candidate', '--navtune', '--out', '--from',
              '--pitch'):   # --root: older name of --opfor
        if k in args:
            i = args.index(k)
            opts[k] = args[i + 1]
            del args[i:i + 2]
    yes = '--yes' in args
    install_flag = '--install' in args
    cfg: dict[str, Any] | None = None    # build settings over the Rust defaults
    if '--no-detail' in args:              # build / apply / all: no D lines or F cells (the output before 2026-10-26)
        cfg = {'detail_features': False}
    args = [a for a in args if a not in ('--yes', '--install', '--no-detail')]
    if not args or args[0] in ('-h', '--help', 'help'):
        print(__doc__)
        return 0
    box = [float(v) for v in opts['--box'].split(',')] if '--box' in opts else None
    y = [float(v) for v in opts['--y'].split(',')] if '--y' in opts else None
    confirm = None if yes else ask
    try:
        pr = Project(opts.get('--opfor') or opts.get('--root'))
        cmd = args[0]
        if cmd == 'levels':
            levels(pr)
            return 0
        if len(args) < 2:
            raise NavKitError(f'{cmd}: which level? (e.g. navkit.py {cmd} SP_01)')
        lv = pr.level(args[1])
        if cmd in ('build', 'apply', 'all'):
            import navwalk
            navwalk.require_native()       # the build runs in the Rust core: say so before preparing or sampling
        if cmd == 'prepare':
            prepare(pr, lv, box=box, y=y)
        elif cmd == 'sample':
            if '--pitch' in opts:
                sample(pr, lv, confirm=confirm, box=box, y=y, pitch=float(opts['--pitch']))
            else:
                sample_level(pr, lv, confirm=confirm, box=box, y=y)
        elif cmd == 'build':
            build(pr, lv, cfg=cfg)
        elif cmd == 'compare':
            compare(pr, lv, reference=opts.get('--reference'), candidate=opts.get('--candidate'))
        elif cmd == 'check':
            check(pr, lv)
        elif cmd == 'apply':
            apply(pr, lv, opts.get('--navtune'), opts.get('--out'), install_after=install_flag, cfg=cfg)
        elif cmd == 'install':
            install(pr, lv, src=opts.get('--from'))
        elif cmd == 'restore':
            restore(pr, lv)
        elif cmd == 'status':
            status(pr, lv)
        elif cmd == 'all':
            prepare(pr, lv, box=box, y=y)
            if sample_level(pr, lv, confirm=confirm):
                build(pr, lv, cfg=cfg)
                check(pr, lv)
        else:
            raise NavKitError(f'unknown command {cmd!r}')
    except NavKitError as e:
        print('error:', e)
        import logging
        logging.getLogger('bfbc2').error(f'{" ".join(argv[1:])}: {e}')
        return 1
    return 0


if __name__ == '__main__':
    from bfbc2core import errors
    errors.setup('navkit', gui=False)          # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
