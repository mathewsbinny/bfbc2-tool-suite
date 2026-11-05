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
sys.path[:0] = [os.path.dirname(HERE), os.path.join(SUITE, 'core'), os.path.join(SUITE, 'bfbc2-bf1943-import')]
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
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
