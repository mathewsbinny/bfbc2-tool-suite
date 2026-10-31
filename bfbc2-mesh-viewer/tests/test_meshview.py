r"""meshview.py on the game's files (game tier; skipped without them):

- the M1 Garand's first-person mesh reads as DICE's parts (2005 + 3825 triangles for the rifle; the bayonet, launcher
  and rifle grenade parts are stubs of 1, 2 and 8), with its colour textures decoded;
- a weapon from an async bundle (the Vietnam M1 Garand) and a rigid projectile are found and read;
- the page embeds every part, and the picture shows the model (a few thousand coloured pixels).

  python test_meshview.py
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import meshview  # noqa: E402
import shaderdb  # noqa: E402

GARAND = {'jntWpn_1_M1Garand_Main': 2005, 'jntWpn_1_Metal': 3825, 'jntWpn_6_US_LongKnife_knife': 1,
          'jntWpn_7_M1Garand_rgl_M1Garand_Main': 2, 'jntWpn_7_Grenade': 8}


def main():
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
