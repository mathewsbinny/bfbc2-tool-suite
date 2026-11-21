r"""Records the mesh viewer's answers on real meshes for the map editor's asset preview (round GR3).

  python record_preview_parity.py [--pictures <folder>]

For each mesh below and each of its levels of detail: `meshview.load_mesh`'s parts (name, surface, colour texture,
vertex and triangle counts, SHA-1 of the positions, normals, texture coordinates, indices and the decoded colour
texture) and the SHA-1 of `meshview.render`'s side, top and front pictures at 400 pixels; and one stacked picture of
two rifles (`stack_models`). Written to rust\bc2-editor\tests\data\meshview_parity.json.gz: digests and counts only,
never game content. The editor's `gr3` tests read the same meshes in Rust and compare (rust\bc2-editor\tests\gr3.rs).
With --pictures, the Python's pictures are saved there too (a scratch folder: they are game content).
"""
import gzip
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import meshview  # noqa: E402
import numpy as np  # noqa: E402
import shaderdb  # noqa: E402

MESHES = [
    'Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh',
    'Objects/Buildings/GuardTower/GuardTower_Mesh',
    'Objects/Buildings/Barrack/Barrack_Mesh',
    'Objects/Vegetation/Trees/PalmTree_01/PalmTree_01_Banger_Mesh',
    'Objects/Vehicles/Land/M1A2/M1A2_Mesh',
    'Characters/Beach/3P/Us_Infantry/Us_Infantry_mesh_Mesh',
    'Objects/Objects/AmmoCrate/AmmoCrate_Mesh',
    'Characters/Animals/Parrot/Parrot_01_Mesh',
]
STACKED = ['Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh',
           'Objects/Weapons/Handheld/Beach/Jap_Type5/Jap_Type5_Mesh']
SIZE = 400
OUT = os.path.join(os.path.dirname(os.path.dirname(HERE)), 'rust', 'bc2-editor', 'tests', 'data',
                   'meshview_parity.json.gz')


def sha(a):
    return hashlib.sha1(a).hexdigest() if a is not None else None


def part(p):
    return {'name': p['name'], 'surface': p['surface'], 'texture': p['texture'],
            'vertices': int(len(p['positions'])), 'triangles': int(len(p['indices'])),
            'positions': sha(p['positions'].astype('<f4').tobytes()),
            'normals': sha(p['normals'].astype('<f4').tobytes()) if p['normals'] is not None else None,
            'uvs': sha(p['uvs'].astype('<f4').tobytes()) if p['uvs'] is not None else None,
            'indices': sha(p['indices'].astype('<u4').tobytes()),
            'image': ([int(p['image'].shape[1]), int(p['image'].shape[0]), sha(np.ascontiguousarray(p['image']).tobytes())]
                      if p['image'] is not None else None)}


def picture(laid, view, stem, pictures):
    img = meshview.render(laid, SIZE, view)
    if pictures:
        from PIL import Image
        Image.fromarray(img).save(os.path.join(pictures, f'{stem}_{view}.png'))
    return [int(img.shape[1]), int(img.shape[0]), sha(np.ascontiguousarray(img).tobytes())]


def main(argv):
    pictures = argv[argv.index('--pictures') + 1] if '--pictures' in argv else None
    if pictures:
        os.makedirs(pictures, exist_ok=True)
    ix = meshview.Index(shaderdb.find_opfor(None))
    rows: list[dict] = []
    out: dict = {'size': SIZE, 'meshes': rows, 'stacked': None}
    for name in MESHES:
        lod = 0
        while f'{name}_lod{lod}_data.res'.lower() in ix.where:
            parts = meshview.load_mesh(ix, name, lod)
            laid = meshview.stack_models([(name, parts)])
            stem = f'{name.split("/")[-1]}_lod{lod}'
            rows.append({'mesh': name, 'lod': lod, 'parts': [part(p) for p in parts],
                                  'offset': [float(v) for v in laid[0][2]],
                                  'pictures': {v: picture(laid, v, stem, pictures) for v in ('side', 'top', 'front')}})
            print(f'{stem}: {len(parts)} parts', flush=True)
            lod += 1
    laid = meshview.stack_models([(n, meshview.load_mesh(ix, n, 0)) for n in STACKED])
    out['stacked'] = {'meshes': STACKED, 'offsets': [[float(v) for v in off] for _, _, off in laid],
                      'side': picture(laid, 'side', 'stacked', pictures)}
    with gzip.open(OUT, 'wt', encoding='utf-8') as fh:
        json.dump(out, fh, indent=1)
    print(f'written: {OUT}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
