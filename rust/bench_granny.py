r"""Times bfbc2-anim-tools/granny.py with its Python code and with the Rust core (rust\BENCHMARKS.md, "E3i").

  python rust\bench_granny.py [--opfor <Project OPFOR BFBC2 folder>] [--runs N] [--reference <granny.py>]

The corpus: every Granny resource (GrannyAnimation, DeltaAnimation, GrannyModel) of the Xbox mp_common / sp_common
that the console readers unpacked into OPFOR's work\xbox (read only). Timed, best of N runs (default 3), the files
in memory:

  Granny(b)            the tables only (DICE's table, the strings, the sections and relocations)
  Granny(b).root()     the object tree (what bone_map, animations and the tests read): each tree dropped before the
                       next is read, or all of them kept (then the cycle collector's passes between the calls walk
                       every tree still alive, and its time dominates both columns)
  roots(blobs)         the same trees, read on all cores, then made into Python objects
  to_pc(b)             each Xbox resource as the PC's, one call at a time
  to_pc_many(blobs)    all of them in one call (all cores)

The Python column is granny.py with the Rust core off (its Python reader; to_pc and to_pc_many run in the Rust core
either way since their Python was retired on 2026-11-18), or --reference: another granny.py (the retired Python
conversion: git show ee25186:bfbc2-anim-tools/granny.py > old.py); one without roots / to_pc_many runs root / to_pc
one by one there.
"""
import os
import sys
import time
from collections.abc import Callable

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'anim-tools')
import granny  # noqa: E402
from bfbc2core import paths  # noqa: E402

KINDS = ('GrannyAnimation', 'DeltaAnimation', 'GrannyModel')


def corpus(root: str) -> list[bytes]:
    out = []
    for lv in ('mp_common', 'sp_common'):
        xdir = os.path.join(root, 'work', 'xbox', lv)
        man = os.path.join(xdir, 'fbrb.manifest')
        if not os.path.isfile(man):
            continue
        for line in open(man, encoding='utf-8', errors='replace'):
            f = line.rstrip('\n').split('|')
            if len(f) > 1 and f[1] in KINDS:
                p = os.path.join(xdir, *f[0].split('/'))
                if os.path.isfile(p):
                    out.append(open(p, 'rb').read())
    return out


def best(fn: Callable[[], object], runs: int) -> float:
    t = float('inf')
    for _ in range(runs):
        s = time.perf_counter()
        fn()
        t = min(t, time.perf_counter() - s)
    return t


def main(argv: list[str]) -> int:
    root = argv[argv.index('--opfor') + 1] if '--opfor' in argv else paths.find_opfor(walk=False, suite=SUITE)
    runs = int(argv[argv.index('--runs') + 1]) if '--runs' in argv else 3
    blobs = corpus(root)
    if not blobs:
        print('no Xbox Granny resources (work\\xbox\\mp_common, sp_common)')
        return 1
    native = granny._import_native()
    print(f'{len(blobs)} Xbox Granny resources, {sum(map(len, blobs)) / 1e6:.1f} MB; Rust core: '
          f'{"yes" if native else "no"}')
    pcs = [granny.to_pc(b) for b in blobs]
    ref = granny
    if '--reference' in argv:
        import importlib.util
        spec = importlib.util.spec_from_file_location('granny_reference', argv[argv.index('--reference') + 1])
        if spec is None or spec.loader is None:
            print('--reference: not a Python file')
            return 1
        ref = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ref)

    def rows(m):
        return [
            ('Granny(b), Xbox', lambda: [m.Granny(b) for b in blobs]),
            ('Granny(b).root(), Xbox, each dropped', lambda: [len(m.Granny(b).root()) for b in blobs]),
            ('Granny(b).root(), Xbox, all kept', lambda: [m.Granny(b).root() for b in blobs]),
            ('Granny(b).root(), PC (converted)', lambda: [m.Granny(b).root() for b in pcs]),
            ('roots(blobs), Xbox', (lambda: m.roots(blobs)) if hasattr(m, 'roots')
             else lambda: [m.Granny(b).root() for b in blobs]),
            ('to_pc(b)', lambda: [m.to_pc(b) for b in blobs]),
            ('to_pc_many(blobs)', (lambda: m.to_pc_many(blobs)) if hasattr(m, 'to_pc_many')
             else lambda: [m.to_pc(b) for b in blobs]),
        ]
    print(f'{"":36s} {"Python s":>10s} {"Rust s":>10s} {"x":>6s}')
    for (name, py_fn), (_, rs_fn) in zip(rows(ref), rows(granny)):
        granny._native = None
        py = best(py_fn, runs)
        granny._native = native
        rs = best(rs_fn, runs) if native else float('nan')
        print(f'{name:36s} {py:10.2f} {rs:10.3f} {py / rs:6.1f}', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
