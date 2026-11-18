r"""Round E4a: the Rust core (rust\bc2-stockmp) against the Python reference on every stock multiplayer level of
Project OPFOR BFBC2's game copy - each level-00 bundle's level partition (Dist and Package), read in memory:

  - the level model (levelmodel.model_of against levelmodel.Model on dbxbin), Rush stages included;
  - the authoring of a generated configuration (Conquest and Rush levels: 8 bots a team, 4 + 4 per Rush stage, no
    path data, so every candidate point is kept): bytes and report, author_native against author.Author;
  - the round trip of the partition;
  - validate's readings (structure, changes, references into other partitions, stage instances) of the authored
    partition against its stock one, with the Rust core and without.

  python tests\test_native_levels.py          (needs the game copy and the Rust core; nothing is written)
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('stockmp', 'core', 'navkit', 'navkit-tools')

import author  # noqa: E402
import config as cfgmod  # noqa: E402
import dbxbin  # noqa: E402
import levelmodel  # noqa: E402
import pipeline  # noqa: E402
import stockcore  # noqa: E402
import validate  # noqa: E402
from bfbc2core import fbrb as corefbrb  # noqa: E402
from test_stockmp import python_author  # noqa: E402

FIELDS = ('name', 'game_mode', 'mode', 'factions', 'attacker', 'has_ai', 'events', 'objects', 'capture_points',
          'spawners', 'mcoms')


def partitions(game):
    """(name, level partition bytes) of every multiplayer level-00 bundle (Dist and Package)."""
    out = []
    for kind, base in (('dist', os.path.join(game, 'Dist', 'win32', 'levels')), ('pkg', os.path.join(game, 'Package', 'levels'))):
        if not os.path.isdir(base):
            continue
        for lv in sorted(os.listdir(base)):
            b = os.path.join(base, lv, 'level-00.fbrb')
            if not lv.lower().startswith('mp_') or lv.lower() == 'mp_common' or not os.path.isfile(b):
                continue
            names = [e.name for e in corefbrb.entries(b) if not e.deleted
                     and e.name.replace('\\', '/').lower() == f'levels/{lv.lower()}.dbx']
            if names:
                out.append((f'{lv} {kind}', lv.lower(), corefbrb.read(b, names[:1])[names[0]]))
    return out


def main():
    failed = []

    def check(ok, what):
        print(f'{"ok  " if ok else "FAIL"} {what}', flush=True)
        if not ok:
            failed.append(what)
    nat = stockcore.native()
    if nat is None:
        print('the Rust core is not built (python rust\\build_native.py): nothing to compare')
        return 1
    paths = pipeline.Paths('mp_005')
    donor_lvl, _so = pipeline.donor(paths)
    donor = open(donor_lvl, 'rb').read()
    kits = pipeline.kit_refs(paths)
    parts = partitions(paths.game)
    check(len(parts) >= 20, f'{len(parts)} multiplayer level partitions found')
    authored_n = refused = 0
    for name, lv, data in parts:
        mn, mp = levelmodel.model_of(data), levelmodel.Model(dbxbin.parse(data))
        same = mn.d is None and all(getattr(mn, f) == getattr(mp, f) for f in FIELDS) and mn.rush_stages() == mp.rush_stages()
        check(same, f'{name}: the level model ({mp.mode}, {len(mp.spawners)} spawners, {len(mp.capture_points)} capture '
                    f'points, {len(mp.rush_stages())} stages)')
        check(dbxbin.round_trips([data]) == [dbxbin.write(dbxbin.parse(data)) == data], f'{name}: the round trip')
        try:
            c = cfgmod.generate(mp, lv, kits, {'infantry': None, 'vehicle': None})
        except (ValueError, KeyError) as e:
            print(f'     {name}: not generated ({e})')
            continue
        got, want = author.author_native(data, donor, c), python_author(data, donor, c)
        if got is None:
            refused += 1
            print(f'     {name}: left to Python ({nat.stockmp_author_refusal(data, donor, c)})')
            continue
        authored_n += 1
        check(got == want and list(got[1]) == list(want[1]),
              f'{name}: the authoring, {len(c["spawners"])} spawners / {len(c["orders"])} orders ({len(got[0]):,} bytes)')
        results = []
        for on in (True, False):
            stockcore.use_native(on)
            try:
                results.append(run_checks(data, got[0], c))
            finally:
                stockcore.use_native(True)
        check(results[0] == results[1], f'{name}: validate\'s readings with the Rust core = without ({results[0][0][:60]})')
    check(authored_n >= 10, f'{authored_n} levels authored in the Rust core ({refused} left to Python)')
    print(f'{len(failed)} failed' if failed else 'all passed')
    return 1 if failed else 0


def run_checks(orig, auth, c):
    """structure, changes, external references and stages of the partitions (written to a temporary folder)."""
    import tempfile
    d = tempfile.mkdtemp(prefix='stockmp_native_')
    try:
        po, pa = os.path.join(d, 'o.dbx'), os.path.join(d, 'a.dbx')
        open(po, 'wb').write(orig)
        open(pa, 'wb').write(auth)
        out = []
        for fn, args in ((validate.check_structure, (po, pa, c)), (validate.check_changes, (po, pa, c, d)),
                         (validate.check_external, (po, pa, [d])), (validate.check_stages, (pa, c))):
            try:
                out.append(str(fn(*args)))
            except Exception as e:
                out.append(f'{type(e).__name__}: {e}')
        return out
    finally:
        import shutil
        shutil.rmtree(d, ignore_errors=True)


if __name__ == '__main__':
    sys.exit(main())
