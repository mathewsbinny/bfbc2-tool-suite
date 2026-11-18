"""NavKit: object placements and pathfinding flags of a level, from its extracted data.

Reads an unpacked + XML-converted level (FbrbTool unpack of Dist and Package level-00.fbrb, then
DbxEdit xml on both folders) and writes, next to it:
  objects.json       individually placed objects (Entity.ReferenceObjectData): blueprint, flags, matrix
  group_instances.json  instances of static-model groups / forest / middlegrowth (Physics.CompositeHavokAsset:
                     Assets[i] + Transforms[i]), with blueprint flags
  forbid_tris.npy    world-space collision triangles (N x 3 x 3) of every placed object whose blueprint has
                     ForbidPathfinding = true (from the blueprint's <name>_Havok.res packfile)
  stair_tris.npy     collision of staircases and ramps (blueprint names with stair / ramp / steps): floor that
                     does not block its own users' clearance
  dynamic_tris.npy   the same for dynamic (movable / destructible) objects - DynamicModelEntityData blueprints
                     such as crates and barrels - for objects with DisablePathfinding = PdtWhenAlive (doors), and
                     for barrier arms (a forbidden object that is one thin raised bar): path data treats them as absent

Usage: levelobjects.py <level dir> <level name> [extra blueprint dirs ...]
  e.g. levelobjects.py work\\levels\\sp_01 SP_01 work\\audit\\sp_common work\\audit\\mp_common
"""
import glob
import json
import os
import re
import sys

import numpy as np

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
from bfbc2core import collision, xmlinst  # noqa: E402

SEP = chr(92)


def order_points(level_xml):
    """AI order positions of a level: the Transform translation of every AreaGeometryEntityData (the points the
    squad's area / patrol orders send soldiers to; sp_004_b's stairs-top go-to targets are two of them)."""
    pts = []
    for _typ, _guid, body in xmlinst.instances(level_xml, 'Entity.AreaGeometryEntityData'):
        t = re.search(r'<complex name="Transform">([^<]*)</complex>', body)
        if t:
            v = [float(a) for a in t.group(1).split('/')]
            if len(v) >= 15 and any(abs(c) > 1e-3 for c in v[12:15]):
                pts.append(v[12:15])
    return pts


def area_orders(level_xml):
    """The positions of the level's AreaOrder entities (where an area order sends soldiers - the M-COM targets in
    Onslaught / Rush), for navwalk.OBJECTIVE_FLOOR."""
    pts = []
    for _typ, _guid, body in xmlinst.instances(level_xml, '.AreaOrderEntityData'):
        t = re.search(r'<complex name="Transform">([^<]*)</complex>', body)
        if t:
            v = [float(a) for a in t.group(1).split('/')]
            if len(v) >= 15 and any(abs(c) > 1e-3 for c in v[12:15]):
                pts.append(v[12:15])
    return pts


def scan(bases):
    """The Rust core's one walk over the bases (round E4e: navkit.py prepare spent 327 of sp_009's 887 s in
    blueprint_flags reading every XML file of the level and the common bundles one at a time): per base, the .xml
    files with flags or a Mesh reference and the *_havok.res files, in glob's order, each file read once on all cores
    (blueprint_flags reads its flags from them; the havok index and the mesh aliases are bfbc2core.collision's since
    round F1a). None: the core not in use (navnative), or a file it could not read - blueprint_flags then reads as
    before (its Python is the reference)."""
    import navnative
    m = navnative.core('navkit_level_scan')
    return m.navkit_level_scan([os.fspath(b) for b in bases]) if m is not None else None


def blueprint_flags(bases, scanned=None):
    """lower-case blueprint path -> (ForbidPathfinding, ExcludeFromPathfinding, AllowPathdataSeeds, dynamic,
    DisablePathfinding value - e.g. PdtNever / PdtWhenAlive). scanned: scan(bases)'s records."""
    flags: dict[str, tuple[bool, bool, bool, bool, str]] = {}
    if scanned is not None:
        for (records, _havok) in scanned:
            for rel, fl, _mesh in records:
                if fl is not None:
                    flags.setdefault(rel.replace(SEP, '/').lower()[:-4], tuple(fl))
        return flags
    for base in bases:
        for f in glob.glob(os.path.join(base, '**', '*.xml'), recursive=True):
            t = open(f, encoding='utf-8', errors='replace').read()
            dynamic = 'type="GameSharedResources.DynamicModelEntityData"' in t
            if 'ForbidPathfinding' not in t and not dynamic:
                continue
            rel = os.path.relpath(f, base).replace(SEP, '/').lower()[:-4]
            dis = re.search(r'<field name="DisablePathfinding">([A-Za-z]*)<', t)
            flags.setdefault(rel, ('<field name="ForbidPathfinding">true' in t,
                                   '<field name="ExcludeFromPathfinding">true' in t,
                                   '<field name="AllowPathdataSeeds">true' in t, dynamic, dis.group(1) if dis else ''))
    return flags


def mesh_aliases(bases):
    """lower-case blueprint path -> blueprint path of its Mesh reference (without _mesh), for blueprints that
    reuse another object's mesh and collision (InvWall_SP03 -> invisiblewall/invwall). bfbc2core.collision's index
    (the Rust core; this function's Python was retired 2026-11-18, round F1a): the error that stopped it is raised."""
    _, aliases = collision.index(bases)
    if isinstance(aliases, Exception):
        raise aliases
    return aliases


def havok_index(bases):
    """lower-case blueprint path -> its <name>_Havok.res packfile under the folders (the first folder that has one
    wins); some assets name their packfile <name>_Havok_Havok.res (sp_009's animated aircraft door, PdtWhenAlive:
    without its collision the closed door stayed solid and cut the plane's cabins apart; inv_Wall): it is found under
    <name> too. bfbc2core.collision.havok_index (the Rust core; the Python walk was retired 2026-11-18, round F1a)."""
    return collision.havok_index(bases)


def triangles(path):
    """Collision triangles (local space) of a blueprint's Havok asset: storage extended mesh subparts (16-bit indices
    in groups of four: three corners + padding) and the bounding box of each convex shape (its FourVector vertices);
    None for a file that is not a packfile or has none. bfbc2core.collision.triangles (the Rust core,
    bc2_formats::havok; this function's Python over hkpack.py was retired 2026-11-18, round F1a)."""
    return collision.triangles(path)


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 1
    level_dir, level = argv[1], argv[2]
    bases = [os.path.join(level_dir, 'pkg'), os.path.join(level_dir, 'dist')] + argv[3:]
    scanned = scan(bases)
    flags = blueprint_flags(bases, scanned)
    x = open(os.path.join(level_dir, 'dist', 'levels', level + '.xml'), encoding='utf-8', errors='replace').read()
    objs = []
    for _typ, _guid, body in xmlinst.instances(x, 'Entity.ReferenceObjectData'):
        t = re.search(r'<complex name="Transform">([^<]*)</complex>', body)
        r = xmlinst.field_ref(body, 'ReferencedObject')
        if t and r is not None:
            bp = r.rsplit('/', 1)[0].lower()
            objs.append({'bp': bp, 'flags': flags.get(bp), 'm': [float(a) for a in t.group(1).split('/')]})
    groups = []
    for f in glob.glob(os.path.join(level_dir, 'dist', 'levels', level, '*_HavokAsset.xml')):
        text = open(f, encoding='utf-8').read()
        trm = re.search(r'<array name="Transforms">([^<]*)<', text)
        if not trm:
            continue
        tr = trm.group(1).split('/')
        assets = re.findall(r'<item ref="([^"]*)" />', text.split('<array name="Assets">')[1].split('</array>')[0])
        for i, a in enumerate(assets):
            bp = re.sub(r'_havok$', '', a.rsplit('/', 1)[0].lower())
            groups.append({'group': os.path.basename(f), 'bp': bp, 'flags': flags.get(bp),
                           'm': [float(v) for v in tr[i * 16:(i + 1) * 16]]})
    # soldier spawn points (seed points of the path data)
    spawns = []
    for _typ, _guid, body in xmlinst.instances(x, '.SoldierSpawnEntityData'):
        t = re.search(r'<complex name="Transform">([^<]*)</complex>', body)
        if t:
            v = [float(a) for a in t.group(1).split('/')]
            if any(abs(c) > 1e-3 for c in v[12:15]):
                spawns.append(v[12:15])
    json.dump(spawns, open(os.path.join(level_dir, 'soldier_spawns.json'), 'w'))
    json.dump(order_points(x), open(os.path.join(level_dir, 'order_points.json'), 'w'))
    json.dump(area_orders(x), open(os.path.join(level_dir, 'area_orders.json'), 'w'))
    json.dump(objs, open(os.path.join(level_dir, 'objects.json'), 'w'))
    json.dump(groups, open(os.path.join(level_dir, 'group_instances.json'), 'w'))

    havok, aliases = collision.index(bases)
    if isinstance(aliases, Exception):
        raise aliases
    # blueprints without their own collision asset use the one of the object their Mesh reference names
    for bp, target in aliases.items():
        if bp not in havok and target in havok:
            havok[bp] = havok[target]
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
    # dynamic objects, and objects with DisablePathfinding = PdtWhenAlive (doors ...): path data ignores them
    def barrier_arm(o):
        """Whole collision is one thin raised bar (boom barrier): >= 0.7 m above the object's origin, <= 0.5 m
        thick, >= 2 m long. Path data treats it as passable (DICE's routes cross closed checkpoint poles)."""
        if o['bp'] not in havok:
            return False
        if o['bp'] not in cache:
            cache[o['bp']] = triangles(havok[o['bp']])
        t = cache[o['bp']]
        if t is None:
            return False
        v = t.reshape(-1, 3)
        lo, hi = v.min(0), v.max(0)
        return lo[1] >= 0.7 and hi[1] - lo[1] <= 0.5 and max(hi[0] - lo[0], hi[2] - lo[2]) >= 2.0
    dyn = [w for w in (tris_of(o) for o in objs + groups
                       if o['flags'] and ((len(o['flags']) > 3 and o['flags'][3]) or
                                          (len(o['flags']) > 4 and o['flags'][4] == 'PdtWhenAlive') or
                                          (o['flags'][0] and barrier_arm(o)))) if w is not None]
    # staircases and ramps (DICE blueprint names): their own collision is floor, not an obstacle to clearance
    stair_like = re.compile(r'stair|ramp|steps', re.I)
    stairs = [w for w in (tris_of(o) for o in objs + groups if stair_like.search(o['bp'].rsplit('/', 1)[-1]))
              if w is not None]
    np.save(os.path.join(level_dir, 'stair_tris.npy'), np.concatenate(stairs) if stairs else np.zeros((0, 3, 3)))
    print(f'{sum(len(w) for w in stairs)} stair triangles from {len(stairs)} staircases / ramps')
    print('barrier arms:', sorted({o['bp'].split('/')[-1] for o in objs + groups if o['flags'] and o['flags'][0] and barrier_arm(o)}))
    np.save(os.path.join(level_dir, 'dynamic_tris.npy'), np.concatenate(dyn) if dyn else np.zeros((0, 3, 3)))
    print(f'{sum(len(w) for w in dyn)} dynamic-object triangles from {len(dyn)} instances')
    # objects excluded from path finding (fences, sandbags, palm trees): their collision does not block clearance
    # (added to the dynamic set); only their footprint at ground level - parts reaching within 0.5 m of the
    # object's origin height - stays a barrier (exclude_footprint_tris.npy) instead of the whole object
    excluded = [o for o in objs + groups if o['flags'] and o['flags'][1]]
    ex_all, ex_foot = [], []
    for o in excluded:
        w = tris_of(o)
        if w is None:
            continue
        ex_all.append(w)
        base = o['m'][13]
        ex_foot.append(w[w[:, :, 1].min(1) <= base + 0.5])
    np.save(os.path.join(level_dir, 'exclude_tris.npy'), np.concatenate(ex_all) if ex_all else np.zeros((0, 3, 3)))
    np.save(os.path.join(level_dir, 'exclude_footprint_tris.npy'),
            np.concatenate(ex_foot) if ex_foot else np.zeros((0, 3, 3)))
    print(f'{len(ex_all)} excluded objects: {sum(len(w) for w in ex_all)} triangles, footprint {sum(len(w) for w in ex_foot)}')
    world = []
    for o in objs + groups:
        if not (o['flags'] and o['flags'][0]) or o['flags'][1]:
            continue
        w = tris_of(o)
        if w is not None:
            world.append(w)
    np.save(os.path.join(level_dir, 'forbid_tris.npy'), np.concatenate(world) if world else np.zeros((0, 3, 3)))
    print(f'{len(objs)} placed objects, {len(groups)} group instances, {len(flags)} blueprints with flags; '
          f'{sum(len(w) for w in world)} forbid triangles from {len(world)} instances; no collision: {sorted(missing)}')
    return 0


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('levelobjects', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
