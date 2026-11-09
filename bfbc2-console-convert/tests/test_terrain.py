"""Terrain conversion (terrain.py) against BFBC2's own Xbox / PC twins, and on Battlefield 1943's Wake Island.

Needs the user's files: BFBC2's Xbox levels unpacked by the console tools (OPFOR work\\xbox) and the PC game (OPFOR
game\\); Battlefield 1943 through the suite's bfbc2-bf1943-import. Skips what is missing.
"""
import glob
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'bf1943-import')
import terrain  # noqa: E402
from bfbc2core import fbrb  # noqa: E402
from bfbc2core import terrain as hf  # noqa: E402

OPFOR = os.path.join(SUITE, '..', 'project-opfor-bfbc2')
# the levels DICE did not edit the PC terrain of after the Xbox build (the others' heights differ in places)
UNEDITED = {'mp_002', 'mp_002sr', 'mp_003', 'mp_004', 'mp_004sdm', 'mp_005', 'mp_005sr'}


def twins(kind):
    for X in sorted(glob.glob(os.path.join(OPFOR, 'work', 'xbox', 'dist', 'xenon', 'levels', '*', 'terrain-00.fbrb'))):
        lv = os.path.basename(os.path.dirname(X))
        P = os.path.join(OPFOR, 'game', 'Dist', 'win32', 'levels', lv, 'terrain-00.fbrb')
        if not os.path.isfile(P):
            continue
        k = next((n for n in fbrb.names(X) if n.lower().endswith(kind)), None)
        kp = {n.lower(): n for n in fbrb.names(P)}.get((k or '').lower())
        if k and kp:
            yield lv, fbrb.read(X, [k])[k], fbrb.read(P, [kp])[kp]


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    pairs = list(twins('.heightfield-000.res'))
    if not pairs:
        print('SKIP: no BFBC2 Xbox / PC terrain twins')
    exact = same_shape = 0
    for lv, x, p in pairs:
        c = terrain.heightfield(x)
        if lv in UNEDITED:
            exact += c == p
        same_shape += len(c) == len(p) and c[:0x31] == p[:0x31]
    if pairs:
        n_unedited = sum(lv in UNEDITED for lv, _, _ in pairs)
        check(exact == n_unedited and exact > 0, f'heightfield: {exact} unedited levels converted byte for byte')
        check(same_shape == len(pairs), f'heightfield: {same_shape} of {len(pairs)} levels - the PC header and size')
    waters = list(twins('.water.mesh.res'))
    if waters:
        ok = sum(terrain.water_mesh(x) == p for _, x, p in waters)
        check(ok == len(waters), f'water mesh: {ok} of {len(waters)} levels byte for byte')
    maps = list(twins('-materialmap.res'))
    if maps:
        ok = sum(terrain.material_map(x) == p for _, x, p in maps)
        check(ok == len(maps), f'material map: {ok} of {len(maps)} levels byte for byte')
    visuals = list(twins('.visual.res'))
    if visuals:
        visual_terrain(visuals, check)
    waters = list(twins('.water.res'))
    if waters:
        visual_water(waters, check)
    try:
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
        w = src.data('terrains/wake_island_s/wake_island_s.heightfield-000.res')
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP Battlefield 1943: {e}')
        return 1 if fails else 0
    c = terrain.heightfield(w, source='bf1943')
    import tempfile
    path = os.path.join(tempfile.mkdtemp(prefix='terrain_'), 'wake.res')
    open(path, 'wb').write(c)
    h = hf.read(path)
    import numpy as np
    raw = np.frombuffer(w, '>u2', 2048 * 2048, 0x2D).reshape(2048, 2048)
    check(h.size == 2048 and np.array_equal(h.raw, raw) and struct.unpack_from('<I', c, 0x2D)[0] == 181056,
          'Wake Island heightfield: BFBC2\'s layout, its heights, first section 181,056 bytes')
    names = terrain.material_names(terrain.material_map(src.data('terrains/wake_island_s/wake_island_s-materialmap.res')))
    check(names[:3] == ['defaultmaterial', 'Dirt', 'Grass'] and 'Water_solidground' in names,
          f'Wake Island material map: converted, {len(names)} materials')
    for lv, blocks in (('wake_island_s', 1), ('iwo_jima_s', 13), ('guadal_canal', 13)):
        parts = dict(terrain.visual_terrain_parts(terrain.visual_terrain(src.data(f'terrains/{lv}/{lv}.visual.res'))))
        hf_name = parts['patch map 0 names'].split(b'\0')[0].decode().lower()
        check(struct.unpack('<I', parts['blocks'])[0] == blocks and hf_name.endswith(f'{lv}.heightfield-000'),
              f'{lv} visual terrain: converted and read as PC, {blocks} terrain block(s), {len(parts)} parts')
    for lv in ('wake_island_s', 'iwo_jima_s', 'guadal_canal', 'coral_sea'):
        x = src.data(f'terrains/{lv}/{lv}.water.res')
        v, parts = terrain.visual_water_parts(terrain.visual_water(x))
        n = v['n'][:, :3].astype(np.float64)
        none = (n == -1.5).all(1)                    # vertices without a normal (BFBC2's mp_006, mp_009 have some)
        length = np.linalg.norm(n[~none], axis=1)
        check(len(v) == struct.unpack_from('>I', x, 8)[0] // 28 and abs(length - 1).max() < 0.01
              and b'Water' in dict(parts)['materials'],
              f'{lv} visual water: converted and read as PC, {len(v)} vertices, unit normals ({none.sum()} without)')
    return 1 if fails else 0


# DICE rebuilt these levels' roads for the PC after the Xbox build (patches reordered, mp_004 one vertex more,
# mp_001 road layers 1 -> 2), and mp_008's patch shaders (17,572 bytes on the Xbox, 17,510 on the PC) and its main
# patch map's last grid
ROADS_REBUILT = ('mp_001', 'mp_004', 'mp_006', 'mp_008', 'mp_012')
VISUAL_EDITED = {'mp_008': {'patch shaders', 'patch map 0 cells'}}


def visual_terrain(pairs, check):
    exact, layout, roads, roads_ok, unexplained, coarser = [], 0, 0, 0, [], 0
    for lv, x, p in pairs:
        c = terrain.visual_terrain(x)
        if c == p:
            exact.append(lv)
        pc, cc = terrain.visual_terrain_parts(p), terrain.visual_terrain_parts(c)
        layout += [n for n, _ in pc] == [n for n, _ in cc]
        cd, pd = dict(cc), dict(pc)
        for name, data in pc:
            if name.startswith('roads'):
                roads += 1
                roads_ok += cd[name] == data
                if cd[name] != data and not lv.startswith(ROADS_REBUILT):
                    unexplained.append((lv, name))
            elif cd[name] != data and name not in VISUAL_EDITED.get(lv, ()):
                block = ' '.join(name.split()[:3])                 # 'patch map i'
                if pd[block][40:44] != cd[block][40:44]:          # its grid size W
                    coarser += name == block
                else:
                    unexplained.append((lv, name))
    check(len(exact) >= 3 and {'mp_003', 'mp_005', 'mp_005sr'} <= set(exact),
          f'visual terrain: {len(exact)} levels byte for byte ({", ".join(exact)})')
    check(layout == len(pairs), f'visual terrain: {layout} of {len(pairs)} levels convert to the PC\'s parts')
    check(not unexplained, f'visual terrain: every other part byte for byte but DICE\'s PC edits - roads {roads_ok} of '
          f'{roads} (rebuilt on {", ".join(ROADS_REBUILT)}), {coarser} sub-terrain patch maps PC-built coarser '
          '(4 x 4 cells: its sub-heightfields 64 samples, the Xbox\'s 128 and 8 x 8)', f'unexplained {unexplained[:8]}')


def visual_water(pairs, check):
    import numpy as np
    exact, rest, worst, n_exact, n_all = [], 0, 0.0, 0, 0
    for lv, x, p in pairs:
        c = terrain.visual_water(x)
        if c == p:
            exact.append(lv)
        vc, pc_ = terrain.visual_water_parts(c)
        vp, pp = terrain.visual_water_parts(p)
        same = c[:12] == p[:12] and pc_ == pp and all(np.array_equal(vc[k], vp[k]) for k in ('pos', 'uv'))
        rest += same
        for k in ('n', 't'):
            a, b = vc[k].astype(np.float64), vp[k].astype(np.float64)
            worst = max(worst, float(np.abs(a - b).max()))
            n_exact += int((vc[k] == vp[k]).sum())
            n_all += vc[k].size
    check(len(exact) >= 6, f'visual water: {len(exact)} levels byte for byte ({", ".join(exact)})')
    check(rest == len(pairs), f'visual water: {rest} of {len(pairs)} levels byte for byte but the normals / tangents')
    check(worst < 2.5 / 511, f'visual water: normals / tangents {100 * n_exact / n_all:.1f}% exact, the rest within '
          f'{worst * 511:.2f} / 511 (the Xbox keeps 10 bits, the PC 4 x f16)')


if __name__ == '__main__':
    sys.exit(main())
