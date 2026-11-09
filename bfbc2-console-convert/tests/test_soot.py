"""Soot mesh conversion (soot.py) against BFBC2's own Xbox / PC twins, and on Battlefield 1943's soot meshes.

Needs the user's files: BFBC2's Xbox bundles unpacked by the console tools (OPFOR work\\xbox) and the PC game (OPFOR
game\\); Battlefield 1943 through the suite's bfbc2-bf1943-import. Skips what is missing.
"""
import glob
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402
suite_paths.use('console-convert', 'core', 'bf1943-import')
import soot  # noqa: E402
from bfbc2core import fbrb  # noqa: E402

OPFOR = os.path.join(SUITE, '..', 'project-opfor-bfbc2')


def soot_entries(pattern):
    out: dict[str, tuple[str, str]] = {}
    for p in glob.glob(pattern, recursive=True):
        try:
            entries = fbrb.entries(p)
        except Exception:                                        # noqa: BLE001 - not a bundle this reader takes
            continue
        for e in entries:
            if e.type == 'SootMesh':
                out.setdefault(e.name.lower(), (p, e.name))
    return out


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    X = soot_entries(os.path.join(OPFOR, 'work', 'xbox', '**', '*.fbrb'))
    P = soot_entries(os.path.join(OPFOR, 'game', '**', '*.fbrb'))
    both = sorted(set(X) & set(P))
    if not both:
        print('SKIP: no BFBC2 Xbox / PC soot mesh twins')
    else:
        same = 0
        for k in both:
            x = fbrb.read(X[k][0], [X[k][1]])[X[k][1]]
            p = fbrb.read(P[k][0], [P[k][1]])[P[k][1]]
            same += soot.soot_mesh(x) == p
        check(same == len(both), f'soot mesh: {same} of {len(both)} BFBC2 twins byte for byte')
    try:
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP Battlefield 1943: {e}')
        return 1 if fails else 0
    ks = [k for k, (_, e) in src.entries.items() if e.get('type') == 'SootMesh']
    done, full, refused = 0, 0, []
    for k in ks:
        x = src.data(k)
        try:
            soot.soot_mesh(x)
            done += 1
            full += len(x) > soot.EMPTY
        except soot.SootError as e:
            refused.append((k, str(e)))
    unproven = [k for k, e in refused if '16-byte vertices' in e and 'warehouse_t01' in k]
    check(done + len(unproven) == len(ks) and full > 0, f'Battlefield 1943: {done} of {len(ks)} soot meshes convert '
          f'({full} with geometry, {done - full} empty); {len(unproven)} refused - the warehouse\'s 16-byte vertices',
          str([r for r in refused if r[0] not in unproven][:3]))
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
