r"""meshview.py on the game's files (game tier; skipped without them):

- the M1 Garand's first-person mesh reads as DICE's parts (2005 + 3825 triangles for the rifle; the bayonet, launcher
  and rifle grenade parts are stubs of 1, 2 and 8), with its colour textures decoded;
- a weapon from an async bundle (the Vietnam M1 Garand) and a rigid projectile are found and read;
- the page embeds every part, and the picture shows the model (a few thousand coloured pixels);
- a soldier's texture coordinates are its half2 (offset 40), not the grey half4 at 16 that comes first among the
  usage-0 elements (US body: every part with coordinates; most span the texture; mesh.decode reads the same).

  python test_meshview.py
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('mesh-viewer')
import meshview  # noqa: E402
import shaderdb  # noqa: E402

GARAND = {'jntWpn_1_M1Garand_Main': 2005, 'jntWpn_1_Metal': 3825, 'jntWpn_6_US_LongKnife_knife': 1,
          'jntWpn_7_M1Garand_rgl_M1Garand_Main': 2, 'jntWpn_7_Grenade': 8}


def main() -> int:
    try:
        ix = meshview.Index(shaderdb.find_opfor(None))
    except shaderdb.ShaderDbError:
        print('SKIP: no Project OPFOR BFBC2 game copy')
        return 0
    fails = []
    parts = meshview.load_mesh(ix, 'Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh')
    got = {p['name']: len(p['indices']) for p in parts}
    if got != GARAND:
        fails.append(f'Garand parts {got}')
    if any(p['image'] is None for p in parts):
        fails.append('a Garand part has no decoded texture: ' + ', '.join(p['name'] for p in parts if p['image'] is None))
    for name in ('Objects/Weapons/Handheld/Vietnam/NAM_rif_M1Garand/NAM_rif_M1Garand_Mesh',
                 'Objects/Weapons/Projectiles/handgrenade_beach/US_Pineapple_Projectile_Mesh'):
        try:
            if not meshview.load_mesh(ix, name):
                fails.append(f'{name}: no parts')
        except meshview.ViewError as e:
            fails.append(str(e))
    soldier = 'characters/us/us_models/us_02_body_mesh'
    body = meshview.load_mesh(ix, soldier)
    rec = {p['name']: ix.parts[f'{soldier}_lod0/{p["name"]}/standard'.lower()] for p in body}
    b = ix.read(f'{soldier}_lod0_data.res')
    bad, seen, inside = [], 0, 0
    for p in body:
        uv = p['uvs']
        els, _ = meshview.meshdata.elements(rec[p['name']])
        k = meshview.meshdata.texcoord_element(els)
        if k is None:                                     # depth-only splits: positions and skinning only
            continue
        seen += 1
        dec = meshview.meshdata.decode(b, rec[p['name']], p['name'], big_endian=False)['uvs']
        first = next(e for e in els if e[1] == 0)
        if (uv is None or tuple(els[k]) != (0x06, 0, 40, 0) or tuple(first) != (0x08, 0, 16, 0)
                or not np.array_equal(dec, uv)):
            bad.append(p['name'])
        elif uv.min() >= -0.01 and uv.max() <= 1.01 and uv.std(0).min() > 0.05:
            inside += 1
    if seen < 16 or bad or inside < 10:
        fails.append(f'soldier texture coordinates: {len(bad)} of {seen} parts wrong {bad[:3]}')
    print(f'soldier: {seen} parts with texture coordinates, all from the half2 at 40 (not the half4 at 16), '
          f'{inside} spanning the texture')
    laid = meshview._layout([('garand', parts)])
    html = meshview.page(laid, 'test')
    if html.count('"tris"') != len(parts):
        fails.append('the page does not embed every part')
    img = meshview.render(laid, 400)
    lit = int((np.abs(img.astype(int) - 40).sum(2) > 30).sum())
    if lit < 2000:
        fails.append(f'the picture shows only {lit} pixels')
    print(f'Garand: {len(parts)} parts; picture {img.shape[1]}x{img.shape[0]}, {lit} model pixels')
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
