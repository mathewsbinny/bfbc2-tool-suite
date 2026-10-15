"""NavKit: object placements and pathfinding flags of a level, from its extracted data.

Reads an unpacked + DbxTool-converted level (FbrbTool unpack of Dist and Package level-00.fbrb, then
DbxTool on both folders) and writes, next to it:
  objects.json       individually placed objects (Entity.ReferenceObjectData): blueprint, flags, matrix
  group_instances.json  instances of static-model groups / forest / middlegrowth (Physics.CompositeHavokAsset:
                     Assets[i] + Transforms[i]), with blueprint flags
  forbid_tris.npy    world-space collision triangles (N x 3 x 3) of every placed object whose blueprint has
                     ForbidPathfinding = true (from the blueprint's <name>_Havok.res packfile)
  dynamic_tris.npy   the same for dynamic (movable / destructible) objects - DynamicModelEntityData blueprints
                     such as crates, barrels, sandbags; path data treats them as absent

Usage: levelobjects.py <level dir> <level name> [extra blueprint dirs ...]
  e.g. levelobjects.py work\\levels\\sp_01 SP_01 work\\audit\\sp_common work\\audit\\mp_common
"""
import glob
import json
import os
import re
import struct
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hkpack  # noqa: E402

SEP = chr(92)


def blueprint_flags(bases):
    """lower-case blueprint path -> (ForbidPathfinding, ExcludeFromPathfinding, AllowPathdataSeeds, dynamic)."""
    flags = {}
    for base in bases:
        for f in glob.glob(os.path.join(base, '**', '*.xml'), recursive=True):
            t = open(f, encoding='utf-8', errors='replace').read()
            dynamic = 'type="GameSharedResources.DynamicModelEntityData"' in t
            if 'ForbidPathfinding' not in t and not dynamic:
                continue
            rel = os.path.relpath(f, base).replace(SEP, '/').lower()[:-4]
            flags.setdefault(rel, ('<field name="ForbidPathfinding">true' in t,
                                   '<field name="ExcludeFromPathfinding">true' in t,
                                   '<field name="AllowPathdataSeeds">true' in t, dynamic))
    return flags


def havok_index(bases):
    out = {}
    for base in bases:
        for f in glob.glob(os.path.join(base, '**', '*_havok.res'), recursive=True):
            rel = os.path.relpath(f, base).replace(SEP, '/').lower()[:-len('_havok.res')]
            out.setdefault(rel, f)
    return out


def triangles(path):
    """Collision triangles (local space) of a blueprint's Havok asset (storage extended mesh subparts;
    16-bit indices in groups of four: three corners + padding)."""
    try:
        pf = hkpack.load(path)
    except ValueError:
        return None
    tris = []
    for off in pf.objects_of('hkpStorageExtendedMeshShapeMeshSubpartStorage'):
        p = pf.deref(off + 8)
        n = struct.unpack_from('<i', pf.d, off + 12)[0]
        ip = pf.deref(off + 0x14)
        ni = struct.unpack_from('<i', pf.d, off + 0x18)[0]
        if not (p and n > 0 and ip and ni > 0):
            continue
        v = np.frombuffer(pf.d, dtype='<f4', count=n * 4, offset=p).reshape(-1, 4)[:, :3]
        idx = np.frombuffer(pf.d, dtype='<u2', count=ni, offset=ip).reshape(-1, 4)[:, :3]
        if idx.max() < n:
            tris.append(v[idx])
    return np.concatenate(tris) if tris else None


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 1
    level_dir, level = argv[1], argv[2]
    bases = [os.path.join(level_dir, 'pkg'), os.path.join(level_dir, 'dist')] + argv[3:]
    flags = blueprint_flags(bases)
    x = open(os.path.join(level_dir, 'dist', 'levels', level + '.xml'), encoding='utf-8', errors='replace').read()
    objs = []
    for m in re.finditer(r'type="Entity\.ReferenceObjectData">(.*?)</instance>', x, re.S):
        t = re.search(r'<complex name="Transform">([^<]*)</complex>', m.group(1))
        r = re.search(r'<field ref="([^"]*)" name="ReferencedObject"', m.group(1))
        if t and r:
            bp = r.group(1).rsplit('/', 1)[0].lower()
            objs.append({'bp': bp, 'flags': flags.get(bp), 'm': [float(a) for a in t.group(1).split('/')]})
    groups = []
    for f in glob.glob(os.path.join(level_dir, 'dist', 'levels', level, '*_HavokAsset.xml')):
        t = open(f, encoding='utf-8').read()
        tr = re.search(r'<array name="Transforms">([^<]*)<', t)
        if not tr:
            continue
        tr = tr.group(1).split('/')
        assets = re.findall(r'<item ref="([^"]*)" />', t.split('<array name="Assets">')[1].split('</array>')[0])
        for i, a in enumerate(assets):
            bp = re.sub(r'_havok$', '', a.rsplit('/', 1)[0].lower())
            groups.append({'group': os.path.basename(f), 'bp': bp, 'flags': flags.get(bp),
                           'm': [float(v) for v in tr[i * 16:(i + 1) * 16]]})
    json.dump(objs, open(os.path.join(level_dir, 'objects.json'), 'w'))
    json.dump(groups, open(os.path.join(level_dir, 'group_instances.json'), 'w'))

    havok = havok_index(bases)
    cache, missing = {}, set()

    def tris_of(o):
        if o['bp'] not in cache:
            cache[o['bp']] = triangles(havok[o['bp']]) if o['bp'] in havok else None
        t = cache[o['bp']]
        if t is None:
            missing.add(o['bp'])
            return None
        m = np.array(o['m']).reshape(4, 4)        # rows: right, up, forward, translation
        return (t @ m[:3, :3] + m[3, :3]).astype(np.float32)
    dyn = [w for w in (tris_of(o) for o in objs if o['flags'] and len(o['flags']) > 3 and o['flags'][3]) if w is not None]
    np.save(os.path.join(level_dir, 'dynamic_tris.npy'), np.concatenate(dyn) if dyn else np.zeros((0, 3, 3)))
    print(f'{sum(len(w) for w in dyn)} dynamic-object triangles from {len(dyn)} instances')
    world = []
    for o in objs + groups:
        if not (o['flags'] and o['flags'][0]):
            continue
        w = tris_of(o)
        if w is not None:
            world.append(w)
    np.save(os.path.join(level_dir, 'forbid_tris.npy'), np.concatenate(world) if world else np.zeros((0, 3, 3)))
    print(f'{len(objs)} placed objects, {len(groups)} group instances, {len(flags)} blueprints with flags; '
          f'{sum(len(w) for w in world)} forbid triangles from {len(world)} instances; no collision: {sorted(missing)}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
