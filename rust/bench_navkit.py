r"""Times one NavKit build (rust/BENCHMARKS.md, "The converter pipeline and NavKit"): navregress.build_level on a level's
in-game samples (Project OPFOR BFBC2's, read only), into a temporary folder.

  python rust\bench_navkit.py [--profile <out.prof>] [--top N] [level ...]
                                     default: sp_004_b; --profile: under cProfile, the stats saved and the N
                                     (default 25) functions with the most time of their own printed

navbuild / navwalk / navsample on the Rust core (bc2_navbuild): Level's grid walks, the floods, walkable's rules,
build_areas, the tracing, the roof rule, the sample records, build_graph whole for every sector at once, a stage's
areas and edge rows kept in the core, the floods' union-find, the unseeds. The Python backend (E2a's, E3c's and E3h's
twins, --backend python) was retired with them on 2026-11-18 (round R1; rust/BENCHMARKS.md "Retired Python").

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
    prof = opt('--profile')
    top = int(opt('--top', '25') or 25)
    levels = args or ['sp_004_b']
    navbuild.require_native()
    root = navkit.find_opfor(None)
    with tempfile.TemporaryDirectory() as tmp:
        for lv in levels:
            out = os.path.join(tmp, f'{lv}.hpd')
            t = time.perf_counter()
            if prof:
                p = cProfile.Profile()
                p.runcall(navregress.build_level, root, lv, out)
                p.dump_stats(prof)
            else:
                navregress.build_level(root, lv, out)
            dt = time.perf_counter() - t
            data = open(out, 'rb').read()
            print(f'{lv}: {dt:.1f} s, {len(data):,} bytes, sha1 {hashlib.sha1(data).hexdigest()}')
            if prof:
                pstats.Stats(prof).sort_stats('tottime').print_stats(top)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
