r"""The level side of shaderdb in the Rust core (round OR3b): `retexture` (bc2_native.shaderdb_retexture), the level
cover index (`level_cover_prefixes` / `level_coverage` / `donor_levels` on bc2_native.shadercover_levels, its cache
`shadercover` one .bin per level bundle and Dx, keyed by content) and the level scan (`level_scan`: each level
database's entry signatures, textures, layouts, skinning kinds and mesh parts, read in Rust - Project OPFOR's
wake_shaders --clone donor scan).

- retexture against the retired Python's answers recorded before it went (levels_golden.json, "retexture"): built
  parameter blocks and random blocks, swaps, chains, a name that is the tail of a longer one, an empty name, names
  longer than their field, hits added to; each answer's bytes (SHA-256), error and hits.
- the cover index against the retired Python index's answers (levels_golden.json, "cover"): stand-in game copies
  (Dist and Package levels, Package winning; a level without a Dx9 database, with two, with a damaged one, an
  unknown level, a damaged bundle; 24 seeded random copies), each level's prefixes in their order or its error, the
  index's hits and loads on a first and a second pass, level_coverage and donor_levels; then what only the new index
  does: a bundle rewritten is indexed again, a damaged or foreign index file is rebuilt, the cache off.
- the level scan against the Python queries it gathers (load, entry_signatures, entry_textures, entry_layouts, the
  main-pass kinds, the parts), on the stand-in copies.

With --game (Project OPFOR's game copy): the cover index of every level against the golden's digests
(levels_golden_game.json) and against coverage on the loaded databases; the level scan of every level, Dx9 / 10 / 11,
against the Python queries.

  python test_shaderdb_levels.py [--game] [--record]   (--record: write the goldens from the Python twin; only
                                                         before the twin was retired)
"""
from __future__ import annotations

import hashlib
import json
import os
import random
import shutil
import sys
import tempfile
import time
from collections.abc import Callable
from typing import Any, cast

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core')
import shaderdb as sd  # noqa: E402
from bfbc2core import fbrb  # noqa: E402

sys.path.insert(0, HERE)
from test_shaderdb import Checks, build, param_block  # noqa: E402

GOLDEN = os.path.join(HERE, 'levels_golden.json')
GOLDEN_GAME = os.path.join(HERE, 'levels_golden_game.json')
RECORD = '--record' in sys.argv
GAME = '--game' in sys.argv


def digest(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()[:24]


def outcome(fn: Callable[[], Any]) -> Any:
    try:
        return ['ok', fn()]
    except Exception as e:                              # noqa: BLE001 - the answer compared
        return ['err', type(e).__name__, str(e)]


# ------------------------------------------------------------------------------------------------ retexture

NAMES = [b'Textures/A/Metal_C', b'Textures/A/Metal_N', b'Textures/B/Rock', b'Rock', b'A', b'Textures/Caf\xe9_C',
         b'Textures/A/Metal_C_long_name_beyond', b'x' * 0x7f, b'Textures/D/Wood']


def raw_block(rng: random.Random) -> bytes:
    """Random bytes with names placed in them: at the start, after a zero, after another byte (a tail), at the end
    without a terminator, in a field shorter than 0x80."""
    out = bytearray()
    for _ in range(rng.randrange(1, 6)):
        kind = rng.randrange(5)
        n = rng.choice(NAMES)
        if kind == 0:
            out += n + bytes(rng.randrange(0, 0x90))
        elif kind == 1:
            out += b'Z' + n + b'\0' + bytes(rng.randrange(0, 8))
        elif kind == 2:
            out += bytes(rng.randrange(1, 4)) + n + b'\0' + bytes(rng.randrange(1, 4)) + b'Q'
        elif kind == 3:
            out += bytes(rng.randrange(256) for _ in range(rng.randrange(1, 40)))
        else:
            out += bytes(1) + n + b'\0'
    if rng.random() < 0.3:
        out += rng.choice(NAMES)                        # a name ending the block, no terminator
    return bytes(out)


def built_block(rng: random.Random) -> bytes:
    e = rng.choice('<>')
    tex = [(s, rng.choice(NAMES).decode('latin-1')) for s in range(rng.randrange(0, 5))]
    # param_block encodes names as UTF-8: put latin-1 bytes in place afterwards where it matters
    b = param_block(e, [(s, n.encode('latin-1').decode('ascii', 'replace').replace('�', '?')) for s, n in tex],
                    [('Const', 3, 4, (1.0, 2.0, 3.0, 4.0))])
    return b


def renames(rng: random.Random, block: bytes) -> dict[bytes, bytes]:
    out: dict[bytes, bytes] = {}
    pool = NAMES + [b'Textures/A/Metal', b'?', b'']
    for _ in range(rng.randrange(0, 5)):
        old = rng.choice(pool)
        kind = rng.randrange(6)
        if kind == 0:
            new = rng.choice(NAMES)
        elif kind == 1:
            new = old[: rng.randrange(0, len(old) + 1)]
        elif kind == 2:
            new = old + b'_more' * rng.randrange(1, 40)       # longer: refused unless the field's zeros take it
        elif kind == 3:
            new = bytes(rng.randrange(0x20, 0x100) for _ in range(rng.randrange(0, 12)))
        elif kind == 4 and out:
            k = rng.choice(list(out))                           # a swap or a chain through another rename
            new = k
        else:
            new = b''
        out[old] = new
    return out


def retexture_cases() -> list[tuple[str, bytes, dict[bytes, bytes], dict[bytes, int] | None]]:
    cases: list[tuple[str, bytes, dict[bytes, bytes], dict[bytes, int] | None]] = []
    a, b, c = NAMES[0], NAMES[1], NAMES[2]
    blk = param_block('<', [(0, a.decode()), (1, b.decode()), (2, c.decode())])
    cases += [
        ('swap', blk, {a: b, b: a}, {}),
        ('chain', blk, {a: b, b: c, c: b'Textures/Z'}, {a: 2}),
        ('none', blk, {}, None),
        ('absent', blk, {b'Nope': b'X'}, {}),
        ('longer than the field', blk, {a: b'y' * 0x80}, {}),
        ('exactly the field', blk, {a: b'y' * 0x7f}, {}),
        ('tail of a longer name', b'\0Textures/Rock\0' + bytes(20), {b'Rock': b'Sand'}, {}),
        ('at the start', b'Rock\0' + bytes(20), {b'Rock': b'Mud'}, {}),
        ('at the end, no zeros after', b'\0Rock\0', {b'Rock': b'Mud'}, {}),
        ('at the end, too short', b'\0Rock\0', {b'Rock': b'Muddy'}, {}),
        ('empty old name', b'\0\0Rock\0\0\0', {b'': b'Z'}, {}),
        ('empty new name', blk, {a: b''}, {}),
        ('twice in a block', blk + blk, {c: b'T'}, {c: 1, b'other': 5}),
        ('second rename refused, first not counted', blk, {a: b'Q', b: b'y' * 0x90}, {a: 7}),
        ('empty block', b'', {a: b'Q'}, {}),
        ('latin-1 name', b'\0Textures/Caf\xe9_C\0' + bytes(8), {b'Textures/Caf\xe9_C': b'Textures/Th\xe9'}, {}),
        ('latin-1 name too long', b'\0Caf\xe9\0', {b'Caf\xe9': b'Caf\xe9\xe9'}, {}),
        ('overlapping renames', b'\0A\0\0\0\0Rock\0' + bytes(4), {b'': b'', b'A': b'BB'}, {}),
    ]
    rng = random.Random(1943)
    for i in range(400):
        block = built_block(rng) if i % 2 else raw_block(rng)
        hits = None if rng.random() < 0.3 else {n: rng.randrange(3) for n in rng.sample(NAMES, rng.randrange(0, 3))}
        cases.append((f'random {i}', block, renames(rng, block), hits))
    return cases


def retexture_answer(fn: Callable[..., bytes], block: bytes, textures: dict[bytes, bytes],
                     hits: dict[bytes, int] | None) -> list[Any]:
    h = None if hits is None else dict(hits)
    got = outcome(lambda: fn(block, dict(textures), 'block 7', h))
    if got[0] == 'ok':
        got = ['ok', len(got[1]), digest(got[1]), got[1] is block]
    return [got, None if h is None else sorted([k.hex(), v] for k, v in h.items())]


def decided(want: list[Any], got: list[Any]) -> bool:
    """The refusals decided against the Python: a texture name that is not ASCII in the message is named as latin-1,
    as every other shaderdb message (the Python's bytes.decode() read it as UTF-8, or raised UnicodeDecodeError while
    making the message when it was not UTF-8)."""
    if not (want[0][0] == got[0][0] == 'err' and got[0][1] == 'ShaderDbError' and want[1] == got[1]
            and 'longer than the field of' in got[0][2]):
        return False
    if want[0][1] == 'UnicodeDecodeError':
        return True
    try:
        return want[0][1] == 'ShaderDbError' and got[0][2].encode('latin-1') == want[0][2].encode('utf-8')
    except UnicodeEncodeError:
        return False


def check_retexture(c: Checks, golden: dict[str, Any]) -> None:
    cases = retexture_cases()
    if RECORD:
        golden['retexture'] = {name: retexture_answer(sd.retexture, b, t, h) for name, b, t, h in cases}
        return
    want = golden['retexture']
    bad, allowed = [], 0
    for name, b, t, h in cases:
        got = retexture_answer(sd.retexture, b, t, h)
        if got == want[name]:
            continue
        if decided(want[name], got):
            allowed += 1
            continue
        bad.append(f'{name}: {want[name]} != {got}')
    c.check(not bad and len(cases) == len(want),
            f'retexture: {len(cases)} cases as the retired Python answered them ({allowed} refusals naming a '
            f'texture that is not ASCII: named as latin-1, decided)', '; '.join(bad[:3]))
    # the Rust answer for a block that is not bytes: bytearray and memoryview taken as their bytes
    blk = param_block('<', [(0, 'Textures/A/Metal_C')])
    plain = sd.retexture(blk, {b'Textures/A/Metal_C': b'X'}, 'b')
    c.check(sd.retexture(cast(bytes, bytearray(blk)), {b'Textures/A/Metal_C': b'X'}, 'b') == plain
            and sd.retexture(cast(bytes, memoryview(blk)), {b'Textures/A/Metal_C': b'X'}, 'b') == plain,
            'retexture: a bytearray or memoryview block answers as its bytes')


# ------------------------------------------------------------------------------------------------ stand-in game copies

def entry_names(rng: random.Random, n: int) -> list[str]:
    tops = ['Objects/Vehicles/Air/AH64', 'objects/vehicles/air/ah64', 'Objects/Props/Caf\xe9', 'Terrains/MP_A',
            'Objects/Gameplay/Flag_US', 'Characters/WW2/3P', 'Objects/Single']
    out = []
    for _ in range(n):
        top = rng.choice(tops)
        out.append(f'{top}/{rng.choice(["AH64_Mesh", "Mesh", "Shaders", "X"])}/{rng.choice(["Metal", "Glass", "M\xc9TAL", "Rock"])}'
                   + ('' if rng.random() < 0.6 else f'/{rng.choice(["Snow", "A", "a"])}'))
    if n > 2:
        out.append(out[0])                                 # a name twice
        out.append('Single/X')                             # a short name
    return out


def db_bytes(dx: int, names: list[str], seed: int) -> bytes:
    db = build(dx, [(n, 1 + (i % 2)) for i, n in enumerate(names)], seed)
    # build names its parts from the entries; a name listed twice keeps both entries
    return db.to_bytes()


def level_items(lv: str, kind: str, rng: random.Random, seed: int) -> list[tuple[str, str, bytes]]:
    names = entry_names(rng, rng.randrange(1, 12))
    own = f'levels/{lv}/shaders'
    dbs = {dx: db_bytes(dx, names, seed + dx) for dx in (9, 10, 11)}
    items = [(f'levels/{lv}/{lv}.dbx', 'DBX', b'<x/>')]
    if kind == 'nodb9':
        return items + [(f'{own}/dx10_single.res', 'Dx10ShaderDatabase', dbs[10])]
    if kind == 'two':
        return items + [('levels/a/shaders/dx9_single.res', 'Dx9ShaderDatabase', dbs[9]),
                        ('levels/b/shaders/dx9_single.res', 'Dx9ShaderDatabase', dbs[9])]
    if kind == 'bad':
        return items + [(f'{own}/dx9_single.res', 'Dx9ShaderDatabase', b'not a shader database at all' * 3)]
    if kind == 'other':                                     # the level's own among several
        items.append(('levels/other/shaders/dx9_single.res', 'Dx9ShaderDatabase', dbs[9]))
    return items + [(f'{own}/dx9_single.res', 'Dx9ShaderDatabase', dbs[9]),
                    (f'{own}/dx10_single.res', 'Dx10ShaderDatabase', dbs[10]),
                    (f'{own}/dx11_single.res', 'DX11SHADERDATABASE', dbs[11])]


def stand_in(root: str, seed: int) -> list[str]:
    """A game copy under root: levels in Dist and Package; returns the level names to ask for (in an order of
    their own, an unknown one among them)."""
    rng = random.Random(seed)
    levels: list[str] = []
    kinds = ['plain'] * 4 + ['nodb9', 'two', 'bad', 'other']
    for i in range(rng.randrange(3, 9)):
        lv = f'mp_{seed}_{i}' + ('_Case' if rng.random() < 0.2 else '')
        kind = rng.choice(kinds)
        where = rng.choice(['dist', 'package', 'both'])
        for top in (['dist', 'package'] if where == 'both' else [where]):
            d = (os.path.join(root, 'game', 'Dist', 'win32', 'levels', lv) if top == 'dist'
                 else os.path.join(root, 'game', 'Package', 'levels', lv))
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, 'level-00.fbrb'), 'wb') as f:
                f.write(fbrb.build(level_items(lv.lower(), kind, rng, seed * 100 + i + (top == 'package'))))
        levels.append(lv if rng.random() < 0.7 else lv.upper())
    levels.append('no_such_level')
    rng.shuffle(levels)
    return levels


def strip(x: Any, root: str) -> Any:
    if isinstance(x, str):
        return x.replace(root, '<root>')
    if isinstance(x, (list, tuple)):
        return [strip(v, root) for v in x]
    return x


def cover_answer(root: str, levels: list[str], need: list[str]) -> dict[str, Any]:
    """Everything the index answers on one stand-in copy: a pass from an empty cache, a second pass, the cache off,
    level_coverage and donor_levels."""
    def rows(cache: bool) -> list[Any]:
        out = []
        for lv, p in sd.level_cover_prefixes(levels, 9, root, cache):
            out.append([lv, ['err', str(p)] if isinstance(p, sd.ShaderDbError) else ['ok', list(p.items())]])
        return strip(out, root)

    def stats(fn: Callable[[], Any]) -> list[Any]:
        s0 = dict(sd.COVER_STATS)
        got = fn()
        return [got, sd.COVER_STATS['index_hits'] - s0['index_hits'], sd.COVER_STATS['loaded'] - s0['loaded']]

    return {
        'first': stats(lambda: rows(True)),
        'second': stats(lambda: rows(True)),
        'off': stats(lambda: rows(False)),
        'coverage': strip([[lv, ['err', str(p)] if isinstance(p, sd.ShaderDbError) else sorted(p.items())]
                           for lv, p in sd.level_coverage(levels, need, 9, root)], root),
        'donors': sd.donor_levels(levels, need, 9, root),
    }


NEED = ['objects/vehicles/air/ah64/ah64_mesh', 'objects/props/caf\xe9', 'objects/props/caf\xe9/mesh/m\xe9tal',
        'terrains/mp_a', 'single', 'objects/gameplay', 'no/such/mesh', 'Objects/Single/X']


def check_cover(c: Checks, golden: dict[str, Any], scratch: str) -> None:
    old = os.environ.get('BFBC2_SHADERCOVER_CACHE')
    answers: dict[str, Any] = {}
    try:
        for seed in range(24):
            root = os.path.join(scratch, f'copy{seed}')
            levels = stand_in(root, seed)
            os.environ['BFBC2_SHADERCOVER_CACHE'] = os.path.join(scratch, f'cover{seed}')
            answers[f'copy {seed}'] = cover_answer(root, levels, NEED)
        # a damaged bundle among the levels: raised as load_levels raises it
        root = os.path.join(scratch, 'copy0')
        lv = sorted(os.listdir(os.path.join(root, 'game', 'Dist', 'win32', 'levels')))[0]
        p = os.path.join(root, 'game', 'Dist', 'win32', 'levels', lv, 'level-00.fbrb')
        if not os.path.isfile(os.path.join(root, 'game', 'Package', 'levels', lv, 'level-00.fbrb')):
            data = open(p, 'rb').read()
            with open(p, 'wb') as f:
                f.write(data[: len(data) // 2])
            os.environ['BFBC2_SHADERCOVER_CACHE'] = os.path.join(scratch, 'cover_damaged')
            answers['damaged bundle'] = strip(outcome(lambda: list(sd.level_cover_prefixes([lv], 9, root))), root)
    finally:
        if old is None:
            os.environ.pop('BFBC2_SHADERCOVER_CACHE', None)
        else:
            os.environ['BFBC2_SHADERCOVER_CACHE'] = old
    answers = json.loads(json.dumps(answers))
    if RECORD:
        golden['cover'] = answers
        return
    want = golden['cover']
    bad = [k for k in want if want[k] != answers.get(k)]
    n = sum(len(v['first'][0]) for k, v in want.items() if k.startswith('copy'))
    c.check(not bad and set(want) == set(answers),
            f'cover index: 24 stand-in game copies, {n} levels: prefixes in order, errors, hits and loads on a first, '
            f'a second and an uncached pass, level_coverage, donor_levels, a damaged bundle - as the retired Python '
            f'index answered', f'differ: {bad[:4]}' + (f' {want[bad[0]]} != {answers.get(bad[0])}' if bad else ''))


def check_cover_rust(c: Checks, scratch: str) -> None:
    """What only the Rust index does: its files, content keys, damage."""
    old = os.environ.get('BFBC2_SHADERCOVER_CACHE')
    folder = os.path.join(scratch, 'cover_rust')
    os.environ['BFBC2_SHADERCOVER_CACHE'] = folder
    try:
        root = os.path.join(scratch, 'copy_rust')
        levels = stand_in(root, 77)
        first = dict(sd.level_cover_prefixes(levels, 9, root))
        files = sorted(f for f in os.listdir(folder) if not f.startswith('.'))     # but the pruning's markers
        c.check(files and all(f.endswith('.bin') for f in files),
                f'cover index: one .bin per level bundle indexed ({len(files)})', str(files))
        # a bundle rewritten with other entries (same path, size may differ): indexed again
        bundles = sd.level_bundles(root)
        ok_levels = [lv for lv in levels if lv.lower() in bundles and not isinstance(first[lv], sd.ShaderDbError)]
        lv = ok_levels[0]
        b = bundles[lv.lower()]
        st = os.stat(b)
        items = [(f'levels/{lv.lower()}/shaders/dx9_single.res', 'Dx9ShaderDatabase',
                  db_bytes(9, ['Objects/New/New_Mesh/Rewritten'], 5))]
        with open(b, 'wb') as f:
            f.write(fbrb.build(items))
        os.utime(b, ns=(st.st_atime_ns, st.st_mtime_ns))     # the old time: only the content says it changed
        s0 = dict(sd.COVER_STATS)
        again = dict(sd.level_cover_prefixes([lv], 9, root))
        got = again[lv]
        c.check(sd.COVER_STATS['loaded'] - s0['loaded'] == 1 and isinstance(got, dict)
                and 'objects/new/new_mesh/rewritten' in got,
                'cover index: a bundle rewritten in place with its old time is indexed again (content key)')
        # damaged and foreign index files: read again from the bundle, the same answer
        for name in [n for n in os.listdir(folder) if n.endswith('.bin')]:
            p = os.path.join(folder, name)
            data = open(p, 'rb').read()
            with open(p, 'wb') as fh:
                fh.write(data[:-3] + bytes(3) if len(data) > 40 else b'junk')
        s0 = dict(sd.COVER_STATS)
        third = dict(sd.level_cover_prefixes(levels, 9, root))
        same = all(str(third[k]) == str(first[k]) if isinstance(first[k], sd.ShaderDbError) else third[k] == first[k]
                   for k in levels if k != lv)
        c.check(same and sd.COVER_STATS['index_hits'] == s0['index_hits'],
                'cover index: damaged index files are not read (each level read from its bundle again, the same answer)')
        s0 = dict(sd.COVER_STATS)
        fourth = dict(sd.level_cover_prefixes(levels, 9, root))
        loaded = sd.COVER_STATS['loaded'] - s0['loaded']
        shown = {k: str(v) if isinstance(v, sd.ShaderDbError) else v for k, v in third.items()}
        c.check({k: str(v) if isinstance(v, sd.ShaderDbError) else v for k, v in fourth.items()} == shown
                and loaded == 1,
                'cover index: rewritten after damage (a second pass reads only the index; the unknown level loaded)',
                f'{loaded} loaded')
    finally:
        if old is None:
            os.environ.pop('BFBC2_SHADERCOVER_CACHE', None)
        else:
            os.environ['BFBC2_SHADERCOVER_CACHE'] = old


# ------------------------------------------------------------------------------------------------ the level scan

def kinds_ref(db: sd.ShaderDb) -> dict[str, set[int]]:
    progs = db.entry_programs()
    return {name: {db.keys[i][0x33] for i in progs.get(g, []) if db.keys[i][0x30] == 0} for name, g in db.entries}


def scan_ref(levels: list[str], dx: int, root: str) -> list[Any]:
    """The level scan as Python queries make it: S.load of each level, the four queries and the parts."""
    out: list[Any] = []
    for lv in levels:
        try:
            db = sd.load(f'level:{lv}', dx, root)[0]
        except sd.ShaderDbError as e:
            out.append([lv, 'err', str(e)])
            continue
        out.append([lv, 'ok', sd.entry_signatures(db), sd.entry_textures(db), sd.entry_layouts(db), kinds_ref(db),
                    list(db.parts)])
    return out


def scan_got(levels: list[str], dx: int, root: str) -> list[Any]:
    out: list[Any] = []
    for lv, s in sd.level_scan(levels, dx, root):
        if isinstance(s, sd.ShaderDbError):
            out.append([lv, 'err', str(s)])
        else:
            out.append([lv, 'ok', s.signatures, s.textures, s.layouts, s.kinds, s.parts])
    return out


def same_scan(a: list[Any], b: list[Any]) -> list[str]:
    bad = []
    for x, y in zip(a, b):
        if x[:2] != y[:2] or x[2:] != y[2:]:
            bad.append(x[0])
        elif x[1] == 'ok' and any(list(p) != list(q) for p, q in zip(x[2:6], y[2:6])):
            bad.append(f'{x[0]} (order)')
    if len(a) != len(b):
        bad.append(f'{len(a)} != {len(b)} levels')
    return bad


def check_scan(c: Checks, scratch: str) -> None:
    bad: list[str] = []
    n = 0
    for seed in range(24):
        root = os.path.join(scratch, f'scan{seed}')
        levels = stand_in(root, seed)
        for dx in (9, 10, 11):
            ref, got = scan_ref(levels, dx, root), scan_got(levels, dx, root)
            n += len(ref)
            bad += [f'copy {seed} Dx{dx} {x}' for x in same_scan(ref, got)]
    c.check(not bad, f'level scan: {n} level databases (24 stand-in copies, Dx9 / 10 / 11): signatures, textures, '
                     f'layouts, kinds and parts as the Python queries give them, in their order', '; '.join(bad[:4]))


# ------------------------------------------------------------------------------------------------ the game copy

def check_game(c: Checks, golden: dict[str, Any], scratch: str) -> None:
    try:
        root = sd.find_opfor()
    except sd.ShaderDbError:
        print('SKIP --game: no game copy')
        return
    levels = sorted(sd.level_bundles(root))
    old = os.environ.get('BFBC2_SHADERCOVER_CACHE')
    os.environ['BFBC2_SHADERCOVER_CACHE'] = os.path.join(scratch, 'cover_game')
    try:
        t = time.perf_counter()
        got = {lv: (['err', strip(str(p), root)] if isinstance(p, sd.ShaderDbError)
                    else ['ok', len(p), digest(json.dumps(list(p.items())).encode())])
               for lv, p in sd.level_cover_prefixes(levels)}
        t_first = time.perf_counter() - t
        t = time.perf_counter()
        second = dict(sd.level_cover_prefixes(levels))
        t_second = time.perf_counter() - t
    finally:
        if old is None:
            os.environ.pop('BFBC2_SHADERCOVER_CACHE', None)
        else:
            os.environ['BFBC2_SHADERCOVER_CACHE'] = old
    if RECORD:
        with open(GOLDEN_GAME, 'w', encoding='utf-8') as f:
            json.dump({'cover': got}, f, indent=0, sort_keys=True)
            f.write('\n')
        print(f'recorded {GOLDEN_GAME}: {len(got)} levels ({t_first:.1f} s, then {t_second:.2f} s)')
        return
    want = json.load(open(GOLDEN_GAME, encoding='utf-8'))['cover']
    bad = sorted(lv for lv in set(want) | set(got) if want.get(lv) != got.get(lv))
    c.check(not bad and len(second) == len(levels),
            f'cover index, game copy: {len(levels)} levels as the retired Python index answered ({t_first:.1f} s from '
            f'an empty index, {t_second:.2f} s from the index)', str(bad[:5]))
    t_ref = t_got = 0.0
    bad = []
    for dx in (9, 10, 11):
        t = time.perf_counter()
        ref = scan_ref(levels, dx, root)
        t_ref += time.perf_counter() - t
        t = time.perf_counter()
        got_scan = scan_got(levels, dx, root)
        t_got += time.perf_counter() - t
        bad += [f'Dx{dx} {x}' for x in same_scan(ref, got_scan)]
    c.check(not bad, f'level scan, game copy: {len(levels)} levels x Dx9 / 10 / 11 as the Python queries give them '
                     f'(Python {t_ref:.1f} s, Rust {t_got:.1f} s)', '; '.join(bad[:4]))


def main() -> int:
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(errors='backslashreplace')   # a failure's detail may name latin-1 text
    c = Checks()
    golden: dict[str, Any] = {} if RECORD else json.load(open(GOLDEN, encoding='utf-8'))
    scratch = tempfile.mkdtemp(prefix='shaderdb_levels_')
    try:
        check_retexture(c, golden)
        check_cover(c, golden, scratch)
        if RECORD:
            with open(GOLDEN, 'w', encoding='utf-8') as f:
                json.dump(golden, f, indent=0, sort_keys=True)
                f.write('\n')
            print(f'recorded {GOLDEN}')
        else:
            check_cover_rust(c, scratch)
            check_scan(c, scratch)
        if GAME:
            check_game(c, golden, scratch)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    print('PASS' if not c.failed else f'FAIL: {c.failed}')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main())
