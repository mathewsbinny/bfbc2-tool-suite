r"""Times the menu tools' APT model (bfbc2-menu-tools aptgraft.py), aptcheck and aptrefs on the 133-movie corpus of
rounds E3n / E4c (rust\BENCHMARKS.md "E4c"): every distinct SwfMovie of the game copy (Dist and Package), OPFOR's
pristine menu bundle (original\) and the BIGF movies of work\xbox_dlc - read only, into memory first.

  python rust\bench_apt.py [--model native|python|both] [--only load,writes,...] [--runs 1] [--profile WORKLOAD]

Workloads:
  load       aptgraft.Movie(members) of every movie (the parse)
  writes     every write of each unchanged model: keep, pack (gap 16), the other byte order, dead=False
  roundtrip  aptgraft.roundtrip(<movie>.res, pack=True) of every movie (files in a temp folder), one by one
  rtmain     aptgraft roundtrip --pack --quiet <the temp folder> (the command: all movies, batched with the Rust model)
  graft      two sprites of each movie grafted into the next movie of its byte order, placed, a label alias, written
             three ways (test_apt_e3n.graft_all)
  check      aptcheck's rules on every movie (aptcheck.check_many: the Rust scan, or Code + findings)
  refs       aptrefs' movie_refs of every movie (aptrefs.refs_many)
  xref       aptrefs xref over every movie set and the exe's table (written to a temp folder)
--model python runs aptgraft's Python model (aptgraft.use_native(False)) and aptcheck / aptrefs' Python over apt.py's
records; native the Rust model and scans (the default); both runs each workload twice. --profile prints cProfile's
top 30 (cumulative) of one workload instead.
"""
import argparse
import contextlib
import cProfile
import io
import os
import pstats
import shutil
import sys
import tempfile
import time
from collections.abc import Callable
from typing import Any

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('menu-tools', 'apt', 'core')
sys.path.insert(0, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests'))
import aptcheck  # noqa: E402
import aptgraft as ag  # noqa: E402
import aptrefs  # noqa: E402
import menucommon as mc  # noqa: E402
import test_apt_e3n as T  # noqa: E402

Members = list[tuple[str, bytes]]


def corpus() -> list[tuple[str, Members]]:
    out = []
    for where, data in T.game_movies():
        try:
            mem = ag.read_bigf_bytes(data, where)
            ag.Movie(mem, where)
        except ag.AptError:
            continue
        out.append((where, mem))
    return out


def set_model(native: bool) -> None:
    ag.use_native(native)
    aptcheck.use_native(native)
    aptrefs.use_native(native)


def w_load(movies: list[tuple[str, Members]], tmp: str) -> str:
    n = 0
    for where, mem in movies:
        ag.Movie(mem, where)
        n += 1
    return f'{n} movies'


def w_writes(movies: list[tuple[str, Members]], tmp: str) -> str:
    n = 0
    for where, mem in movies:
        m = ag.Movie(mem, where)
        other = '<' if m.big else '>'
        for kw in ({}, {'layout': 'pack', 'gap': 16}, {'order': other}, {'dead': False}):
            m.write(**kw)
            n += 1
    return f'{n} writes'


def _files(movies: list[tuple[str, Members]], tmp: str) -> list[str]:
    d = os.path.join(tmp, 'rt')
    os.makedirs(d, exist_ok=True)
    out = []
    for k, (_, mem) in enumerate(movies):
        p = os.path.join(d, f'{k:03d}.res')
        if not os.path.isfile(p):
            with open(p, 'wb') as f:
                f.write(ag.write_bigf(mem))
        out.append(p)
    return out


def w_roundtrip(movies: list[tuple[str, Members]], tmp: str) -> str:
    bad = 0
    for p in _files(movies, tmp):
        probs, _ = ag.roundtrip(p, pack=True)
        bad += bool(probs)
    return f'{len(movies)} movies, {bad} with problems'


def w_rtmain(movies: list[tuple[str, Members]], tmp: str) -> str:
    _files(movies, tmp)
    with contextlib.redirect_stdout(io.StringIO()):
        code = ag.main(['aptgraft.py', 'roundtrip', os.path.join(tmp, 'rt'), '--pack', '--quiet'])
    return f'exit {code}'


def w_graft(movies: list[tuple[str, Members]], tmp: str) -> str:
    n = 0
    bigs = [ag.Movie(mem).big for _, mem in movies]
    for k, (_, mem) in enumerate(movies):
        nxt = next((m for j, (_, m) in enumerate(movies[k + 1:] + movies[:k]) if bigs[(k + 1 + j) % len(movies)] == bigs[k]),
                   None)
        if nxt is not None:
            try:
                T.graft_all(mem, nxt, 2)
                n += 1
            except ag.AptError:
                pass
    return f'{n} grafts'


def _apt_movies(movies: list[tuple[str, Members]]) -> list[Any]:
    return [mc.movie_from_members(dict(mem), where) for where, mem in movies]


def w_check(movies: list[tuple[str, Members]], tmp: str) -> str:
    res = aptcheck.check_many(_apt_movies(movies))
    return f'{len(res)} movies, {sum(bool(r[3]) for r in res)} flagged'


def w_refs(movies: list[tuple[str, Members]], tmp: str) -> str:
    res = aptrefs.refs_many(_apt_movies(movies))
    return f'{len(res)} movies, {sum(len(r["extern"]) for r in res)} extern names'


def w_xref(movies: list[tuple[str, Members]], tmp: str) -> str:
    d = os.path.join(tmp, 'xref')
    os.makedirs(d, exist_ok=True)
    with contextlib.redirect_stdout(io.StringIO()):
        code = aptrefs.main(['aptrefs.py', 'xref', '--out', os.path.join(d, 'x.txt'), '--json', os.path.join(d, 'x.json')])
    return f'exit {code}'


WORK: dict[str, Callable[[list[tuple[str, Members]], str], str]] = {
    'load': w_load, 'writes': w_writes, 'roundtrip': w_roundtrip, 'rtmain': w_rtmain, 'graft': w_graft,
    'check': w_check, 'refs': w_refs, 'xref': w_xref}


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--model', default='native', choices=('native', 'python', 'both'))
    ap.add_argument('--only', default=','.join(WORK))
    ap.add_argument('--runs', type=int, default=1)
    ap.add_argument('--profile', default=None)
    a = ap.parse_args(argv[1:])
    movies = corpus()
    print(f'{len(movies)} movies; aptgraft model {ag.backend()} available')
    tmp = tempfile.mkdtemp(prefix='bench_apt_')
    try:
        if a.profile:
            set_model(a.model != 'python')
            pr = cProfile.Profile()
            pr.enable()
            print(WORK[a.profile](movies, tmp))
            pr.disable()
            pstats.Stats(pr).sort_stats('cumulative').print_stats(30)
            return 0
        models = {'native': [True], 'python': [False], 'both': [False, True]}[a.model]
        for name in a.only.split(','):
            for native in models:
                set_model(native)
                best, what = float('inf'), ''
                for _ in range(a.runs):
                    t0 = time.perf_counter()
                    what = WORK[name](movies, tmp)
                    best = min(best, time.perf_counter() - t0)
                print(f'{name:10s} {"rust" if native else "python":7s} {best:8.2f} s   {what}', flush=True)
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == '__main__':
    sys.exit(main(sys.argv))
