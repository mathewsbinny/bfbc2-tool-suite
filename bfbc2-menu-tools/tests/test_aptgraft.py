r"""aptgraft.py (the APT movie writer and character grafter) on movies built here (quick), and with --game on Project
OPFOR BFBC2's game copy:

- a movie built through the model (imports, a shape with geometry, a sprite with a named placement, clip events, an
  action with a ConstantPool, PushData, a DefineFunction2 with parameters, branches, GetURL, string opcodes), written
  little-endian: apt.py reads it, aptcheck finds nothing, the parser accounts for every byte, it round-trips byte for
  byte, packed (every structure moved) and through the other byte order;
- grafting the sprite from the big-endian copy into another little-endian movie: renumbered, the import mapped,
  constants re-indexed (the copied code decodes to the same text), geometry copied, placed on the timeline; a code
  range taken with a ConstantPool of its own; code built with Builder; old data left where it was (keep layout);
- dead data (bytes nothing points at) kept by the keep layout and reported; overlapping structures refused;
- the command line (roundtrip, convert, deps, graft).
With --game: every SwfMovie of the game copy's bundles (Dist and Package) round-trips byte for byte and packed; with
OPFOR's copy of the Xbox 360 HUD (work\xbox_dlc\hud\con_ingame, skipped when absent), the Onslaught timer graft
into the PC hudlayer_shared the game loads: aptcheck finds nothing, apt.py's dump only gains lines.

  python test_aptgraft.py [--game]
"""
import difflib
import hashlib
import os
import struct
import subprocess
import sys
import tempfile
import shutil

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, ROOT)
import menucommon as mc  # noqa: E402
import aptcheck  # noqa: E402
import aptgraft as ag  # noqa: E402
import apt  # noqa: E402
from aptgraft import Ref, Str, Words, Code, Ins, Builder  # noqa: E402

SIG = 0x09876543


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, ok, what, detail=''):
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


def blank(name):
    """The members of an empty movie: one frame, no characters but the movie."""
    a = bytearray(b'Apt Data:74\x1a\0\0\0\0')
    mv = len(a)
    a += struct.pack('<15I', 9, SIG, 1, mv + 60, 0, 1, mv + 68, 1280, 720, 33, 0, 0, 0, 0, 0)
    a += struct.pack('<2I', 0, 0)                    # frame 0: no items
    a += struct.pack('<I', mv)                       # character table: the movie
    c = b'Apt constant file\x1a\0\0' + struct.pack('<3I', mv, 0, 32)
    g = b'Apt PCGL geometry file\x1a\0' + struct.pack('<2I', 0, 32)
    return [(name + '.apt', bytes(a)), (name + '.const', c), (name + '.geo', g)]


def S(t):
    return Ref(Str('apt', None, t.encode()))


def build_source():
    """A little-endian movie with an import (char 1), a shape (2) and a sprite (3) placing both, placed on the
    timeline as "widget"; the timeline's action defines a function and registers it."""
    m = ag.Movie(blank('src'))
    imp = m.add_import('TextManagerComponent', 'TextManager')
    shape = Words('apt', None, [1, SIG, ag.f32(0), ag.f32(0), ag.f32(100), ag.f32(50), 0], 'shape')
    sid = m.add_char(shape)
    shape.vals[6] = sid
    verts = Words('apt', None, [ag.f32(v) for v in (0, 0, 100, 0, 100, 50, 0, 0, 100, 50, 0, 50)], 'geo.vertices')
    part = Words('geo', None, [1, 2, Ref(verts), 0xBF000000], 'geo.part1')
    verts.file = 'geo'
    m.add_geometry(sid, Words('geo', None, [Ref(part)], 'geo.parts'))
    # the sprite: frame 0 places the shape and the import (with a clip event), an action with a function
    sprite = Words('apt', None, [5, SIG, 1, 0, 0], 'sprite')
    spid = m.add_char(sprite)
    clipcode = Code('apt', None, [Ins(0xA1, [S('propText')]), Ins(0xA6, [S('HELLO')]), Ins(0)])
    events = Words('apt', None, [0x40000, 0, Ref(clipcode)], 'clipevents')
    clip = Words('apt', None, [1, Ref(events)], 'clipactions')
    p1 = ag.place_item(m, 1, sid, 'box', 0, 9)
    p2 = ag.place_item(m, 3, imp, 'label', 4, 46, flags=0xA6)
    p2.vals[15] = Ref(clip)
    b = Builder(m)
    b.pool('setValue', 'value', 'field', 'shown', '_visible')
    params = Words('apt', None, [1, S('v')], 'f2params')
    body = [Ins(0x96, [1, Ref(Words('apt', None, [m.const_index((4, 1))], 'pushlist'))]),
            Ins(0x5A), Ins(OPS('EA_PushValueOfVar'), [2]), Ins(OPS('EA_CallNamedMethodPop'), [0])]
    b.ins.append(Ins(0x8E, [S('setValue'), 1, 2 | (0x2A << 16), Ref(params), *ag.FN_MAGIC], body=body))
    b.op('EA_PushConstant', 'shown').op('EA_PushTrue').op('SetVariable')
    b.op('EA_PushConstant', 'shown').op('EA_PushValueOfVar', 'shown').op('Not').op('BranchIfTrue', 'skip')
    b.op('EA_PushValueOfVar', 'field').op('EA_PushConstant', '_visible').op('EA_PushFalse').op('SetMember')
    b.label('skip')
    b.ins.append(Ins(0x83, [S('FSCommand:test'), S('arg')]))
    b.ins.append(Ins(0xB7, [123456]))
    b.ins.append(Ins(0x17))
    act = Words('apt', None, [1, Ref(b.code())], 'action')
    for it in (p1, p2, act):
        m.add_item(0, it, sprite)
    m.add_item(0, ag.place_item(m, 5, spid, 'widget', 96, 397))
    return m, imp, sid, spid


def OPS(name):
    return ag.OPS[name]


def target():
    """A little-endian target movie with the same import in another slot, a few constants and a shape of its own."""
    m = ag.Movie(blank('dst'))
    m.add_import('weaponicon', 'HealthCross')
    m.add_import('TextManagerComponent', 'TextManager')
    for t in ('alpha', 'beta', 'value'):
        m.const_index(('s', t.encode()))
    b = Builder(m)
    b.pool('alpha', 'beta')
    b.op('EA_PushConstant', 'alpha').op('EA_PushConstant', 'beta').op('SetVariable')
    m.add_item(0, Words('apt', None, [1, Ref(b.code())], 'action'))
    return ag.Movie(m.write(), 'target')


def reread(members, label):
    return ag.Movie(members, label)


def apt_movie(members):
    d = dict(members)
    n = next(k for k in d if k.endswith('.apt'))[:-4]
    return apt.Movie(data=(n, d[n + '.apt'], d[n + '.const']))


def run(*args):
    r = subprocess.run([sys.executable, os.path.join(ROOT, 'aptgraft.py'), *args], capture_output=True, text=True,
                       encoding='utf-8', errors='replace')
    return r.returncode, r.stdout + r.stderr


def quick(c, work):
    m, imp, sid, spid = build_source()
    le = m.write()
    src = reread(le, 'built')
    am = apt_movie(le)
    dump = '\n'.join(am.dump())
    c.check('PlaceObject depth=5 char=3' in dump and 'name="widget"' in dump and 'DefineFunction2 setValue(1 params)' in dump
            and 'GetURL "FSCommand:test", "arg"' in dump and 'EA_SetStringVar "HELLO"' in dump,
            'built movie: apt.py reads the sprite, placement, function, GetURL and clip event')
    c.check(not aptcheck.Code(am).findings(), 'built movie: aptcheck finds nothing')
    c.check(not src.leftovers(), 'built movie: every byte accounted for', str(src.leftovers()[:3]))
    c.check(src.write() == le, 'built movie: written back byte for byte')
    res = os.path.join(work, 'built.res')
    open(res, 'wb').write(ag.write_bigf(le))
    probs, notes = ag.roundtrip(res, pack=True)
    c.check(not probs and not notes, 'built movie: round trip, packed layout, other byte order and back', str(probs))
    be = src.write(order='>')
    big = reread(be, 'built, big-endian')
    c.check(big.big and apt_movie(be).big and ag.dump_text(be)[1:] == ag.dump_text(le)[1:],
            'big-endian copy: apt.py reads the same movie')
    c.check(big.write(order='<') == le, 'big-endian copy converts back byte for byte')
    c.check(b'\x02\x00\x2a\x00' in le[0][1] and b'\x00\x02\x00\x2a' in be[0][1]
            and 'setValue(1 params) regs=2' in '\n'.join(apt_movie(be).dump()),
            'DefineFunction2: register count and flags are two 16-bit fields (PC 02 00 2A 00, Xbox 360 00 02 00 2A)')

    # graft the sprite from the big-endian copy into the target
    dst = target()
    before = dict(dst.write())
    g = ag.Grafter(big, dst)
    new = g.char(spid)
    dst.add_item(0, ag.place_item(dst, 7, new, 'widget', 96, 397))
    out = dst.write(layout=ag.GRAFT_LAYOUT)
    res2 = reread(out, 'grafted')
    am2 = apt_movie(out)
    d2 = '\n'.join(am2.dump())
    kinds = sorted(k for _, k, _ in g.added)
    c.check(new == 3 and kinds == ['shape', 'sprite'] and g.cmap[imp] == 2,
            'graft: renumbered after the target\'s characters, the import mapped to the target\'s', str(g.added))
    c.check(f'PlaceObject depth=7 char={new}' in d2 and f'PlaceObject depth=3 char=2 flags=0xa6 name="label"' in d2,
            'graft: placed, and the import placement points at the target\'s TextManager')
    ent = res2.geo_entry(g.cmap[sid])
    c.check(ent is not None and res2.char(g.cmap[sid]).vals[6] == g.cmap[sid],
            'graft: the shape\'s geometry copied with its new id')

    def action_text(text, sp):
        part = text[text.index(f'char {sp}: sprite'):]
        part = part[part.index('Action @'):].splitlines()[1:14]
        return [ln.strip().split('  ', 1)[-1].split('   [list')[0].split(' -> ')[0] for ln in part]
    c.check(action_text(d2, new) == action_text('\n'.join(apt_movie(be).dump()), spid),
            'graft: the copied code decodes to the same instructions and constants (re-indexed)')
    c.check(not aptcheck.Code(am2).findings(), 'graft: aptcheck finds nothing in the result')
    c.check('setValue(1 params) regs=2' in d2, 'graft: the copied function keeps 2 registers and its flags')
    c.check(not res2.leftovers() and res2.write(layout=ag.GRAFT_LAYOUT) == out, 'graft: the result reads back completely')
    kept = [n for n in dst.cache.values() if n.file == 'apt' and n.size0 and n.size(n.at % 4) == n.size0]
    c.check(kept and all(n.new_at == n.at for n in kept) and len(out[0][1]) > len(before['dst.apt']),
            'graft (keep layout): the target\'s old structures stay at their offsets, new data after them')
    nconst = len(dst.const_entries.vals) // 2
    c.check(res2.const_entries.at == 32 and nconst > 3, 'graft: constants added, the .const table grown in place')
    # a code range with a ConstantPool of its own
    srcact = next(it for it in big.frame_items(0, big.char(spid)) if it.tag == 'action').vals[1].node
    tops = srcact.ins
    rng = g.code_range(srcact, tops[2].at, tops[5].at)
    c.check(rng.ins[0].op == 0x88 and len(rng.ins[0].args[1].node.vals) == 1 and rng.ins[-1].op == 0,
            'code range: its own ConstantPool (only the constant used) and End')
    try:
        g.code_range(srcact, tops[5].at, tops[9].at)
        c.check(False, 'code range: a branch leaving the range refused')
    except ag.AptError:
        c.check(True, 'code range: a branch leaving the range refused')
    dst.add_item(0, Words('apt', None, [1, Ref(rng)], 'action'))
    out2 = dst.write(layout=ag.GRAFT_LAYOUT)
    c.check(not aptcheck.Code(apt_movie(out2)).findings() and 'EA_PushConstant "shown"' in '\n'.join(
        apt_movie(out2).dump()[-0:]), 'code range: added as a frame action, aptcheck finds nothing')
    # dead data kept, overlaps refused
    a = bytearray(le[0][1])
    a += b'\x11\x22\x33\x44'
    dead = [(le[0][0], bytes(a))] + le[1:]
    md = reread(dead, 'dead')
    c.check(md.write() == dead and md.leftovers() and md.write(dead=False)[0][1][-4:] == b'\0\0\0\0',
            'dead data: kept by the keep layout, reported, zero when not kept')
    bad = bytearray(le[0][1])
    tab = src.chartable.at
    struct.pack_into('<I', bad, tab + 4 * sid, src.chartable.vals[spid].node.at + 4)
    try:
        ag.Movie([(le[0][0], bytes(bad))] + le[1:], 'broken')
        c.check(False, 'a broken character pointer refused')
    except ag.AptError:
        c.check(True, 'a broken character pointer refused')
    # extern reads found and replaced (branches to a read land on its replacement)
    em = ag.Movie(blank('ext'))
    b = Builder(em)
    b.pool('extern', 'foo', 'gameModeID')
    b.op('EA_PushTrue').op('BranchIfTrue', 'read')
    b.label('read')
    b.op('EA_PushGlobalVar').op('EA_GetNamedMember', 'extern').op('EA_GetNamedMember', 'foo').op('Pop')
    b.op('EA_PushGlobalVar').op('EA_GetNamedMember', 'extern').op('EA_GetNamedMember', 'gameModeID').op('Pop')
    code = b.code()
    em.add_item(0, Words('apt', None, [1, Ref(code)], 'action'))
    reads = ag.extern_reads(em, code)
    c.check([r[2] for r in reads] == ['foo', 'gameModeID'], 'extern reads found (extern.foo, extern.gameModeID)')
    lst, k, _ = reads[0]
    ag.replace_extern(lst, k, Ins(0x76))
    em_out = em.write()
    dd = '\n'.join(apt_movie(em_out).dump())
    c.check('"foo"' not in dd.split('ConstantPool')[1].split('End')[0] and 'EA_PushUndefined2' in dd
            and not aptcheck.Code(apt_movie(em_out)).findings() and code.ins[2].target is lst[k],
            'extern read replaced, the branch to it lands on the replacement')
    # a mode switch gains a case (mp_objectives' getMapDesc shape)
    mm = ag.Movie(blank('modes'))
    names = ['gameModeId', 'conquest', 'KEY_CQ', 'Missing DESC tag!! ']
    pool = Ins(0x88, [4, Ref(Words('apt', None, [mm.const_index(('s', t.encode())) for t in names], 'poollist'))])
    end, fb, l1 = Ins(0xB9, [2]), Ins(0xA2, [3]), Ins(0xA2, [2])
    body = [Ins(0xAE, [0]), Ins(0xA2, [1]), Ins(0x66), Ins(0x9D, target=l1), Ins(0x99, target=fb),
            l1, Ins(0x87, [2]), Ins(0x17), Ins(0x99, target=end),
            fb, Ins(0xAE, [0]), Ins(0x47), Ins(0x87, [2]), Ins(0x17), Ins(0x99, target=end), end, Ins(0x3E)]
    fn = Ins(0x8E, [S('getMapDesc'), 0, 3 | (0x29 << 16), Ref(Words('apt', None, [], 'f2params')), *ag.FN_MAGIC],
             body=body)
    mm.add_item(0, Words('apt', None, [1, Ref(Code('apt', None, [pool, fn, Ins(0)]))], 'action'))
    n = ag.add_mode_case(mm, 'getMapDesc', 'onslaught', 'KEY_ONS')
    md = '\n'.join(apt_movie(mm.write()).dump())
    c.check(n == 1 and 'EA_PushConstant "onslaught"' in md and 'EA_PushConstant "KEY_ONS"' in md
            and body[4].target is body[9] and not aptcheck.Code(apt_movie(mm.write())).findings(),
            'mode-case: getMapDesc gains an onslaught case ahead of its fallback, branches kept')
    # command line
    code_, outp = run('roundtrip', '--pack', res)
    c.check(code_ == 0 and '1 of 1 different movies' in outp, 'command line: roundtrip', outp[-300:])
    bres = os.path.join(work, 'built_be.res')
    code_, outp = run('convert', res, bres)
    c.check(code_ == 0 and ag.Movie.load(bres).big, 'command line: convert to big-endian', outp[-300:])
    code_, outp = run('deps', bres, str(spid))
    c.check(code_ == 0 and 'TextManagerComponent::TextManager' in outp and 'geometry:   2' in outp, 'command line: deps',
            outp[-300:])
    tres = os.path.join(work, 'target.res')
    open(tres, 'wb').write(ag.write_bigf(target().write()))
    gres = os.path.join(work, 'grafted.res')
    code_, outp = run('graft', tres, bres, str(spid), gres, '--name', 'widget', '--x', '10', '--y', '20')
    c.check(code_ == 0 and 'name="widget"' in '\n'.join(apt.Movie(gres).dump()), 'command line: graft', outp[-300:])
    code_, outp = run('graft', tres, bres, '99', gres)
    c.check(code_ == 1 and 'neither defined nor imported' in outp, 'command line: a missing character explained')


def game(c):
    root = mc.find_opfor()
    from bfbc2core import fbrb
    seen, bad, files = set(), [], 0
    for top in (os.path.join(root, 'game', 'Dist', 'win32'), os.path.join(root, 'game', 'Package')):
        for dp, _, fs in os.walk(top):
            for f in sorted(fs):
                if not f.endswith('.fbrb'):
                    continue
                p = os.path.join(dp, f)
                ents = [e.name for e in fbrb.entries(p) if e.type == 'SwfMovie']
                if not ents:
                    continue
                for name, data in fbrb.read(p, ents).items():
                    files += 1
                    h = hashlib.md5(data).hexdigest()
                    if h in seen:
                        continue
                    seen.add(h)
                    tmp = os.path.join(tempfile.gettempdir(), 'aptgraft_test.res')
                    open(tmp, 'wb').write(data)
                    try:
                        probs, _ = ag.roundtrip(tmp, pack=True)
                    except ag.AptError as e:
                        probs = [str(e)]
                    if probs:
                        bad.append(f'{os.path.relpath(p, root)}:{name}: {probs[0]}')
    c.check(len(seen) > 80 and not bad, f'every SwfMovie of the game copy round-trips byte for byte and packed '
                                        f'({len(seen)} different, {files} entries)', '; '.join(bad[:4]))
    con = os.path.join(root, 'work', 'xbox_dlc', 'hud', 'con_ingame', 'Menu', 'hudlayer_shared.res')
    pcb = os.path.join(root, 'game', 'Package', 'levels', 'ons_mp_002', 'ingame-00.fbrb')
    if os.path.isfile(pcb + '.opfor-hud-orig'):
        # OPFOR's console_hud grafted the installed one already: graft into (a copy of) its original
        import shutil
        orig = os.path.join(tempfile.gettempdir(), 'aptgraft_test_ons_mp_002_ingame-00.fbrb')
        shutil.copyfile(pcb + '.opfor-hud-orig', orig)
        pcb = orig
    if not (os.path.isfile(con) and os.path.isfile(pcb)):
        print('skip the Onslaught timer graft (no console HUD in work\\xbox_dlc or no ons_mp_002 bundle)')
        return
    pc = ag.Movie.load(pcb + ':hudlayer_shared')
    before = pc.write()
    report, g = ag.onslaught_timer(pc, ag.Movie.load(con))
    out = pc.write(layout=ag.GRAFT_LAYOUT)
    am = apt_movie(out)
    diff = [ln for ln in difflib.unified_diff(ag.dump_text(before), ag.dump_text(out), lineterm='', n=0)
            if ln[:1] in '+-' and ln[:3] not in ('+++', '---')]
    removed = [ln for ln in diff if ln.startswith('-') and not ln.startswith(('-Movie ', '-Movie frame'))]
    c.check(not aptcheck.Code(am).findings() and not removed and not g.notes and len(g.added) == 6,
            'Onslaught timer graft: aptcheck finds nothing, the dump only gains lines, no textures needed',
            f'{removed[:3]} {g.notes}')
    d = '\n'.join(am.dump())
    c.check(d.count('name="onslaught_timer"') == 2 and '"coopRoundTime"' in d and '"coopOldBestRoundTime"' in d,
            'Onslaught timer graft: placed (playing, killed), both events registered')
    acts0 = [it for it in pc.frame_items(0) if it.tag == 'action']          # grafted nodes have no file offset yet
    c.check(len(acts0) == 5 and all(it.at is None for it in acts0[:3]) and acts0[3].at is not None and
            not any(it.tag == 'action' and it.at is None for it in pc.frame_items(1)),
            'Onslaught timer graft: events registered in frame 0 ahead of the PC script (it jumps past frame 1, '
            'where the console registers them - the timer stayed at zero)')
    src = ag.Movie(before, 'PC')
    pc_names = {n for x in ag.code_nodes(src) for _, _, n in ag.extern_reads(src, x)}
    res = ag.Movie(out, 'grafted')
    new = {n for x in ag.code_nodes(res) for _, _, n in ag.extern_reads(res, x)}
    c.check(new == pc_names and 'gameModeID' in new and sum('replaced' in r for r in report) == 2,
            'Onslaught timer graft: reads no extern the PC movie does not read (coopOldBestRoundTime and isScreenHD '
            'replaced - the first crashed the game)', str(sorted(new - pc_names)))
    pc2 = ag.Movie.load(pcb + ':hudlayer_shared')
    ag.onslaught_timer(pc2, ag.Movie.load(con), place=False)
    d2 = '\n'.join(apt_movie(pc2.write(layout=ag.GRAFT_LAYOUT)).dump())
    c.check('name="onslaught_timer"' not in d2 and '"coopRoundTime"' in d2,
            'Onslaught timer graft, --no-place variant: events and code, nothing placed')
    c.check('setValue(2 params) regs=3' in d and 'regs=42' not in d and 'EA_SetStringVar "BEST"' in d,
            'Onslaught timer graft: functions keep their register counts (the console\'s 00 03 00 2A), label "BEST"')
    obj = ag.Movie.load(pcb + ':mp_objectives')
    ag.add_mode_case(obj, 'getMapDesc', 'onslaught', 'MP_LOADING_OBJECTIVE_ONSLAUGHT')
    od = apt_movie(obj.write(layout=ag.GRAFT_LAYOUT))
    import bc2loc
    loc = bc2loc.load('english', root)
    c.check(not aptcheck.Code(od).findings() and '"MP_LOADING_OBJECTIVE_ONSLAUGHT"' in '\n'.join(od.dump())
            and 'In Onslaught' in (loc.lookup('MP_LOADING_OBJECTIVE_ONSLAUGHT')[2] or ''),
            'mp_objectives: getMapDesc returns MP_LOADING_OBJECTIVE_ONSLAUGHT (a PC text) for onslaught')
    hr = ag.Movie.load(pcb + ':hud_root')
    n = ag.add_mode_alias(hr, 'onslaught', 'goldrush')
    hd = apt_movie(hr.write(layout=ag.GRAFT_LAYOUT))
    lines = hd.dump()
    at = [k for k, ln in enumerate(lines) if '"onslaught"' in ln]
    k = at[0] if len(at) == 1 else 0
    c.check(n == 1 and not aptcheck.Code(hd).findings() and len(at) == 1 and '"goldrush"' in lines[k + 4]
            and 'BranchIfTrue' in lines[k + 2] and lines[k + 2].split('->')[-1] == lines[k + 6].split('->')[-1],
            'hud_root: mode-alias sends onslaught down goldrush\'s branch (team names; the spawn screen showed '
            '"Missing DESC tag!! onslaught"), aptcheck finds nothing')
    c.check(ag.add_mode_alias(hr, 'onslaught', 'goldrush') == 0, 'mode-alias: a second run changes nothing')
    hc = ag.Movie.load(pcb + ':hud_root')
    n = ag.add_mode_alias(hc, 'onslaught', 'conquest')
    lines = apt_movie(hc.write(layout=ag.GRAFT_LAYOUT)).dump()
    at = [k for k, ln in enumerate(lines) if '"onslaught"' in ln]
    k = at[0] if len(at) == 1 else 0
    c.check(n == 1 and len(at) == 1 and '"conquest"' in lines[k - 4] and 'PushData reg0' in lines[k - 1]
            and lines[k + 2].split('->')[-1] == lines[k - 2].split('->')[-1]
            and ag.add_mode_alias(hc, 'onslaught', 'conquest') == 0,
            'hud_root: mode-alias onto a switch\'s first compare (SetRegister 0): register 0 compared after it, same '
            'branch (team names by faction), a second run changes nothing')


def main(argv):
    c = Checks()
    work = tempfile.mkdtemp(prefix='aptgraft_')
    try:
        quick(c, work)
        if '--game' in argv:
            game(c)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print('all passed' if not c.failed else f'{c.failed} check(s) failed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
