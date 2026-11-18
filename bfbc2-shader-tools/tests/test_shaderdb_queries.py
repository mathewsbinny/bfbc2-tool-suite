r"""Round E4b: the shader database kept in the Rust core (bc2_native.ShaderDbHandle) and the queries it answers,
against shaderdb.py's Python (the *_reference functions, on a database read by the Python reader): the same answers
on a database whose sections were never made, on one whose sections were made, and after the caller changed them
(entries appended, a group replaced, an inner program list changed in place, blocks swapped, a list replaced, a name
that is not latin-1); the same bytes written; OPFOR's wake_shaders.signatures (its code as it is) the same on the
Rust core's block readers as entry_signatures.

  python test_shaderdb_queries.py          quick: built databases (Dx9, Dx10, Xbox 360 v120 / v117, textures and
                                           constants), damaged parameter blocks; mp_008cq's three databases when the
                                           game copy is there
  python test_shaderdb_queries.py --game   + every shader database test_shaderdb_native's corpus has (the game copy's
                                           bundles, OPFOR's Xbox levels, BF1943's levels, OPFOR's work outputs: read
                                           only), each query on every entry

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import os
import pickle
import random
import struct
import sys
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core')
sys.path.insert(0, HERE)
import shaderdb as sd  # noqa: E402
import test_shaderdb as ts  # noqa: E402
from bfbc2core import dbx, paths  # noqa: E402

NATIVE: Any = sd._with_shaderdb(dbx._import_native())
HAS_HANDLE = NATIVE is not None and hasattr(NATIVE, 'ShaderDbHandle')


def plain(x: Any) -> Any:
    """x with floats as their bits (NaN compares) and sets / frozensets sorted (an order to compare)."""
    if isinstance(x, float):
        return ('f', struct.pack('<d', x))
    if isinstance(x, (set, frozenset)):
        return ('set', sorted((plain(v) for v in x), key=repr))
    if isinstance(x, dict):
        return ('dict', [(plain(k), plain(v)) for k, v in x.items()])
    if isinstance(x, (list, tuple)):
        return (type(x).__name__, [plain(v) for v in x])
    return x


def outcome(f: Any, *args: Any) -> Any:
    try:
        return 'ok', plain(f(*args))
    except Exception as e:  # noqa: BLE001 - every exception is compared
        return type(e).__name__, str(e)


def needed_ref(db: sd.ShaderDb, want: list[str]) -> Any:
    """add_donor's candidates in Python: entries passing needed_test, parts whose mesh does."""
    mine = sd.needed_test(want)
    return ([(n, g) for n, g in db.entries if mine(n)],
            [(n, g) for n, g in db.parts if (pm := sd.part_mesh(n)) is not None and mine(pm)])


def needed_native(db: sd.ShaderDb, want: list[str]) -> Any:
    h = sd._handle(db)
    got = h.select_needed(db.__dict__, want) if h is not None else None
    return needed_ref(db, want) if got is None else got


def opfor_signatures(db: sd.ShaderDb, names: Any = None) -> Any:
    """OPFOR's wake_shaders.signatures, its code as it is (it calls sd.block_constants / block_textures)."""
    progs, out, cache = db.entry_programs(), {}, {}
    for name, g in db.entries:
        if names is not None and name not in names:
            continue
        consts, samplers = set(), set()
        for i in progs.get(g, []):
            for k in db.program_refs(i)[2:]:
                if k not in cache:
                    cache[k] = ({c[0] for c in sd.block_constants(db.params[k], db.e)},
                                {t[0] for t in sd.block_textures(db.params[k], db.e)})
                consts |= cache[k][0]
                samplers |= cache[k][1]
        out[name] = (tuple(sorted(consts)), len(samplers))
    return out


def queries(db: sd.ShaderDb, names: list[str], rng: random.Random, native: bool, sample: int) -> list[tuple[str, Any]]:
    """[(query, outcome)] of every query on db, through the Rust handle (native) or the references."""
    if native:
        tex, sig, lay, cov = sd.entry_textures, sd.entry_signatures, sd.entry_layouts, sd.coverage
        meshes = lambda: db.mesh_names()          # noqa: E731
        sel = needed_native
    else:
        tex, sig, lay = sd.entry_textures_reference, sd.entry_signatures_reference, sd.entry_layouts_reference
        cov, meshes, sel = sd.coverage_reference, db.mesh_names_reference, needed_ref
    picked = sorted(set(rng.sample(names, min(sample, len(names))))) if names else []
    some = set(picked[: max(1, len(picked) // 3)]) | {'No/Such/Entry', 'WЖeird'}
    try:
        ms = sorted(meshes())
    except Exception:  # noqa: BLE001 - compared below
        ms = []
    want = ms[::3] + [m.rsplit('/', 1)[0] for m in ms[1::5]] + ['objects', 'x/Ж', '']
    out = [('entry_textures', outcome(tex, db)), ('entry_textures some', outcome(tex, db, some)),
           ('entry_textures list', outcome(tex, db, list(some))),
           ('entry_signatures', outcome(sig, db)), ('entry_signatures some', outcome(sig, db, frozenset(some))),
           ('entry_layouts', outcome(lay, db)), ('entry_layouts some', outcome(lay, db, some)),
           ('mesh_names', outcome(meshes)), ('coverage', outcome(cov, db, want)),
           ('coverage dict', outcome(cov, db, dict.fromkeys(want, 1))),
           ('select_needed', outcome(sel, db, want)),
           ('check', outcome(db.check if native else db.check_reference))]
    for n in picked:                             # the per-name pattern (OPFOR's fix_layouts, clone, graft)
        out.append((f'entry_textures {{{n}}}', outcome(tex, db, {n})))
        out.append((f'entry_signatures {{{n}}}', outcome(sig, db, {n})))
    if native:
        out.append(('opfor signatures', outcome(opfor_signatures, db)))
    else:
        out.append(('opfor signatures', outcome(sd.entry_signatures_reference, db)))
    return out


def mutate(db: sd.ShaderDb, step: int) -> str:
    """The step-th change a caller makes to a database's lists (the same on both sides); its name."""
    if step == 0:
        g = db.groups[len(db.groups) // 2][0] if db.groups else bytes(16)
        db.entries.append(('Objects/Added/Added_Mesh/Shaders/New', g))
        return 'an entry appended (an existing group\'s GUID)'
    if step == 1 and len(db.groups) > 1:
        g, idx = db.groups[1]
        db.groups[1] = (g, list(reversed(idx))[:max(1, len(idx) // 2)])
        return 'a group replaced'
    if step == 2 and db.groups:
        db.groups[0][1].append(len(db.programs) // 2) if db.programs else None
        return 'a group\'s program list changed in place'
    if step == 3 and len(db.params) > 3:
        db.params[1], db.params[-1] = db.params[-1], db.params[1]
        return 'two parameter blocks swapped'
    if step == 4:
        db.programs = list(reversed(db.programs))
        db.keys = list(reversed(db.keys))
        return 'the program and key lists replaced'
    if step == 5 and db.entries:
        db.entries[0] = ('WЖeird/Mesh/Shader', db.entries[0][1])
        return 'a name that is not latin-1'
    if step == 6 and db.groups:
        db.groups[-1] = (db.groups[-1][0], [len(db.programs) + 5])
        return 'a group naming a program past the end'
    return 'nothing'


def compare_db(data: bytes, dx: int | None, label: str, seed: int, sample: int, mutations: bool = True) \
        -> tuple[int, list[str]]:
    """(queries compared, differences) for one database's bytes."""
    rng_seed = seed
    sd.use_native(False)
    try:
        ref = sd.ShaderDb.parse(data, dx, where=label)
    except sd.ShaderDbError:
        sd.use_native(True)
        return 0, []
    sd.use_native(True)
    new = sd.ShaderDb.parse(data, dx, where=label)
    diffs = []
    n = 0
    # never made (the file), then made (the lists from the file), then changed step by step
    for stage in ['fresh', 'made'] + (list(range(7)) if mutations else []):
        what = str(stage)
        if stage == 'made':
            for s in sd.SECTIONS:
                getattr(new, s)
        elif isinstance(stage, int):
            what = mutate(ref, stage)
            mutate(new, stage)
        names = [nm for nm, _ in ref.entries]
        a = queries(ref, names, random.Random(rng_seed), False, sample)
        b = queries(new, names, random.Random(rng_seed), True, sample)
        n += len(a)
        for (qa, ra), (_, rb) in zip(a, b):
            if ra != rb:
                diffs.append(f'{label} [{what}] {qa}: {str(ra)[:120]} / {str(rb)[:120]}')
        sd.use_native(False)
        wa = outcome(ref.to_bytes)
        sd.use_native(True)
        wb = outcome(new.to_bytes)
        n += 1
        if wa != wb:
            diffs.append(f'{label} [{what}] to_bytes differs')
        if stage == 'fresh' and wb != ('ok', data):
            diffs.append(f'{label} written back: not the file\'s bytes')
    # every parameter block, every texture name: the block readers and texture_kind
    blocks = list(ref.params)
    for e in ('<', '>'):
        for blk in blocks:
            for f, g in ((sd.block_textures, sd.block_textures_reference),
                         (sd.block_constants, sd.block_constants_reference),
                         (sd.block_engine_textures, sd.block_engine_textures_reference)):
                n += 1
                if outcome(f, blk, e) != outcome(g, blk, e):
                    diffs.append(f'{label} {f.__name__} differs ({e})')
    tnames = {t for blk in blocks for _, _, t in sd.block_textures_reference(blk, ref.e)}
    for t in sorted(tnames) + ['X_NM', 'x_İ', 'k_K', '']:
        n += 1
        if sd.texture_kind(t) != sd.texture_kind_reference(t):
            diffs.append(f'{label} texture_kind({t!r})')
    return n, diffs


def lazy_checks(c: ts.Checks, data: bytes, dx: int) -> None:
    """A database read in Rust holds no section lists until they are read; pickles and copies hold plain lists."""
    import copy
    db = sd.ShaderDb.parse(data, dx)
    made_before = [s for s in sd.SECTIONS if s in db.__dict__]
    sd.entry_textures(db)
    sd.coverage(db, ['objects'])
    made_after = [s for s in sd.SECTIONS if s in db.__dict__]
    again = pickle.loads(pickle.dumps(db))
    cp = copy.deepcopy(db)
    c.check(not made_before and not made_after and '_h' not in again.__dict__ and again.to_bytes() == data
            and cp.to_bytes() == data and again.entries == db.entries,
            'a database read in Rust: no section made by parsing or by the queries; pickled and deep-copied as plain '
            'lists, the same bytes', f'{made_before} {made_after}')
    # the handle answers itself (no decline) on a fresh database, a made one and a changed one; declines on a name
    # that is not latin-1
    answered = []
    for stage in ('fresh', 'made', 'changed', 'not latin-1'):
        d = sd.ShaderDb.parse(data, dx)
        if stage != 'fresh':
            for s in sd.SECTIONS:
                getattr(d, s)
        if stage == 'changed':
            d.entries.append(('Objects/X/X_Mesh/S', d.groups[0][0]))
            d.groups[0][1].append(0)
        if stage == 'not latin-1':
            d.entries[0] = ('Ж', d.entries[0][1])
        h: Any = d.__dict__['_h']
        got = [h.entry_textures(d.__dict__, False), h.entry_signatures(d.__dict__, False),
               h.entry_layouts(d.__dict__), h.coverage(d.__dict__, ['objects']), h.mesh_names(d.__dict__),
               h.select_needed(d.__dict__, ['objects']), h.write(d.__dict__, False, d.version, d.flag),
               h.check(d.__dict__, False, False)]
        answered.append([g is not None for g in got])
    want = [[True] * 8] * 3 + [[False, False, False, False, True, False, False, False]]
    c.check(answered == want, 'the handle answers on a fresh, a made and a changed database (8 queries) and '
            'declines where a name is not latin-1 (entries and the writer; the part names still answer)', str(answered))
    try:
        db.no_such_attribute  # noqa: B018
        ok = False
    except AttributeError:
        ok = True
    c.check(ok and not hasattr(sd.ShaderDb(9), '_h'), 'other attributes still raise AttributeError')


def built(c: ts.Checks) -> None:
    rng = random.Random(4)
    ents = [('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 3), ('Objects/B/B_Mesh/Objects/B/Shaders/W\xe9ird', 2),
            ('Objects/C/C_Mesh/Objects/C/Shaders/None', 0), ('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 1)]
    cases: list[tuple[str, bytes, int | None]] = []
    for dx in (9, 10):
        db = ts.build(dx, ents, dx)
        db = ts.with_textures(db, {ents[0][0]: [(0, 'Textures/A_C'), (1, 'Textures/A_Nm'), (0, 'Textures/A_C')],
                                   ents[1][0]: [(3, 'Sky_\xe9'), (2, 'T_Sp')]})
        cases.append((f'Dx{dx}', db.to_bytes(), dx))
    x = ts.build(9, ents, 4, big_endian=True)
    x = ts.with_textures(x, {ents[0][0]: [(5, 'Textures/X_Df')]})
    cases.append(('Xbox 360 v120', x.to_bytes(), None))
    x.version = 117
    x.keys = [k + b'\x01\x02\x03\x04' for k in x.keys]
    cases.append(('Xbox 360 v117', x.to_bytes(), None))
    total, diffs = 0, []
    for label, data, kind in cases:
        n, d = compare_db(data, kind, label, rng.randrange(1 << 30), 8)
        total += n
        diffs += d
    c.check(not diffs, f'{len(cases)} built databases (Dx9, Dx10, Xbox 360 v120 / v117; a repeated entry name, a '
            f'latin-1 name, an entry without programs): {total} query answers the reference\'s - never made, made, '
            'and after 7 kinds of change', '; '.join(diffs[:4]))
    # damaged and random parameter blocks
    bad = 0
    m = 0
    for _ in range(3000):
        blk = bytearray(ts.param_block(rng.choice('<>'), [(rng.randrange(16), 'T' * rng.randrange(0x90))
                                                        for _ in range(rng.randrange(4))],
                                       [('C', 1, 4, (rng.random(), float('nan'), -0.0, 1e38))]))
        for _ in range(rng.randrange(4)):
            i = rng.randrange(len(blk))
            blk[i] = rng.randrange(256)
        if rng.random() < 0.3:
            blk = blk[:rng.randrange(len(blk) + 1)]
        b = bytes(blk)
        for e in '<>':
            for f, g in ((sd.block_textures, sd.block_textures_reference),
                         (sd.block_constants, sd.block_constants_reference),
                         (sd.block_engine_textures, sd.block_engine_textures_reference)):
                m += 1
                bad += outcome(f, b, e) != outcome(g, b, e)
    c.check(bad == 0, f'{m} reads of damaged / random parameter blocks (bytes changed, cut short, NaN and -0 '
            'values, long names): the reference\'s answers', f'{bad} differ')
    lazy_checks(c, cases[0][1], 9)


def _game_item(item: tuple[str, str, str, int | None]) -> tuple[str, int, list[str], int]:
    import test_shaderdb_native as tn
    if item[0] == 'bf1943':
        paths.use_folder(tn._BF1943_TOOL[0])
    label = item[1] + (f':{item[2]}' if item[2] else '')
    try:
        data = tn._load(item)
    except Exception as e:  # noqa: BLE001
        return label, 0, [f'not loaded: {e}'], 0
    dx = item[3]
    n, d = compare_db(data, 10 if dx == 11 else dx, label, len(data), 40, mutations=False)
    return label, n, d, len(data)


def game(c: ts.Checks, root: str) -> None:
    import test_shaderdb_native as tn
    t = time.time()
    items, tool, skipped = tn.corpus(root, os.path.dirname(os.path.dirname(HERE)))
    if skipped:
        print(f'skipped: {skipped}')
    total = 0
    diffs: list[str] = []
    kinds: Counter[str] = Counter()
    nbytes = 0
    with ProcessPoolExecutor(2, initializer=tn._init, initargs=(tool,)) as pool:
        for item, (_label, n, d, size) in zip(items, pool.map(_game_item, items, chunksize=2)):
            kinds[item[0]] += 1
            total += n
            nbytes += size
            diffs += d
    c.check(not diffs, f'{len(items)} shader databases ({dict(kinds)}; {nbytes / 1e9:.2f} GB): {total:,} query '
            f'answers (every entry\'s textures, signatures, layouts, coverage, mesh names, the checks, the merge\'s needed names, '
            f'40 entries one by one, every block read both byte orders, every texture name\'s kind, the writer) the '
            f'reference\'s - never made and made ({time.time() - t:.0f} s)', '; '.join(diffs[:4]))


def main(argv: list[str]) -> int:
    if not HAS_HANDLE:
        print('skipped: the Rust core is not built (python rust\\build_native.py); shaderdb runs on Python')
        return 0
    c = ts.Checks()
    built(c)
    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the game copy (no Project OPFOR BFBC2)')
    elif '--game' in argv:
        game(c, root)
    else:
        total, diffs = 0, []
        for dx in (9, 10, 11):
            data, label, _ = sd.resolve('level:mp_008cq', dx)
            n, d = compare_db(data, 10 if dx == 11 else dx, label, dx, 25)
            total += n
            diffs += d
        c.check(not diffs, f'mp_008cq\'s three databases: {total:,} query answers the reference\'s - never made, '
                'made, after 7 kinds of change', '; '.join(diffs[:4]))
    print('all passed' if not c.failed else f'{c.failed} check(s) failed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
