r"""shaderdb.py on databases built here (quick), and with --game on Project OPFOR BFBC2's game copy:

- built Dx9 and Dx10/11 databases: parse -> write gives the same bytes, the consistency checks pass, a truncated or
  padded file is refused, a broken file is caught by the checks (an entry GUID that is not MD5(name), programs out
  of order);
- merge: a base and two donors - entries added only for needed meshes the base has none of, from every donor (all
  their shader sets), the first donor winning a name both have; --all-names also for meshes the base has; records
  shared by content are not copied twice, programs re-sorted
  and entry groups renumbered, the merged file re-parses to the same bytes, passes the checks and every entry draws
  with what it drew with in its source; a Dx9 donor for a Dx10 base is refused; the program limit; the same inputs
  give the same bytes; the command line (info, list, merge, roundtrip) on the built files;
- built Xbox 360 (big-endian) databases: recognised from their first bytes, parse and write the same bytes, pass the
  checks (GUIDs with big-endian fields, sorted by their PC byte order); parameter blocks' textures and constants read
  in both byte orders; an Xbox donor for a PC base is refused;
- with --game: the three databases of mp_008cq and ons_mp_008 read from the bundles round trip; ONS_MP_008 merged
  with mp_008cq's flags and flagpole (all three kinds): the flags covered, the base unchanged entry by entry;
- with --game or --xbox (each part skipped when its files are missing): every BFBC2 Xbox level database
  (work\xbox\dist\xenon\levels\<lv>\level-00.fbrb) round trips and passes the checks; Xbox vs PC Dx9 twins of
  mp_002, mp_005, mp_008: entries (GUIDs equal in PC byte order), mesh-part names and their layouts' elements, and
  each entry's textures (sampler, name) match; Battlefield 1943's Wake Island database (Xbox only) reads, with its
  entries' textures (Hangar_t01's printed) and its part layouts equal to bfbc2-console-convert's console_layouts.

  python test_shaderdb.py [--game] [--xbox]
"""
import hashlib
import os
import random
import struct
import subprocess
import sys
import tempfile
from collections.abc import Sequence

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402
suite_paths.use('shader-tools')
import shaderdb as sd  # noqa: E402


class Checks:
    def __init__(self) -> None:
        self.failed = 0

    def check(self, ok: object, what: str, detail: str = '') -> None:
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


# ------------------------------------------------------------------------------------------------ built databases

def guid(rng: random.Random) -> bytes:
    return bytes(rng.randrange(256) for _ in range(16))


def build(dx: int, entries: list[tuple[str, int]], seed: int, shared: bytes = b'',
          big_endian: bool = False) -> sd.ShaderDb:
    """A consistent database. entries: [(name, programs)]; each program gets its own shaders and parameter block,
    plus one parameter block, binding table and register table every file shares (shared: extra bytes in them, so two
    files can share or not). big_endian: an Xbox 360 one (Xenon vertex elements, GUIDs with big-endian fields)."""
    rng = random.Random(seed)
    db = sd.ShaderDb(dx, big_endian)
    e = db.e
    db.version, db.flag = 120, 0
    db.params = [struct.pack(e + 'I', 8 + len(shared)) + b'PARM' + shared]
    db.bindings = [struct.pack(e + 'HH', 1, 7) + b'\x24\x09\x00\x01']
    db.regs = [struct.pack(e + 'I', 1) + b'REGS']
    frag = (f'Systems/Fragment_{seed}', guid(rng))
    db.fragments = [('Systems/Emitter/ShaderFragment', b'F' * 16), frag]
    layout = guid(rng)
    db.layouts = [b'L' * 16 + bytes(0x49), layout + bytes(range(0x49))]
    progs = []
    for name, n in entries:
        g = db.name_guid(name)
        db.entries.append((name, g))
        mesh, material = '/'.join(name.split('/')[:3]), name.rsplit('/', 1)[1]
        db.parts.append((f'{mesh}_lod0/part|{material}/Standard', layout))
        for _ in range(n):
            pi = len(db.params)
            db.params.append(struct.pack(e + 'I', 12) + guid(rng)[:8])
            if big_endian:
                vt = struct.pack(e + 'I', 1) + bytes(12) + bytes([12]) + bytes(15)
            elif dx == 9:
                vt = struct.pack('<I', 1) + bytes(8)
            else:
                vt = struct.pack('<I', 3) + b'abc' + struct.pack('<I', 1) + bytes(0x1c) + struct.pack('<I', 1) + b'POSITION\0'
            db.vs.append(sd.Shader(guid(rng), b'\x01' + guid(rng), 0, 0, 0, vt))
            db.ps.append(sd.Shader(guid(rng), b'\x00' + guid(rng) * 2, pi, 0, 0))
            key = g + (frag[1] if rng.random() < 0.5 else bytes(16)) + layout + bytes(40)
            rec = guid(rng) + bytes(16) + struct.pack(e + '4I', len(db.vs) - 1, len(db.ps) - 1, pi, 0)
            progs.append((rec, key))
    progs.sort(key=lambda t: db.pc_guid(t[0][:16]))
    db.programs = [p for p, _ in progs]
    db.keys = [k for _, k in progs]
    groups: dict[bytes, list[int]] = {}
    for i, k in enumerate(db.keys):
        groups.setdefault(k[:16], []).append(i)
    db.groups = sorted(groups.items(), key=lambda t: db.pc_guid(t[0]))
    db.label = f'built{seed}'
    return db


def param_block(e: str, textures: list[tuple[int, str]],
                constants: Sequence[tuple[str, int, int, tuple[float, ...]]] = ()) -> bytes:
    """A parameter block as DICE's are laid out (FORMAT.md section 2, 'Section 1'): textures [(sampler, name)],
    constants [(name, register, components, 4 values)], one sampler-state record."""
    tex = b''.join(bytes([s, 0, 0, 0]) + name.encode().ljust(0x80, b'\0') + bytes(8) for s, name in textures)
    con = b''.join(name.encode().ljust(0x20, b'\0') + struct.pack(e + 'I', 0x1234) + bytes([reg, comps, 0, 0])
                   + struct.pack(e + '4f', *vals) for name, reg, comps, vals in constants)
    samplers = bytes(4) + bytes([1, 3, 2, 2, 1, 1, 1, 0])
    tex_end = 0x20 + len(tex)
    con_end = tex_end + len(con)
    size = con_end + len(samplers)
    head = struct.pack(e + '6I', size, 0x20, 0x20, tex_end, con_end, con_end)
    head += bytes([1, 1, 0, len(textures), len(constants), 0, 1, 0])
    return head + tex + con + samplers


def with_textures(db: sd.ShaderDb, per_entry: dict[str, list[tuple[int, str]]]) -> sd.ShaderDb:
    """Gives each program of an entry a second parameter block naming the entry's textures (per_entry: {entry
    name: [(sampler, texture name)]})."""
    groups = db.entry_programs()
    for name, g in db.entries:
        if name not in per_entry:
            continue
        k = len(db.params)
        db.params.append(param_block(db.e, per_entry[name], [('DirtColor', 8, 3, (0.5, 0.25, 1.0, 0.0))]))
        for i in groups[g]:
            db.programs[i] = db.programs[i][:0x2c] + struct.pack(db.e + 'I', k)
    return db


def reparse(db: sd.ShaderDb) -> tuple[sd.ShaderDb, bytes]:
    data = db.to_bytes()
    again = sd.ShaderDb.parse(data, 9 if db.dx == 9 else 10)
    again.dx, again.label = db.dx, getattr(db, 'label', '')
    return again, data


def quick(c: Checks, tmp: str) -> None:
    for dx in (9, 10, 11):
        db = build(dx, [('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 3), ('Objects/B/B_Mesh/Objects/B/Shaders/Wood', 2)], 1)
        again, data = reparse(db)
        c.check(again.to_bytes() == data, f'Dx{dx}: built database parses and writes the same bytes')
        c.check(db.check() == [], f'Dx{dx}: built database passes the checks', '; '.join(db.check()))
        c.check(len(again.entries) == 2 and len(again.programs) == 5 and again.vs[0].tail == db.vs[0].tail,
                f'Dx{dx}: sections read back (entries, programs, vertex shader input description)')
        for bad, what in ((data[:-3], 'truncated'), (data + b'\0', 'one byte too many')):
            try:
                sd.ShaderDb.parse(bad, 9 if dx == 9 else 10)
                c.check(False, f'Dx{dx}: a {what} file is refused')
            except sd.ShaderDbError:
                c.check(True, f'Dx{dx}: a {what} file is refused')
    db = build(9, [('M/x_Mesh/S/Shaders/a', 2)], 2)
    db.entries[0] = ('M/x_Mesh/S/Shaders/b', db.entries[0][1])
    c.check(any('MD5' in p for p in db.check()), 'checks: an entry GUID that is not MD5(name) is caught')
    db = build(9, [('M/x_Mesh/S/Shaders/a', 3)], 2)
    db.programs.reverse()
    c.check(any('sorted' in p for p in db.check()), 'checks: programs out of GUID order are caught')
    c.check(sd.split_name('Objects/A/A_Mesh/Objects/A/Shaders/Metal', set()) == ('Objects/A/A_Mesh', 'Objects/A/Shaders/Metal')
            and sd.split_name('Shaders/Decals/Decal_1', set()) == ('', 'Shaders/Decals/Decal_1')
            and sd.split_name('Objects/Edge/Brick/Brick_t01/Objects/Edge/Shaders/X', {'objects/edge/brick/brick_t01'})
            == ('Objects/Edge/Brick/Brick_t01', 'Objects/Edge/Shaders/X'), 'entry names split into mesh and shader')

    # ---- merging
    A, A2, B, B2, C = ('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 'Objects/A/A_Mesh/Objects/A/Shaders/MetalSnow',
                       'Objects/B/B_Mesh/Objects/B/Shaders/Wood', 'Objects/B/B_Mesh/Objects/B/Shaders/WoodSnow',
                       'Objects/C/C_Mesh/Objects/C/Shaders/Glass')
    for dx in (9, 10):
        base = build(dx, [(A, 3)], 10, shared=b'same')
        d1 = build(dx, [(A, 2), (B, 4), (C, 2)], 11, shared=b'same')
        d2 = build(dx, [(B, 1), (B2, 2), (A2, 1)], 12, shared=b'other')
        need = ['objects/a/a_mesh', 'objects/b/b_mesh']
        base_c = sd.entry_contents(base)
        d1_c, d2_c = sd.entry_contents(d1), sd.entry_contents(d2)
        nparams = len(base.params)
        m = sd.merge(base, [(d1, d1.label), (d2, d2.label)], need)
        out, data = reparse(base)
        got = sd.entry_contents(out)
        c.check([n for n, _ in out.entries] == [A, B, B2], f'Dx{dx} merge: B_Mesh\'s entries from both donors (Wood '
                f'from the first, WoodSnow from the second); none for A_Mesh (in the base) or C (not needed)',
                str([n for n, _ in out.entries]))
        c.check(got[A] == base_c[A] and got[B] == d1_c[B] and got[B] != d2_c[B] and got[B2] == d2_c[B2],
                f'Dx{dx} merge: A draws as in the base, Wood as in the first donor (which wins), WoodSnow as in the second')
        c.check(out.check() == [] and sd.verify_merge(out, [(base, base_c), (d1, None), (d2, None)], m) == [],
                f'Dx{dx} merge: the result passes the checks and the content check', '; '.join(out.check()))
        again = sd.ShaderDb.parse(data, 9 if dx == 9 else 10)
        c.check(again.to_bytes() == data, f'Dx{dx} merge: the merged file re-parses to the same bytes')
        c.check(len(out.programs) == 9 and len(out.params) == nparams + 7,
                f'Dx{dx} merge: programs 3 + 4 + 2; the first donor\'s shared parameter block (the base\'s bytes) not '
                f'copied, the second\'s (other bytes) copied once',
                f'{len(out.programs)} programs, {len(out.params)} params')
        c.check([p[:16] for p in out.programs] == sorted(p[:16] for p in out.programs)
                and all(out.keys[i][:16] == g for g, idx in out.groups for i in idx),
                f'Dx{dx} merge: programs re-sorted, entry groups renumbered to them')
        c.check(any(n.startswith('Objects/B/B_Mesh_lod0/') for n, _ in out.parts)
                and len(out.fragments) >= 2 and len({r[:16] for r in out.layouts}) == len(out.layouts),
                f'Dx{dx} merge: B\'s mesh-part names, fragment and vertex layout came with it')
        base2 = build(dx, [(A, 3)], 10, shared=b'same')
        sd.merge(base2, [(d1, d1.label), (d2, d2.label)], need)
        c.check(base2.to_bytes() == data, f'Dx{dx} merge: the same inputs give the same bytes')
        base3 = build(dx, [(A, 3)], 10, shared=b'same')
        sd.merge(base3, [(build(dx, [(A, 2), (B, 4), (C, 2)], 11, shared=b'same'), 'd1'),
                         (build(dx, [(B, 1), (B2, 2), (A2, 1)], 12, shared=b'other'), 'd2')], need, all_names=True)
        names = [n for n, _ in base3.entries]
        c.check(names == [A, B, B2, A2] and base3.check() == [],
                f'Dx{dx} merge --all-names: also MetalSnow, another shader set of a mesh the base has', str(names))
    try:
        sd.merge(build(10, [(A, 1)], 1), [(build(9, [(B, 1)], 2), 'dx9')], ['objects/b/b_mesh'])
        c.check(False, 'a Dx9 donor for a Dx10 base is refused')
    except sd.ShaderDbError:
        c.check(True, 'a Dx9 donor for a Dx10 base is refused')
    old = sd.MAX_PROGRAMS
    sd.MAX_PROGRAMS = 6
    try:
        sd.merge(build(9, [(A, 3)], 1), [(build(9, [(B, 4)], 2), 'big')], ['objects/b/b_mesh'])
        c.check(False, 'the program limit (entry groups index programs with u16) is enforced')
    except sd.ShaderDbError:
        c.check(True, 'the program limit (entry groups index programs with u16) is enforced')
    finally:
        sd.MAX_PROGRAMS = old

    # ---- the command line
    paths: dict[str, str] = {}
    for label, db in (('base', build(9, [(A, 3)], 20)), ('donor', build(9, [(A, 1), (B, 2)], 21))):
        paths[label] = os.path.join(tmp, label, 'Dx9_Single.res')
        os.makedirs(os.path.dirname(paths[label]))
        open(paths[label], 'wb').write(db.to_bytes())
    needf = os.path.join(tmp, 'need.txt')
    open(needf, 'w').write('# needed\nObjects/B/B_Mesh\n')
    out_res = os.path.join(tmp, 'out_res', 'Dx9_Single.res')
    env = {**os.environ, 'LOCALAPPDATA': tmp}

    def run(*args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, os.path.join(ROOT, 'shaderdb.py'), *args], capture_output=True,
                              text=True, errors='replace', env=env)
    r = run('merge', paths['base'], paths['donor'], '--need', needf, '-o', out_res)
    c.check(r.returncode == 0 and os.path.isfile(out_res) and os.path.isfile(out_res + '.merge.txt'),
            'command line: merge writes the file and its report', r.stdout + r.stderr)
    r = run('info', out_res)
    c.check(r.returncode == 0 and 'identical bytes' in r.stdout and 'consistent' in r.stdout,
            'command line: info on the merged file (identical bytes, consistent)', r.stdout + r.stderr)
    r = run('list', out_res, 'b_mesh')
    c.check(r.returncode == 0 and 'Objects/B/B_Mesh  |  Objects/B/Shaders/Wood' in r.stdout,
            'command line: list shows the added entry as mesh | shader', r.stdout + r.stderr)
    r = run('roundtrip', paths['base'], paths['donor'], out_res)
    c.check(r.returncode == 0 and r.stdout.count('ok  ') == 3, 'command line: roundtrip', r.stdout + r.stderr)
    junk = os.path.join(tmp, 'junk.res')
    open(junk, 'wb').write(b'this is not a shader database')
    r = run('info', junk)
    c.check(r.returncode == 1 and 'shader database' in (r.stdout + r.stderr).lower(),
            'command line: a file that is not a database is refused in words', r.stdout + r.stderr)

    # ---- Xbox 360 (big-endian) databases, and parameter blocks
    tex = {A: [(1, 'Textures/A/Metal_Nm'), (2, 'Textures/A/Metal_C')], B: [(1, 'Textures/B/Wood_Df')]}
    for big in (False, True):
        kind = 'Xbox 360' if big else 'PC Dx9'
        db = with_textures(build(9, [(A, 3), (B, 2)], 30, big_endian=big), tex)
        data = db.to_bytes()
        c.check(sd.ShaderDb.is_big_endian(data) == big, f'{kind}: byte order recognised from the first bytes')
        again = sd.ShaderDb.parse(data)
        c.check(again.big_endian == big and again.to_bytes() == data and again.check() == [],
                f'{kind}: built database parses, writes the same bytes and passes the checks', '; '.join(again.check()))
        named = sd.entry_textures(again)
        c.check(named == {A: [(1, 0, 'Textures/A/Metal_Nm'), (2, 0, 'Textures/A/Metal_C')],
                          B: [(1, 0, 'Textures/B/Wood_Df')]},
                f'{kind}: entry_textures gives each entry its textures and samplers', str(named))
        k = again.program_refs(again.entry_programs()[again.entries[0][1]][0])[3]
        consts = sd.block_constants(again.params[k], again.e)
        c.check(consts == [('DirtColor', 8, 3, (0.5, 0.25, 1.0, 0.0))], f'{kind}: block_constants reads a constant',
                str(consts))
        if big:
            c.check(again.entries[0][1] == sd.swap_guid(hashlib.md5(A.encode()).digest()),
                    'Xbox 360: entry GUIDs are MD5(name) with big-endian fields')
            wrong = build(9, [(A, 12)], 31, big_endian=True)
            wrong.programs.sort(key=lambda p: p[:16])         # the file's own byte order: not DICE's
            c.check(wrong.programs != sorted(wrong.programs, key=lambda p: wrong.pc_guid(p[:16]))
                    and any('sorted' in p for p in wrong.check()),
                    'Xbox 360: programs must be sorted by the PC byte order of their GUIDs')
            try:
                sd.merge(build(9, [(A, 1)], 1), [(db, 'xbox')], ['objects/b/b_mesh'])
                c.check(False, 'an Xbox 360 donor for a PC base is refused')
            except sd.ShaderDbError:
                c.check(True, 'an Xbox 360 donor for a PC base is refused')
            path = os.path.join(tmp, 'Single.res')
            open(path, 'wb').write(data)
            r = run('textures', path, 'b_mesh')
            c.check(r.returncode == 0 and 'Textures/B/Wood_Df   [color]' in r.stdout and '1 of 2 entries' in r.stdout,
                    'command line: textures on an Xbox 360 file', r.stdout + r.stderr)
            r = run('info', path)
            c.check(r.returncode == 0 and 'Xbox 360' in r.stdout and 'identical bytes' in r.stdout,
                    'command line: info on an Xbox 360 file', r.stdout + r.stderr)
    c.check([sd.texture_kind(n) for n in ('A_Nm', 'A_C', 'A_Df', 'A_Sp', 'A_Gm', 'A_Occlusion', 'A_RGBM')]
            == ['normal', 'color', 'color', 'specular', 'gloss', 'occlusion', ''], 'texture_kind from name suffixes')


# ------------------------------------------------------------------------------------------------ the game copy

def game(c: Checks, tmp: str) -> None:
    try:
        sd.find_opfor()
    except sd.ShaderDbError as e:
        c.check(True, f'game tests skipped: {e}')
        return
    for spec in ('level:mp_008cq', 'level:ons_mp_008'):
        for dx in (9, 10, 11):
            db, data = sd.load(spec, dx)
            c.check(db.to_bytes() == data and db.check() == [], f'{spec} Dx{dx}: round trip and checks '
                    f'({len(data):,} bytes, {len(db.entries)} entries)', '; '.join(db.check()))
    need = ['objects/gameplay/flag_neutral', 'objects/gameplay/flag_rus', 'objects/gameplay/flag_us',
            'objects/objects/mp_flagpole_mobile_01']
    for dx in (9, 10, 11):
        base, _ = sd.load('level:ons_mp_008', dx)
        donor, _ = sd.load('level:mp_008cq', dx)
        before = sd.entry_contents(base)
        c.check(all(v == 0 for v in sd.coverage(base, need).values()), f'Dx{dx}: ONS_MP_008 has no flag entries')
        m = sd.merge(base, [(donor, donor.label)], need)
        out, data = reparse(base)
        cover = sd.coverage(out, need)
        problems = out.check() + sd.verify_merge(out, [(base, before), (donor, None)], m)
        c.check(all(cover.values()) and not problems and len(m.added) == 5,
                f'Dx{dx}: ONS_MP_008 + mp_008cq: flags and flagpole covered ({len(m.added)} entries), consistent, '
                f'every entry draws as in its source', '; '.join(problems) + str(cover))


# ------------------------------------------------------------------------------------------------ Xbox 360 data

TWINS = ('mp_002', 'mp_005', 'mp_008')


def _elements(body: bytes) -> list[tuple[int, int]]:
    """A layout record body's elements as (usage, stream): the formats differ between Xbox and PC (packed normals)."""
    return [(body[4 * i + 1], body[4 * i + 3]) for i in range(body[64])]


def xbox(c: Checks) -> None:
    opfor: str | None
    try:
        opfor = sd.find_opfor()
    except sd.ShaderDbError as e:
        opfor = None
        print(f'SKIP BFBC2 Xbox levels: {e}')
    levels = os.path.join(opfor, 'work', 'xbox', 'dist', 'xenon', 'levels') if opfor else ''
    if opfor and not os.path.isdir(levels):
        print(f'SKIP BFBC2 Xbox levels: no unpacked Xbox copy at {levels}')
    elif opfor:
        bundles = {lv.lower(): os.path.join(levels, lv, 'level-00.fbrb') for lv in sorted(os.listdir(levels))
                   if os.path.isfile(os.path.join(levels, lv, 'level-00.fbrb'))}
        n = 0
        bad: list[str] = []
        for lv, path in bundles.items():
            if not sd.bundle_databases(path):
                continue                                    # mp_common, sp_common: no database
            db, data = sd.load(path)
            n += 1
            if not db.big_endian or db.to_bytes() != data or db.check():
                bad.append(f'{lv}: {"; ".join(db.check()) or "not read as Xbox 360 / other bytes"}')
        c.check(n > 0 and not bad, f'BFBC2 Xbox: {n} level databases read as Xbox 360, write the same bytes, pass '
                f'the checks', '; '.join(bad[:3]))
        for lv in TWINS:
            if lv not in bundles:
                print(f'SKIP {lv}: no Xbox bundle')
                continue
            x, _ = sd.load(bundles[lv])
            p, _ = sd.load(f'level:{lv}', 9)
            xe, pe = dict(x.entries), dict(p.entries)
            common = xe.keys() & pe.keys()
            c.check(len(common) >= 0.99 * len(xe) and all(sd.swap_guid(xe[n]) == pe[n] for n in common),
                    f'{lv} Xbox vs PC: {len(common)} entries in both (Xbox {len(xe)}, PC {len(pe)}), the same GUIDs '
                    f'in PC byte order', f'{len(xe.keys() - pe.keys())} Xbox only')
            xl, pl = sd.part_layouts(x), sd.part_layouts(p)
            std = [n for n in xl if n.endswith('/Standard')]
            same = [n for n in std if n in pl and _elements(xl[n]) == _elements(pl[n])]
            c.check(len(same) >= 0.98 * len(std), f'{lv} Xbox vs PC: {len(same)} of {len(std)} Xbox mesh parts '
                    f'(Standard) are in PC with the same layout elements (usage, stream); PC has {len(pl)} parts '
                    f'(+ StreamInstancingFloat)')
            tx, tp = sd.entry_textures(x), sd.entry_textures(p)
            eq = [n for n in common if tx[n] == tp[n]]
            names = [n for n in common if {t for _, _, t in tx[n]} == {t for _, _, t in tp[n]}]
            odd = sorted(n for n in common if tx[n] != tp[n] and not n.startswith('Terrains/'))
            c.check(len(eq) >= 0.97 * len(common) and odd == ['Shaders/WaterOcean'],
                    f'{lv} Xbox vs PC: {len(eq)} of {len(common)} entries name the same textures on the same samplers '
                    f'({len(names)} the same texture names; the others terrain patches with an extra Xbox normal map, '
                    f'and the ocean)', str(odd[:4]))
    wake(c)


def wake(c: Checks) -> None:
    xenon = os.environ.get('BF1943_XENON') or os.path.join(ROOT, '..', '..', 'operation-rising-sun', 'work',
                                                           'extracted', 'dist', 'xenon')
    lvl = os.path.join(xenon, 'levels', 'wake_island_s', 'reduced-level-00.fbrb')
    fb = os.path.join(ROOT, '..', '..', 'bf1943-tool-suite', 'bf1943-fbrb-tool')
    if not os.path.isfile(lvl) or not os.path.isfile(os.path.join(fb, 'fbrb1943.py')):
        print('SKIP Battlefield 1943 Wake Island: no extracted copy or no BF1943 bundle reader')
        return
    sys.path.insert(0, fb)
    import fbrb1943
    bundle = fbrb1943.Bundle(open(lvl, 'rb').read())
    name = next(e['name'] for e in bundle.entries if e['type'] == 'Dx9ShaderDatabase')
    data = bundle.data(name)
    db = sd.ShaderDb.parse(data, where=name)
    c.check(db.big_endian and db.version == 117 and db.key_size == 0x5c and db.to_bytes() == data and db.check() == [],
            f'BF1943 Wake {name}: Xbox 360, version 117 (program keys 0x5c bytes), {len(data):,} bytes, '
            f'{len(db.entries)} entries, {len(db.programs)} programs, {len(db.parts)} mesh-part names, '
            f'{len(db.layouts)} vertex layouts; writes the same bytes, passes the checks', '; '.join(db.check()))
    tex = sd.entry_textures(db)
    allt = {t for v in tex.values() for _, _, t in v}
    hangar = [n for n in tex if n.startswith('Objects/Architecture/Beach/Hangar_t01/Hangar_t01_Mesh/')]
    c.check(len(hangar) >= 5 and all(tex[n] for n in hangar),
            f'BF1943 Wake: {sum(1 for v in tex.values() if v)} of {len(tex)} entries name textures, {len(allt)} '
            f'textures in all; Hangar_t01_Mesh has {len(hangar)} entries, each with textures')
    for n in hangar[:3]:
        print(f'     {n}')
        for s, _, t in tex[n]:
            print(f'         s{s}  {t}  {sd.texture_kind(t)}')
    try:
        suite_paths.use('console-convert')
        import mesh
    except ImportError as e:
        print(f'SKIP BF1943 Wake layouts vs console_layouts: {e}')
        return
    mine = {n.lower(): b for n, b in sd.part_layouts(db).items()}
    theirs = mesh.console_layouts(data)
    c.check(theirs and all(mine.get(n) == b for n, b in theirs.items()),
            f'BF1943 Wake: part_layouts agrees with console_layouts on all {len(theirs)} parts it finds '
            f'(part_layouts: {len(mine)}, with the standalone names)')


def main(argv: list[str]) -> int:
    c = Checks()
    with tempfile.TemporaryDirectory(prefix='shaderdb_') as tmp:
        quick(c, tmp)
        if '--game' in argv:
            game(c, tmp)
        if '--game' in argv or '--xbox' in argv:
            xbox(c)
    print(f'{c.failed} failed' if c.failed else 'all passed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
