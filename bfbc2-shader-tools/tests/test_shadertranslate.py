r"""The BF1943 shader translation's driver in the Rust core (round OR3c: bc2_convert::shadertranslate through
shaderdb.translate_plan, missing_entries, foreign_entries, discards and the soldier glue), on built databases:

- the plan of a built BF1943 / PC pair (a seeded Xenos vertex shader, its fetch records, a mesh part and its layout):
  the entry, its program, the vertex wiring by element order, the fragment, key and id; the plan's stops where the
  Python driver stopped (no main program, no group, no mesh part, a vertex shader that does not decode, a layout
  missing) and the entries left out (filters, excludes, the donor materials, DICE-drawn meshes, only);
- the entries made for missing and foreign materials, the templates' alpha test (discards on hand-built DXBC);
- the soldiers' glue: names matched, permutations, part users, the skinning kinds offered, the blocks named.

Project OPFOR's test_ww2_translate_parity checks the whole translation against the retired Python driver's answers
(Wake's real databases and 44 damaged copies); the Rust unit tests check the pieces.

  python test_shadertranslate.py
"""
from __future__ import annotations

import hashlib
import os
import random
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'core')
import shaderdb as sd  # noqa: E402
import xenos  # noqa: E402
import xenos_hlsl  # noqa: E402

sys.path.insert(0, HERE)
from test_shaderdb import Checks  # noqa: E402
from test_xenos_native import build as xenos_build  # noqa: E402

NAME = 'Objects/Crate/Crate_Mesh/Objects/Crate/Shaders/Wood'
MESH = 'objects/crate/crate_mesh'


def layout_body(els: list[tuple[int, int, int, int]], stride: int) -> bytes:
    b = bytearray(0x49)
    for i, e in enumerate(els):
        b[4 * i:4 * i + 4] = bytes(e)
    b[64], b[69] = len(els), stride
    return bytes(b)


def vertex_shader(seed: int) -> tuple[bytes, list[int]]:
    """A seeded Xenos vertex shader that decodes, and its inputs' usage indices."""
    rng = random.Random(seed)
    while True:
        blob = xenos_build(rng, False)
        try:
            sh = xenos.parse_shader(blob, False)
        except Exception:                                   # noqa: BLE001 - another seed
            continue
        ins = [el.usage_index for el in sh.header.vertex_elements]
        if ins and len(set(ins)) == len(ins) and len(ins) <= 8:
            return blob, ins


def pair(seed: int = 1) -> tuple[sd.ShaderDb, sd.ShaderDb, list[int]]:
    """(BF1943's database w, the PC database db, the vertex shader's usage indices): one entry, one main program."""
    blob, ins = vertex_shader(seed)
    w = sd.ShaderDb(9, True)
    g = w.name_guid(NAME)
    w.entries = [(NAME, g)]
    tail = struct.pack('>I', len(ins)) + b''.join(
        struct.pack('>HHIBBBB', 0, 4 * k, 0x80 | 57, 0, 0, ui, 0) for k, ui in enumerate(sorted(ins)))
    w.vs = [sd.Shader(b'V' * 16, blob, 0, 0, 0, tail)]
    w.ps = [sd.Shader(b'P' * 16, b'', 0, 0, 0)]
    key = bytearray(0x58)
    key[:16] = g
    key[0x33] = 1
    w.keys = [bytes(key)]
    w.programs = [b'R' * 16 + bytes(16) + struct.pack('>4I', 0, 0, 0, 0)]
    w.groups = [(g, [0])]
    db = sd.ShaderDb(11)
    pg = db.name_guid(NAME)
    lg = bytes(range(16))
    db.entries = [(NAME, pg)]
    db.parts = [(f'{MESH}_lod0/body/Standard', lg)]
    db.layouts = [lg + layout_body([(1, 0, 4 * k, 0) for k in range(len(ins))], 4 * len(ins))]
    tkey = bytearray(0x58)
    tkey[:16] = pg
    tkey[0x20:0x30] = lg
    db.keys = [bytes(tkey)]
    db.programs = [b'T' * 16 + bytes(16) + struct.pack('<4I', 0, 0, 0, 0)]
    db.groups = [(pg, [0])]
    return w, db, sorted(ins)


def plan(w: sd.ShaderDb, db: sd.ShaderDb, **kw: Any) -> list[sd.TranslateEntry]:
    args: dict[str, Any] = dict(filters=['Objects/'], keep_donor=('glass',), planned=dict)
    args.update(kw)
    return sd.translate_plan(w, db, 11, **args)


def check_plan(c: Checks) -> None:
    w, db, ins = pair()
    got = plan(w, db)
    c.check(len(got) == 1 and got[0].name == NAME and got[0].error is None and len(got[0].programs) == 1,
            'plan: the entry and its one program', repr(got)[:300])
    p = got[0].programs[0]
    lg = bytes(range(16))
    want_inputs = [(0, 4 * k, 41, ui, False) for k, ui in enumerate(ins)]
    c.check(p.layout == lg and not p.instanced and p.by == 'order' and p.rank == 1 and p.kind == 1
            and p.inputs == want_inputs and p.src == {ui: 4 * k for k, ui in enumerate(ins)},
            'plan: wired by element order (no BF1943 layout), BF1943\'s mesh kind', repr(p)[:300])
    key = bytearray(db.keys[0])
    key[0x33] = 1
    pid = hashlib.md5(NAME.encode() + lg + bytes(key[0x30:])).digest()
    c.check(p.fragment == bytes(16) and p.key == bytes(key) and p.pid == pid,
            'plan: a plain mesh\'s zero fragment, the template\'s key with the layout and kind, the id')
    # left out
    c.check(plan(w, db, exclude=['crate']) == [] and plan(w, db, filters=['Terrains/']) == []
            and plan(w, db, dice_twins=[MESH]) == [] and plan(w, db, only=set()) == []
            and plan(w, db, keep_donor=('wood',)) == [], 'plan: excludes, filters, DICE-drawn meshes, only, donors')
    # stops
    w2, db2, _ = pair()
    k = bytearray(w2.keys[0])
    k[0x30] = 5
    w2.keys = [bytes(k)]
    e = plan(w2, db2)[0].error
    c.check(isinstance(e, xenos_hlsl.TranslateError) and str(e) == 'BF1943 has no main program',
            'plan: no main program', repr(e))
    w2, db2, _ = pair()
    w2.groups = []
    e = plan(w2, db2)[0].error
    c.check(isinstance(e, KeyError) and e.args[0] == w2.entries[0][1], 'plan: no group (KeyError of the GUID)', repr(e))
    w2, db2, _ = pair()
    db2.parts = []
    asked: list[int] = []
    e = plan(w2, db2, planned=lambda: asked.append(1) or {})[0].error
    c.check(isinstance(e, xenos_hlsl.TranslateError) and str(e) == 'no mesh parts in the database' and asked == [1],
            'plan: no mesh parts, the planned parts asked once', repr(e))
    w2, db2, ins2 = pair()
    db2.parts = []
    lg2 = bytes(range(16))
    got2 = plan(w2, db2, planned=lambda: {MESH: [(f'{MESH}_lod0/body', lg2, False)]})
    c.check(len(got2[0].programs) == 1 and got2[0].error is None, 'plan: the parts the install will register')
    w2, db2, _ = pair()
    s = w2.vs[0]
    w2.vs = [sd.Shader(s.guid, b'\0' * 64, 0, 0, 0, s.tail)]
    e = plan(w2, db2)[0].error
    c.check(isinstance(e, xenos.XenosError) and 'bad magic' in str(e), 'plan: a vertex shader that does not decode',
            repr(e))
    w2, db2, _ = pair()
    db2.layouts = []
    e = plan(w2, db2)[0].error
    c.check(isinstance(e, KeyError) and e.args[0] == bytes(range(16)), 'plan: a layout not in section 7', repr(e))
    # a fragment BF1943 has and the PC database lacks
    w2, db2, _ = pair()
    k = bytearray(w2.keys[0])
    k[0x10:0x20] = b'X' * 16
    w2.keys = [bytes(k)]
    w2.fragments = [('Systems/Emitter/ShaderFragmentXenon', b'X' * 16)]
    f = plan(w2, db2)[0].programs[0].fragment
    c.check(isinstance(f, xenos_hlsl.TranslateError) and 'ShaderFragmentXenon' in str(f),
            'plan: BF1943\'s fragment without a PC counterpart', repr(f))
    db2.fragments = [('Systems/Emitter/ShaderFragmentXenon', b'Y' * 16)]
    c.check(plan(w2, db2)[0].programs[0].fragment == b'Y' * 16, 'plan: BF1943\'s fragment by name in the PC database')


def dxbc(tokens: list[int]) -> bytes:
    shex = struct.pack('<2I', 0x00400050, len(tokens) + 2) + struct.pack(f'<{len(tokens)}I', *tokens)
    return b'DXBC' + bytes(16) + struct.pack('<4I', 1, 0, 1, 36) + b'SHEX' + struct.pack('<I', len(shex)) + shex


def check_entries(c: Checks) -> None:
    cut, solid = dxbc([0x0300000d, 0, 0, 0x0100003e]), dxbc([0x03000036, 0, 0, 0x0100003e])
    c.check(sd.discards(cut) and not sd.discards(solid) and not sd.discards(b'xx'), 'discards: the instruction')
    w = sd.ShaderDb(9, True)
    w.entries = [('Objects/T/T_Mesh/Objects/T/Bark', b'1' * 16), ('Objects/T/T_Mesh/Objects/T/Glass', b'2' * 16)]
    db = sd.ShaderDb(11)
    db.entries = [('Objects/T/T_Mesh/Objects/T/Frond', b'3' * 16),
                  ('Objects/Vegetation/A/A_Mesh/Objects/Vegetation/Shaders/Frond', b'4' * 16),
                  ('Objects/Vegetation/A/A_Mesh/Objects/Vegetation/Shaders/Bark', b'5' * 16)]
    db.parts = [('Objects/T/T_Mesh_lod0/p/Standard', b'L' * 16)]
    db.ps = [sd.Shader(b'a' * 16, cut, 0, 0, 0), sd.Shader(b'b' * 16, solid, 0, 0, 0)]
    db.programs = [bytes(32) + struct.pack('<4I', 0, 0, 0, 0), bytes(32) + struct.pack('<4I', 0, 1, 0, 0)]
    db.keys = [b'4' * 16 + bytes(0x48), b'5' * 16 + bytes(0x48)]
    db.groups = [(b'4' * 16, [0]), (b'5' * 16, [1])]
    got = sd.missing_entries(w, db, ['Objects/'], [], ['glass'], ['objects/t/t_mesh'])
    c.check(got == [('Objects/T/T_Mesh/Objects/T/Bark', 'Objects/T/T_Mesh/Objects/T/Frond')],
            'missing_entries: a ported mesh\'s material, cloned from a sibling', repr(got))
    levels = [('levels/iwo/shaders/single.res', ['Objects/T/T_Mesh/Objects/T/Leaf_Iwo', 'Objects/T/T_Mesh/Objects/T/Bark',
                                                 'Objects/T/T_Mesh/Objects/T/Rock_iwo'])]
    mats = {'objects/t/t_mesh': {'objects/t/leaf_iwo', 'objects/t/bark', 'objects/t/rock_iwo', 'objects/t/rock_wake'}}
    got = sd.foreign_entries(['Objects/W/W_Mesh/Objects/W/X'], db, levels, ['Objects/'], [], ['objects/t/t_mesh'],
                             ['iwo'], None, lambda mo: mats.get(mo, set()))
    veg = 'Objects/Vegetation/A/A_Mesh/Objects/Vegetation/Shaders/'
    c.check(got == [('make', levels[0][0], levels[0][1][0], veg + 'Frond', True),
                    ('make', levels[0][0], levels[0][1][1], veg + 'Bark', False)],
            'foreign_entries: the cut-out leaf on the discarding template, the bark on the solid one, the Iwo variant '
            'of a Wake material left out', repr(got))
    got = sd.foreign_entries([], db, levels, ['Objects/'], [], ['objects/t/t_mesh'], ['iwo'],
                             {levels[0][1][0]: 'Nowhere'}, lambda mo: mats.get(mo, set()))
    c.check(got == [('missing', levels[0][1][0], 'Nowhere'), ('missing', levels[0][1][1], None)],
            'foreign_entries: templates chosen elsewhere and not here', repr(got))


def check_soldiers(c: Checks) -> None:
    w = sd.ShaderDb(9, True)
    w.entries = [('Characters/Beach/3P/US/M/Characters/Beach/3P/US/Body', b'g' * 16)]
    db = sd.ShaderDb(11)
    db.entries = [('Characters/WW2/3P/US/M/Characters/WW2/3P/US/Shaders/Body', b'h' * 16),
                  ('Characters/WW2/3P/US/M/Characters/WW2/3P/US/Glass', b'i' * 16)]
    db.parts = [('Characters/WW2/3P/US/M_lod0/body/Standard', b'L' * 16)]
    key = bytearray(0x58)
    key[0x20:0x30] = b'L' * 16
    key[0x33] = 4
    db.keys = [bytes(key)]
    db.programs = [bytes(48)]
    db.groups = [(b'h' * 16, [0]), (b'i' * 16, [])]
    ours = db.entries[0][0]
    c.check(sd.match_entries(w, db, ['glass'], {}) == {ours: w.entries[0][0]}, 'match_entries: BF1943\'s soldier')
    c.check(sd.entry_perms(db, [ours]) == {ours: {(0, b'L' * 16, 0, 0, 4, bytes(16))}}, 'entry_perms')
    c.check(sd.part_users(db, [ours]) == {ours: {b'L' * 16}}, 'part_users')
    alias = sd.ShaderDb(9, True)
    alias.entries = [(ours, b'g' * 16)]
    k1 = bytearray(0x58)
    k1[0x33] = 1
    alias.keys, alias.programs = [bytes(k1)], [b'p' * 48]
    alias.groups = [(b'g' * 16, [0])]
    lines: list[str] = []
    bad = sd.offer_kinds(alias, {ours: ours}, {ours: {(0, 4)}}, {ours: True}, lines.append)
    c.check(bad == {} and alias.groups == [(b'g' * 16, [0, 1])] and alias.keys[1][0x33] == 4
            and alias.programs[1] == b'p' * 48 and len(lines) == 1, 'offer_kinds: kind 1 offered as 4')
    alias.programs = [b'p' * 32 + struct.pack('>4I', 0, 0, 3, 4)]
    alias.keys, alias.groups = [bytes(k1)], [(b'g' * 16, [0])]
    alias.vs, alias.ps = [sd.Shader(b'v' * 16, b'', 5, 0, 0)], [sd.Shader(b'q' * 16, b'', 6, 0, 0)]
    c.check(sd.entry_blocks(alias, [ours]) == {3, 4, 5, 6}, 'entry_blocks')


def main() -> int:
    c = Checks()
    check_plan(c)
    check_entries(c)
    check_soldiers(c)
    print(f'\n{c.failed} failed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main())
