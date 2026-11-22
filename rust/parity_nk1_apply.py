r"""Round NK1's parity on real levels: navkit apply on a DICE base, the old Python (a suite checkout from before NK1,
4a515a4 or earlier: its navkit.py with navlinks.py / navtraversal.py) against the new Rust (a checkout's
bin\bfbc2.exe navkit apply), on every DICE path data file of Project OPFOR's game copy and every NavKit regression build (work\navkit\regress\<level>.hpd, used as
the "DICE" file of a stand-in folder). Each level gets a side file of generated operations (cli_o4_navkit.apply_ops:
links, route edges added / removed, border edges, traversals, notes). Compared: the edited .hpd, the traversal JSON,
the report and stdout (the timing line's seconds aside). Timed, with each process's peak memory (a job object).

  python rust\parity_nk1_apply.py <old suite> <new suite> <opfor> <out.json>
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

OLD, NEW, OPFOR, OUT = sys.argv[1:5]
sys.path.insert(0, os.path.join(NEW, 'core', 'tests'))
sys.path.insert(0, NEW)
import cli_o4_navkit as cases  # noqa: E402

import run_tests  # noqa: E402

SCRATCH = tempfile.mkdtemp(prefix='nk1_parity_')


def files():
    out = []
    lv_root = os.path.join(OPFOR, 'game', 'Output', 'win32', 'ai', 'levels')
    for lv in sorted(os.listdir(lv_root)):
        p = os.path.join(lv_root, lv, 'infantry', 'pathdata.hpd')
        if os.path.isfile(p):
            out.append((f'dice {lv}', lv, p))
    reg = os.path.join(OPFOR, 'work', 'navkit', 'regress')
    for n in sorted(os.listdir(reg)):
        m = re.fullmatch(r'((?:sp|ons_mp|mp)_[0-9a-z]+?(?:_b)?)\.hpd', n)
        if m:
            out.append((f'navkit {m.group(1)}', m.group(1), os.path.join(reg, n)))
    return out


def stand_in(root, lv, data, side):
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


def run(cmd, env):
    g = run_tests._ProcessGroup()
    t = time.perf_counter()
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env)
    g.add(p)
    out, _ = p.communicate()
    took = time.perf_counter() - t
    peak = g.peak_mb()
    return p.returncode, out, took, peak


def outputs(root, lv):
    d = os.path.join(root, 'work', 'navkit', lv)
    got = {}
    for n in ('pathdata_dice_edited.hpd', f'{lv}.traversal.json', 'pathdata_dice_edited.hpd.report.txt'):
        p = os.path.join(d, n)
        got[n] = open(p, 'rb').read() if os.path.isfile(p) else None
    return got


def norm(b, root):
    b = b.replace(root.encode(), b'<opf>')
    return re.sub(rb'\(\d+ s in all\)', b'(N s in all)', b)


def main():
    rows = []
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BFBC2_CLI', 'PYTHON'))}
    env['LOCALAPPDATA'] = os.path.join(SCRATCH, 'lad')
    bad = 0
    try:
        for label, lv, src in files():
            data = open(src, 'rb').read()
            ops = cases.apply_ops(data, 17, n=36)
            side = cases._side('dice', ops, hashlib.sha1(data).hexdigest())
            res = {}
            for who, cmd in (('python', [sys.executable, os.path.join(OLD, 'bfbc2-auto-navkit', 'navkit.py')]),
                             ('rust', [os.path.join(NEW, 'bin', 'bfbc2.exe'), 'navkit'])):
                root = os.path.join(SCRATCH, who)
                shutil.rmtree(root, ignore_errors=True)
                stand_in(root, lv, data, side)
                e = dict(env)
                if who == 'rust':
                    e['BFBC2_CLI_TRACE'] = os.path.join(SCRATCH, 'trace.txt')
                code, out, took, peak = run(cmd + ['apply', lv, '--opfor', root], e)
                res[who] = (code, norm(out, root), {k: (None if v is None else norm(v, root)) for k, v in outputs(root, lv).items()},
                            took, peak)
            py, rs = res['python'], res['rust']
            same = py[0] == rs[0] and py[1] == rs[1] and py[2] == rs[2]
            who = open(os.path.join(SCRATCH, 'trace.txt')).read().split()[-1] if os.path.isfile(os.path.join(SCRATCH, 'trace.txt')) else '?'
            hpd = rs[2]['pathdata_dice_edited.hpd']
            applied = re.search(rb'(\d+) of (\d+) link', rs[1])
            row = {'label': label, 'size': len(data), 'ops': len(ops), 'identical': same, 'answered_by': who,
                   'exit': [py[0], rs[0]], 'applied': applied.group(0).decode() if applied else None,
                   'hpd_sha1': hashlib.sha1(hpd).hexdigest() if hpd else None,
                   'changed': hpd != data if hpd else None,
                   'python_s': round(py[3], 3), 'rust_s': round(rs[3], 3),
                   'python_peak_mb': py[4], 'rust_peak_mb': rs[4]}
            if not same:
                bad += 1
                row['diff'] = [k for k in py[2] if py[2][k] != rs[2][k]] + (['stdout'] if py[1] != rs[1] else [])
                row['py_out'] = py[1].decode('latin-1')[-600:]
                row['rs_out'] = rs[1].decode('latin-1')[-600:]
            rows.append(row)
            print(json.dumps({k: v for k, v in row.items() if k not in ('py_out', 'rs_out')}), flush=True)
    finally:
        shutil.rmtree(SCRATCH, ignore_errors=True)
    json.dump(rows, open(OUT, 'w'), indent=1)
    print(f'{len(rows) - bad} of {len(rows)} identical')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
