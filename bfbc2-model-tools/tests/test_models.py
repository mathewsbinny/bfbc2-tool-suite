r"""skinmesh.py, shapes.py and the skinned mesh sets (console converter's meshset.py) on the game's files (game
tier; skipped without them):

- round trip: every LOD of the M1 Garand (first and third person) and the Type 5 is written back byte-identical by
  SkinnedLod, and their SkinnedMeshSets by meshset.write;
- replace: a stub part filled with a lathe and an extrusion - the file parses, the part has the new triangles, every
  other part's vertex and index data are unchanged, the positions read back (float exact in the first-person
  layout, half precision in the third-person one), bones and the size the mesh set records follow;
- shapes: after fix_winding every triangle faces its vertices' normals; tangents are unit and across the normals.

  python test_models.py
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path[:0] = [os.path.join(HERE, '..'), os.path.join(HERE, '..', '..', 'bfbc2-mesh-viewer'),
                os.path.join(HERE, '..', '..', 'bfbc2-console-convert')]
import meshset  # noqa: E402
import skinmesh  # noqa: E402
from shapes import extrude, fix_winding, lathe  # noqa: E402

MESHES = [('Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh', 1),
          ('Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh3p', 3),
          ('Objects/Weapons/Handheld/Beach/Jap_Type5/Jap_Type5_Mesh', 4)]


def lod(ix, name, k):
    b = ix.read(f'{name}_lod{k}_data.res')
    m = skinmesh.meshdata.parse(b, False)
    lay = {p['name'].decode(): ix.parts[f'{name}_lod{k}/{p["name"].decode()}/standard'.lower()] for p in m['parts']}
    return b, skinmesh.SkinnedLod(b, lay, f'{name} LOD {k}')


def main():
    import meshview
    import shaderdb
    try:
        ix = meshview.Index(shaderdb.find_opfor(None))
    except shaderdb.ShaderDbError:
        print('SKIP: no Project OPFOR BFBC2 game copy')
        return 0
    fails = []
    n = 0
    for name, lods in MESHES:
        for k in range(lods):
            b, s = lod(ix, name, k)
            n += 1
            if s.to_bytes() != b:
                fails.append(f'{name} LOD {k}: not written back identical')
        ms = ix.read(name + '.res')
        if meshset.write(meshset.parse(ms, 'SkinnedMeshSet', False), 'SkinnedMeshSet') != ms:
            fails.append(f'{name}: mesh set not written back identical')
    print(f'round trip: {n} LODs, {len(MESHES)} mesh sets' + (' ok' if not fails else ' FAILED'))

    g = fix_winding(lathe([(1.10, 0.0085), (1.10, 0.0120), (1.30, 0.0110)], (0.0, 1.2), (0.5, 0.7, 0.6, 0.9), segments=12))
    g = g + fix_winding(extrude([(1.1, 1.14), (1.5, 1.14), (1.5, 1.165), (1.1, 1.165)], 0.003, (0.9, 0.0, 0.1, 0.3),
                                (1.1, 1.165, 1.5, 1.14), (0.6, 0.3, 0.9, 0.4)))
    g = g.with_bone(10)
    a, b_, c = g.pos[g.tris[:, 0]], g.pos[g.tris[:, 1]], g.pos[g.tris[:, 2]]
    facing = np.sum(np.cross(b_ - a, c - a) * g.nrm[g.tris].sum(1), axis=1)
    if (facing <= 0).any():
        fails.append(f'shapes: {int((facing <= 0).sum())} triangles face away from their normals')
    t, bn = g.tangents()
    if np.abs(np.linalg.norm(t, axis=1) - 1).max() > 1e-6 or np.abs(np.sum(t * g.nrm / np.linalg.norm(g.nrm, axis=1, keepdims=True), axis=1)).max() > 1e-6:
        fails.append('shapes: tangents not unit or not across the normals')

    for name, k, part, tol in (('Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh', 0, 'jntWpn_6_US_LongKnife_knife', 1e-6),
                               ('Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh3p', 0, 'jntWpn_6_US_LongKnife_knife', 2e-3)):
        b, s = lod(ix, name, k)
        before = {p.name: (p.rows.tobytes(), p.idx.tobytes()) for p in s.parts}
        s.replace(part, g)
        out = s.to_bytes()
        _, s2 = lod(ix, name, k) if False else (None, None)
        m = skinmesh.meshdata.parse(out, False)
        lay = {p['name'].decode(): ix.parts[f'{name}_lod{k}/{p["name"].decode()}/standard'.lower()] for p in m['parts']}
        s2 = skinmesh.SkinnedLod(out, lay, 'written')
        p2 = s2.part(part)
        if len(p2.idx) != len(g.tris) or p2.info['bones'] != [10]:
            fails.append(f'{name}: replaced part has {len(p2.idx)} triangles, bones {p2.info["bones"]}')
        err = np.abs(s2.decode(p2, skinmesh.POS)[:, :3] - g.pos).max()
        if err > tol:
            fails.append(f'{name}: positions read back off by {err}')
        for p in s2.parts:
            if p.name != part and (p.rows.tobytes(), p.idx.tobytes()) != before[p.name]:
                fails.append(f'{name}: part {p.name} changed')
        if skinmesh.lod_size(out) != m['vertex_bytes'] + m['index_bytes'] + 12:
            fails.append(f'{name}: lod size')
        print(f'replace in {name.split("/")[-1]}: {len(g.tris)} triangles, position error {err:.2g}')
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
