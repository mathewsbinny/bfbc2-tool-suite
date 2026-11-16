r"""Times bfbc2core.dbx with the Python reader and with the Rust core, on the same files read into memory first
(rust\BENCHMARKS.md). The Rust-only timings (no Python objects) are rust\bc2-formats\examples\dbx_bench.rs.

  python rust\bench_dbx.py [--distinct] [--skip-python-xml] <folder> [<folder> ...]

Best of three runs (one run for the Python reader on more than 5,000 files); every result is kept, as a level load
keeps its trees. "1 by 1" calls parse / to_xml once per file; "many" passes every file at once (parse_many /
to_xml_many: the cycle collector paused; with the Rust core the files are read on all cores, then the Python trees
built in order). --stream drops each result at once (a check over a corpus; keeping 48,000 trees does not fit in
16 GB): "many" then takes 500 files at a time.
"""
import contextlib
import os
import sys
import time
from collections.abc import Callable, Sequence

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx  # noqa: E402


def files_in(folders: list[str], distinct: bool) -> list[bytes]:
    out, seen = [], set()
    for top in folders:
        for d, _, names in os.walk(top):
            for n in sorted(names):
                if n.lower().endswith('.dbx'):
                    data = open(os.path.join(d, n), 'rb').read()
                    if distinct and data in seen:
                        continue
                    seen.add(data)
                    out.append(data)
    return out


def best(fn: Callable[[], int], runs: int) -> tuple[float, int]:
    times, n = [], 0
    for _ in range(runs):
        t = time.perf_counter()
        n = fn()
        times.append(time.perf_counter() - t)
    return min(times), n


STREAM = False


def each(fn: Callable[[bytes], object], items: list[bytes]) -> int:
    """fn of each file, every result kept (as a level load keeps its trees) unless --stream."""
    kept = []
    for d in items:
        with contextlib.suppress(ValueError):
            r = fn(d)
            kept.append(None if STREAM else r)
    return len(kept)


def many(fn: Callable[[list[bytes]], Sequence[object]], items: list[bytes]) -> int:
    """fn of all files at once, or of 500 at a time with --stream."""
    if not STREAM:
        return ok(fn(items))
    return sum(ok(fn(items[i:i + 500])) for i in range(0, len(items), 500))


def ok(results: Sequence[object]) -> int:
    return sum(not isinstance(r, ValueError) for r in results)


def main(argv: list[str]) -> int:
    global STREAM
    STREAM = '--stream' in argv
    folders = [a for a in argv if not a.startswith('--')]
    items = files_in(folders, '--distinct' in argv)
    print(f'{len(items)} files, {sum(map(len, items)) / 1e6:.1f} MB, backend {dbx.backend()}')
    slow = 1 if len(items) > 5000 else 3
    rows = []
    if dbx.use_native(False) != 'python':
        return 1
    rows.append(('parse   Python, 1 by 1', *best(lambda: each(dbx.parse, items), slow)))
    rows.append(('parse   Python, many', *best(lambda: many(dbx.parse_many, items), slow)))
    if '--skip-python-xml' not in argv:
        rows.append(('to_xml  Python', *best(lambda: each(dbx.to_xml, items), slow)))
    if dbx.use_native(True) != 'rust':
        print('the Rust core is not built (python rust/build_native.py)')
        return 1
    rows.append(('parse   Rust, 1 by 1', *best(lambda: each(dbx.parse, items), 3)))
    rows.append(('parse   Rust, many (parallel)', *best(lambda: many(dbx.parse_many, items), 3)))
    rows.append(('to_xml  Rust, 1 by 1', *best(lambda: each(dbx.to_xml, items), 3)))
    rows.append(('to_xml  Rust, many (parallel)', *best(lambda: many(dbx.to_xml_many, items), 3)))
    for name, t, n in rows:
        print(f'{name:30s} {t:8.3f} s  ({n} files)')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
