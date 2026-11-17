r"""Times one NavKit build (rust/BENCHMARKS.md, "The converter pipeline and NavKit"): navregress.build_level on a level's
in-game samples (Project OPFOR BFBC2's, read only), on one backend, into a temporary folder.

  python rust\bench_navkit.py [--backend rust|python] [--profile <out.prof>] [--top N] [level ...]
                                     default: rust, sp_004_b; --profile: under cProfile, the stats saved and the
                                     N (default 25) functions with the most time of their own printed

  rust     navbuild / navwalk on the Rust core (bc2_navbuild): Level's grid walks, the floods, walkable's rules,
           build_areas and build_graph's cell picking
  python   walkable's rules, build_areas and the cell picking on their Python reference (navbuild.use_native(False));
           the grid walks and floods run in Rust either way (their Python was retired on 2026-11-17)

Prints the time and the sha1 of the path data. The files carry each sector's build time, so two builds' sha1 differ;
tests\test_navbuild_native.py --game compares a build's file with the recorded one (the clock fixed).
"""
import cProfile
import hashlib
import os
import pstats
import sys
import tempfile
import time

RUST = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(RUST)
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit', 'navkit-tools')
import navbuild  # noqa: E402
import navkit  # noqa: E402
import navregress  # noqa: E402


def main(argv: list[str]) -> int:
    args = argv[1:]

    def opt(k: str, d: str | None = None) -> str | None:
        if k not in args:
            return d
        i = args.index(k)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    backend = opt('--backend', 'rust') or 'rust'
    prof = opt('--profile')
    top = int(opt('--top', '25') or 25)
    levels = args or ['sp_004_b']
    if navbuild.use_native(backend == 'rust') != backend:
        print(f'the {backend} backend is not available (python rust\\build_native.py)')
        return 1
    root = navkit.find_opfor(None)
    with tempfile.TemporaryDirectory() as tmp:
        for lv in levels:
            out = os.path.join(tmp, f'{lv}_{backend}.hpd')
            t = time.perf_counter()
            if prof:
                p = cProfile.Profile()
                p.runcall(navregress.build_level, root, lv, out)
                p.dump_stats(prof)
            else:
                navregress.build_level(root, lv, out)
            dt = time.perf_counter() - t
            data = open(out, 'rb').read()
            print(f'{lv} ({backend}): {dt:.1f} s, {len(data):,} bytes, sha1 {hashlib.sha1(data).hexdigest()}')
            if prof:
                pstats.Stats(prof).sort_stats('tottime').print_stats(top)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
