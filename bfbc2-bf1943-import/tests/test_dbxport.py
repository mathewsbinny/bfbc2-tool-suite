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
import re
import subprocess
import sys
import tempfile
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import bf1943import as bi  # noqa: E402
import dbxport  # noqa: E402
from bfbc2core import (
    dbx,  # noqa: E402
    paths,  # noqa: E402
)

MESH = 'objects/weapons/handheld/beach/us_springfield1903/springfield_mesh.dbx'
M24 = 'objects/weapons/handheld/us_sni_m24/us_sni_m24_mesh.dbx'


def same(a: dbx.Element, b: dbx.Element) -> bool:
    return (a.tag, a.kind, a.attributes, a.text, a.raw) == (b.tag, b.kind, b.attributes, b.text, b.raw) and \
        len(a.children or []) == len(b.children or []) and all(same(x, y) for x, y in zip(a.children or [], b.children or []))


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
    files = glob.glob(os.path.join(opfor, 'work', 'audit', 'mp_common', '**', '*.dbx'), recursive=True)
    if files:
        random.seed(7)
        bad = 0
        sample = random.sample(files, min(300, len(files)))
        for path in sample:
            x = dbx.parse(open(path, 'rb').read())
            y = dbx.parse(dbx.write(x))
            bad += not (len(x.roots) == len(y.roots) and all(same(a, b) for a, b in zip(x.roots, y.roots)))
        check(bad == 0, f'write: {len(sample)} BFBC2 partitions parse back to the same tree', f'{bad} differ')
    else:
        print('SKIP write: no unpacked BFBC2 partitions (work\\audit\\mp_common)')
    try:
        src = bi.Source(bi.default_source())
        suite_paths.use('mesh-viewer', 'shader-tools')
        import meshview
        import shaderdb
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP port: {e}')
        return 1 if fails else 0
    T = dbxport.Templates([('m24_mesh', ix.read(M24))])
    out, report = dbxport.port(src.data(MESH), T)
    f: Any                          # the ported partitions: elements the test knows are there (child / attr not None)
    t: Any
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
    # the weapon: BF1943's single mesh fields become BFBC2's arrays, its embedded PersistentWeapon is left out (DICE's
    # convention) and the reference to it takes the donor's (a stand-in), the anim trees (partitions neither game will
    # have) the donor's through the caller's stand-ins - null without them
    import dbxcatalogue
    cat = dbxcatalogue.load(ix)
    WPN, DONOR = 'objects/weapons/handheld/beach/us_springfield1903/springfield1903.dbx', \
        'objects/weapons/handheld/us_sni_m24/us_sni_m24.dbx'
    T = dbxport.Templates([('m24', ix.read(DONOR))])
    folder = 'objects/weapons/handheld/beach/us_springfield1903/'
    stand_ins = {(f, '*'): dbxport.TEMPLATE for f in ('AnimTree1p', 'AnimTree3p', 'Persistence')}
    wout, wrep = dbxport.port(src.data(WPN), T, lambda p: p.startswith(folder) and 'animtree' not in p or
                              f'{p}.dbx' in ix.where, drop_untemplated=True, catalogue=cat, stand_ins=stand_ins)
    w: Any = dbx.parse(wout)
    state = next(i for i in w.instances if i.attr('type') == 'GameSharedResources.SoldierWeaponData') \
        .child('WeaponStates').children[0]
    meshes = [c.attr('ref') for c in state.child('Meshes1p').children]
    donor: Any = dbx.parse(ix.read(DONOR))
    d_state = next(i for i in donor.instances
                   if i.attr('type') == 'GameSharedResources.SoldierWeaponData').child('WeaponStates').children[0]
    check(meshes and meshes[0].startswith(folder + 'springfield_mesh/') and
          state.child('AnimTree1p').attr('ref') == d_state.child('AnimTree1p').attr('ref') and
          not any(i.attr('type') == 'GameSharedResources.PersistentWeapon' for i in w.instances) and
          any(r.startswith('converted') for r in wrep),
          'port: the Springfield weapon - its own mesh (Mesh1p -> Meshes1p), the donor\'s anim tree for BF1943\'s '
          '(a stand-in), BF1943\'s PersistentWeapon left out')
    nout, _ = dbxport.port(src.data(WPN), T, lambda p: p.startswith(folder) and 'animtree' not in p or
                           f'{p}.dbx' in ix.where, drop_untemplated=True, catalogue=cat)
    nf: Any = dbx.parse(nout)
    n_state = next(i for i in nf.instances if i.attr('type') == 'GameSharedResources.SoldierWeaponData')         .child('WeaponStates').children[0]
    check(n_state.child('AnimTree1p').attr('ref') == 'null', 'port: without a stand-in the tree BF1943 names is null')
    # a building: its physics part table is BF1943's own (PartMaterialDatas -> PartInfoDatas, 37 parts, not the
    # template's), its part links named from their indices (the template's names replaced)
    H = 'objects/architecture/beach/beachhouse_t01/beachhouse_t01'
    T = dbxport.Templates([(p, ix.read(p + '.dbx')) for p in
                           ('objects/objects/boat_house/boat_house_havok', 'objects/buildings/barrack/barrack')])
    hav: Any = dbx.parse(dbxport.port(src.data(H + '_havok.dbx'), T)[0])
    names = [c.child('Name').text for c in next(i for i in hav.instances if i.child('PartInfoDatas') is not None)
             .child('PartInfoDatas').children]
    theirs = re.findall(r'<field name="Name">(part[^<]*|ruin)</field>', dbx.to_xml(src.data(H + '_havok.dbx')))
    check(names == theirs and len(names) > 5, f'port: a building\'s physics part table is its own ({len(names)} parts)',
          f'{names[:5]} vs {theirs[:5]}')
    b: Any = dbx.parse(dbxport.port(src.data(H + '.dbx'), T)[0])
    by = {i.attr('guid').lower(): i for i in b.instances}
    sm = next(i for i in b.instances if i.attr('type') == 'GameSharedResources.StaticModelEntityData')
    comps = [by[c.attr('ref').lower()].child('Name').text for c in sm.child('Components').children]
    links = [by[c.attr('ref').lower()] for c in sm.child('PartLinks').children]
    check(links and all(comps[int(dbx.value(k.child('PartComponentIndex1')))] == k.child('PartComponentName1').text and
                        comps[int(dbx.value(k.child('PartComponentIndex2')))] == k.child('PartComponentName2').text
                        for k in links), f'port: a building\'s {len(links)} part links named from their indices')
    # a level (Wake Island): with prune, an entity left out takes its event connections with it (none is redirected
    # to a template's entity); with required, a placement of an object that will not exist is left out
    L = 'levels/wake_island_s.dbx'
    T = dbxport.Templates([('mp_005', ix.read('levels/mp_005.dbx'))])
    gone = 'GameSharedResources.LiveSessionEntityData'
    lv, rep = dbxport.port(src.data(L), T, lambda p: not p.startswith('effects/'), drop_types={gone}, prune=True,
                           required=dbxport.REQUIRED)
    f = dbx.parse(lv)
    data = next(i for i in f.instances if i.child('EventConnections') is not None)
    guids = {i.attr('guid').lower() for i in f.instances}
    ends = [c.child(k).attr('ref').lower() for c in data.child('EventConnections').children for k in ('Source', 'Target')]
    places = [i.child('ReferencedObject').attr('ref') for i in f.instances
              if i.attr('type') == 'Entity.ReferenceObjectData']
    n_before = len([1 for i in dbx.parse(src.data(L)).instances if i.attr('type') == 'Entity.ReferenceObjectData'])
    check(all(g in guids or '/' in g for g in ends) and any(r.startswith('pruned') for r in rep) and
          not any(p.startswith('effects/') for p in places) and len(places) < n_before,
          f'port: a level pruned - every event connection\'s ends exist, {n_before - len(places)} placements of '
          f'missing objects left out')
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
