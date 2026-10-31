r"""mesh.py's decode and console_layouts (game tier; each part skipped when its files are not on the PC).

- BFBC2's own Xbox level (Valparaiso, as Project OPFOR's console port unpacks it into
  work\onslaught\build_ons_mp_002\con_level): every visible mesh part that the PC copy has with the same triangle
  count, decoded from both copies with each copy's layout, has the same indices, bones and positions, normals and
  tangents within the packing's precision (0.01), and the same UVs where the PC's are in a texture's range
  (|u|, |v| < 16; a few parts hold placeholder UVs, far out of range and different on each copy);
- console_layouts: no name keeps a stray leading letter (every name starts with a top folder its run uses);
- Battlefield 1943 (the Rising Sun workspace next to the suite): the Garand's and the Type 5's visible parts decode
  with BF1943's layouts - unit normals, UVs inside the texture, one bone per part from the part's palette.

  python test_mesh_decode.py
"""
import glob
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path[:0] = [os.path.join(HERE, '..'), os.path.join(SUITE, 'bfbc2-mesh-viewer'), os.path.join(SUITE, 'bfbc2-shader-tools')]
import mesh  # noqa: E402

TOL = 0.01


def bfbc2_xbox(check, opfor):
    con = os.path.join(opfor, 'work', 'onslaught', 'build_ons_mp_002', 'con_level')
    dbs = glob.glob(os.path.join(con, 'Levels', '*', 'Shaders', 'Single.res'))
    if not dbs:
        print('SKIP BFBC2 Xbox: no unpacked console level (work\\onslaught\\build_ons_mp_002\\con_level)')
        return
    import meshview
    import shaderdb
    layouts = mesh.console_layouts(open(dbs[0], 'rb').read())
    roots = {n.split('/')[0] for n in layouts}
    check(roots <= {'objects', 'terrains', 'characters', 'levels', 'vegetation', 'fx', 'weapons', 'decals'},
          f'console_layouts: {len(layouts)} names, top folders {sorted(roots)}')
    ix = meshview.Index(shaderdb.find_opfor(opfor))
    compared, fails = 0, []
    for line in open(os.path.join(con, 'fbrb.manifest'), encoding='utf-8', errors='replace'):
        f = line.rstrip('\n').split('|')
        if len(f) < 2 or f[1] != 'MeshData':
            continue
        pc = ix.read(f[0].lower())
        if pc is None:
            continue
        cb = open(os.path.join(con, *f[0].split('/')), 'rb').read()
        mc, mp = mesh.parse(cb, True), mesh.parse(pc, False)
        stem = f[0][:-len('_data.res')].lower()
        for i, (a_p, b_p) in enumerate(zip(mc['parts'], mp['parts'])):
            pn = a_p['name'].decode('latin-1').lower()
            key = f'{stem}/{pn}/standard'
            if pn.endswith('_zonly') or key not in layouts or key not in ix.parts or a_p['triangles'] != b_p['triangles']:
                continue
            a, b = mesh.decode(cb, layouts[key], i, True), mesh.decode(pc, ix.parts[key], i, False)
            bad = []
            if not np.array_equal(a['indices'], b['indices']):
                bad.append('indices')
            for k in ('positions', 'normals', 'tangents'):
                if k in a and k in b and np.abs(a[k][:, :3] - b[k][:, :3]).max(initial=0) > TOL:
                    bad.append(k)
            if 'bones' in a and 'bones' in b and not np.array_equal(a['bones'], b['bones']):
                bad.append('bones')
            if 'uvs' in a and 'uvs' in b:
                ok = (np.abs(b['uvs']) < 16).all(1)
                if np.abs(a['uvs'][ok] - b['uvs'][ok]).max(initial=0) > TOL:
                    bad.append('uvs')
            compared += 1
            if bad:
                fails.append(f'{f[0]} {pn}: {", ".join(bad)}')
    check(compared >= 100 and not fails, f'BFBC2 Xbox vs PC: {compared} mesh parts decode alike', '; '.join(fails[:4]))


def bf1943(check):
    xenon = os.environ.get('BF1943_XENON') or os.path.join(SUITE, '..', 'operation-rising-sun', 'work', 'extracted',
                                                           'dist', 'xenon')
    fb = os.path.join(SUITE, '..', 'bf1943-tool-suite', 'bf1943-fbrb-tool')
    lvl = os.path.join(xenon, 'levels', 'guadal_canal', 'reduced-level-00.fbrb')
    if not os.path.isfile(lvl) or not os.path.isfile(os.path.join(fb, 'fbrb1943.py')):
        print('SKIP Battlefield 1943: no extracted copy or no BF1943 bundle reader')
        return
    sys.path.insert(0, fb)
    import fbrb1943
    lb = fbrb1943.Bundle(open(lvl, 'rb').read())
    db = next(e['name'] for e in lb.entries if e['type'] == 'Dx9ShaderDatabase')
    layouts = mesh.console_layouts(lb.data(db))
    common = fbrb1943.Bundle(open(os.path.join(xenon, 'levels', 'levels-common-00.fbrb'), 'rb').read())
    for res, stem in (('Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh_lod0_data.res', 'm1garand/m1garand_mesh_lod0'),
                      ('Objects/Weapons/Handheld/Beach/Jap_Type5/Jap_Type5_Mesh_lod0_data.res', 'jap_type5/jap_type5_mesh_lod0')):
        b = common.data(res)
        m = mesh.parse(b, True)
        notes = []
        for i, p in enumerate(m['parts']):
            pn = p['name'].decode('latin-1')
            if pn.lower().endswith('_zonly'):
                continue
            d = mesh.decode(b, layouts[f'objects/weapons/handheld/beach/{stem}/{pn.lower()}/standard'], i, True)
            nl = np.linalg.norm(d['normals'][:, :3], axis=1)
            used = np.unique(d['indices'])
            uv = d['uvs'][used]
            ok = (np.abs(nl - 1) < 0.01).all() and (uv > -0.01).all() and (uv < 1.01).all() and \
                set(np.unique(d['bones'][:, 0])) <= set(p['bones'])
            notes.append(f'{pn} {"ok" if ok else "BAD"}')
            if not ok:
                check(False, f'BF1943 {pn}: normals, UVs or bones out of range')
        check(all(n.endswith('ok') for n in notes), f'BF1943 {res.split("/")[-1]}: ' + ', '.join(notes))


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    opfor = None
    for root in (os.environ.get('OPFOR_ROOT'), os.path.join(SUITE, '..', 'project-opfor-bfbc2')):
        if root and os.path.isdir(os.path.join(root, 'game')):
            opfor = os.path.abspath(root)
            break
    if opfor is None:
        print('SKIP: no Project OPFOR BFBC2 game copy')
    else:
        bfbc2_xbox(check, opfor)
    bf1943(check)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
