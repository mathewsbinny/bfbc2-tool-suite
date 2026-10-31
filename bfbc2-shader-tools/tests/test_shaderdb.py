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
- with --game: the three databases of mp_008cq and ons_mp_008 read from the bundles round trip; ONS_MP_008 merged
  with mp_008cq's flags and flagpole (all three kinds): the flags covered, the base unchanged entry by entry.

  python test_shaderdb.py [--game]
"""
import hashlib
import os
import random
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, ROOT)
import shaderdb as sd  # noqa: E402


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, ok, what, detail=''):
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


# ------------------------------------------------------------------------------------------------ built databases

def guid(rng):
    return bytes(rng.randrange(256) for _ in range(16))


def build(dx, entries, seed, shared=b''):
    """A consistent database. entries: [(name, programs)]; each program gets its own shaders and parameter block,
    plus one parameter block, binding table and register table every file shares (shared: extra bytes in them, so two
    files can share or not)."""
    rng = random.Random(seed)
    db = sd.ShaderDb(dx)
    db.version, db.flag = 120, 0
    db.params = [struct.pack('<I', 8 + len(shared)) + b'PARM' + shared]
    db.bindings = [struct.pack('<HH', 1, 7) + b'\x24\x09\x00\x01']
    db.regs = [struct.pack('<I', 1) + b'REGS']
    frag = (f'Systems/Fragment_{seed}', guid(rng))
    db.fragments = [('Systems/Emitter/ShaderFragment', b'F' * 16), frag]
    layout = guid(rng)
    db.layouts = [b'L' * 16 + bytes(0x49), layout + bytes(range(0x49))]
    progs = []
    for name, n in entries:
        g = hashlib.md5(name.encode('latin-1')).digest()
        db.entries.append((name, g))
        mesh, material = '/'.join(name.split('/')[:3]), name.rsplit('/', 1)[1]
        db.parts.append((f'{mesh}_lod0/part|{material}/Standard', layout))
        for _ in range(n):
            pi = len(db.params)
            db.params.append(struct.pack('<I', 12) + guid(rng)[:8])
            vt = struct.pack('<I', 1) + bytes(8) if dx == 9 else \
                struct.pack('<I', 3) + b'abc' + struct.pack('<I', 1) + bytes(0x1c) + struct.pack('<I', 1) + b'POSITION\0'
            db.vs.append(sd.Shader(guid(rng), b'\x01' + guid(rng), 0, 0, 0, vt))
            db.ps.append(sd.Shader(guid(rng), b'\x00' + guid(rng) * 2, pi, 0, 0))
            key = g + (frag[1] if rng.random() < 0.5 else bytes(16)) + layout + bytes(40)
            rec = guid(rng) + bytes(16) + struct.pack('<4I', len(db.vs) - 1, len(db.ps) - 1, pi, 0)
            progs.append((rec, key))
    progs.sort(key=lambda t: t[0][:16])
    db.programs = [p for p, _ in progs]
    db.keys = [k for _, k in progs]
    groups = {}
    for i, k in enumerate(db.keys):
        groups.setdefault(k[:16], []).append(i)
    db.groups = sorted(groups.items())
    db.label = f'built{seed}'
    return db


def reparse(db):
    data = db.to_bytes()
    again = sd.ShaderDb.parse(data, 9 if db.dx == 9 else 10)
    again.dx, again.label = db.dx, getattr(db, 'label', '')
    return again, data


def quick(c, tmp):
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
    paths = {}
    for label, db in (('base', build(9, [(A, 3)], 20)), ('donor', build(9, [(A, 1), (B, 2)], 21))):
        paths[label] = os.path.join(tmp, label, 'Dx9_Single.res')
        os.makedirs(os.path.dirname(paths[label]))
        open(paths[label], 'wb').write(db.to_bytes())
    needf = os.path.join(tmp, 'need.txt')
    open(needf, 'w').write('# needed\nObjects/B/B_Mesh\n')
    out = os.path.join(tmp, 'out', 'Dx9_Single.res')
    env = {**os.environ, 'LOCALAPPDATA': tmp}

    def run(*args):
        return subprocess.run([sys.executable, os.path.join(ROOT, 'shaderdb.py'), *args], capture_output=True,
                              text=True, errors='replace', env=env)
    r = run('merge', paths['base'], paths['donor'], '--need', needf, '-o', out)
    c.check(r.returncode == 0 and os.path.isfile(out) and os.path.isfile(out + '.merge.txt'),
            'command line: merge writes the file and its report', r.stdout + r.stderr)
    r = run('info', out)
    c.check(r.returncode == 0 and 'identical bytes' in r.stdout and 'consistent' in r.stdout,
            'command line: info on the merged file (identical bytes, consistent)', r.stdout + r.stderr)
    r = run('list', out, 'b_mesh')
    c.check(r.returncode == 0 and 'Objects/B/B_Mesh  |  Objects/B/Shaders/Wood' in r.stdout,
            'command line: list shows the added entry as mesh | shader', r.stdout + r.stderr)
    r = run('roundtrip', paths['base'], paths['donor'], out)
    c.check(r.returncode == 0 and r.stdout.count('ok  ') == 3, 'command line: roundtrip', r.stdout + r.stderr)
    junk = os.path.join(tmp, 'junk.res')
    open(junk, 'wb').write(b'this is not a shader database')
    r = run('info', junk)
    c.check(r.returncode == 1 and 'shader database' in (r.stdout + r.stderr).lower(),
            'command line: a file that is not a database is refused in words', r.stdout + r.stderr)


# ------------------------------------------------------------------------------------------------ the game copy

def game(c, tmp):
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


def main(argv):
    c = Checks()
    with tempfile.TemporaryDirectory(prefix='shaderdb_') as tmp:
        quick(c, tmp)
        if '--game' in argv:
            game(c, tmp)
    print(f'{c.failed} failed' if c.failed else 'all passed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
