r"""The shader merge in the Rust core (bc2_native.ShaderMerger, made by shaderdb.merger; round OR3a) against the
Python shaderdb.Merger it retired, on that merger's own answers recorded before it went (merge_golden.json; with
--game merge_golden_game.json):

- built databases (Dx9, Dx10, Dx11, Xbox 360): clone_entry with texture renames (a swap, a chain), from another
  database and from the base itself (an entry cloned earlier in the same merge too: the Python read the base as it
  stood, its programs not yet there), add_part (new, the same again, another layout, a layout no file has),
  add_donor (needed names, all_names, shared and conflicting programs, part conflicts), a kind mismatch, the program
  limit (0x10000), each refusal leaving the merger as it was;
- damaged inputs: programs and shaders pointing past their tables, entry groups past the programs, keys naming
  fragments and vertex layouts no file has, short records, names that are not latin-1, textures longer than their
  field or not used;
- 240 seeded random cases: random databases, damage and operation sequences.

Each operation's answer (its value, or the error's type and message) is compared, and after each one the base as
it stands (its bytes) and the merger's lists (added, report, conflicts, part_conflicts, shared_programs, entry_names,
part_names); after finish the merged bytes and the donors' (never changed). A donor that only exists in memory (its
bytes do not read back as a database: a shader naming a table past its end, a short record) is read in Rust from its
bytes and refused there, with another message: for those only the refusal (a ShaderDbError, nothing added) must
match.

With --game: Project OPFOR's game copy - ONS_MP_008 with mp_008cq's entries cloned (renamed textures), cloned from
itself, mp_008cq's part layouts and its flags merged in, on Dx9, Dx10 and Dx11 (digests only).

  python test_shaderdb_merge.py [--game]
"""
from __future__ import annotations

import functools
import hashlib
import json
import os
import random
import struct
import sys
from collections.abc import Callable, Iterator
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core')
import shaderdb as sd  # noqa: E402

sys.path.insert(0, HERE)
from test_shaderdb import build, param_block  # noqa: E402

GOLDEN = os.path.join(HERE, 'merge_golden.json')
GOLDEN_GAME = os.path.join(HERE, 'merge_golden_game.json')

Op = tuple[Any, ...]
Case = tuple[sd.ShaderDb, list[sd.ShaderDb], list[str], bool, list[Op]]
Maker = Callable[[sd.ShaderDb, list[str], bool], Any]

A = 'Objects/A/A_Mesh/Objects/A/Shaders/Metal'
A2 = 'Objects/A/A_Mesh/Objects/A/Shaders/MetalSnow'
B = 'Objects/B/B_Mesh/Objects/B/Shaders/Wood'
B2 = 'Objects/B/B_Mesh/Objects/B/Shaders/WoodSnow'
C = 'Objects/C/C_Mesh/Objects/C/Shaders/Glass'
N = 'Objects/N/N_Mesh/Objects/N/Shaders/WoodClone'
NAMES = [A, A2, B, B2, C, N, 'Objects/D/D_Mesh/Objects/D/Shaders/Stone', 'Shaders/Decals/Decal_1']
WOOD = [(1, 'Textures/B/Wood_Df'), (2, 'Textures/B/Wood_Nm')]


def textured(db: sd.ShaderDb, per_entry: dict[str, list[tuple[int, str]]]) -> sd.ShaderDb:
    """test_shaderdb.with_textures, tolerant of entries without programs: each program of an entry gets a second
    parameter block naming the entry's textures."""
    groups = db.entry_programs()
    for name, g in db.entries:
        if name in per_entry:
            k = len(db.params)
            db.params.append(param_block(db.e, per_entry[name], [('DirtColor', 8, 3, (0.5, 0.25, 1.0, 0.0))]))
            for i in groups.get(g, []):
                db.programs[i] = db.programs[i][:0x2c] + struct.pack(db.e + 'I', k)
    return db


def kind_db(dx: int | str, entries: list[tuple[str, int]], seed: int, shared: bytes = b'') -> sd.ShaderDb:
    if dx == 'xbox':
        return build(9, entries, seed, shared, big_endian=True)
    assert isinstance(dx, int)
    return build(dx, entries, seed, shared)


def part_of(db: sd.ShaderDb, i: int = 0) -> tuple[str, bytes]:
    n, g = db.parts[i % len(db.parts)]
    return n, g


# ------------------------------------------------------------------------------------------------ the cases

def built_cases() -> Iterator[tuple[str, Callable[[], Case]]]:
    for dx in (9, 10, 11, 'xbox'):
        def clones(dx: int | str = dx) -> Case:
            base = textured(kind_db(dx, [(A, 2)], 40), {A: [(1, 'Textures/A/Metal_C')]})
            donor = textured(kind_db(dx, [(B, 3), (C, 1)], 41), {B: WOOD, C: [(3, 'Textures/C/Glass_C')]})
            swap = {'Textures/B/Wood_Df': 'Textures/B/Wood_Nm', 'Textures/B/Wood_Nm': 'Textures/B/Wood_Df'}
            chain = {'Textures/B/Wood_Df': 'Textures/B/Wood_Nm', 'Textures/B/Wood_Nm': 'Textures/B/Wood_X'}
            return base, [donor], [], False, [
                ('clone', 0, B, N, swap, 'd'), ('clone', 0, B, N + '2', chain, 'd'), ('clone', 0, B, A, None, 'd'),
                ('clone', 0, B, N, None, 'd'), ('clone', 0, 'Objects/Q/Shaders/None', N + '3', None, 'd'),
                ('clone', 0, B, N + '4', {'Textures/B/wood_df': 'x'}, 'd'),
                ('clone', 0, B, N + '5', {'Textures/B/Wood_Df': 'X' * 0x80}, 'd'),
                ('clone', 0, B, N + '6', {'Textures/B/Wood_Df': 'Textures/B/Wood_D' + 'f' * 0x6e}, 'd'),
                ('clone', 'base', A, 'Objects/S/S_Mesh/Objects/S/Shaders/Self', {'Textures/A/Metal_C': 'Textures/S/C'},
                 'self'),
                ('clone', 'base', N, 'Objects/S/S_Mesh/Objects/S/Shaders/OfClone', None, 'self'),
                ('clone', 0, C, 'Objects/C/C_Mesh/Objects/C/Shaders/Glass2', {}, 'd'),
                ('clone', 0, B, 'Objects/Ā/Shaders/X', None, 'd'),
                ('finish',)]
        yield f'built {dx}: clone_entry', clones

        def parts(dx: int | str = dx) -> Case:
            base = kind_db(dx, [(A, 1)], 50)
            donor = kind_db(dx, [(B, 2)], 51)
            bn, bg = part_of(base)
            dn, dg = part_of(donor)
            return base, [donor], [], False, [
                ('part', 'Objects/N/N_Mesh_lod0/part/Standard', dg, 0, 'd'), ('part', bn, bg, 0, 'd'),
                ('part', bn, dg, 0, 'd'), ('part', 'Objects/N/N_Mesh_lod1/part/Standard', b'Z' * 16, 0, 'd'),
                ('part', 'Objects/N/N_Mesh_lod0/part/Standard', dg, 0, 'd'),
                ('part', 'Objects/S/S_Mesh_lod0/p/Standard', bg, 'base', 'self'),
                ('part', dn, dg, 0, 'd'), ('finish',)]
        yield f'built {dx}: add_part', parts

        def donors(dx: int | str = dx) -> Case:
            base = kind_db(dx, [(A, 3)], 10, shared=b'same')
            d1 = kind_db(dx, [(A, 2), (B, 4), (C, 2)], 11, shared=b'same')
            d2 = kind_db(dx, [(B, 1), (B2, 2), (A2, 1)], 12, shared=b'other')
            # a program of d2 with one of d1's ids and other content (a conflict), one with d1's id and content (shared)
            g1 = d1.entry_programs()[d1.name_guid(B)]
            g2 = d2.entry_programs()[d2.name_guid(B2)]
            d2.programs[g2[0]] = d1.programs[g1[0]][:16] + d2.programs[g2[0]][16:]
            # a part name both have, another layout in d2
            d2.parts.append((d1.parts[1][0], d2.parts[0][1]))
            return base, [d1, d2], ['objects/b/b_mesh', 'objects/a/a_mesh'], False, [
                ('donor', 0, 'd1'), ('donor', 1, 'd2'), ('clone', 1, A2, 'Objects/A/A_Mesh/Objects/A/Shaders/X', None,
                                                          'd2'), ('donor', 1, 'again'), ('finish',)]
        yield f'built {dx}: add_donor', donors

        def all_names(dx: int | str = dx) -> Case:
            base = kind_db(dx, [(A, 3)], 10, shared=b'same')
            d1 = kind_db(dx, [(A, 2), (B, 4), (C, 2)], 11, shared=b'same')
            d2 = kind_db(dx, [(B, 1), (B2, 2), (A2, 1)], 12, shared=b'other')
            return base, [d1, d2], ['objects/b/b_mesh', 'objects/a/a_mesh', 'objects/c'], True, [
                ('donor', 0, 'd1'), ('donor', 1, 'd2'), ('finish',)]
        yield f'built {dx}: add_donor, all_names', all_names

    def kinds() -> Case:
        base = build(10, [(A, 1)], 1)
        return base, [build(9, [(B, 1)], 2), build(9, [(B, 1)], 3, big_endian=True), build(11, [(B, 1)], 4)], \
            ['objects/b/b_mesh'], False, [('donor', 0, 'dx9'), ('donor', 1, 'xbox'), ('clone', 0, B, N, None, 'dx9'),
                                          ('part', 'x_lod0/p/Standard', b'L' * 16, 1, 'xbox'), ('donor', 2, 'dx11'),
                                          ('finish',)]
    yield 'kind mismatch', kinds

    def limit() -> Case:
        base = build(9, [(A, 1)], 5)
        # 0xfffe programs on one shader pair: the clone of a 1-program entry stays under the limit, of 2 reaches it
        p0, k0 = base.programs[0], base.keys[0]
        rng = random.Random(7)
        progs = [(rng.randbytes(16) + p0[16:], k0) for _ in range(0xfffe - 1)] + [(p0, k0)]
        progs.sort(key=lambda t: t[0][:16])
        base.programs, base.keys = [p for p, _ in progs], [k for _, k in progs]
        base.groups = [(base.name_guid(A), list(range(len(progs))))]
        donor = build(9, [(B, 2), (C, 1)], 6)
        return base, [donor], [], False, [('clone', 0, B, N, None, 'd'), ('finish',)]
    yield 'program limit: 0xfffe + 2', limit

    def limit_ok() -> Case:
        base, donors, need, alln, _ = limit()
        return base, donors, need, alln, [('clone', 0, C, N, None, 'd'), ('finish',)]
    yield 'program limit: 0xfffe + 1', limit_ok

    def empty() -> Case:
        return build(9, [(A, 1)], 8), [], [], False, [('finish',)]
    yield 'nothing added', empty


def damaged_cases() -> Iterator[tuple[str, Callable[[], Case]]]:
    def damage(how: str) -> Callable[[], Case]:
        def make() -> Case:
            base = build(9, [(A, 1)], 60)
            donor = textured(build(9, [(B, 3)], 61), {B: WOOD})
            g = donor.entry_programs()[donor.name_guid(B)]
            i = g[-1]
            e = donor.e
            if how == 'program refs past their tables':
                donor.programs[i] = donor.programs[i][:0x20] + struct.pack(e + '4I', 99, 0, 0, 0)
            elif how == 'a parameter block index past its table':
                donor.programs[i] = donor.programs[i][:0x2c] + struct.pack(e + 'I', 999)
            elif how == 'an entry group past the programs':
                donor.groups = [(gg, idx + [len(donor.programs) + 3] if gg == donor.name_guid(B) else idx)
                                for gg, idx in donor.groups]
            elif how == 'a fragment no file names':
                donor.keys[i] = donor.keys[i][:16] + b'Z' * 16 + donor.keys[i][32:]
            elif how == 'a vertex layout no file has':
                donor.keys[i] = donor.keys[i][:32] + b'Q' * 16 + donor.keys[i][48:]
            elif how == 'a shader table past its end (memory only)':
                s = donor.ps[sd.ShaderDb.program_refs(donor, i)[1]]
                donor.ps.append(sd.Shader(s.guid, s.code, 999, s.binding, s.regs))
                donor.programs[i] = donor.programs[i][:0x24] + struct.pack(e + 'I', len(donor.ps) - 1) + \
                    donor.programs[i][0x28:]
            elif how == 'a short key (memory only)':
                donor.keys[i] = donor.keys[i][:40]
            elif how == 'a short program (memory only)':
                donor.programs[i] = donor.programs[i][:0x28]
            elif how == 'a duplicated program in a group':
                donor.groups = [(gg, idx + idx[:1] if gg == donor.name_guid(B) else idx) for gg, idx in donor.groups]
            elif how == 'two entries of one name':
                donor.entries.append((B, b'G' * 16))
            elif how == 'a fragment named twice':
                f = donor.fragments[1]
                donor.fragments.append(('Systems/Other', f[1]))
            ops: list[Op] = [('clone', 0, B, N, {'Textures/B/Wood_Df': 'Textures/B/W'}, 'd'),
                             ('clone', 0, B, N + '2', None, 'd')]
            if 'memory only' not in how:
                # (a donor only memory holds: the Python took its layouts and needed entries as far as they went,
                # the Rust core reads the donor's bytes and refuses it whole - only clone_entry, atomic in both, is
                # compared)
                ops += [('part', 'Objects/N/N_Mesh_lod0/p/Standard', donor.parts[0][1], 0, 'd'), ('donor', 0, 'd')]
            return base, [donor], ['objects/b/b_mesh'], False, ops + [('finish',)]
        return make
    for how in ('program refs past their tables', 'a parameter block index past its table',
                'an entry group past the programs', 'a fragment no file names', 'a vertex layout no file has',
                'a shader table past its end (memory only)', 'a short key (memory only)',
                'a short program (memory only)', 'a duplicated program in a group', 'two entries of one name',
                'a fragment named twice'):
        yield f'damaged: {how}', damage(how)


def random_case(seed: int) -> Case:
    rng = random.Random(seed)
    kinds: list[int | str] = [9, 10, 11, 'xbox']
    dx = rng.choice(kinds)

    def entries() -> list[tuple[str, int]]:
        names = rng.sample(NAMES, rng.randint(1, 4))
        return [(n, rng.randint(0, 3)) for n in names]
    base = kind_db(dx, entries(), seed * 7 + 1, shared=rng.choice([b'', b'same']))
    donors = []
    for k in range(rng.randint(1, 3)):
        d = kind_db(dx if rng.random() < 0.95 else 9, entries(), seed * 7 + 2 + k, shared=rng.choice([b'', b'same']))
        named = {n: rng.sample([(1, 'Tex/A'), (2, 'Tex/B'), (3, 'Tex/Longer/Name_C'), (4, 'Tex/A_Nm')],
                               rng.randint(1, 3)) for n, _ in d.entries if rng.random() < 0.7}
        textured(d, named)
        if d.programs and rng.random() < 0.3:          # damage of a kind a file can hold
            i = rng.randrange(len(d.programs))
            kind = rng.randrange(5)
            if kind == 0:
                d.programs[i] = d.programs[i][:0x20] + struct.pack(d.e + 'I', 500) + d.programs[i][0x24:]
            elif kind == 1:
                d.keys[i] = d.keys[i][:16] + rng.randbytes(16) + d.keys[i][32:]
            elif kind == 2:
                d.keys[i] = d.keys[i][:32] + rng.randbytes(16) + d.keys[i][48:]
            elif kind == 3 and d.groups:
                g, idx = d.groups[0]
                d.groups[0] = (g, idx + [len(d.programs) + rng.randrange(4)])
            elif kind == 4 and base.programs:
                d.programs[i] = base.programs[0][:16] + d.programs[i][16:]
        if d.parts and rng.random() < 0.3:
            d.parts.append((d.parts[0][0], rng.choice([d.layouts[0][:16], rng.randbytes(16)])))
        donors.append(d)
    ops: list[Op] = []
    made: list[str] = []
    for _ in range(rng.randint(2, 9)):
        r = rng.random()
        di: Any = rng.randrange(len(donors))
        d = donors[di]
        if r < 0.5:
            if rng.random() < 0.15:
                di, d = 'base', base
            pool = [n for n, _ in d.entries] + made + ['Objects/Missing/Shaders/X']
            entry = rng.choice(pool)
            new = rng.choice(NAMES + [f'Objects/R{seed}/R_Mesh/Objects/R/Shaders/C{len(ops)}'] * 3 + made)
            tex: dict[str, str] | None = None
            if rng.random() < 0.6:
                olds = ['Tex/A', 'Tex/B', 'Tex/Longer/Name_C', 'Tex/A_Nm', 'tex/a']
                tex = {o: rng.choice(['Tex/Z', 'Tex/B', 'Tex/A', 'T' * rng.choice([5, 20, 0x7f, 0x80])])
                       for o in rng.sample(olds, rng.randint(0, 3))}
            ops.append(('clone', di, entry, new, tex, f'l{di}'))
            made.append(new)
        elif r < 0.75:
            if rng.random() < 0.1:
                di, d = 'base', base
            name = rng.choice([n for n, _ in d.parts + base.parts] + ['Objects/R/R_Mesh_lod0/p/Standard'])
            lg = rng.choice([r_[:16] for r_ in d.layouts] + [rng.randbytes(16)])
            ops.append(('part', name, lg, di, f'l{di}'))
        else:
            ops.append(('donor', di, f'l{di}'))
    ops.append(('finish',))
    need = rng.sample(['objects/a/a_mesh', 'objects/b/b_mesh', 'objects/c', 'objects/d/d_mesh', 'shaders/decals',
                       'objects'], rng.randint(0, 3))
    return base, donors, need, rng.random() < 0.3, ops


RANDOM = 240


def all_cases() -> Iterator[tuple[str, Callable[[], Case]]]:
    yield from built_cases()
    yield from damaged_cases()
    for seed in range(RANDOM):
        yield f'random {seed}', functools.partial(random_case, seed)


# ------------------------------------------------------------------------------------------------ running

def digest(b: bytes) -> str:
    return hashlib.sha1(b).hexdigest()


def db_digest(db: sd.ShaderDb) -> str:
    try:
        return digest(db.to_bytes())
    except Exception as e:                                  # noqa: BLE001 - recorded as it is
        return f'unwritable: {type(e).__name__}'


def merger_state(m: Any) -> str:
    def plain(x: Any) -> Any:
        return x.hex() if isinstance(x, bytes) else x
    state = {'added': [list(a) for a in m.added], 'report': list(m.report), 'conflicts': list(m.conflicts),
             'part_conflicts': list(m.part_conflicts), 'shared_programs': m.shared_programs,
             'entry_names': sorted(m.entry_names), 'part_names': sorted((k, plain(v)) for k, v in m.part_names.items())}
    return digest(json.dumps(state, sort_keys=True).encode())


def readable(db: sd.ShaderDb) -> bool:
    """Do the database's bytes read back as the same database (a file can hold it)?"""
    try:
        data = db.to_bytes()
        return sd.ShaderDb.parse(data, 9 if db.dx == 9 else 10, big_endian=db.big_endian).to_bytes() == data
    except Exception:                                       # noqa: BLE001 - not a file
        return False


def run_case(case: Case, make: Maker) -> list[dict[str, Any]]:
    base, donors, need, all_names, ops = case
    out: list[dict[str, Any]] = []
    try:
        m = make(base, need, all_names)
    except Exception as e:                                  # noqa: BLE001 - recorded
        return [{'op': 'new', 'error': [type(e).__name__, str(e)]}]
    for op in ops:
        rec: dict[str, Any] = {'op': op[0]}

        def donor(ref: Any) -> sd.ShaderDb:
            return base if ref == 'base' else donors[ref]
        try:
            if op[0] == 'clone':
                _, di, entry, new, tex, label = op
                rec['memory'] = not readable(donor(di))
                rec['value'] = m.clone_entry(donor(di), entry, new, tex, label)
            elif op[0] == 'part':
                _, name, lg, di, label = op
                rec['memory'] = not readable(donor(di))
                m.add_part(name, lg, donor(di), label)
            elif op[0] == 'donor':
                _, di, label = op
                rec['memory'] = not readable(donor(di))
                m.add_donor(donor(di), label)
            else:
                m.finish()
                rec['check'] = base.check()
                rec['donors'] = [db_digest(d) for d in donors]
        except Exception as e:                              # noqa: BLE001 - recorded
            rec['error'] = [type(e).__name__, str(e)]
        rec['base'] = db_digest(base)
        rec['merger'] = merger_state(m)
        out.append(rec)
    return out


def compare(name: str, want: list[dict[str, Any]], got: list[dict[str, Any]]) -> list[str]:
    """The differences (empty: the same answers). A donor only memory holds may be refused with another message."""
    bad = []
    if len(want) != len(got):
        return [f'{name}: {len(got)} answers, {len(want)} recorded']
    for i, (w, g) in enumerate(zip(want, got, strict=True)):
        w2, g2 = dict(w), dict(g)
        if 'error' in w and 'error' in g and w['error'][0] == g['error'][0] == 'IndexError':
            w2.pop('error')                 # a database index outside its section: the Python's message was Python's
            g2.pop('error')
        elif 'error' in w and 'error' in g and g['error'][0] == 'ShaderDbError' and (
                (w.get('memory') and w['error'][0] == 'ShaderDbError')
                or (w['error'][0] == 'UnicodeEncodeError' and 'latin-1' in g['error'][1])):
            # a donor only memory holds, refused when read; a name that is not latin-1, refused in words (the Python
            # raised UnicodeEncodeError)
            w2.pop('error')
            g2.pop('error')
        if w2 != g2:
            keys = sorted(k for k in set(w2) | set(g2) if w2.get(k) != g2.get(k))
            bad.append(f'{name} op {i} ({w["op"]}): ' + '; '.join(f'{k}: {w2.get(k)!r} recorded, {g2.get(k)!r} now'
                                                               for k in keys))
    return bad


def native_maker(base: sd.ShaderDb, need: list[str], all_names: bool) -> Any:
    return sd.merger(base, need, all_names)


# ------------------------------------------------------------------------------------------------ the game copy

GAME_LEVELS = ('level:ons_mp_008', 'level:mp_008cq')


def game_cases() -> Iterator[tuple[str, Callable[[], Case]]]:
    for dx in (9, 10, 11):
        def make(dx: int = dx) -> Case:
            base = sd.load(GAME_LEVELS[0], dx)[0]
            donor = sd.load(GAME_LEVELS[1], dx)[0]
            tex = sd.entry_textures(donor)
            names = sorted(n for n, t in tex.items() if len(t) >= 2)[:12]
            ops: list[Op] = []
            for k, n in enumerate(names):
                ts = [t[2] for t in tex[n]]
                ren = {ts[0]: ts[1], ts[1]: ts[0]} if k % 3 == 0 else {ts[0]: ts[0][:-1]} if k % 3 == 1 else None
                ops.append(('clone', 0, n, f'Objects/OR3a/Clone_{k}_Mesh/{n.rsplit("/", 1)[-1]}', ren, 'mp_008cq'))
            own = sorted(n for n, _ in base.entries)[:4]
            ops += [('clone', 'base', n, f'Objects/OR3a/Self_{k}_Mesh/x', None, 'self') for k, n in enumerate(own)]
            ops += [('part', f'Objects/OR3a/Part_{k}_Mesh_lod0/{p.rsplit("/", 2)[-2]}/Standard', g, 0, 'mp_008cq')
                    for k, (p, g) in enumerate(donor.parts[:20])]
            ops += [('donor', 0, 'mp_008cq'), ('finish',)]
            return base, [donor], ['objects/gameplay/flag_neutral', 'objects/gameplay/flag_rus',
                                   'objects/gameplay/flag_us', 'objects/objects/mp_flagpole_mobile_01'], False, ops
        yield f'game Dx{dx}: ONS_MP_008 + mp_008cq', make


# ------------------------------------------------------------------------------------------------ main

def record(path: str, cases: Iterator[tuple[str, Callable[[], Case]]], make: Maker) -> None:
    """Writes the answers of `make` (the Python merger, before it was retired) as the goldens."""
    got = {name: run_case(c(), make) for name, c in cases}
    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(got, f, indent=0, sort_keys=True)
        f.write('\n')


def check(path: str, cases: Iterator[tuple[str, Callable[[], Case]]]) -> int:
    with open(path, encoding='utf-8') as f:
        golden = json.load(f)
    bad: list[str] = []
    seen = 0
    ops = 0
    for name, c in cases:
        seen += 1
        if name not in golden:
            bad.append(f'{name}: no recorded answers')
            continue
        got = run_case(c(), native_maker)
        ops += len(got)
        bad += compare(name, golden[name], got)
    if seen != len(golden):
        bad.append(f'{len(golden)} cases recorded, {seen} run')
    for line in bad[:40]:
        print('FAIL ' + line)
    print(f'{"ok  " if not bad else "FAIL"} {os.path.basename(path)}: {seen} cases, {ops} operations against the '
          f'Python merger\'s recorded answers' + (f' ({len(bad)} differences)' if bad else ''))
    return 1 if bad else 0


def native_only() -> int:
    """What only the Rust merger has to show: a donor is not kept alive and is read again once changed; a part name
    that is not latin-1 is refused (the Python appended it and failed when writing); the base holds what was added
    before finish, its programs after; no call after finish."""
    import gc
    import weakref
    bad: list[str] = []

    def ok(cond: bool, what: str) -> None:
        print(f'{"ok  " if cond else "FAIL"} {what}')
        if not cond:
            bad.append(what)
    base = build(9, [(A, 2)], 70)
    donor = textured(build(9, [(B, 3)], 71), {B: WOOD})
    m = sd.merger(base)
    n0 = len(base.programs)
    ok(m.clone_entry(donor, B, N, None, 'd') == 3 and N in [n for n, _ in base.entries]
       and len(base.programs) == n0 and N in m.entry_names,
       'between calls the base holds the new entry (its programs join at finish)')
    g = donor.name_guid(B)
    i = donor.entry_programs()[g][0]
    donor.keys[i] = donor.keys[i][:32] + b'Q' * 16 + donor.keys[i][48:]          # changed: read again
    try:
        m.clone_entry(donor, B, N + '2', None, 'd')
        ok(False, 'a donor changed between calls is read again')
    except sd.ShaderDbError as e:
        ok('vertex layout' in str(e), f'a donor changed between calls is read again ({e})')
    gone = weakref.ref(donor)
    del donor
    gc.collect()
    ok(gone() is None, 'the merger keeps no donor alive')
    before = base.to_bytes()
    try:
        m.add_part('Objects/Ā_lod0/p/Standard', base.layouts[0][:16], base, 'own')
        ok(False, 'a part name that is not latin-1 is refused')
    except sd.ShaderDbError:
        ok(base.to_bytes() == before, 'a part name that is not latin-1 is refused, nothing added')
    out = m.finish()
    ok(out is base and len(base.programs) == n0 + 3 and not base.check(), 'finish re-sorts and returns the base')
    try:
        m.clone_entry(base, A, N + '3')
        ok(False, 'no call after finish')
    except sd.ShaderDbError:
        ok(True, 'no call after finish')
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    failed = native_only()
    failed |= check(GOLDEN, all_cases())
    if '--game' in argv:
        try:
            sd.find_opfor()
        except sd.ShaderDbError as e:
            print(f'skipped: the game copy ({e})')
        else:
            failed |= check(GOLDEN_GAME, game_cases())
    print('all passed' if not failed else 'failed')
    return failed


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
