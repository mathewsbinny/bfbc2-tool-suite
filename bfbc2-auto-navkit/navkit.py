"""NavKit driver: one command per step, from a level name to an installed pathdata.hpd.

  python navkit.py levels                     list the game's levels (and which can be sampled)
  python navkit.py prepare SP_01              unpack + convert the level, extract its objects,
                                              choose the sampling box and sector offsets
  python navkit.py sample  SP_01 [--yes]      two in-game collision sampling runs (launches the game)
  python navkit.py build   SP_01              generate pathdata.hpd
  python navkit.py check   SP_01              score it (against DICE's file if the level has one)
  python navkit.py apply   SP_01 [--navtune file] [--out file] [--install]
                                              build with the edits of the level's navigation side file
                                              (work\\navkit\\<level>\\<level>.navtune.json, made in the
                                              Navigation Studio) into work\\navkit\\<level>\\pathdata.hpd,
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
         --opfor <Project OPFOR folder> (default: OPFOR_ROOT, else "Project OPFOR" next to the toolkit).

NavKit works on Project OPFOR's game copy and uses its in-game sampler. FbrbTool / DbxEdit come from the
toolkit (..\\bfbc2-fbrb-tool, ..\\bfbc2-dbx-tools, built with their build.bat) or from OPFOR's tools\\ folder.

Files (in the OPFOR folder): work\\levels\\<level>\\ (unpacked level, objects), work\\navkit\\<level>\\
(settings, pathdata.hpd, check report), work\\navsample\\<level>_navkit*.bin (samples, written by OPFOR's
[dev] navsample). See QUICKSTART.md.
"""
import glob
import json
import math
import os
import re
import shutil
import subprocess
import sys
import time

NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)   # no console flashes when run from the GUI
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, 'tools'))

AGENTS = [('normal', '0.4,0.28,1.9', ''), ('high capsule', '0.4,0.6,1.9', '_hi')]   # radius,stepMax,height
SAMPLE_DONE = re.compile(r'navsample: (done|no physics|cannot create|empty box|needs \[debug\])')
BACKUP_SUFFIX = '.opfor-orig'


class NavKitError(Exception):
    pass


def find_opfor(given=None):
    """The Project OPFOR folder: --opfor, else OPFOR_ROOT, else a "Project OPFOR" folder next to one of the folders
    this tool sits in (e.g. next to the toolkit that holds the suite)."""
    up = []
    d = HERE
    while os.path.dirname(d) != d:
        d = os.path.dirname(d)
        up.append(os.path.join(d, 'Project OPFOR'))
    candidates = [given, os.environ.get('OPFOR_ROOT')] + up
    for c in candidates:
        if c and os.path.isdir(os.path.join(c, 'game')):
            return os.path.abspath(c)
    raise NavKitError('Project OPFOR not found - pass --opfor <folder> or set OPFOR_ROOT '
                      f'(looked for a game\\ folder in: {", ".join(c for c in candidates if c)})')


def find_tool(opfor, exe, toolkit_dir):
    """A toolkit tool: built in the toolkit (..\\<toolkit_dir>\\<exe>), else OPFOR's tools\\ folder."""
    for p in (os.path.join(os.path.dirname(HERE), toolkit_dir, exe), os.path.join(opfor, 'tools', exe)):
        if os.path.isfile(p):
            return p
    raise NavKitError(f'{exe} not found - run build.bat in the toolkit\'s {toolkit_dir} folder')


class Project:
    def __init__(self, root=None):
        self.root = find_opfor(root)
        self.game = os.path.join(self.root, 'game')
        self.work = os.path.join(self.root, 'work')
        self.ini = os.path.join(self.game, 'opfor.ini')
        self.log_path = os.path.join(self.game, 'logs', 'opfor.log')
        if not os.path.isfile(os.path.join(self.game, 'BFBC2Game.exe')):
            raise NavKitError(f'no game copy in {self.game} (run OPFOR\'s setup_game.bat first)')
        self.fbrbtool = find_tool(self.root, 'FbrbTool.exe', 'bfbc2-fbrb-tool')
        self.dbxedit = find_tool(self.root, 'DbxEdit.exe', 'bfbc2-dbx-tools')

    # --- levels --------------------------------------------------------------------------------------
    def level_dirs(self):
        base = os.path.join(self.game, 'Dist', 'win32', 'levels')
        return {d.lower(): os.path.join(base, d) for d in sorted(os.listdir(base))
                if os.path.isfile(os.path.join(base, d, 'level-00.fbrb')) and not d.lower().endswith('_common')
                and not d.lower().startswith('opfor_')}   # opfor_*: OPFOR's data-fill bundles, not levels

    def pathdata(self, lv):
        return os.path.join(self.game, 'Output', 'win32', 'ai', 'levels', lv, 'infantry', 'pathdata.hpd')

    def dice_pathdata(self, lv):
        """The level's original path data (the backup if NavKit's file is installed), or None."""
        p = self.pathdata(lv)
        for f in (p + BACKUP_SUFFIX, p):
            if os.path.isfile(f):
                return f
        return None

    def level(self, name):
        lv = name.replace('\\', '/').split('/')[-1].lower()
        dirs = self.level_dirs()
        if lv not in dirs:
            raise NavKitError(f'unknown level {name!r} (see: navkit.py levels)')
        return lv

    def level_work(self, lv):
        return os.path.join(self.work, 'levels', lv)

    def out_dir(self, lv):
        d = os.path.join(self.work, 'navkit', lv)
        os.makedirs(d, exist_ok=True)
        return d

    def settings_path(self, lv):
        return os.path.join(self.out_dir(lv), 'navkit.json')

    def settings(self, lv):
        p = self.settings_path(lv)
        if not os.path.isfile(p):
            raise NavKitError(f'{lv}: not prepared yet (navkit.py prepare {lv.upper()})')
        return json.load(open(p))

    def samples(self, lv, suffix=''):
        return os.path.join(self.work, 'navsample', f'{lv}_navkit{suffix}.bin')

    def run(self, args, log):
        r = subprocess.run(args, capture_output=True, text=True, errors='replace', creationflags=NO_WINDOW)
        out = (r.stdout + r.stderr).strip().splitlines()
        if r.returncode != 0:
            raise NavKitError(f'{os.path.basename(args[0])} failed: ' + ' | '.join(out[-3:]))
        return out

    def unpack(self, bundle, dest, log):
        if os.path.isdir(dest) and os.listdir(dest):
            log(f'  {os.path.relpath(dest, self.root)} already unpacked')
            return
        log(f'  unpacking {os.path.relpath(bundle, self.game)} ...')
        self.run([self.fbrbtool, 'unpack', bundle, dest], log)
        log('  converting binary DBX to XML ...')
        log('  ' + self.run([self.dbxedit, 'xml', dest], log)[-1])


# --- steps -----------------------------------------------------------------------------------------------
def levels(pr, log=print):
    log('level        AI path data   NavKit')
    for lv, d in pr.level_dirs().items():
        ai = 'yes (samplable)' if pr.dice_pathdata(lv) else 'no'
        done = []
        if os.path.isfile(os.path.join(pr.work, 'navkit', lv, 'navkit.json')):
            done.append('prepared')
        if os.path.isfile(os.path.join(pr.work, 'navkit', lv, 'pathdata.hpd')):
            done.append('built')
        if os.path.isfile(pr.pathdata(lv) + BACKUP_SUFFIX):
            done.append('INSTALLED')
        log(f'{lv:12s} {ai:14s} {", ".join(done)}')


def level_names(pr):
    """[(level, samplable)] for the GUI."""
    return [(lv, pr.dice_pathdata(lv) is not None) for lv in pr.level_dirs()]


def prepare(pr, lv, log=print, box=None, y=None):
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

    xoff = zoff = None
    so = glob.glob(os.path.join(L, 'dist', 'levels', lv, 'Precomputed', 'Infantry', 'SectorOffsets.xml'))
    if so:
        t = open(so[0], encoding='utf-8').read()
        xoff = int(re.search(r'name="XOffset">(-?\d+)<', t).group(1))
        zoff = int(re.search(r'name="ZOffset">(-?\d+)<', t).group(1))
    auto_box, auto_y, source = sampling_box(pr, lv, L)
    s = {'level': LV, 'box': box or auto_box, 'y': y or auto_y, 'box_source': 'given' if box else source}
    s['xoff'] = xoff if xoff is not None else int(math.floor(s['box'][0] / 100.0))
    s['zoff'] = zoff if zoff is not None else int(math.floor(s['box'][1] / 100.0))
    s['offsets_source'] = 'SectorOffsets' if xoff is not None else 'from the box'
    json.dump(s, open(pr.settings_path(lv), 'w'), indent=1)
    cols = (s['box'][2] - s['box'][0]) / 0.8 * (s['box'][3] - s['box'][1]) / 0.8
    log(f'  box {fmt(s["box"])} ({s["box_source"]}), y {fmt(s["y"])}, sector offsets {s["xoff"]}, {s["zoff"]} '
        f'({s["offsets_source"]}); {cols / 1e6:.1f} M columns per sampling run')
    log(f'  settings: {os.path.relpath(pr.settings_path(lv), pr.root)}')


def level_partition_name(L, lv):
    for f in glob.glob(os.path.join(L, 'dist', 'levels', '*.xml')):
        b = os.path.splitext(os.path.basename(f))[0]
        if b.lower() == lv:
            return b
    raise NavKitError(f'{lv}: level partition levels/{lv}.xml not found in the bundle')


def sampling_box(pr, lv, L):
    """Box (minX, minZ, maxX, maxZ) and y (bottom, top): the original path data's extent plus 20 m if the
    level has path data, else the extent of its placed objects and spawn points plus 50 m."""
    pts = []
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


def fmt(v):
    return ','.join(f'{a:g}' for a in v)


def game_running():
    r = subprocess.run(['tasklist', '/FI', 'IMAGENAME eq BFBC2Game.exe', '/NH'], capture_output=True, text=True,
                       creationflags=NO_WINDOW)
    return 'BFBC2Game.exe' in r.stdout


def ini_set(text, section, key, value):
    """Sets key=value in [section] of an ini text (adds the key if missing)."""
    lines = text.split('\n')
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


def ini_backup(pr):
    return os.path.join(pr.work, 'navkit', 'opfor.ini.backup')


def recover_ini(pr, log=print):
    """A sampling run that was killed leaves opfor.ini changed: put the saved copy back."""
    b = ini_backup(pr)
    if os.path.isfile(b):
        shutil.copyfile(b, pr.ini)
        os.remove(b)
        log('  restored opfor.ini from the copy an interrupted sampling run left behind')


def sample(pr, lv, log=print, confirm=None, box=None, y=None, timeout=3600):
    s = pr.settings(lv)
    if box or y:
        s['box'], s['y'] = box or s['box'], y or s['y']
        json.dump(s, open(pr.settings_path(lv), 'w'), indent=1)
    log(f'== sample {lv}: box {fmt(s["box"])}, y {fmt(s["y"])}')
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
    for label, agent, suffix in AGENTS:
        log(f'  run {label} ({agent}) ...')
        out = pr.samples(lv, suffix)
        if os.path.isfile(out):
            os.remove(out)
        result = sample_run(pr, s, agent, os.path.basename(out)[:-4], log, timeout)
        if not os.path.isfile(out) or 'done' not in result:
            raise NavKitError(f'sampling did not finish: {result or "no result in the log"}')
    log('  samples: ' + ', '.join(os.path.relpath(pr.samples(lv, a[2]), pr.root) for a in AGENTS))
    return True


def sample_run(pr, s, agent, name, log, timeout):
    orig = open(pr.ini, 'rb').read()
    shutil.copyfile(pr.ini, ini_backup(pr))
    log_start = os.path.getsize(pr.log_path) if os.path.isfile(pr.log_path) else 0
    t = orig.decode('utf-8', errors='replace')
    for sec, k, v in (('dev', 'level', 'Levels/' + s['level']), ('dev', 'autostart', '1'), ('dev', 'navsample', '1'),
                      ('dev', 'navsample_name', name), ('dev', 'navsample_box', fmt(s['box'])),
                      ('dev', 'navsample_y', fmt(s['y'])), ('dev', 'navsample_pitch', '0.8'),
                      ('dev', 'navsample_offset', '0'), ('dev', 'navsample_agent', agent), ('debug', 'probes', '1')):
        t = ini_set(t, sec, k, v)
    result = ''
    try:
        open(pr.ini, 'wb').write(t.encode('utf-8'))
        subprocess.Popen([os.path.join(pr.root, 'OPFOR.exe')], cwd=pr.root)
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
            m = [SAMPLE_DONE.search(l) for l in new]
            m = [x for x in m if x]
            if m:
                result = m[-1].group(1)
                break
            if not game_running():
                result = 'the game exited'
                break
        else:
            result = 'timed out'
    finally:
        close_game()
        open(pr.ini, 'wb').write(orig)
        os.remove(ini_backup(pr))
    return result


def read_log(pr, start):
    with open(pr.log_path, 'rb') as f:
        f.seek(start)
        return f.read().decode('utf-8', errors='replace').splitlines()


def close_game():
    if not game_running():
        return
    subprocess.run(['taskkill', '/IM', 'BFBC2Game.exe'], capture_output=True, creationflags=NO_WINDOW)
    for _ in range(10):
        time.sleep(1)
        if not game_running():
            return
    subprocess.run(['taskkill', '/F', '/IM', 'BFBC2Game.exe'], capture_output=True, creationflags=NO_WINDOW)
    time.sleep(2)


def build(pr, lv, log=print):
    import navbuild
    s = pr.settings(lv)
    L = pr.level_work(lv)
    for suffix in ('', '_hi'):
        if not os.path.isfile(pr.samples(lv, suffix)):
            raise NavKitError(f'{lv}: samples missing ({os.path.basename(pr.samples(lv, suffix))}) - run sample first')
    out = os.path.join(pr.out_dir(lv), 'pathdata.hpd')
    log(f'== build {lv} -> {os.path.relpath(out, pr.root)}')
    t0 = time.time()
    navbuild.build(pr.samples(lv), os.path.join(L, 'forbid_tris.npy'), s['xoff'], s['zoff'], out,
                   dynamic_path=os.path.join(L, 'dynamic_tris.npy'), high_path=pr.samples(lv, '_hi'),
                   stairs_path=os.path.join(L, 'stair_tris.npy'),
                   seeds=json.load(open(os.path.join(L, 'soldier_spawns.json'))),
                   log=lambda *a: log('  ' + ' '.join(str(x) for x in a)))
    log(f'  built in {time.time() - t0:.0f} s ({os.path.getsize(out) / 1e6:.1f} MB)')


def check(pr, lv, log=print):
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


def compare(pr, lv, log=print, reference=None, candidate=None):
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


def traversal_path(folder, lv):
    """The traversal annotations written next to a level's path data (navtraversal.py)."""
    return os.path.join(folder, f'{lv}.traversal.json')


def install(pr, lv, log=print, src=None):
    """Copies a built file into the game (DICE's original is kept once, as <file>.opfor-orig), with the level's
    traversal annotations when it has them (next to the installed path data, for OPFOR's bot layer)."""
    ours = src or os.path.join(pr.out_dir(lv), 'pathdata.hpd')
    if not os.path.isfile(ours):
        raise NavKitError(f'{lv}: not built yet')
    if game_running():
        raise NavKitError('the game is running - close it first')
    dest = pr.pathdata(lv)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    if os.path.isfile(dest) and not os.path.isfile(dest + BACKUP_SUFFIX):
        shutil.copyfile(dest, dest + BACKUP_SUFFIX)
        log(f'  original kept as {os.path.basename(dest)}{BACKUP_SUFFIX}')
    shutil.copyfile(ours, dest)
    log(f'== installed {os.path.relpath(ours, pr.root)} as {os.path.relpath(dest, pr.root)}')
    trav, trav_dest = traversal_path(os.path.dirname(ours), lv), traversal_path(os.path.dirname(dest), lv)
    if os.path.isfile(trav):
        shutil.copyfile(trav, trav_dest)
        log(f'  traversal annotations installed as {os.path.relpath(trav_dest, pr.root)}')
    elif os.path.isfile(trav_dest):
        os.remove(trav_dest)                     # none with this file: no stale annotations from an earlier one


def restore(pr, lv, log=print):
    dest = pr.pathdata(lv)
    if not os.path.isfile(dest + BACKUP_SUFFIX):
        log(f'{lv}: nothing to restore (no NavKit file installed over an original)')
        return
    if game_running():
        raise NavKitError('the game is running - close it first')
    shutil.move(dest + BACKUP_SUFFIX, dest)
    trav = traversal_path(os.path.dirname(dest), lv)
    if os.path.isfile(trav):
        os.remove(trav)
    log(f'== restored the original {os.path.relpath(dest, pr.root)}')


def apply(pr, lv, navtune_path=None, out=None, log=print, install_after=False):
    """Builds the level with the edits of its navigation side file (DESIGN.md sections 5.1-5.2): walkability
    edits (regions, seeds, unseeds) go into the build, then links and route edges into its route graph
    (navlinks.py); on a DICE base only the graph edits, on a copy of DICE's file. Traversal annotations go to
    <level>.traversal.json next to it (navtraversal.py). The result is read back to check it, the whole log is
    kept as <output>.report.txt, and with install_after the result is installed. Returns the output path."""
    lines = []

    def say(msg):
        lines.append(msg)
        log(msg)
    try:
        out = _apply(pr, lv, navtune_path, out, say)
    except NavKitError as e:
        if out:
            open(out + '.report.txt', 'w', encoding='utf-8').write('\n'.join(lines + [f'error: {e}']) + '\n')
        raise
    open(out + '.report.txt', 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    if install_after:
        install(pr, lv, log=log, src=out)
    return out


def _apply(pr, lv, navtune_path, out, log):
    sys.path.insert(0, os.path.join(HERE, '..', 'core'))
    from bfbc2core import navtune
    import navbuild
    import navregress
    import hpdtool
    navtune_path = navtune_path or navtune.default_path(pr.root, lv)
    if not os.path.isfile(navtune_path):
        raise NavKitError(f'{lv}: no side file {navtune_path} (make edits in the Navigation Studio first)')
    try:
        tune = navtune.load(navtune_path)
    except ValueError as e:
        raise NavKitError(str(e))
    import navlinks
    walk = [o for o in tune.operations if o['op'] in ('walkable', 'unwalkable', 'seed', 'unseed')]
    graph = [o for o in tune.operations if o['op'] in navlinks.GRAPH_OPS]
    later = [o for o in tune.operations if o['op'] == 'traversal']
    t0 = time.time()
    if tune.kind == 'navkit':
        out = out or os.path.join(pr.out_dir(lv), 'pathdata.hpd')
        try:
            samples, xoff, zoff, f = navregress.inputs(pr.root, lv)
        except RuntimeError as e:
            raise NavKitError(str(e))
        log(f'== apply {lv}: {len(tune.operations)} edit(s) from {os.path.relpath(navtune_path, pr.root)} '
            f'-> {os.path.relpath(out, pr.root)}')
        navbuild.build(samples, f['forbid_tris.npy'], xoff, zoff, out, dynamic_path=f['dynamic_tris.npy'],
                       high_path=samples[:-4] + '_hi.bin', stairs_path=f['stair_tris.npy'],
                       seeds=json.load(open(f['soldier_spawns.json'])), edits=walk,
                       log=lambda *a: log('  ' + ' '.join(str(x) for x in a)))
        log(f'  built in {time.time() - t0:.0f} s; {len(walk)} walkability edit(s) applied')
    else:
        # a DICE base: no rebuild - the graph edits go into a copy of DICE's file, for this PC only (never
        # shared: it contains DICE's data; the side file is what can be shared)
        if walk:
            raise NavKitError(f'{lv}: region edits need a NavKit base (DESIGN.md 5.1)')
        src = pr.dice_pathdata(lv)
        if not src:
            raise NavKitError(f'{lv}: the side file\'s base is DICE\'s data, but the level has none')
        out = out or os.path.join(pr.out_dir(lv), 'pathdata_dice_edited.hpd')
        log(f'== apply {lv}: {len(tune.operations)} edit(s) on a copy of DICE\'s file -> {os.path.relpath(out, pr.root)} '
            f'(for this PC only)')
        if tune.base_changed(src):
            log(f'  note: DICE\'s file is not the one the edits were made on (another game version?); the edits are '
                f'by position and are applied, but check them')
        shutil.copyfile(src, out)
    if graph:
        f = hpdtool.parse(open(out, 'rb').read())
        results = navlinks.apply(f, graph, log=log)
        open(out, 'wb').write(hpdtool.write(f))
        log(f'  {sum(ok for _, ok, _ in results)} of {len(graph)} link / route-edge edit(s) applied')
    data = open(out, 'rb').read()
    f = hpdtool.parse(data)
    if hpdtool.write(f) != data:
        raise NavKitError(f'{out}: does not read back exactly - not usable')
    trav = traversal_path(os.path.dirname(out), lv)
    if later:
        import navtraversal
        navtraversal.write(f, out, later, tune.level, trav, log=log)
        log(f'  {len(later)} traversal annotation(s) written to {os.path.relpath(trav, pr.root)} (for OPFOR\'s bot layer; '
            f'not route edges - the game\'s AI cannot climb or jump them)')
    elif os.path.isfile(trav):
        os.remove(trav)                          # an earlier run's annotations no longer apply
    notes = sum(o['op'] == 'note' for o in tune.operations)
    if notes:
        log(f'  {notes} note(s): kept in the side file only')
    log(f'  read back exactly ({time.time() - t0:.0f} s in all)')
    return out


def status(pr, lv, log=print):
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
    log(f'  installed in game  : {"yes" if os.path.isfile(pr.pathdata(lv) + BACKUP_SUFFIX) else "no"}')
    del L


def ask(question):
    return input(question + ' [y/N] ').strip().lower() in ('y', 'yes')


def main(argv):
    args = [a for a in argv[1:]]
    opts = {}
    for k in ('--box', '--y', '--opfor', '--root', '--reference', '--candidate', '--navtune', '--out', '--from'):   # --root: older name of --opfor
        if k in args:
            i = args.index(k)
            opts[k] = args[i + 1]
            del args[i:i + 2]
    yes = '--yes' in args
    install_flag = '--install' in args
    args = [a for a in args if a not in ('--yes', '--install')]
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
        if cmd == 'prepare':
            prepare(pr, lv, box=box, y=y)
        elif cmd == 'sample':
            sample(pr, lv, confirm=confirm, box=box, y=y)
        elif cmd == 'build':
            build(pr, lv)
        elif cmd == 'compare':
            compare(pr, lv, reference=opts.get('--reference'), candidate=opts.get('--candidate'))
        elif cmd == 'check':
            check(pr, lv)
        elif cmd == 'apply':
            apply(pr, lv, opts.get('--navtune'), opts.get('--out'), install_after=install_flag)
        elif cmd == 'install':
            install(pr, lv, src=opts.get('--from'))
        elif cmd == 'restore':
            restore(pr, lv)
        elif cmd == 'status':
            status(pr, lv)
        elif cmd == 'all':
            prepare(pr, lv, box=box, y=y)
            if sample(pr, lv, confirm=confirm):
                build(pr, lv)
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
    sys.path.insert(0, os.path.join(HERE, '..', 'core'))
    from bfbc2core import errors
    errors.setup('navkit', gui=False)          # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
