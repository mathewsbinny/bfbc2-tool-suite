r"""Times dbxport.port on the WW2 level ports (rust/BENCHMARKS.md, "E4d"): each of BF1943's four level partitions
ported against a slice of BFBC2's level partitions as test_dbxport_ranking.py --game ports them (OPFOR's wake_level
ports against all 157), by the Rust core and by the Python reference, each in a fresh Templates (the ranking indexes
and the catalogue copy built inside the timing), the outputs compared.

  python rust\bench_dbxport.py [--part K/N] [--repeat R]

The game copy and BF1943's copy are read only; nothing is written.
"""
import hashlib
import os
import re
import sys
import time

RUST = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(RUST))
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'mesh-viewer', 'shader-tools', 'core')
import bf1943import as bi  # noqa: E402
import dbxcatalogue  # noqa: E402
import dbxport  # noqa: E402
import meshview  # noqa: E402
import shaderdb  # noqa: E402

LEVELS = ['levels/wake_island_s', 'levels/coral_sea', 'levels/guadal_canal', 'levels/iwo_jima_s']


def main(argv: list[str]) -> int:
    part = (0, 3)
    if '--part' in argv:
        part = tuple(map(int, argv[argv.index('--part') + 1].split('/')))   # type: ignore[assignment]
    repeat = int(argv[argv.index('--repeat') + 1]) if '--repeat' in argv else 1
    ix = meshview.Index(shaderdb.find_opfor(None))
    src = bi.Source(bi.default_source())
    cat = dbxcatalogue.load(ix)
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
    ported = {k[:-4] for k in src.entries if k.endswith('.dbx')}
    levels = sorted(n[:-4] for n in ix.where if re.fullmatch(r'levels/[^/]+\.dbx', n))[part[0]::part[1]]
    datas = {n: ix.read(n + '.dbx') for n in levels}
    print(f'{len(levels)} BFBC2 level partitions (slice {part[0]}/{part[1]})', flush=True)
    outs: dict[str, dict[str, str]] = {}
    for mode in ('python', 'rust') * repeat:
        dbxport.NATIVE_PORT = mode == 'rust'
        dbxport._NATIVE_CATS.clear()
        t = time.perf_counter()
        T = dbxport.Templates([(n, datas[n]) for n in levels], cat)
        tt = time.perf_counter() - t
        row = []
        for name in LEVELS:
            t = time.perf_counter()
            data, rep = dbxport.port(src.data(name + '.dbx'), T, lambda p: p.lower() in ported or p.lower() in bc2,
                                     drop_types=dbxport.NOT_IN_BFBC2, prune=True, required=dbxport.REQUIRED,
                                     catalogue=cat, name=name)
            row.append(time.perf_counter() - t)
            outs.setdefault(mode, {})[name] = hashlib.sha256(data + '\n'.join(rep).encode()).hexdigest()
        print(f'{mode:6}: Templates {tt:.2f} s; ports ' + ', '.join(f'{n.split("/")[1]} {s:.2f} s'
                                                                  for n, s in zip(LEVELS, row)) +
              f'; total {tt + sum(row):.2f} s', flush=True)
    same = outs['python'] == outs['rust']
    print(f'outputs (bytes and report) {"identical" if same else "DIFFERENT"} both ways', flush=True)
    return 0 if same else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
