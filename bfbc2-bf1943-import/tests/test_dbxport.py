r"""The DBX writer (bfbc2core.dbx.write) and dbxport.py (game tier; each part skipped without its data).

- write: 300 of BFBC2's partitions (OPFOR's unpacked work\audit\mp_common) parse back to the same element tree;
- port: BF1943's Springfield mesh partition takes BFBC2's SkinnedMeshAsset / MeshShaderSetAsset schema (BFBC2's
  fields in BFBC2's order, BFBC2-only fields added - MemoryReportGroup, HighPriorityOccluderMesh, RadiosityMesh,
  RadiosityAlbedoTexture - BF1943's values kept: name, LOD count, skeleton, surface shaders); DbxEdit reads the result
  and writes it back unchanged (its round trip).

  python test_dbxport.py
"""
import glob
import os
import random
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path[:0] = [os.path.join(HERE, '..'), os.path.join(SUITE, 'core')]
import bf1943import as bi  # noqa: E402
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

MESH = 'objects/weapons/handheld/beach/us_springfield1903/springfield_mesh.dbx'
M24 = 'objects/weapons/handheld/us_sni_m24/us_sni_m24_mesh.dbx'


def same(a, b):
    return (a.tag, a.kind, a.attributes, a.text, a.raw) == (b.tag, b.kind, b.attributes, b.text, b.raw) and \
        len(a.children or []) == len(b.children or []) and all(same(x, y) for x, y in zip(a.children or [], b.children or []))


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    opfor = os.path.join(SUITE, '..', 'project-opfor-bfbc2')
    files = glob.glob(os.path.join(opfor, 'work', 'audit', 'mp_common', '**', '*.dbx'), recursive=True)
    if files:
        random.seed(7)
        bad = 0
        sample = random.sample(files, min(300, len(files)))
        for f in sample:
            x = dbx.parse(open(f, 'rb').read())
            y = dbx.parse(dbx.write(x))
            bad += not (len(x.roots) == len(y.roots) and all(same(a, b) for a, b in zip(x.roots, y.roots)))
        check(bad == 0, f'write: {len(sample)} BFBC2 partitions parse back to the same tree', f'{bad} differ')
    else:
        print('SKIP write: no unpacked BFBC2 partitions (work\\audit\\mp_common)')
    try:
        src = bi.Source(bi.default_source())
        sys.path[:0] = [os.path.join(SUITE, 'bfbc2-mesh-viewer'), os.path.join(SUITE, 'bfbc2-shader-tools')]
        import meshview
        import shaderdb
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP port: {e}')
        return 1 if fails else 0
    T = dbxport.Templates([('m24_mesh', ix.read(M24))])
    out, report = dbxport.port(src.data(MESH), T)
    f, t = dbx.parse(out), dbx.parse(ix.read(M24))
    ok = True
    for inst in f.instances:
        tmpl = next(i for i in t.instances if i.attr('type') == inst.attr('type'))
        ok = ok and [c.attr('name') for c in inst.children] == [c.attr('name') for c in tmpl.children]
    mesh = f.instances[0]
    kept = mesh.child('Name').text == 'Objects/Weapons/Handheld/Beach/US_Springfield1903/Springfield_Mesh' and \
        mesh.child('Skeleton').attr('ref').startswith('characters/skeletons/ske01/')
    added = {r.split('.')[-1].split(' ')[0] for r in report if r.startswith('added')}
    check(ok and kept and {'MemoryReportGroup', 'HighPriorityOccluderMesh', 'RadiosityMesh'} <= added,
          f'port: Springfield mesh in BFBC2\'s schema ({len(report)} changes), BF1943\'s values kept')
    exe = os.path.join(SUITE, 'bfbc2-dbx-tools', 'DbxEdit.exe')
    if os.path.isfile(exe):
        tmp = tempfile.mkdtemp(prefix='dbxport_')
        p = os.path.join(tmp, 'springfield_mesh.dbx')
        open(p, 'wb').write(out)
        r = subprocess.run([exe, 'roundtrip', p], capture_output=True, text=True)
        check(r.returncode == 0, 'DbxEdit reads the port and writes it back unchanged', (r.stdout + r.stderr)[-200:])
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
