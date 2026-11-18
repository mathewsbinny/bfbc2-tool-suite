r"""Times the Xbox 360 shader translator (bfbc2-shader-tools xenos.py / xenos_hlsl.py) in Python and in the Rust core
(rust\bc2-shader, round E3f) on Battlefield 1943's Xbox shader databases (rust\BENCHMARKS.md, "E3f"). The
translation's Python (translate_python) was retired on 2026-11-18: its rows now time the Rust translator on the
Python decoder's trees (handed over attribute by attribute).

  python rust\bench_xenos.py [--db wake_island_s | --all] [--runs 3] [--chunk 200]

The programs of one level's database (default Wake Island; --all: every level's), read into memory first; each
translated with its own interpolators and inputs (the plain interface). The work runs a chunk of programs at a time
and drops each chunk's trees before the next (the whole corpus's Python trees held at once took ~4 GB), so the
process stays small; the times are the chunks' sums, best of --runs. Rows: decoding (Python; the Rust core's tree,
its microcode not built; parse_many on all cores; the Rust tree with its microcode built in Python objects),
translating a decoded program (a Python tree handed over; Rust one at a time; translate_many on all cores), and both
together.
"""
import argparse
import hashlib
import os
import sys
import time
from collections.abc import Callable
from typing import Any

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core', 'console-convert', 'bf1943-import')
import xenos  # noqa: E402
import xenos_hlsl as X  # noqa: E402

Prog = tuple[bytes, bool]


def programs(db: str | None) -> list[Prog]:
    """The levels' database programs, one bundle in memory at a time (test_xenos_native.bf1943_programs)."""
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests'))
    import test_xenos_native as T
    out: list[Prog] = []
    levels = [db] if db else sorted(os.listdir(os.path.join(__import__('bf1943import').default_source(), 'levels')))
    for lv in levels:
        out += [(b, p) for g, k, b, p in T.bf1943_programs(lv) if k.startswith('levels/')]
    return out


def plain(sh: Any) -> X.Interface:
    return X.Interface(sorted(it.usage_index for it in sh.header.interpolators),
                       vs_inputs={el.usage_index: 'float4' for el in getattr(sh.header, 'vertex_elements', [])})


def safe(fn: Callable[[Any, Any], str], sh: Any, iface: Any) -> str:
    try:
        return fn(sh, iface)
    except X.TranslateError as e:
        return f'TranslateError: {e}'


def digest(texts: list[str]) -> bytes:
    return hashlib.sha256('\0'.join(texts).encode()).digest()


def timed(progs: list[Prog], chunk: int, runs: int, prepare: Callable[[list[Prog]], Any],
          work: Callable[[Any], Any]) -> tuple[float, list[Any]]:
    """Best of runs of the sum over chunks of work(prepare(chunk)) (prepare untimed); work's results, chunk by
    chunk, from the last run (small values only: digests)."""
    best = float('inf')
    results: list[Any] = []
    for _ in range(runs):
        total, results = 0.0, []
        for k in range(0, len(progs), chunk):
            arg = prepare(progs[k:k + chunk])
            t = time.perf_counter()
            r = work(arg)
            total += time.perf_counter() - t
            results.append(r)
            del arg, r
        best = min(best, total)
    return best, results


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--db', default='wake_island_s')
    ap.add_argument('--all', action='store_true', help='every level\'s database')
    ap.add_argument('--runs', type=int, default=3)
    ap.add_argument('--chunk', type=int, default=200)
    a = ap.parse_args()
    progs = programs(None if a.all else a.db)
    n = len(progs)
    print(f'{n:,} programs ({sum(len(b) for b, _ in progs) / 1e6:.1f} MB), backend {xenos.backend()}, best of {a.runs}, '
          f'{a.chunk} at a time')
    ident = lambda c: c                                                       # noqa: E731
    rows = []
    t, _ = timed(progs, a.chunk, a.runs, ident, lambda c: [xenos.parse_shader_python(b, p) for b, p in c])
    rows.append(('decode: Python (parse_shader_python)', t, 'decode'))
    t, _ = timed(progs, a.chunk, a.runs, ident, lambda c: [xenos.parse_shader(b, p) for b, p in c])
    rows.append(('decode: Rust tree, microcode on first read (parse_shader)', t, 'decode'))
    t, _ = timed(progs, a.chunk, a.runs, ident, lambda c: xenos.parse_many(c))
    rows.append(('decode: parse_many (all cores)', t, 'decode'))
    t, _ = timed(progs, a.chunk, a.runs, ident,
                 lambda c: [xenos.parse_shader(b, p)._materialize() for b, p in c])
    rows.append(('decode: Rust tree with its microcode built (sh.instructions read)', t, 'decode'))

    def py_trees(c: list[Prog]) -> list[tuple[Any, Any]]:
        return [(sh, plain(sh)) for sh in (xenos.parse_shader_python(b, p) for b, p in c)]

    def rs_trees(c: list[Prog]) -> list[tuple[Any, Any]]:
        return [(sh, plain(sh)) for sh in (xenos.parse_shader(b, p) for b, p in c)]
    t, ref = timed(progs, a.chunk, a.runs, py_trees, lambda j: digest([safe(X.translate, s, i) for s, i in j]))
    rows.append(('translate: the Python decoder\'s tree, handed over (translate)', t, 'translate'))
    t, got = timed(progs, a.chunk, a.runs, rs_trees, lambda j: digest([safe(X.translate, s, i) for s, i in j]))
    rows.append(('translate: Rust, one at a time (translate)', t, 'translate'))
    t, got2 = timed(progs, a.chunk, a.runs, rs_trees, lambda j: digest(
        [x if isinstance(x, str) else f'TranslateError: {x}' for x in X.translate_many(j, errors='return')]))
    rows.append(('translate: translate_many (all cores)', t, 'translate'))
    t, _ = timed(progs, a.chunk, a.runs, ident, lambda c: digest(
        [safe(X.translate, sh, plain(sh)) for sh in (xenos.parse_shader_python(b, p) for b, p in c)]))
    rows.append(('decode + translate: Python decoder, its tree handed over', t, 'both'))
    t, _ = timed(progs, a.chunk, a.runs, ident, lambda c: digest(
        [safe(X.translate, sh, plain(sh)) for sh in (xenos.parse_shader(b, p) for b, p in c)]))
    rows.append(('decode + translate: Rust, one at a time', t, 'both'))
    t, _ = timed(progs, a.chunk, a.runs, ident, lambda c: X.translate_many(
        [(sh, plain(sh)) for sh in xenos.parse_many(c)], errors='return'))
    rows.append(('decode + translate: parse_many + translate_many', t, 'both'))
    base = {kind: next(t for _, t, k in rows if k == kind) for kind in ('decode', 'translate', 'both')}
    for name, t, kind in rows:
        print(f'| {name} | {t:.3f} | {1e3 * t / n:.3f} | {base[kind] / t:.1f}x |')
    same = ref == got == got2
    print(f'same text and errors on every program: {same}')
    return 0 if same else 1


if __name__ == '__main__':
    sys.exit(main())
