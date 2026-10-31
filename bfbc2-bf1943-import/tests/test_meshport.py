r"""meshport.py on Battlefield 1943's models (game tier; skipped without the Rising Sun workspace or the game copy).

- the oracle: BF1943 weapon models that BFBC2 shipped (Type 100, Nambu pistol, M1911A1, Mk 2 grenade, Shin Gunto)
  port to BFBC2's own files: the same parts, counts, offsets, index buffer and bone palettes (renumbered from BF1943's
  soldier), every position exact and every normal within 0.01 on LOD 0;
- the models for the WW2 side project port without error (Springfield, k98, both bazookas, long knife, wrench 1p /
  3p): every LOD, the mesh set with each LOD's size, the textures; each part's PC layout read back gives unit normals
  and in-range UVs.

  python test_meshport.py
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path[:0] = [os.path.join(HERE, '..'), os.path.join(SUITE, 'bfbc2-mesh-viewer'), os.path.join(SUITE, 'bfbc2-shader-tools')]
import bf1943import as bi  # noqa: E402
import meshport  # noqa: E402

B = 'Objects/Weapons/Handheld/Beach/'
ORACLE = [B + 'Nambu100/Nambu100_Mesh', B + 'Jap_Nambu_pistol/jap_nambu_pistol_mesh_Mesh', B + 'M1911A1/US_M1911_Mesh',
          B + 'US_Pineapple_Gren/US_Pineapple_Mesh', B + 'Jap_ShinGunto_Knife/Jap_ShinGunto_knife_Mesh']
PORTS = [(B + 'US_Springfield1903/Springfield_Mesh', [B + 'Thompson/thompson_Mesh']),
         (B + 'K98/K98_Mesh', [B + 'Thompson/thompson_Mesh']),
         (B + 'M18/M18_mesh_Mesh', [B + 'M18/M18_mesh_jap_Mesh']),
         (B + 'M18/M18_mesh_jap_Mesh', [B + 'M18/M18_mesh_jap_Mesh']),
         (B + 'US_LongKnife_knife/US_LongKnife_knife_Mesh', [B + 'Jap_ShinGunto_Knife/Jap_ShinGunto_knife_Mesh']),
         (B + 'Wrench/Wrench_Mesh', [B + 'Thompson/thompson_Mesh']),
         (B + 'Wrench/Wrench_Mesh3p', [B + 'Thompson/thompson_Mesh'])]


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    try:
        src = bi.Source(bi.default_source())
        import meshview
        import shaderdb
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP: {e}')
        return 0
    port = meshport.Porter(src, ix)
    mesh = meshport.mesh
    for name in ORACLE:
        r = port.port(name)
        out, pc = r['lods'][0], ix.read(f'{name}_lod0_data.res'.lower())
        mo, mp = mesh.parse(out, False), mesh.parse(pc, False)
        same = [(a['name'], a['triangles'], a['vertices'], a['first_index'], a['vertex_offset'], a['bones']) ==
                (c['name'], c['triangles'], c['vertices'], c['first_index'], c['vertex_offset'], c['bones'])
                for a, c in zip(mo['parts'], mp['parts'])]
        bad = [] if all(same) and len(mo['parts']) == len(mp['parts']) else ['structure']
        for i, a in enumerate(mo['parts']):
            if a['name'].lower().endswith(b'_zonly'):
                continue
            da = mesh.decode(out, r['pc_layouts'][0][i], i, False)
            dc = mesh.decode(pc, r['pc_layouts'][0][i], i, False)
            if not np.array_equal(da['positions'], dc['positions']):
                bad.append('positions')
            if np.abs(da['normals'][:, :3] - dc['normals'][:, :3]).max(initial=0) > 0.01:
                bad.append('normals')
        check(not bad, f'oracle {name.split("/")[-1]}: ported LOD 0 equals BFBC2\'s', ', '.join(bad))
    for name, donors in PORTS:
        try:
            r = port.port(name, donors)
            sizes = [L['size'] for L in meshport.meshset.parse(r['meshset'], r['kind'], False)['lods']]
            ok = sizes == [meshport.meshset.lod_size(b) for b in r['lods']] and r['textures']
            for b, lays in zip(r['lods'], r['pc_layouts']):
                m = mesh.parse(b, False)
                for i, p in enumerate(m['parts']):
                    if p['name'].lower().endswith(b'_zonly') or not p['triangles']:
                        continue
                    d = mesh.decode(b, lays[i], i, False)
                    nl = np.linalg.norm(d['normals'][:, :3], axis=1)
                    used = np.unique(d['indices'])
                    ok = ok and (np.abs(nl[used] - 1) < 0.02).all() and (np.abs(d['uvs'][used]) < 8).all()
            check(bool(ok), f'port {name.split("/")[-1]}: {len(r["lods"])} LODs, {len(r["textures"])} textures, '
                            f'mesh set sizes, normals and UVs')
        except Exception as e:                                   # noqa: BLE001
            check(False, f'port {name.split("/")[-1]}', str(e))
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
