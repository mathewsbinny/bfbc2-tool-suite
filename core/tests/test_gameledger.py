r"""Round DF2: the game copy's install ledger (bfbc2core.gameledger, the twin of rust\bc2-formats\src\gameledger.rs)
and the installs built on it, on scratch game copies made in a temporary folder (never the real game copy).

- AU4-DS-1's sequence (install NavKit, apply stock-MP, restore NavKit, revert stock-MP) ends with DICE's file, and
  DICE's original is answered at every step; every order of the two tools applied and reverted, with and without an
  original, gives each file back byte for byte;
- an interruption at every write step: a failed write rolls back at once; a crash (a test hook, and a real kill of
  a process by BC2_LEDGER_CRASH_AT) leaves a journal the next operation rolls back, so the copy is as before and the
  operation can be run again;
- the markers of before the ledger: NavKit's and stock-MP's adopted, other tools' and inconsistent ones refused in
  words; Project OPFOR's records of other kinds kept;
- with bin\bfbc2.exe: the Rust store and the Python one read each other's records, and the real command lines -
  `bfbc2 navkit install / restore`, `bfbc2 stockmp install --apply / --revert` (and the Python twins mixed in) - run
  DS-1's sequence on a stand-in OPFOR folder; `bfbc2 stockmp install --apply` killed at every write step leaves the
  game copy as before, and the next run finishes it.

  python core\tests\test_gameledger.py
"""
from __future__ import annotations

import contextlib
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import gameledger as gl  # noqa: E402

HPD = 'Output/win32/ai/levels/mp_005/infantry/pathdata.hpd'
DICE, NAV, SMP = b"DICE's own path data", b"NavKit's build", b"stock-MP's build"
EXE = os.path.join(SUITE, 'bin', 'bfbc2.exe')


class Game:
    def __init__(self, root: str) -> None:
        self.root = root
        os.makedirs(root, exist_ok=True)
        self.put(HPD, DICE)

    def led(self) -> gl.Ledger:
        return gl.Ledger(self.root)

    def path(self, rel: str) -> str:
        return os.path.join(self.root, *rel.split('/'))

    def put(self, rel: str, data: bytes | None) -> None:
        p = self.path(rel)
        if data is None:
            if os.path.isfile(p):
                os.remove(p)
            return
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, 'wb') as f:
            f.write(data)

    def get(self, rel: str) -> bytes | None:
        p = self.path(rel)
        if not os.path.isfile(p):
            return None
        with open(p, 'rb') as f:
            return f.read()

    def files(self) -> dict[str, bytes]:
        out = {}
        for d, ds, fs in os.walk(self.root):
            if os.path.normcase(d) == os.path.normcase(os.path.join(self.root, 'opfor')):
                ds[:] = []
                continue
            for f in fs:
                p = os.path.join(d, f)
                with open(p, 'rb') as fh:
                    out[os.path.relpath(p, self.root).replace('\\', '/')] = fh.read()
        return out

    def records(self) -> dict[str, bytes]:
        root = gl.default_root(self.root)
        out = {}
        if os.path.isdir(root):
            for f in os.listdir(root):
                if f.endswith('.json') and not f.startswith('.'):
                    with open(os.path.join(root, f), 'rb') as fh:
                        out[f] = fh.read()
        return out

    def kill(self) -> None:
        with open(os.path.join(gl.default_root(self.root), '.lock'), 'w') as f:
            f.write('{"pid": 4000000000, "started": "2026-11-21T00:00:00Z"}')

    def original(self) -> bytes | None:
        p = self.led().original(HPD)
        if p is None:
            return None
        with open(p, 'rb') as f:
            return f.read()


def nav(g: Game) -> gl.Plan:
    return g.led().install('navkit:mp_005', {HPD: NAV})


def smp(g: Game) -> gl.Plan:
    return g.led().install('stockmp:mp_005', {HPD: SMP})


def raises(fn: Callable[[], Any]) -> str | None:
    try:
        fn()
    except gl.LedgerError as e:
        return str(e)
    return None


def python_twin(check: Callable[..., None], tmp: str) -> None:
    g = Game(os.path.join(tmp, 'ds1'))
    nav(g)
    smp(g)
    p = g.led().uninstall('navkit:mp_005')
    check(g.get(HPD) == SMP and g.original() == DICE and any('taken out from under stockmp:mp_005' in n for n in p.notes),
          'DS-1: NavKit restored under stock-MP - spliced out, the file stays stock-MP\'s, DICE\'s still the original')
    g.led().uninstall('stockmp:mp_005')
    check(g.get(HPD) == DICE and g.original() == DICE and not g.led().records(),
          'DS-1: stock-MP reverted after - DICE\'s file back, nothing left in the ledger')
    blobs = os.path.join(gl.default_root(g.root), 'blobs')
    check(not os.listdir(blobs), 'DS-1: blobs no record names deleted', str(os.listdir(blobs)))

    bad = []
    for first, second in (('navkit:mp_005', 'stockmp:mp_005'), ('stockmp:mp_005', 'navkit:mp_005')):
        for out in ((first, second), (second, first)):
            for none in (False, True):
                g = Game(os.path.join(tmp, f'order_{first[:3]}_{out[0][:3]}_{none}'))
                if none:
                    g.put(HPD, None)
                base = g.get(HPD)

                def data(o: str) -> bytes:
                    return NAV if o.startswith('navkit') else SMP
                stack = []
                for o in (first, second):
                    g.led().install(o, {HPD: data(o)})
                    stack.append(o)
                    if g.get(HPD) != data(o) or g.original() != base:
                        bad.append(f'{first}+{second} after installing {o}')
                for o in out:
                    g.led().uninstall(o)
                    stack.remove(o)
                    want = data(stack[-1]) if stack else base
                    if g.get(HPD) != want or g.original() != base:
                        bad.append(f'{first}+{second}, out {out}, after {o}')
    check(not bad, 'every order of two tools applied and reverted, with and without an original: each file as expected '
          'after each step', '; '.join(bad[:3]))

    g = Game(os.path.join(tmp, 'derived'))
    nav(g)
    g.led().install('cull:mp_005', {HPD: b'cull on navkit'}, derived=True)
    msg = raises(lambda: g.led().uninstall('navkit:mp_005'))
    check(msg is not None and "cull:mp_005 was built on navkit:mp_005's file: revert cull:mp_005 first" in msg
          and g.get(HPD) == b'cull on navkit', 'a layer built on another (derived) refuses that one leaving first', str(msg))
    g = Game(os.path.join(tmp, 'outside'))
    nav(g)
    g.put(HPD, b'someone else')
    m1, m2 = raises(lambda: smp(g)), raises(lambda: g.led().uninstall('navkit:mp_005'))
    check(m1 is not None and 'changed outside the ledger since navkit:mp_005 installed it' in m1 and m2 is not None
          and g.get(HPD) == b'someone else', 'a file changed outside the ledger: install over it and revert refused',
          f'{m1} | {m2}')
    g = Game(os.path.join(tmp, 'redo'))
    nav(g)
    g.led().install('navkit:mp_005', {HPD: b'v2'})
    smp(g)
    msg = raises(lambda: g.led().install('navkit:mp_005', {HPD: b'v3'}))
    check(g.original() == DICE and msg is not None and 'stockmp:mp_005 was installed after navkit:mp_005' in msg,
          'a redo keeps its prior; a redo under another install is refused', str(msg))


def interruptions(check: Callable[..., None], tmp: str) -> None:
    seq: list[Callable[[gl.Ledger], gl.Plan]] = [
        lambda led: led.install('navkit:mp_005', {HPD: NAV}),
        lambda led: led.install('stockmp:mp_005', {HPD: SMP}),
        lambda led: led.uninstall('navkit:mp_005'),
        lambda led: led.uninstall('stockmp:mp_005')]
    bad, hits = [], 0
    for crash in (False, True):
        for i in range(len(seq)):
            for k in range(1, 9):
                g = Game(os.path.join(tmp, f'int_{crash}_{i}_{k}'))
                g.put('Dist/other.fbrb', b'untouched')
                for s in seq[:i]:
                    s(g.led())
                files, recs = g.files(), g.records()
                led = g.led()
                n = [0]

                def hook(label: str, k: int = k, n: list[int] = n, crash: bool = crash) -> None:
                    n[0] += 1
                    if n[0] == k:
                        raise gl.Interrupt(crash=crash)
                led.hook = hook
                try:
                    seq[i](led)
                    shutil.rmtree(g.root)
                    continue                   # fewer write steps than k
                except gl.LedgerError:
                    pass
                hits += 1
                if crash:
                    g.kill()
                    g.led().recover()
                if g.files() != files or g.records() != recs:
                    bad.append(f'op {i} step {k} crash {crash}: not as before')
                for s in seq[i:]:
                    s(g.led())
                if g.get(HPD) != DICE or g.get('Dist/other.fbrb') != b'untouched':
                    bad.append(f'op {i} step {k} crash {crash}: the sequence did not end with DICE\'s file')
                shutil.rmtree(g.root)
    check(not bad and hits > 20, f'an interruption at every write step ({hits} cases, a failed write and a crash): the '
          'copy as before, the operation run again completes', '; '.join(bad[:3]))

    # a real kill: a child process stopped (exit 9) before its k-th write step by BC2_LEDGER_CRASH_AT
    bad, hits = [], 0
    code = ('import sys; sys.path.insert(0, sys.argv[1]); from bfbc2core import gameledger as gl; '
            "gl.Ledger(sys.argv[2]).install('stockmp:mp_005', {sys.argv[3]: b'stock-MP killed build', "
            "'Dist/win32/levels/mp_005/level-00.fbrb': b'new dist'})")
    for k in range(1, 9):
        g = Game(os.path.join(tmp, f'kill_{k}'))
        g.put('Dist/win32/levels/mp_005/level-00.fbrb', b'dice dist')
        nav(g)
        files, recs = g.files(), g.records()
        r = subprocess.run([sys.executable, '-c', code, os.path.join(SUITE, 'core'), g.root, HPD],
                           env=dict(os.environ, BC2_LEDGER_CRASH_AT=str(k)), capture_output=True, text=True)
        if r.returncode == 0:
            shutil.rmtree(g.root)
            continue
        hits += 1
        if r.returncode != 9:
            bad.append(f'step {k}: exit {r.returncode} {r.stderr[-200:]}')
            continue
        notes = smp(g).notes                    # the next operation rolls the killed one back first
        if not any('stale lock' in n for n in notes):
            bad.append(f'step {k}: the stale lock not cleared ({notes})')
        g.led().uninstall('stockmp:mp_005')
        if g.files() != files or g.records() != recs:
            bad.append(f'step {k}: not as before after the next install and its revert')
        shutil.rmtree(g.root)
    check(not bad and hits >= 5, f'a process killed at every write step ({hits} kills): the next operation rolls it back',
          '; '.join(bad[:3]))


def legacy(check: Callable[..., None], tmp: str) -> None:
    g = Game(os.path.join(tmp, 'legacy_nav'))
    g.put(HPD, NAV)
    g.put(HPD + '.opfor-orig', DICE)
    p = gl.adopt_legacy(g.led(), 'navkit:mp_005', [HPD], 'navkit')
    check(p is not None and not gl.legacy_markers(g.path(HPD)) and g.original() == DICE,
          "legacy: NavKit's .opfor-orig adopted (DICE's file kept in the ledger), the marker removed")
    g.led().uninstall('navkit:mp_005')
    check(g.get(HPD) == DICE, 'legacy: the adopted install reverts to DICE\'s file')

    g = Game(os.path.join(tmp, 'legacy_smp'))
    g.put(HPD, SMP)
    g.put(HPD + '.opfor-stockmp-orig', DICE)
    g.put(HPD + '.opfor-stockmp-applied', hashlib.sha256(SMP).hexdigest().encode())
    gl.adopt_legacy(g.led(), 'stockmp:mp_005', [HPD], 'stockmp')
    g.led().uninstall('stockmp:mp_005')
    check(g.get(HPD) == DICE and not gl.legacy_markers(g.path(HPD)), "legacy: stock-MP's markers adopted and reverted")

    cases = [('foreign', {'.opfor-cull-orig': DICE}, 'navkit', 'migrate them first'),
             ('changed', {'.opfor-stockmp-orig': DICE, '.opfor-stockmp-applied': b'0' * 64}, 'stockmp',
              "changed after stock-MP's install"),
             ('backup without record', {'.opfor-stockmp-orig': DICE}, 'stockmp', 'with no install record'),
             ('interrupted', {'.opfor-stockmp-prev': DICE}, 'stockmp', 'was interrupted before the install ledger')]
    bad = []
    for name, marks, fam, want in cases:
        g = Game(os.path.join(tmp, 'legacy_' + name.replace(' ', '_')))
        g.put(HPD, SMP)
        for m, d in marks.items():
            g.put(HPD + m, d)
        before = g.files()
        def adopt(g: Game = g, fam: str = fam) -> gl.Plan | None:
            return gl.adopt_legacy(g.led(), f'{fam}:mp_005', [HPD], fam)
        msg = raises(adopt)
        if msg is None or want not in msg or g.files() != before:
            bad.append(f'{name}: {msg}')
    check(not bad, 'legacy: another tool\'s markers and inconsistent ones refused in words, nothing changed',
          '; '.join(bad))

    # Project OPFOR's record of other kinds: kept, its blob kept, a whole-file install over its slots refused
    g = Game(os.path.join(tmp, 'opfor_rec'))
    root = gl.default_root(g.root)
    os.makedirs(os.path.join(root, 'blobs'))
    blob = hashlib.sha256(b'old entry').hexdigest()
    with open(os.path.join(root, 'blobs', blob + '.bin'), 'wb') as f:
        f.write(b'old entry')
    rec = {'format': gl.FORMAT, 'owner': 'pacific', 'txn': 't', 'installed': 'i', 'note': '', 'contributions': [
        {'kind': 'mft_block', 'file': 'Dist/win32/package.mft', 'slot': 'levels/x', 'value': 'b', 'prior': None,
         'below': None, 'stack': True, 'extra': {}},
        {'kind': 'bundle_entry', 'file': 'Package/async/startup-00.fbrb', 'slot': 'e', 'value': {'sha': 'x'},
         'prior': {'sha': blob, 'blob': blob}, 'below': None, 'stack': False, 'extra': {'name': 'E'}}]}
    text = json.dumps(rec, indent=1) + '\n'
    with open(os.path.join(root, 'pacific.json'), 'w', newline='\n') as ft:
        ft.write(text)
    nav(g)
    g.led().uninstall('navkit:mp_005')
    with open(os.path.join(root, 'pacific.json'), newline='\n') as fr:
        kept = fr.read() == text
    msg = raises(lambda: g.led().install('navkit:x', {'Package/async/startup-00.fbrb': b'whole'}))
    check(kept and os.path.isfile(os.path.join(root, 'blobs', blob + '.bin')) and msg is not None
          and 'pacific holds parts of it' in msg,
          "Project OPFOR's records of other kinds kept byte for byte with their blobs; a whole-file install over their "
          'slots refused', str(msg))


# ---------------------------------------------------------------------------------------------- with bfbc2.exe

def rust(args: list[str], env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run([EXE, *args], capture_output=True, text=True, env=dict(os.environ, **(env or {})))


def stand_in(tmp: str) -> tuple[str, dict[str, bytes]]:
    """A stand-in OPFOR folder: a game copy with mp_005 (DICE's path data, Dist and Package bundles), stock-MP's
    build of it (cli_o3_stockmp's made-up outputs and report) and a NavKit build file."""
    sys.path.insert(0, HERE)
    import cli_o3_stockmp
    root = os.path.join(tmp, 'opfor')
    files = cli_o3_stockmp.built('mp_005')
    dice = {'game\\BFBC2Game.exe': b'', 'game\\Dist\\win32\\levels\\mp_005\\level-00.fbrb': b'dice dist',
            'game\\Package\\levels\\mp_005\\level-00.fbrb': b'dice pkg', 'game\\' + HPD.replace('/', '\\'): DICE,
            'nav.hpd': NAV}
    for k, v in {**{k.replace('opfor\\', '', 1): v for k, v in files.items()}, **dice}.items():
        p = os.path.join(root, k if k.startswith(('game', 'nav')) else k.replace('work\\', 'work\\stockmp\\', 1))
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, 'wb') as f:
            f.write(v if isinstance(v, bytes) else v.encode())
    built = {k.split('out\\game\\', 1)[1].replace('\\', '/'): v for k, v in files.items() if 'out\\game\\' in k}
    return root, built


def cross(check: Callable[..., None], tmp: str) -> None:
    if not os.path.isfile(EXE):
        print('skipped: the Rust side (bin\\bfbc2.exe is not built: python rust\\build_native.py)')
        return
    g = Game(os.path.join(tmp, 'cross'))
    nav(g)
    smp(g)
    h = rust(['gameledger', 'holders', g.root, HPD])
    o = rust(['gameledger', 'original', g.root, HPD])
    with open(o.stdout.strip() or os.devnull, 'rb') as f:
        orig = f.read()
    check(h.stdout.split() == ['navkit:mp_005', 'stockmp:mp_005'] and orig == DICE,
          'the Rust store reads the Python one (holders bottom first, DICE\'s original)', h.stdout + h.stderr + o.stderr)

    root, built = stand_in(tmp)
    game = os.path.join(root, 'game')
    rg = Game.__new__(Game)
    rg.root = game
    before = rg.files()
    smp_args = ['stockmp', 'install', 'mp_005', '--opfor', root, '--work', os.path.join(root, 'work', 'stockmp')]
    steps = [
        ('Rust navkit install', lambda: rust(['navkit', 'install', 'mp_005', '--from', os.path.join(root, 'nav.hpd'),
                                              '--opfor', root])),
        ('Rust stockmp --apply', lambda: rust([*smp_args, '--apply'])),
        ('Rust navkit restore', lambda: rust(['navkit', 'restore', 'mp_005', '--opfor', root])),
        ('Rust stockmp --revert', lambda: rust([*smp_args, '--revert']))]
    bad = []
    want_hpd = [NAV, built[HPD], built[HPD], DICE]
    for (name, fn), want in zip(steps, want_hpd, strict=True):
        r = fn()
        if r.returncode != 0 or rg.get(HPD) != want or rg.original() != DICE:
            bad.append(f'{name}: exit {r.returncode}, pathdata {(rg.get(HPD) or b'')[:20]!r} {r.stdout[-300:]} {r.stderr[-300:]}')
    check(not bad and rg.files() == before,
          "DS-1 through the real command lines (bfbc2 navkit install, stockmp --apply, navkit restore, stockmp "
          "--revert): DICE's file at the end, every file of the game copy byte for byte as before", '; '.join(bad[:2]))

    # the Python twins mixed in: Rust NavKit, the Python stock-MP install, Rust restore, the Python revert
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-stockmp-ai'))
    import pipeline
    import stockmplib
    paths = pipeline.Paths('mp_005', opfor=root, work=os.path.join(root, 'work', 'stockmp'))
    out = io.StringIO()
    r1 = steps[0][1]()
    with contextlib.redirect_stdout(out):
        c2 = stockmplib.install(paths, ['--apply'])
    r3 = steps[2][1]()
    with contextlib.redirect_stdout(out):
        c4 = stockmplib.install(paths, ['--revert'])
    check((r1.returncode, c2, r3.returncode, c4) == (0, 0, 0, 0) and rg.files() == before
          and 'taken out from under stockmp:mp_005' in r3.stdout,
          'DS-1 mixing the implementations (Rust NavKit, Python stock-MP): each reads the other\'s records, DICE\'s file '
          'at the end', f'{r1.returncode} {c2} {r3.stdout[-200:]} {c4} {out.getvalue()[-300:]}')

    # bfbc2 stockmp install --apply killed before each write step: the game copy as before, the next run finishes
    bad, hits = [], 0
    for k in range(1, 12):
        r = rust([*smp_args, '--apply'], {'BC2_LEDGER_CRASH_AT': str(k)})
        if r.returncode == 0:
            rust([*smp_args, '--revert'])
            break
        hits += 1
        if r.returncode != 9:
            bad.append(f'step {k}: exit {r.returncode} {r.stderr[-200:]}')
            break
        rc = rust(['gameledger', 'recover', game])
        if rg.files() != before or 'rolled back' not in rc.stdout and k > 1:
            bad.append(f'step {k}: not as before after recover ({rc.stdout.strip()[-200:]})')
        r2 = rust([*smp_args, '--apply'])
        if r2.returncode != 0 or rg.get(HPD) != built[HPD]:
            bad.append(f'step {k}: the next --apply did not finish ({r2.stderr[-200:]})')
        rust([*smp_args, '--revert'])
        if rg.files() != before:
            bad.append(f'step {k}: the revert after it did not give the copy back')
    check(not bad and hits >= 4, f'bfbc2 stockmp install --apply killed at each write step ({hits} kills): recovered to '
          'as before, the next run installs, its revert gives the copy back', '; '.join(bad[:3]))
    r = rust(['gameledger', 'status', game])
    check(r.returncode == 0 and 'nothing installed' in r.stdout, 'gameledger status: nothing left installed', r.stdout)


def main(argv: list[str]) -> int:
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    tmp = tempfile.mkdtemp(prefix='gameledger_')
    try:
        python_twin(check, tmp)
        interruptions(check, tmp)
        legacy(check, tmp)
        cross(check, tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print(f'\n{"ALL PASSED" if not failed else f"{failed} FAILED"}')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
