r"""Round F2b (rust/BENCHMARKS.md, "F2b"): bfbc2core.navtune's validate, side-file text and summary in the Rust core
against the Python (validate_reference, NavTune.save_text_reference, summary_reference) - the same problems, the
same bytes, the same lines.

  python test_navtune_native.py          quick: 20,000 seeded operations (every operation, fields missing or of
                                         every JSON type, polygons and boxes right and wrong, NaN, huge ints,
                                         strings with escapes and non-ASCII text) and 2,000 seeded side files
  python test_navtune_native.py --game   + every side file under Project OPFOR's work\navkit (read only; none
                                         exist yet on the user's PC - they are made by the map editor's AI workspace)

The module is required (the test fails without it, saying how to build it).
"""
import glob
import json
import math
import os
import random
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
from bfbc2core import navtune  # noqa: E402

FAILS: list[str] = []


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def outcome(f, *a):
    try:
        return ('ok', f(*a))
    except Exception as e:                                       # noqa: BLE001 - compared: both must agree
        return ('err', type(e).__name__, str(e))


def rvalue(rnd, depth=0):
    r = rnd.random()
    if r < 0.15:
        return rnd.uniform(-1e3, 1e3)
    if r < 0.25:
        return rnd.choice([0, 1, -7, 10 ** 20, True, False, None, math.nan, math.inf, 1e-7, 1e17, 0.1 + 0.2])
    if r < 0.4:
        return rnd.choice(['', ' ', '\t\n', 'x', 'a"b\\c', 'caf\u00e9', '\u001c', 'ladder', 'add', 'it\'s'])
    if r < 0.7 and depth < 2:
        return [rvalue(rnd, depth + 1) for _ in range(rnd.choice([0, 2, 3, 3, 6]))]
    if depth < 2:
        return {rnd.choice(['polygon', 'box', 'y', 'k']): rvalue(rnd, depth + 1) for _ in range(rnd.randint(0, 3))}
    return rnd.random()


def rop(rnd):
    name = rnd.choice(list(navtune.OPS) + ['fly', None, 3, ['x']])
    o = {'op': name} if rnd.random() < 0.97 else {}
    for k in navtune.OPS.get(name, ()) if isinstance(name, str) else ():
        if rnd.random() < 0.9:
            o[k] = None
    for k in list(o):
        if k in ('at', 'a', 'b', 'bottom', 'top'):
            o[k] = [rnd.uniform(-500, 500) for _ in range(3)] if rnd.random() < 0.8 else rvalue(rnd)
        elif k == 'region':
            if rnd.random() < 0.4:
                o[k] = navtune.polygon([(rnd.uniform(-9, 9), rnd.uniform(-9, 9)) for _ in range(rnd.randint(2, 5))],
                                       rnd.uniform(-5, 5), rnd.uniform(-5, 5))
            elif rnd.random() < 0.7:
                o[k] = navtune.box(*[rnd.uniform(-9, 9) for _ in range(6)])
                if rnd.random() < 0.3:
                    o[k]['box'][rnd.randrange(6)] = rvalue(rnd)
            else:
                o[k] = rvalue(rnd)
        elif k == 'action':
            o[k] = rnd.choice(['add', 'remove', 'move', 1])
        elif k == 'kind':
            o[k] = rnd.choice(list(navtune.TRAVERSALS) + ['swim'])
        elif k == 'text':
            o[k] = rnd.choice(['', '  ', 'a note', '\u001f', 'caf\u00e9', 5])
    if rnd.random() < 0.2:
        o['two-way'] = rnd.choice([True, False, 1, 'yes'])
    if rnd.random() < 0.1:
        o[rnd.choice(['extra', 'at', 'region'])] = rvalue(rnd)
    o['by'] = rnd.choice(['PC', 'me', 7, None])
    o['when'] = '2026-11-18T10:00:00'
    return o


def quick():
    rnd = random.Random(53)
    bad: list[Any] = []
    n = 0
    for _ in range(20000):
        o = rop(rnd)
        for kind in ('navkit', 'dice'):
            n += 1
            a, b = outcome(navtune.validate, o, kind), outcome(navtune.validate_reference, o, kind)
            if a != b:
                bad.append((o, kind, a, b))
    check(not bad, f'validate: {n} seeded checks the same', repr(bad[:1])[:400])
    bad = []
    files, lines = 0, 0
    for i in range(2000):
        ops = [rop(rnd) for _ in range(rnd.randint(0, 6))]
        t = navtune.NavTune(rnd.choice(['Levels/SP_01', 'Levels/MP_005', 'caf\u00e9']),
                            {'kind': rnd.choice(navtune.KINDS), 'sha1': 'ab' * 20, 'file': 'x.navdata'}, ops)
        files += 1
        a, b = outcome(t.save_text), outcome(t.save_text_reference)
        if a != b:
            bad.append(('save', i, a, b))
        for k in range(len(ops)):
            lines += 1
            a, b = outcome(t.summary, k), outcome(t.summary_reference, k)
            if a != b:
                bad.append(('summary', ops[k], a, b))
    check(not bad, f'{files} seeded side files and {lines} summaries the same', repr(bad[:1])[:400])


def game():
    root = os.environ.get('OPFOR_ROOT') or r'C:\Programming\Projects\project-opfor-bfbc2'
    files = sorted(glob.glob(os.path.join(root, 'work', 'navkit', '*', '*.navtune.json')))
    if not files:
        print('     no side files under work\\navkit (none made yet)')
        return
    bad = []
    for f in files:
        t = navtune.load(f)
        if t.save_text() != t.save_text_reference() or open(f, encoding='utf-8').read() != t.save_text():
            bad.append(f)
        for k in range(len(t.operations)):
            if t.summary(k) != t.summary_reference(k):
                bad.append(f'{f} {k}')
        json.dumps(t.to_json())
    check(not bad, f'{len(files)} side files of work\\navkit: text and summaries the same', '; '.join(bad[:3]))


def main(argv):
    from bfbc2core import native
    try:
        native.require('navtune (round F2b)', 'navtune_validate')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    quick()
    if '--game' in argv:
        game()
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
