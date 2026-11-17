r"""The Rust shader database reader and writer (rust/bc2-formats shaderdb, through the bc2_native module) against the
Python reader and writer they were ported from (shaderdb.py's ShaderDb._parse / to_bytes): the same objects, bytes
and messages; and the merge's needed-name test (needed_test) against needed.

  python test_shaderdb_native.py          quick: built databases (Dx9, Dx10 / Dx11, Xbox 360 v120 and v117), damaged
                                          and random ones, the writer's fallback, needed_test, a merge both ways, the
                                          dispatcher; 3 game databases when the game copy is there
  python test_shaderdb_native.py --game   + every shader database of the game copy's bundles (read only), of OPFOR's
                                          unpacked Xbox levels, of Battlefield 1943's extracted levels and of OPFOR's
                                          work\ww2 / work\levels outputs: both readers, both writers, the file's own
                                          bytes; and wake_shaders' merge (the WW2 level builds' shaders step: base
                                          mp_005, every stock level a donor) for each WW2 level's needs list, Dx9 / 10 /
                                          11, Rust core + needed_test against Python + needed (written to a temp folder)

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import os
import random
import re
import struct
import subprocess
import sys
import tempfile
import time
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core')
sys.path.insert(0, HERE)
import shaderdb as sd  # noqa: E402
import test_shaderdb as ts  # noqa: E402
from bfbc2core import dbx, fbrb, paths  # noqa: E402

NATIVE: Any = sd._with_shaderdb(dbx._import_native())
WW2 = ('wake_island_s', 'guadal_canal', 'iwo_jima_s', 'coral_sea')


# ------------------------------------------------------------------------------------------------ both backends

def fields(db: sd.ShaderDb) -> tuple[Any, ...]:
    return (db.dx, db.big_endian, db.version, db.flag, db.key_size, db.params, db.bindings, db.regs,
            [(s.guid, s.code, s.param, s.binding, s.regs, s.tail) for s in db.vs],
            [(s.guid, s.code, s.param, s.binding, s.regs, s.tail) for s in db.ps],
            db.programs, db.keys, db.layouts, db.entries, db.fragments, db.parts, db.groups)


def outcome(data: bytes, dx: int | None, native: bool) -> tuple[str, Any]:
    """('ok', fields, written bytes) or (exception type, message) of ShaderDb.parse with one backend."""
    sd.use_native(native)
    try:
        db = sd.ShaderDb.parse(data, dx, where='f')
    except Exception as e:  # noqa: BLE001 - every exception is compared
        return type(e).__name__, str(e)
    return 'ok', (fields(db), db.to_bytes())


def compare(data: bytes, dx: int | None) -> tuple[str | None, str]:
    """(None or how the backends differ, the outcome's kind)."""
    py, rs = outcome(data, dx, False), outcome(data, dx, True)
    sd.use_native(True)
    if py[0] != rs[0]:
        return f'python {py[0]}, rust {rs[0]}', rs[0]
    if py[1] != rs[1]:
        if py[0] != 'ok':
            return f'messages differ: {py[1]!r} / {rs[1]!r}', rs[0]
        return ('objects differ' if py[1][0] != rs[1][0] else 'written bytes differ'), rs[0]
    if py[0] == 'ok' and py[1][1] != data:
        return 'written bytes are not the file\'s', rs[0]
    return None, rs[0]


def mutations(data: bytes, rng: random.Random, n: int) -> list[bytes]:
    """Damaged copies: cut short at many lengths, bytes changed, runs of 0xFF / 0x00 written, bytes cut out, a u32
    (a count) made huge."""
    out = [data[:i] for i in sorted(set(range(0, min(len(data), 120))) | {len(data) * k // 60 for k in range(60)})]
    for _ in range(n):
        d = bytearray(data)
        for _ in range(rng.randint(1, 3)):
            i = rng.randrange(len(d))
            op = rng.randrange(4)
            if op == 0:
                d[i] = rng.randrange(256)
            elif op == 1:
                d[i:i + 4] = (b'\xff' if rng.random() < 0.5 else b'\0') * 4
            elif op == 2:
                del d[i:i + rng.randint(1, 8)]
            else:
                d[i:i + 4] = struct.pack('<I', rng.choice((0x7fffffff, 0xffffffff, 1 << 20, 3)))
        out.append(bytes(d))
    return out


def built() -> list[tuple[str, bytes, int | None]]:
    """(label, bytes, dx) of databases made here, every kind the reader takes."""
    ents = [('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 3), ('Objects/B/B_Mesh/Objects/B/Shaders/W\xe9ird', 2),
            ('Objects/C/C_Mesh/Objects/C/Shaders/None', 0)]
    out: list[tuple[str, bytes, int | None]] = []
    for dx in (9, 10, 11):
        db = ts.with_textures(ts.build(dx, ents, dx), {ents[0][0]: [(0, 'Textures/A_C'), (1, 'Textures/A_N')]})
        out.append((f'Dx{dx}', db.to_bytes(), dx))
    x = ts.build(9, ents, 4, big_endian=True)
    out.append(('Xbox 360 v120', x.to_bytes(), None))
    x.version = 117
    x.keys = [k + b'\x01\x02\x03\x04' for k in x.keys]
    out.append(('Xbox 360 v117 (BF1943: 0x5c-byte keys)', x.to_bytes(), None))
    out.append(('Dx10 read with no kind given (Dx9, then Dx10)', out[1][1], None))
    out.append(('empty sections', struct.pack('<IB', 120, 0) + bytes(48), 9))
    return out


# ------------------------------------------------------------------------------------------------ the corpus

def _load(item: tuple[str, str, str, int | None]) -> bytes:
    kind, path, entry, _ = item
    if kind == 'file':
        with open(path, 'rb') as fh:
            return fh.read()
    if kind == 'bf1943':
        import fbrb1943
        return fbrb1943.Bundle(open(path, 'rb').read()).data(entry)
    return fbrb.read(path, [entry])[entry]


def _compare_item(item: tuple[str, str, str, int | None]) -> tuple[str, int, str | None, str, int]:
    """(label, bytes, difference, outcome, entries) for one database."""
    if item[0] == 'bf1943':
        paths.use_folder(_BF1943_TOOL[0])
    label = item[1] + (f':{item[2]}' if item[2] else '')
    try:
        data = _load(item)
    except Exception as e:  # noqa: BLE001
        return label, 0, f'not loaded: {type(e).__name__}: {e}', 'load', 0
    dx = item[3]
    diff, kind = compare(data, 10 if dx == 11 else dx)
    n = 0
    if kind == 'ok':
        sd.use_native(True)
        n = len(sd.ShaderDb.parse(data, 10 if dx == 11 else dx).entries)
    return label, len(data), diff, kind, n


_BF1943_TOOL: list[str] = ['']


def _init(tool: str) -> None:
    _BF1943_TOOL[0] = tool


def corpus(root: str, suite: str) -> tuple[list[tuple[str, str, str, int | None]], str, str]:
    """[(kind, path, entry, dx)] of every database: the game copy's bundles, OPFOR's Xbox levels, BF1943's levels, the
    work folders' files; and the BF1943 bundle tool's folder ('' when not there), and what was skipped."""
    items: list[tuple[str, str, str, int | None]] = []
    skipped = []
    bundles = []
    for top in (os.path.join(root, 'game'), os.path.join(root, 'work', 'xbox'), os.path.join(root, 'work', 'xbox_dlc')):
        for d, _, names in os.walk(top):
            bundles += [os.path.join(d, n) for n in names if n.lower().endswith('.fbrb')]
    for b in sorted(bundles):
        try:
            ents = fbrb.entries(b)
        except (OSError, ValueError):
            continue
        for e in ents:
            if e.type.endswith('ShaderDatabase') and not e.deleted:
                m = re.match(r'dx(\d+)', e.type.lower())
                items.append(('bundle', b, e.name, int(m.group(1)) if m else None))
    xenon = paths.sibling(paths.BF1943_WORKSPACE, 'work', 'extracted', 'dist', 'xenon', env='BF1943_XENON', suite=suite)
    tool = paths.sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', suite=suite)
    if os.path.isdir(os.path.join(xenon, 'levels')) and os.path.isfile(os.path.join(tool, 'fbrb1943.py')):
        paths.use_folder(tool)
        import fbrb1943
        for d, _, names in sorted(os.walk(os.path.join(xenon, 'levels'))):
            for f in sorted(names):
                if f.lower().endswith('.fbrb'):
                    p = os.path.join(d, f)
                    for e in fbrb1943.Bundle(open(p, 'rb').read()).entries:
                        if e['type'].endswith('ShaderDatabase'):
                            items.append(('bf1943', p, e['name'], None))
    else:
        tool = ''
        skipped.append('Battlefield 1943 (no extracted copy or bundle tool)')
    for top in ('ww2', 'levels'):
        for d, _, names in os.walk(os.path.join(root, 'work', top)):
            for n in names:
                if re.search(r'^(dx\d+_)?single\.res(\.cloned)?$', n, re.I):
                    items.append(('file', os.path.join(d, n), '', sd.dx_from_name(n)))
    return items, tool, '; '.join(skipped)


def run_corpus(root: str, check: Any) -> None:
    t = time.time()
    items, tool, skipped = corpus(root, os.path.dirname(ROOT))
    if skipped:
        print(f'skipped: {skipped}')
    diffs: Counter[str] = Counter()
    examples: dict[str, str] = {}
    kinds: Counter[str] = Counter()
    groups: Counter[str] = Counter()
    nbytes = nent = 0
    workers = max(1, min(6, (os.cpu_count() or 2) - 1))
    with ProcessPoolExecutor(workers, initializer=_init, initargs=(tool,)) as pool:
        for item, (label, size, diff, kind, n) in zip(items, pool.map(_compare_item, items, chunksize=2)):
            kinds[kind] += 1
            groups[{'bundle': 'game / Xbox bundles', 'bf1943': 'BF1943 bundles', 'file': 'work files'}[item[0]]] += 1
            nbytes += size
            nent += n
            if diff:
                diffs[diff] += 1
                examples.setdefault(diff, label)
    for k, n in diffs.items():
        print(f'       {n} x {k}, e.g. {examples[k]}')
    check(not diffs and kinds['ok'] > 0,
          f'{len(items)} shader databases ({", ".join(f"{n} {g}" for g, n in groups.items())}; {nbytes / 1e9:.2f} GB, '
          f'{nent:,} entries): the same objects, the same bytes written by both writers, the file\'s own bytes '
          f'({kinds["ok"]} read, {len(items) - kinds["ok"]} refused alike)  ({time.time() - t:.0f} s)')


# ------------------------------------------------------------------------------------------------ the merges

def old_needed_test(need: Any) -> Any:
    """The merge's test as it was before needed_test: every name against every needed path (needed)."""
    want = list(need)
    return lambda name: sd.needed(name, want)


def merge_level(need_path: str, out: str, rust: bool) -> list[str]:
    """wake_shaders.main()'s merge (base mp_005, every other stock level a donor that covers a needed mesh still
    missing, need = the level's needs_bfbc2.txt) for Dx9 / 10 / 11 into out; the files written."""
    sd.use_native(rust)
    old = sd.needed_test
    if not rust:      # the merge as it was: every name tested against every needed path
        sd.needed_test = old_needed_test
    try:
        stock = sorted(lv for lv in sd.level_bundles(sd.find_opfor(None))
                       if lv not in WW2 and not lv.startswith(('ons_', 'opfor_')))
        wanted = sd.read_need(need_path)
        files = []
        for dx in (9, 10, 11):
            base, _ = sd.load('level:mp_005', dx)
            have = {n for n, c in sd.coverage(base, wanted).items() if c}
            dbs = []
            for lv in stock:
                if lv == 'mp_005':
                    continue
                try:
                    db = sd.load(f'level:{lv}', dx)[0]
                except sd.ShaderDbError:
                    continue
                got = {n for n, c in sd.coverage(db, wanted).items() if c} - have
                if got:
                    dbs.append((db, lv))
                    have |= got
            m = sd.merge(base, dbs, wanted)
            p = os.path.join(out, f'Dx{dx}_Single.res')
            with open(p, 'wb') as fh:
                fh.write(base.to_bytes())
            with open(p + '.added.txt', 'w', encoding='utf-8') as fh:
                fh.write('\n'.join(f'{n} <- {lb}' for n, lb in m.added) + '\n' + '\n'.join(m.report) + '\n')
            files += [p, p + '.added.txt']
        return files
    finally:
        sd.needed_test = old
        sd.use_native(True)


def check_load_levels(check: Any) -> None:
    """load_levels against load for each level and Dx: the same databases (bytes, label, dx) or the same error."""
    levels = ['mp_008cq', 'ONS_MP_008', 'mp_common', 'no_such_level', 'mp_002']
    t = time.time()
    got = list(sd.load_levels(levels, (9, 10, 11), chunk=2))
    t1 = time.time()
    bad = []
    for lv, dbs in got:
        for dx in (9, 10, 11):
            try:
                ref: Any = sd.load(f'level:{lv}', dx)[0]
            except sd.ShaderDbError as e:
                ref = e
            new = dbs[dx]
            if isinstance(ref, sd.ShaderDbError):
                ok = isinstance(new, sd.ShaderDbError) and str(new) == str(ref)
            else:
                ok = isinstance(new, sd.ShaderDb) and (new.to_bytes(), new.label, new.dx) == (ref.to_bytes(), ref.label, ref.dx)
            if not ok:
                bad.append(f'{lv} Dx{dx}')
    t2 = time.time()
    check([lv for lv, _ in got] == levels and not bad,
          f'load_levels: {len(levels)} levels x Dx9 / 10 / 11 (a level with no database, an unknown one, other case): '
          f'what load gives, or its error ({t1 - t:.1f} s; load one by one {t2 - t1:.1f} s, parsing included)',
          ', '.join(bad))


def run_merges(root: str, work: str, check: Any) -> None:
    needs = []
    for lv in WW2:
        folder = 'wake' if lv == 'wake_island_s' else lv
        p = os.path.join(root, 'work', 'ww2', folder, 'levels', lv, 'shaders', 'needs_bfbc2.txt')
        if os.path.isfile(p):
            needs.append((lv, p))
    if not needs:
        print('skipped: the WW2 merges (no needs_bfbc2.txt in work\\ww2)')
        return
    for lv, p in needs:
        for side in ('python', 'rust'):
            os.makedirs(os.path.join(work, lv, side))
        t = time.time()
        a = merge_level(p, os.path.join(work, lv, 'python'), False)
        t1 = time.time()
        b = merge_level(p, os.path.join(work, lv, 'rust'), True)
        t2 = time.time()
        same = [open(x, 'rb').read() == open(y, 'rb').read() for x, y in zip(a, b)]
        sizes = ', '.join(f'{os.path.getsize(x):,}' for x in a[::2])
        added = sum(1 for line in open(a[0] + '.added.txt', encoding='utf-8') if ' <- ' in line)
        check(len(a) == len(b) == 6 and all(same),
              f'{lv}: wake_shaders\' merge, Dx9 / 10 / 11 ({sizes} bytes, {added} entries added on Dx9): the same '
              f'files and reports with the Rust core and needed_test as with Python and needed (Python {t1 - t:.0f} s, '
              f'Rust core {t2 - t1:.0f} s)', str(same))


# ------------------------------------------------------------------------------------------------ main

def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py); shaderdb runs on Python')
        return 0
    failed = 0

    def check(ok: object, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    made = built()
    bad = [f'{label}: {d}' for label, data, dx in made if (d := compare(data, dx)[0])]
    check(not bad, f'{len(made)} built databases (Dx9, Dx10, Dx11, Xbox 360 v120 / v117, no kind given, empty): the '
          'same objects and bytes in both backends, written back as they were', '; '.join(bad))

    rng = random.Random(1943)
    kinds: Counter[str] = Counter()
    bad_m = []
    n = 0
    for label, data, dx in made[:5]:
        for m in mutations(data, rng, 400):
            n += 1
            d, k = compare(m, dx)
            kinds[k] += 1
            if d:
                bad_m.append(f'{label} #{n}: {d}')
    check(not bad_m and set(kinds) <= {'ok', 'ShaderDbError'},
          f'{n} damaged databases (cut short, bytes changed, runs written, bytes cut out, counts made huge): the same '
          f'objects or the same ShaderDbError message in both, never another exception ({dict(kinds)})',
          '; '.join(bad_m[:3]))
    noise = [struct.pack('<IB', rng.choice((120, 117)), 0) + bytes(rng.randrange(256) for _ in range(rng.randrange(120)))
             for _ in range(400)]
    noise += [bytes([0, 0, 0, 120, 0]) + bytes(rng.choice((0, 1, 2, 255)) for _ in range(rng.randrange(120)))
              for _ in range(400)]
    bad_n = [i for i, m in enumerate(noise) if compare(m, rng.choice((9, 10, None)))[0]]
    check(not bad_n, f'{len(noise)} random files behind a version: both backends agree', f'first: {bad_n[:5]}')

    # the writer: what only Python takes goes to Python, which raises what it raises
    db = sd.ShaderDb.parse(made[1][1], 10)
    cases: list[tuple[str, Any]] = [
        ('a record as bytearray', lambda d: d.params.__setitem__(0, bytearray(d.params[0]))),
        ('a shader\'s code as bytearray', lambda d: setattr(d.vs[0], 'code', bytearray(d.vs[0].code))),
        ('a name not latin-1', lambda d: d.entries.__setitem__(0, ('Objects/Ж', d.entries[0][1]))),
        ('a group index past u16', lambda d: d.groups.__setitem__(0, (d.groups[0][0], [70000]))),
        ('a negative table index', lambda d: setattr(d.ps[0], 'param', -1)),
        ('a flag past u8', lambda d: setattr(d, 'flag', 300)),
        ('a group as a list', lambda d: d.groups.__setitem__(0, list(d.groups[0]))),
        ('a name as a list', lambda d: d.parts.__setitem__(0, list(d.parts[0]))),
        ('names as a tuple', lambda d: setattr(d, 'fragments', tuple(d.fragments))),
    ]
    bad_w = []
    for what, change in cases:
        got: list[tuple[str, Any]] = []
        for on in (False, True):
            sd.use_native(on)
            wdb = sd.ShaderDb.parse(made[1][1], 10)
            change(wdb)
            try:
                got.append(('ok', wdb.to_bytes()))
            except Exception as e:  # noqa: BLE001
                got.append((type(e).__name__, str(e)))
        if got[0] != got[1]:
            bad_w.append(f'{what}: {got[0][0]} / {got[1][0]}')
    sd.use_native(True)
    check(not bad_w and db.to_bytes() == made[1][1],
          f'the writer: {len(cases)} inputs only the Python writer takes (bytearray records and code, a name not '
          'latin-1, numbers out of their fields, lists for tuples) give the same bytes or the same exception',
          '; '.join(bad_w))

    # needed_test against needed
    words = ['objects', 'Objects', 'a', 'a_b', 'mesh', '', 'x.y', 'A']
    names = ['/'.join(rng.choice(words) for _ in range(rng.randint(1, 5))) for _ in range(3000)]
    names += ['', '/', '//', 'a/', '/a', 'objects/a_b/mesh/']
    bad_t = 0
    for _ in range(200):
        need = [n.lower() for n in rng.sample(names, rng.randint(0, 12))] + rng.sample(['', '/', 'a/'], 1)
        test = sd.needed_test(need)
        bad_t += sum(test(n) != sd.needed(n, need) for n in names)
    check(bad_t == 0, f'needed_test gives needed\'s answer: {len(names)} names (empty parts, case, trailing '
          'slashes) against 200 needed lists', f'{bad_t} differ')

    # a merge both ways: Python reader / writer and needed, Rust core and needed_test
    outs = []
    for rust in (False, True):
        sd.use_native(rust)
        base = ts.build(10, [('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 2)], 7)
        donor = ts.build(10, [('Objects/B/B_Mesh/Objects/B/Shaders/Wood', 3), ('Objects/C/C_Mesh/Objects/C/Shaders/X', 1),
                              ('Objects/B/B_Mesh2/Objects/B/Shaders/Wood', 1)], 8)
        base = sd.ShaderDb.parse(base.to_bytes(), 10)
        donor = sd.ShaderDb.parse(donor.to_bytes(), 10)
        old = sd.needed_test
        if not rust:
            sd.needed_test = old_needed_test
        try:
            mg = sd.merge(base, [(donor, 'donor')], ['objects/b/b_mesh', 'objects/c'])
        finally:
            sd.needed_test = old
        outs.append((base.to_bytes(), mg.added, mg.report))
    sd.use_native(True)
    check(outs[0] == outs[1] and len(outs[0][1]) == 2, 'a merge of built databases: the same file, entries and report '
          'both ways (Python + needed, Rust core + needed_test)', str([o[1] for o in outs]))

    # the dispatcher
    check(sd.backend() == 'rust' and sd.use_native(False) == 'python' and sd.use_native(True) == 'rust',
          'use_native switches the backend')
    env = dict(os.environ, **{dbx.PURE_PYTHON_ENV: '1'})
    code = 'import shaderdb; print(shaderdb.backend())'
    cwd = os.path.join(_ROOT, 'bfbc2-shader-tools')
    forced = subprocess.run([sys.executable, '-c', code], env=env, capture_output=True, text=True, cwd=cwd)
    plain = subprocess.run([sys.executable, '-c', code], capture_output=True, text=True, cwd=cwd,
                           env={k: v for k, v in os.environ.items() if k != dbx.PURE_PYTHON_ENV})
    check(forced.stdout.strip() == 'python' and plain.stdout.strip() == 'rust',
          f'{dbx.PURE_PYTHON_ENV}=1 forces Python; without it the Rust core runs',
          f'{forced.stdout.strip()} / {plain.stdout.strip()} {forced.stderr[-300:]}')
    check(sd._with_shaderdb(None) is None and sd._with_shaderdb(sys) is None,
          'a module without the shader database reader (built before it) is not used')

    try:
        root: str | None = paths.find_opfor()
    except paths.PathsError:
        root = None
    if root is None:
        print('skipped: the game runs (no Project OPFOR BFBC2 game copy)')
    elif '--game' in argv:
        run_corpus(root, check)
        with tempfile.TemporaryDirectory(prefix='shaderdb_merge_') as work:
            run_merges(root, work, check)
    else:
        bad_g = []
        for dx in (9, 10, 11):
            data, label, _ = sd.resolve('level:mp_008cq', dx)
            if (d := compare(data, 10 if dx == 11 else dx)[0]):
                bad_g.append(f'{label}: {d}')
        check(not bad_g, 'mp_008cq\'s three databases: the same objects and bytes in both backends, written back as '
              'they were', '; '.join(bad_g))
    if root is not None:
        check_load_levels(check)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
