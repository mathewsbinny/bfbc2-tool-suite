r"""NavKit's level driver through every path (round NK6): the same path data and lines from `bin\bfbc2.exe navkit`
(the command line), the editor's in-process Build (`bc2-editor`'s example `nav_apply`: `nav::start_apply`, the
job the AI workspace's Build button starts) and the export's path-data step (`bc2-levelwrite`'s example
`pathdata_build`: `pathdata::build_bytes`), all on `bc2_navbuild::inputs` / `apply`.

  python rust\parity_navkit_paths.py dice <opfor> <out.json>
      navkit apply on a DICE base (NK1's parity set: every DICE file of the game copy and every NavKit regression
      build, each with NK1's generated side file) by the command line and by the editor in process: the edited file,
      the traversal annotations, the report and the lines (seconds blanked), each in a stand-in OPFOR folder
  python rust\parity_navkit_paths.py build <opfor> <out.jsonl> [level ...]
      each regression level built by the command line (`navkit build`), the editor (`apply` of an empty NavKit side
      file) and the export step, one after another, in a stand-in OPFOR folder whose game, levels, samples and shared
      unpacks are junctions to the real ones (nothing of the real work\navkit is written); the three files compared
      with each sector's build date and time zeroed, and against navregress's build of the level with the golden
      clock (test_navbuild_native.FIXED_CLOCK) when `--regress` is given
  python rust\parity_navkit_paths.py navapply <opfor> <out.jsonl> [level ...]
      navkit apply on a NavKit base (a rebuild with a region, a seed and an unseed, then links, route edges, a
      traversal and a note) by the command line and by the editor: the files and the lines

The editor's and the export's examples are built first (release, this checkout's build folder).
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
sys.path.insert(0, SUITE)
sys.path.insert(0, os.path.join(SUITE, 'core', 'tests'))
sys.path.insert(0, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tools'))
import suite_paths  # noqa: E402

suite_paths.use('core')
import hpdtool  # noqa: E402
from bfbc2core import native  # noqa: E402

sys.path.insert(0, HERE)
from parity_nk4_walk import LEVELS  # noqa: E402

CLI = os.path.join(SUITE, 'bin', 'bfbc2.exe')


def examples() -> tuple[str, str]:
    """The two examples, built (release) into this checkout's build folder."""
    target = native.target_dir()
    for pkg, ex in (('bc2-editor', 'nav_apply'), ('bc2-levelwrite', 'pathdata_build')):
        subprocess.run([sys.executable, os.path.join(HERE, 'build_native.py'), '--cargo', 'build', '--release', '-p',
                        pkg, '--example', ex], check=True)
    ex = os.path.join(target, 'release', 'examples')
    return os.path.join(ex, 'nav_apply.exe'), os.path.join(ex, 'pathdata_build.exe')


def env() -> dict[str, str]:
    drop = ('BFBC2_CLI_PYTHON', 'BFBC2_CLI_SUITE', 'BFBC2_CLI_TRACE', 'BFBC2_CLI_WORKERS')
    e = {k: v for k, v in os.environ.items() if k not in drop and not k.startswith('PYTHON')}
    e['OPENBLAS_NUM_THREADS'] = '1'
    return e


def run(cmd: list[str], e: dict[str, str]) -> tuple[int, str, float]:
    t = time.perf_counter()
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=e)
    return p.returncode, p.stdout.decode('utf-8', 'replace'), time.perf_counter() - t


def norm_text(t: str, root: str) -> str:
    t = t.replace(root, '<opf>').replace('\r\n', '\n')
    t = re.sub(r'\(\d+ s in all\)', '(N s in all)', t)
    t = re.sub(r'\(\d+ s\)', '(N s)', t)
    t = re.sub(r'built in \d+ s', 'built in N s', t)
    return t


def undated(data: bytes | None) -> str | None:
    """The path data's SHA-1 with each sector's build date and time zeroed (None: no file)."""
    if data is None:
        return None
    f = hpdtool.parse(data)
    for s in f.sectors:
        s.date = 0
        s.time = 0
    out = hpdtool.write(f)
    return hashlib.sha1(out).hexdigest()


def junction(link: str, target: str) -> None:
    os.makedirs(os.path.dirname(link), exist_ok=True)
    subprocess.run(['cmd', '/c', 'mklink', '/J', link, target], check=True, stdout=subprocess.DEVNULL)


def unjunction(root: str) -> None:
    """Removes the stand-in folder: every junction with rmdir first (never followed), then the rest."""
    for dp, ds, _fs in os.walk(root, topdown=True):
        keep = []
        for d in ds:
            p = os.path.join(dp, d)
            if os.path.isjunction(p):
                subprocess.run(['cmd', '/c', 'rmdir', p], check=True)
            else:
                keep.append(d)
        ds[:] = keep
    shutil.rmtree(root, ignore_errors=True)


def stand_in(opfor: str, lv: str) -> str:
    """A stand-in OPFOR folder for a NavKit build of lv: junctions to the game, the prepared levels, the samples and
    the shared unpacks; the level's own work\\navkit folder a copy of its settings."""
    root = tempfile.mkdtemp(prefix='nk_paths_')
    junction(os.path.join(root, 'game'), os.path.join(opfor, 'game'))
    for rel in (('work', 'levels'), ('work', 'navsample'), ('work', 'navsample_offline'), ('work', 'navkit', 'common')):
        src = os.path.join(opfor, *rel)
        if os.path.isdir(src):
            junction(os.path.join(root, *rel), src)
    d = os.path.join(root, 'work', 'navkit', lv)
    os.makedirs(d)
    shutil.copyfile(os.path.join(opfor, 'work', 'navkit', lv, 'navkit.json'), os.path.join(d, 'navkit.json'))
    return root


def side_text(kind: str, ops: list[dict[str, Any]], sha1: str = '') -> str:
    import cli_o4_navkit as cases
    return cases._side(kind, ops, sha1)


def regress_child(opfor: str, lv: str) -> dict[str, Any]:
    """navregress.build_level of lv with the golden clock (as parity_nk5_build): its undated SHA-1, and the golden
    record's answer for the levels test_navbuild_native recorded."""
    suite_paths.use('navkit-tools')
    import navregress
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-auto-navkit', 'tests'))
    import test_navbuild_native as golden_test
    out = os.path.join(tempfile.gettempdir(), f'nk_paths_regress_{lv}_{os.getpid()}.hpd')
    clock = time.localtime
    time.localtime = lambda *a: golden_test.FIXED_CLOCK
    try:
        navregress.build_level(opfor, lv, out)
    finally:
        time.localtime = clock
    data = open(out, 'rb').read()
    os.remove(out)
    res: dict[str, Any] = {'sha1': hashlib.sha1(data).hexdigest(), 'undated_sha1': undated(data)}
    g = golden_test.load().get('levels', {}).get(lv)
    if g:
        inputs = {os.path.relpath(q, opfor).replace(os.sep, '/'): golden_test.file_sha1(q)
                  for q in golden_test.input_files(opfor, lv, navregress)}
        res['golden'] = ('inputs changed' if inputs != g['inputs'] else
                         'as recorded' if g['pathdata'] == res['sha1'] else 'DIFFERENT')
    return res


def regress_sha(lv: str, opfor: str) -> dict[str, Any]:
    """regress_child in its own process."""
    p = subprocess.run([sys.executable, os.path.abspath(__file__), '--regress-child', opfor, lv],
                       capture_output=True, text=True, env=env())
    got = [ln[7:] for ln in p.stdout.splitlines() if ln.startswith('RESULT ')]
    return json.loads(got[-1]) if got else {'error': (p.stderr or p.stdout)[-2000:]}


def build_mode(opfor: str, out: str, levels: list[str], regress: bool) -> int:
    nav_apply, pd_build = examples()
    same = 0
    for lv in levels:
        root = stand_in(opfor, lv)
        row: dict[str, Any] = {'level': lv}
        try:
            e = env()
            e['LOCALAPPDATA'] = os.path.join(root, 'lad')
            d = os.path.join(root, 'work', 'navkit', lv)
            code, text, took = run([CLI, 'navkit', 'build', lv, '--opfor', root], e)
            cli = os.path.join(d, 'pathdata.hpd')
            row['cli'] = {'exit': code, 's': round(took, 1),
                          'sha1': undated(open(cli, 'rb').read() if os.path.isfile(cli) else None),
                          'tail': text.strip().splitlines()[-1:] if code else []}
            side = os.path.join(d, f'{lv}.navtune.json')
            open(side, 'w', encoding='utf-8', newline='\n').write(side_text('navkit', []))
            ed = os.path.join(d, 'pathdata_editor.hpd')
            code, text, took = run([nav_apply, root, lv, side, ed], e)
            row['editor'] = {'exit': code, 's': round(took, 1),
                             'sha1': undated(open(ed, 'rb').read() if os.path.isfile(ed) else None),
                             'tail': text.strip().splitlines()[-1:] if code else []}
            ex = os.path.join(d, 'pathdata_export.hpd')
            code, text, took = run([pd_build, root, os.path.join(opfor, 'game'), lv, ex], e)
            row['export'] = {'exit': code, 's': round(took, 1),
                             'sha1': undated(open(ex, 'rb').read() if os.path.isfile(ex) else None),
                             'tail': text.strip().splitlines()[-2:] if code else []}
            if regress:
                r = regress_sha(lv, opfor)
                row['golden'] = r.get('golden', r.get('error'))
                row['regress'] = r.get('undated_sha1')
        finally:
            unjunction(root)
        shas = {row[k]['sha1'] for k in ('cli', 'editor', 'export')} | ({row['regress']} if regress else set())
        row['identical'] = len(shas) == 1 and None not in shas
        same += row['identical']
        with open(out, 'a', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps(row, sort_keys=True) + '\n')
        print(json.dumps(row, sort_keys=True), flush=True)
    print(f'{same} of {len(levels)} levels identical through the command line, the editor and the export step')
    return 0 if same == len(levels) else 1


def dice_files(opfor: str) -> list[tuple[str, str, str]]:
    out = []
    lv_root = os.path.join(opfor, 'game', 'Output', 'win32', 'ai', 'levels')
    for lv in sorted(os.listdir(lv_root)):
        p = os.path.join(lv_root, lv, 'infantry', 'pathdata.hpd')
        if os.path.isfile(p):
            out.append((f'dice {lv}', lv, p))
    reg = os.path.join(opfor, 'work', 'navkit', 'regress')
    for n in sorted(os.listdir(reg)):
        m = re.fullmatch(r'((?:sp|ons_mp|mp)_[0-9a-z]+?(?:_b)?)\.hpd', n)
        if m:
            out.append((f'navkit {m.group(1)}', m.group(1), os.path.join(reg, n)))
    return out


def dice_root(lv: str, data: bytes, side: str) -> str:
    """NK1's stand-in: a game copy with only the level's DICE file, its side file."""
    root = tempfile.mkdtemp(prefix='nk_dice_')
    for p in (os.path.join(root, 'game', 'BFBC2Game.exe'),
              os.path.join(root, 'game', 'Dist', 'win32', 'levels', lv.upper(), 'level-00.fbrb')):
        os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p, 'wb').write(b'x')
    p = os.path.join(root, 'game', 'Output', 'win32', 'ai', 'levels', lv, 'infantry', 'pathdata.hpd')
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, 'wb').write(data)
    s = os.path.join(root, 'work', 'navkit', lv, f'{lv}.navtune.json')
    os.makedirs(os.path.dirname(s), exist_ok=True)
    open(s, 'w', encoding='utf-8', newline='\n').write(side)
    return root


def outputs(d: str, lv: str, name: str) -> dict[str, bytes | None]:
    got = {}
    for n in (name, f'{lv}.traversal.json', name + '.report.txt'):
        p = os.path.join(d, n)
        got[n.replace(name, 'out')] = open(p, 'rb').read() if os.path.isfile(p) else None
    return got


def dice_mode(opfor: str, out: str) -> int:
    import cli_o4_navkit as cases
    nav_apply, _ = examples()
    rows, bad = [], 0
    for label, lv, src in dice_files(opfor):
        data = open(src, 'rb').read()
        ops = cases.apply_ops(data, 17, n=36)
        side = side_text('dice', ops, hashlib.sha1(data).hexdigest())
        res = {}
        for who in ('cli', 'editor'):
            root = dice_root(lv, data, side)
            try:
                e = env()
                e['LOCALAPPDATA'] = os.path.join(root, 'lad')
                d = os.path.join(root, 'work', 'navkit', lv)
                name = 'pathdata_dice_edited.hpd'
                if who == 'cli':
                    code, text, took = run([CLI, 'navkit', 'apply', lv, '--opfor', root], e)
                else:
                    code, text, took = run([nav_apply, root, lv, os.path.join(d, f'{lv}.navtune.json'),
                                            os.path.join(d, name)], e)
                files = {k: (None if v is None else norm_text(v.decode('utf-8', 'replace'), root))
                         for k, v in outputs(d, lv, name).items()}
                res[who] = (code, norm_text(text, root), files, took)
            finally:
                shutil.rmtree(root, ignore_errors=True)
        a, b = res['cli'], res['editor']
        same = a[0] == b[0] and a[1] == b[1] and a[2] == b[2]
        row = {'label': label, 'ops': len(ops), 'identical': same, 'exit': [a[0], b[0]],
               'cli_s': round(a[3], 3), 'editor_s': round(b[3], 3)}
        if not same:
            bad += 1
            row['diff'] = [k for k in a[2] if a[2][k] != b[2][k]] + (['lines'] if a[1] != b[1] else [])
            row['cli_out'], row['editor_out'] = a[1][-800:], b[1][-800:]
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items() if not k.endswith('_out')}, sort_keys=True), flush=True)
    json.dump(rows, open(out, 'w'), indent=1, sort_keys=True)
    print(f'{len(rows) - bad} of {len(rows)} identical')
    return 1 if bad else 0


def nav_ops(data: bytes) -> list[dict[str, Any]]:
    """A NavKit-base side file: a walkable and an unwalkable box, a seed and an unseed around the file's first nodes,
    then NK1's links, route edges, traversals and notes."""
    import cli_o4_navkit as cases
    f = hpdtool.parse(data)
    secs = [s for s in f.sectors if len(s.nodes) > 2]
    p = secs[len(secs) // 2].nodes[0].pos
    q = secs[len(secs) // 3].nodes[1].pos
    by = {'by': 'NK6-TEST', 'when': '2026-11-22T10:00:00'}
    r3 = lambda v: round(float(v), 3)  # noqa: E731
    ops: list[dict[str, Any]] = [
        {'op': 'walkable', 'region': {'box': [r3(p[0] - 4), r3(p[1] - 3), r3(p[2] - 4), r3(p[0] + 4), r3(p[1] + 1),
                                              r3(p[2] + 4)]}, 'value': True, **by},
        {'op': 'unwalkable', 'region': {'polygon': [[r3(q[0] - 3), r3(q[2] - 3)], [r3(q[0] + 3), r3(q[2] - 3)],
                                                    [r3(q[0]), r3(q[2] + 3)]], 'y': [r3(q[1] - 3), r3(q[1] + 1)]},
         **by},
        {'op': 'seed', 'at': [r3(p[0]), r3(p[1] - 0.95), r3(p[2])], **by},
        {'op': 'unseed', 'at': [r3(q[0] + 40), r3(q[1] - 0.95), r3(q[2] + 40)], **by},
    ]
    return ops + cases.apply_ops(data, 23, n=12)


def navapply_mode(opfor: str, out: str, levels: list[str]) -> int:
    nav_apply, _ = examples()
    same = 0
    for lv in levels:
        base = os.path.join(opfor, 'work', 'navkit', 'regress', f'{lv}.hpd')
        ops = nav_ops(open(base, 'rb').read())
        res = {}
        for who in ('cli', 'editor'):
            root = stand_in(opfor, lv)
            try:
                e = env()
                e['LOCALAPPDATA'] = os.path.join(root, 'lad')
                d = os.path.join(root, 'work', 'navkit', lv)
                side = os.path.join(d, f'{lv}.navtune.json')
                open(side, 'w', encoding='utf-8', newline='\n').write(side_text('navkit', ops))
                name = 'pathdata.hpd'
                if who == 'cli':
                    code, text, took = run([CLI, 'navkit', 'apply', lv, '--opfor', root], e)
                else:
                    code, text, took = run([nav_apply, root, lv, side, os.path.join(d, name)], e)
                got = outputs(d, lv, name)
                files = {'out': undated(got['out']),
                         **{k: (None if v is None else norm_text(v.decode('utf-8', 'replace'), root))
                            for k, v in got.items() if k != 'out'}}
                res[who] = (code, norm_text(text, root), files, took)
            finally:
                unjunction(root)
        a, b = res['cli'], res['editor']
        eq = a[0] == b[0] and a[1] == b[1] and a[2] == b[2]
        same += eq
        row = {'level': lv, 'ops': len(ops), 'identical': eq, 'exit': [a[0], b[0]], 'cli_s': round(a[3], 1),
               'editor_s': round(b[3], 1), 'sha1': a[2]['out']}
        if not eq:
            row['diff'] = [k for k in a[2] if a[2][k] != b[2][k]] + (['lines'] if a[1] != b[1] else [])
            row['cli_out'], row['editor_out'] = a[1][-1500:], b[1][-1500:]
        with open(out, 'a', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps(row, sort_keys=True) + '\n')
        print(json.dumps({k: v for k, v in row.items() if not k.endswith('_out')}, sort_keys=True), flush=True)
    print(f'{same} of {len(levels)} identical')
    return 0 if same == len(levels) else 1


def main() -> int:
    if len(sys.argv) == 4 and sys.argv[1] == '--regress-child':
        print('RESULT ' + json.dumps(regress_child(sys.argv[2], sys.argv[3]), sort_keys=True), flush=True)
        return 0
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    mode, opfor, out = sys.argv[1:4]
    rest = [a for a in sys.argv[4:] if not a.startswith('--')]
    if mode == 'dice':
        return dice_mode(opfor, out)
    if mode == 'build':
        return build_mode(opfor, out, rest or LEVELS, '--regress' in sys.argv)
    if mode == 'navapply':
        return navapply_mode(opfor, out, rest or ['sp_009', 'sp_004_b'])
    print(__doc__)
    return 2


if __name__ == '__main__':
    sys.exit(main())
