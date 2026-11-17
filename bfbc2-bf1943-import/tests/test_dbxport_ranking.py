r"""dbxport's template ranking from an index (round E3d): Templates.choose ranks through _Ranking, which must make the
reference scan's (_best's) choice and score on every call - the first candidate with the top score.

Quick (no game data): 20,000 seeded random rankings (lists of 1 - 300 candidates drawn from few inputs, slots and
ordinals, so ties are everywhere; roles with and without inputs and slot, unknown values) - _Ranking.best against
_best, the same candidate object and score; Templates.choose with and without donors against REFERENCE_RANKING,
after add() (the indexes rebuilt).
With --game: the level ports OPFOR's level build makes (wake_level: BF1943's level partition ranked against BFBC2's
157 level partitions, 320,948 candidates, 139,586 of the largest type) for Wake Island, Coral Sea, Guadalcanal and
Iwo Jima - every call's choice checked against _best, the output bytes against a port with the reference scan, and
both timed. All 157 partitions' trees take 3.5 GB, so the pool is every third of them (--part K/N: the K-th of N
slices, default 0/3; --part 0/1 is OPFOR's whole pool).

  python test_dbxport_ranking.py [--game] [--part K/N] [level ...]
"""
import os
import random
import re
import sys
import time

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

fails: list[str] = []


def check(ok: bool, what: str) -> None:
    print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)
    if not ok:
        fails.append(what)


INPUTS = [('InputAction', f'Eia{i}') for i in range(5)] + [('RotationInput', f'Eia{i}') for i in range(3)]
SLOTS = [('Components', i) for i in range(4)] + [('Children', i) for i in range(3)]


def candidates(rng: random.Random, n: int, part: str = 'p') -> list[dbxport.Candidate]:
    out = []
    for _ in range(n):
        k = rng.choice((0, 0, 0, 1, 1, 2, 3))
        inputs = tuple(rng.choice(INPUTS) for _ in range(k))         # repeats within one candidate too
        slot = rng.choice(SLOTS) if rng.random() < 0.5 else None
        out.append(dbxport.Candidate(f'{part}{rng.randrange(6)}', dbx.Element('instance', dbx.CONTAINER),
                                     rng.randrange(6), slot, inputs))
    return out


def role(rng: random.Random) -> tuple:
    k = rng.choice((0, 0, 1, 2, 3))
    inputs = tuple(rng.choice(INPUTS + [('InputAction', 'EiaNone')]) for _ in range(k))
    slot = rng.choice(SLOTS + [('Other', 9)]) if rng.random() < 0.6 else None
    return rng.randrange(8), slot, inputs


def quick() -> None:
    rng = random.Random(20261117)
    n = same = 0
    scores: dict[int, int] = {}
    for _ in range(2000):
        cands = candidates(rng, rng.choice((1, 2, 3, 10, 50, 300)))
        r = dbxport._Ranking(cands)
        for _ in range(10):
            ro = role(rng)
            a, b = r.best(ro), dbxport._best(cands, ro)
            n += 1
            same += a[0] == b[0] and a[1] is b[1]
            scores[b[0]] = scores.get(b[0], 0) + 1
    check(same == n, f'_Ranking.best makes _best\'s choice and score: {same} of {n} random rankings '
          f'(scores {dict(sorted(scores.items()))})')

    # Templates.choose: the index against the reference, donors and pools, before and after add()
    t = dbxport.Templates([])
    types = ['A.X', 'A.Y', 'A.Z']
    for ty in types:
        cs = candidates(rng, 120)
        t.candidates[ty] = cs
    insts = [dbx.Element('instance', dbx.CONTAINER, [('guid', f'g{i}'), ('type', rng.choice(types + ['A.None']))])
             for i in range(400)]
    calls = [(i, role(rng), rng.choice(((), ('p1',), ('p5', 'p2'), ('nothere',)))) for i in insts]

    def run(reference: bool) -> list:
        dbxport.REFERENCE_RANKING = reference
        try:
            return [t.choose(i, ro, d) for i, ro, d in calls]
        finally:
            dbxport.REFERENCE_RANKING = False
    ref, got = run(True), run(False)
    check(all(a[0] == b[0] and a[1] is b[1] for a, b in zip(got, ref)),
          f'Templates.choose: the index\'s step and candidate the reference\'s ({len(calls)} calls, '
          f'{sum(s == "donor" for s, _ in ref)} donor, {sum(s == "role" for s, _ in ref)} role, '
          f'{sum(s == "pool" for s, _ in ref)} pool, {sum(s == "none" for s, _ in ref)} none)')
    f = dbx.DbxFile([], [dbx.Element('partition', dbx.CONTAINER, [], children=[
        dbx.Element('instance', dbx.CONTAINER, [('guid', 'new-1'), ('type', 'A.X')], children=[])])])
    before = len(t._rankings)
    t.add('p9.dbx', f)
    check(before > 0 and not t._rankings, 'add() drops the indexes built before it')
    ref, got = run(True), run(False)
    check(all(a[0] == b[0] and a[1] is b[1] for a, b in zip(got, ref)),
          'Templates.choose after add(): the reference\'s choices again (the new partition ranked)')


def game(names: list[str], part: tuple[int, int]) -> None:
    import hashlib
    suite_paths.use('mesh-viewer', 'shader-tools')
    import bf1943import
    import dbxcatalogue
    import meshview
    import shaderdb
    try:
        ix = meshview.Index(shaderdb.find_opfor(None))
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                      # noqa: BLE001 - only missing game data skips
        print(f'SKIP game: no game data ({type(e).__name__}: {e})')
        return
    cat = dbxcatalogue.load(ix)
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
    ported = {k[:-4] for k in src.entries if k.endswith('.dbx')}
    levels = sorted(n[:-4] for n in ix.where if re.fullmatch(r'levels/[^/]+\.dbx', n))[part[0]::part[1]]
    t0 = time.perf_counter()
    T = dbxport.Templates.from_index(ix, levels, cat)
    print(f'  templates: {len(levels)} level partitions (slice {part[0]}/{part[1]}), {sum(map(len, T.candidates.values()))} candidates '
          f'(largest type {max(map(len, T.candidates.values()))}), {time.perf_counter() - t0:.0f} s', flush=True)

    def port(name: str) -> bytes:
        data, _ = dbxport.port(src.data(name + '.dbx'), T, lambda p: p.lower() in ported or p.lower() in bc2,
                               drop_types=dbxport.NOT_IN_BFBC2, prune=True, required=dbxport.REQUIRED, catalogue=cat,
                               name=name)
        return data

    reference_best = dbxport._best
    ranking_best = dbxport._Ranking.best
    calls = {'n': 0, 'same': 0, 'ref_s': 0.0}

    def timed_best(cands: list, ro: tuple) -> tuple:
        t = time.perf_counter()
        r = reference_best(cands, ro)
        calls['ref_s'] += time.perf_counter() - t
        return r

    def checked(self: dbxport._Ranking, ro: tuple) -> tuple:
        a, b = ranking_best(self, ro), reference_best(self.cands, ro)
        calls['n'] += 1
        calls['same'] += a[0] == b[0] and a[1] is b[1]
        return a

    for name in names:
        calls.update(n=0, same=0, ref_s=0.0)
        dbxport.REFERENCE_RANKING = True
        dbxport._best = timed_best                              # type: ignore[assignment]
        try:
            t = time.perf_counter()
            ref = port(name)
            tr = time.perf_counter() - t
        finally:
            dbxport._best = reference_best
            dbxport.REFERENCE_RANKING = False
        T._rankings.clear()
        t = time.perf_counter()
        got = port(name)
        ti = time.perf_counter() - t
        dbxport._Ranking.best = checked                         # type: ignore[method-assign, assignment]
        try:
            again = port(name)
        finally:
            dbxport._Ranking.best = ranking_best                # type: ignore[method-assign]
        check(got == ref == again and calls['n'] > 0 and calls['same'] == calls['n'],
              f'{name}: every ranking _best\'s choice ({calls["same"]} of {calls["n"]} calls), the same bytes '
              f'({len(ref):,} B, sha1 {hashlib.sha1(ref).hexdigest()[:12]}); port {tr:.1f} s with the scan '
              f'({calls["ref_s"]:.1f} s ranking) -> {ti:.1f} s with the index')


if __name__ == '__main__':
    quick()
    if '--game' in sys.argv:
        args = sys.argv[1:]
        k, n = map(int, args[args.index('--part') + 1].split('/')) if '--part' in args else (0, 3)
        rest = [a for i, a in enumerate(args) if not a.startswith('--') and (i == 0 or args[i - 1] != '--part')]
        game(rest or ['levels/wake_island_s', 'levels/coral_sea', 'levels/guadal_canal', 'levels/iwo_jima_s'], (k, n))
    print('all passed' if not fails else f'{len(fails)} failed')
    sys.exit(1 if fails else 0)
