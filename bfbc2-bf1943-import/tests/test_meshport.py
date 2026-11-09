r"""meshport.py on Battlefield 1943's models (game tier; skipped without the Rising Sun workspace or the game copy).

- the oracle: BF1943 weapon models that BFBC2 shipped (Type 100, Nambu pistol, M1911A1, Mk 2 grenade, Shin Gunto)
  port to BFBC2's own files: the same parts, counts, offsets, index buffer and bone palettes (renumbered from BF1943's
  soldier), every position exact and every normal within 0.01 on LOD 0;
- BF1943's rigid mesh sets read without a flag byte per LOD; at least 37 that BFBC2 shipped write as its files;
- the models for the WW2 side project port without error (Springfield, k98, both bazookas, long knife, wrench 1p /
  3p; Willys, Sherman hull and tracks, LCVP, B-25, Ki-21, the bombing-raid cockpit and bunker, the window mount):
  every LOD, the mesh set with each LOD's size, the textures; each part's PC layout read back gives unit normals
  and in-range UVs;
- rename: BF1943's US scout ported under Characters/WW2/ (next to BFBC2's same-named scout): the same LOD bytes and
  textures as the plain port, every name the mesh set holds (shader sets, shaders, LOD settings) and every output
  name under the new namespace, plan['renamed'] covering them, the skeleton pose converted (a PC Granny file).

  python test_meshport.py
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'mesh-viewer', 'shader-tools')
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
V = 'Objects/Vehicles/Beach/'
VDONORS = [V + 'Kurogane/kurogane_mesh_Mesh', V + 'Type97-ChiHa/ChiHa_mb_Mesh', V + 'US_Corsair/US_corsair_Mesh']
PORTS += [(V + n, VDONORS) for n in ('Willys-MB/Willys-MB_Mesh', 'M4-Sherman/US_ShermanM4_tank_Mesh',
                                     'M4-Sherman/US_ShermanM4_Tracks_mesh', 'LCVP-LandingCraft/gen_Landingcraft_Mesh',
                                     'Mitchell_bomber/Mitchell_bomber_mesh_Mesh', 'Sally_Bomber/Sally_bomber_Mesh',
                                     'Cockpits/Cockpit_BombingRaid_US_Mesh', 'BombingRaid_Prototype/AirRaidDummy_Mesh')]
PORTS += [('Objects/Weapons/Stationary/Mounts/HMG_AGL_WindowMount_Mesh', VDONORS)]


SCOUT = 'Characters/Beach/3P/Us_Scout/US_Scout_Mesh'


def renamed_port(check, p):
    """The scout under Characters/WW2/: everything as the plain port but the names."""
    import granny
    meshset = meshport.meshset
    try:
        plain = p.port(SCOUT)
        r = p.port(SCOUT, rename={'Characters/Beach/': 'Characters/WW2/'})
    except Exception as e:                                       # noqa: BLE001
        check(False, 'rename: the scout ports under Characters/WW2/', repr(e))
        return
    s = meshset.parse(r['meshset'], 'SkinnedMeshSet', False)
    strings = [s['lod_settings']] + [x for L in s['lods'] for n, shs in L['sets'] for x in [n] + shs]
    old = [x for x in strings if x.lower().startswith(b'characters/beach/')]
    names = [r['names']['meshset']] + r['names']['lods'] + list(r['textures']) + list(r['poses'])
    check(r['lods'] == plain['lods'] and sorted(r['textures'].values()) == sorted(plain['textures'].values())
          and len(r['meshset']) == len(plain['meshset']) - 2 * sum(1 for x in strings if x.lower().startswith(b'characters/ww2/')),
          f'rename: the scout\'s {len(r["lods"])} LODs and {len(r["textures"])} textures as the plain port\'s, the mesh '
          f'set shorter by the names only')
    check(not old and all(n.lower().startswith('characters/ww2/') for n in names) and
          all(o in r['plan']['renamed'] for o in [SCOUT, 'Characters/Beach/3P/Jap_Shared/Jap_LodSettings']),
          f'rename: every name under Characters/WW2/ ({len(strings)} mesh set strings, {len(names)} outputs)',
          f'{old[:3]} {names[:3]}')
    pose = next(iter(r['poses'].values()), None)
    try:
        ok = pose is not None and granny.Granny(pose).e == '<'
    except granny.GrannyError:
        ok = False
    check(ok, 'rename: the scout\'s skeleton pose converted under the new name', str(list(r['poses'])))


def rigid_sets(check, src, ix):
    """BF1943's rigid mesh sets have no flag byte a LOD: every one reads so, and those BFBC2 shipped as rigid sets
    write as BFBC2's file once their LOD sizes are BFBC2's (at least 37: DICE re-exported the others)."""
    meshset = meshport.meshset
    read = same = 0
    for k, (_, e) in sorted(src.entries.items()):
        if e['type'] != 'RigidMeshSet':
            continue
        s = meshset.parse(src.data(k), 'RigidMeshSet', True, flag=False)
        read += 1
        pc = ix.read(k)
        if pc is None:
            continue
        try:
            b = meshset.parse(pc, 'RigidMeshSet', False)
        except meshset.ConvertError:
            continue
        if len(s['lods']) == len(b['lods']):
            for L, M in zip(s['lods'], b['lods']):
                L['size'], L['flag'] = M['size'], M['flag']
            same += meshset.write(s, 'RigidMeshSet') == pc
    check(read >= 160 and same >= 37, f'rigid sets: {read} read without the flag byte; {same} shared ones equal BFBC2\'s')


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
    renamed_port(check, port)
    rigid_sets(check, src, ix)
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
            # textures: the model folder's (the bunker and window mount keep none of their own: they draw others')
            ok = sizes == [meshport.meshset.lod_size(b) for b in r['lods']] and \
                (r['textures'] or name.endswith(('AirRaidDummy_Mesh', 'HMG_AGL_WindowMount_Mesh')))
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
