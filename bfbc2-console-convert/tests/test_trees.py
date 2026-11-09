r"""Triangle strips and tree mesh sets (mesh.strip_to_list, meshset TreeMeshSet).

Quick (no game data): strip_to_list keeps a strip's winding (odd triangles reversed) and leaves out degenerate
triangles; index_count reads a strip part's triangles + 2 indices.
Game tier (the game, and Battlefield 1943 in the Rising Sun workspace for the last two):
- every one of BFBC2's PC TreeMeshSet resources reads and writes back byte-identical;
- every Battlefield 1943 TreeMeshSet reads;
- BF1943's palm (palmtree_01), converted: its bark top's strips (k = 5) become DICE's triangle list - the same
  triangles with the same winding as BFBC2's own PC palm, on LODs 0 and 1.

  python test_trees.py [--game]
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'mesh-viewer', 'shader-tools', 'bf1943-import', 'anim-tools')
import mesh  # noqa: E402
import meshset  # noqa: E402

failures: list[str] = []


def check(ok, what):
    print(('ok   ' if ok else 'FAIL ') + what)
    if not ok:
        failures.append(what)


def quick():
    # strip 0 1 2 3 4: (0 1 2), (2 1 3) reversed for the odd one, (2 3 4)
    check(mesh.strip_to_list([0, 1, 2, 3, 4]) == [0, 1, 2, 2, 1, 3, 2, 3, 4], 'strip_to_list: winding alternates')
    # BF1943's bark: "2 2 2 0 1 1 5 5 4" - degenerate joins left out, the real triangles kept with their parity
    got = mesh.strip_to_list([2, 2, 2, 0, 1, 1, 5, 5, 4, 3, 3])
    check(all(len(set(got[i:i + 3])) == 3 for i in range(0, len(got), 3)), 'strip_to_list: no degenerate triangles')
    check(mesh.index_count({'k': mesh.STRIP, 'triangles': 1600}) == 1602 and
          mesh.index_count({'k': mesh.LIST, 'triangles': 212}) == 636, 'index_count: strip n + 2, list 3n')


def game():
    import meshview
    import numpy as np
    import shaderdb
    from bfbc2core import fbrb
    opfor = shaderdb.find_opfor(None)
    ix = meshview.Index(opfor)
    names = set()
    for root, _, files in os.walk(os.path.join(opfor, 'game')):
        for f in files:
            if f.endswith('.fbrb'):
                try:
                    for e in fbrb.entries(os.path.join(root, f)):
                        if e.type == 'TreeMeshSet':
                            names.add(e.name.lower())
                except Exception:                                   # noqa: BLE001 - a bundle the reader skips
                    pass
    bad = []
    for n in sorted(names):
        d = ix.read(n)
        if d is None:
            continue
        try:
            if meshset.write(meshset.parse(d, 'TreeMeshSet', False), 'TreeMeshSet') != d:
                bad.append(n)
        except meshset.ConvertError as e:
            bad.append(f'{n}: {e}')
    check(names and not bad, f'BFBC2 TreeMeshSets: {len(names)} read and write back byte-identical {bad[:3]}')
    try:
        import bf1943import
        import meshport
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                          # noqa: BLE001
        print(f'SKIP Battlefield 1943: {e}')
        return
    trees = [k for k, (_, e) in src.entries.items() if e.get('type') == 'TreeMeshSet']
    failed = []
    for k in trees:
        try:
            meshset.parse(src.data(k), 'TreeMeshSet', True)
        except meshset.ConvertError as e:
            failed.append(f'{k}: {e}')
    check(trees and not failed, f'BF1943 TreeMeshSets: {len(trees)} read {failed[:3]}')
    name = 'objects/vegetation/trees/palmtree_01/palmtree_01_mesh'
    r = meshport.Porter(src, ix).port(name)
    for lod in (0, 1):
        a, d = r['lods'][lod], ix.read(f'{name}_lod{lod}_data.res')
        ma, md = mesh.parse(a, False), mesh.parse(d, False)
        q = next(p for p in md['parts'] if p['name'].endswith(b'Level04_BarkTop'))
        p = next(p for p in ma['parts'] if p['name'] == q['name'])

        def tris(b, m, x):
            i = np.frombuffer(b, '<u2', x['triangles'] * 3, m['data'] + m['vertex_bytes'] + x['first_index'] * 2)
            return {tuple(np.roll(t, -int(np.argmin(t)))) for t in i.reshape(-1, 3).tolist()}
        ta, td = tris(a, ma, p), tris(d, md, q)
        check(p['k'] == mesh.LIST and ta == td, f'palm LOD {lod} bark top: {len(ta)} triangles from strips, DICE '
                                                f'{len(td)}, the same with the same winding')


if __name__ == '__main__':
    quick()
    if '--game' in sys.argv:
        game()
    print(f'{len(failures)} failure(s)')
    sys.exit(1 if failures else 0)
