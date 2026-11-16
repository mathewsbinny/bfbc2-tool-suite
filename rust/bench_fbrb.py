r"""Times bfbc2core.fbrb with the Python reader and with the Rust core (rust\BENCHMARKS.md). The Rust-only timings
are rust\bc2-formats\examples\fbrb_bench.rs.

  python rust\bench_fbrb.py --entries <folder> [<folder> ...]    names() of every bundle (the header only)
  python rust\bench_fbrb.py --extract <folder> [<folder> ...]    read() of every entry of every bundle

Best of three runs (the files from the system cache after the first). "1 by 1" calls names / read once per bundle;
"many" passes every bundle to read_many (with the Rust core one bundle per core).
"""
import functools
import os
import sys
import time
from collections.abc import Callable

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import fbrb  # noqa: E402


def bundles(folders: list[str]) -> list[str]:
    out = []
    for top in folders:
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in sorted(names) if n.lower().endswith('.fbrb')]
    return out


def best(fn: Callable[[], object], runs: int = 3) -> float:
    t = float('inf')
    for _ in range(runs):
        s = time.perf_counter()
        fn()
        t = min(t, time.perf_counter() - s)
    return t


def read_each(wanted: list[tuple[str, list[str]]]) -> list[dict[str, bytes]]:
    return [fbrb.read(f, w) for f, w in wanted]


def main(argv: list[str]) -> int:
    files = bundles([a for a in argv[1:] if not a.startswith('--')])
    size = sum(os.path.getsize(f) for f in files)
    print(f'{len(files)} bundles, {size / 1e6:.1f} MB on disk')
    for backend in (True, False):
        fbrb.use_native(backend)
        name = fbrb.backend()
        if '--entries' in argv:
            t = best(lambda: [fbrb.names(f) for f in files])
            print(f'{name:6}  names, 1 by 1      : {t:7.3f} s')
        if '--extract' in argv:
            wanted: list[tuple[str, list[str]]] = [(f, list(fbrb.names(f))) for f in files]
            n = sum(len(w) for _, w in wanted)
            t = best(functools.partial(read_each, wanted))
            print(f'{name:6}  read every entry, 1 by 1 : {t:7.3f} s  ({n} entries)')
            t = best(functools.partial(fbrb.read_many, wanted))
            print(f'{name:6}  read_many every entry   : {t:7.3f} s')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
