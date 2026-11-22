r"""dbxport.port in the Rust core against its Python reference (round E4d; quick tier: no game data).

Seeded random BF1943 partitions ported on random BFBC2 template pools, catalogues and options - the rules' own
fields among random ones (the weapon conventions, enum numbers, renamed / nested / one-to-array fields, the bones,
part links, references local, cross-partition, relocated, standing in, pruned, required) and random shapes (schema
errors, converters, duplicates, empty arrays). Each port runs both ways (dbxport's DUAL mode): the same bytes,
report lines and records, or the same exception and message. The run also counts the ports the Rust side left to
Python (they must be few: each is a case it says it does not reproduce).

  python test_dbxport_native.py [--cases N] [--seed S] [--game]

--game: also every fair port and counterpart port (rust/bench_bf1943.py's fair workload) and every effect port
(rust/bench_ports.py's) of the user's copies, both ways, none left to Python. With no Rust core built the test says
so and passes (nothing to compare).
"""
import collections
import contextlib
import os
import random
import struct
import sys
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'core')
import dbxcatalogue  # noqa: E402
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

E = dbx.Element
T, N, C = dbx.TEXT, dbx.NUMBERS, dbx.CONTAINER

TEXTS = ['EIAFire', 'EIAThrottle', 'EIUndefined', 'EIAFireCountermeasure', 'EiaYaw', ' EIAPitch ', 'VH_SWITCH',
         'VH_BOAT_THROTTLE', 'wabsShg', 'wabsHg', 'wabsRif', 'ztHoldButtonToZoom', 'ztPressButtonToSwitchZoomLevel',
         'SgptAudio', 'SgptEvent', 'Name1', 'Bad_Enum', 'horn', 'ske01', 'characters/skeletons/ske01', 'Bip01',
         'Bip01 Spine', 'x y', '', '1', 'true', 'EIA']
FIELDS = ['Name', 'Value', 'Speed', 'Enabled', 'Mesh', 'Sound', 'Count', 'Data', 'Extra', 'InputAction',
          'FireInputAction', 'HintAction1', 'HintLine1', 'ActionToSuppress', 'Items', 'Struct']
BONES = ['Root', 'Bip01', 'Bip01 Spine', 'Bip01 Head', 'Bip01 R Hand', 'Weapon', 'Bip01 L Hand']


def num_raw(vals: list, fmt: str) -> bytes:
    raw = bytearray()
    dbx.put_varint(raw, len(vals))
    dbx.put_varint(raw, struct.calcsize(fmt))
    for v in vals:
        raw += struct.pack(fmt, v)
    return bytes(raw)


class Gen:
    """Random partitions: one set of guids per partition, references among them."""

    def __init__(self, rnd: random.Random) -> None:
        self.r = rnd
        self.parts: dict[str, list[str]] = {}      # partition -> its guids
        self.inline_left = 1                        # array items that may name a local instance (see ref)

    def guid(self) -> str:
        return '%08x-%04x-%04x-%04x-%012x' % tuple(self.r.getrandbits(b) for b in (32, 16, 16, 16, 48))

    def num(self, name: str | None, tag: str = 'field') -> dbx.Element:
        r = self.r
        kind = r.random()
        if name in ('MaxMainBoneIndex', 'WeaponBoneIndex', 'BoneIndices', 'MinBoneIndex', 'BoneId'):
            # small integers: the reference counts up to a MaxMainBoneIndex (range(old[0] + 1)), so a float's bits
            # read as one (~4.6e18) would run it out of memory - as no real file does (seeds 9482, 100061)
            kind = 0.7
        if kind < 0.6:
            fmt, vals = '>f', [r.choice([0.0, 1.0, 0.5, -9.0, 24.0, 300.0, 450.0, 600.0, 120.0, -1.0, 2.5])
                               for _ in range(r.choice([1, 1, 1, 3, 4]))]
        elif kind < 0.85:
            fmt, vals = '>i', [r.choice([0, 1, -1, 2, 3, 4, 5, 6, 7, 16, 99]) for _ in range(r.choice([0, 1, 1, 1, 2]))]
        elif kind < 0.95:
            fmt, vals = '>h', [r.choice([0, 1, 7]) for _ in range(r.choice([1, 2]))]
        else:
            fmt, vals = '>d', [r.choice([0.0, 1.5])]
        return E(tag, N, [('name', name)] if name is not None else [], '', num_raw(vals, fmt))

    def text(self, name: str | None) -> dbx.Element:
        return E('field', T, [('name', name)] if name is not None else [], self.r.choice(TEXTS))

    def ref(self, name: str | None, own: list[str], tag: str = 'field') -> dbx.Element:
        r = self.r
        x = r.random()
        # an array item naming a local instance may be held inline (BFBC2's shape); instances held inline in each
        # other many times over grow exponentially in the reference too (seed 9482): one such item per partition
        if tag == 'item' and x < 0.4 and own:
            if self.inline_left <= 0:
                x = 0.45
            else:
                self.inline_left -= 1
        if x < 0.4 and own:
            target = r.choice(own)
        elif x < 0.5:
            target = 'null'
        elif x < 0.6:
            target = self.guid()                    # a local guid nothing holds
        else:
            p = r.choice(list(self.parts) + ['other/missing', 'Objects/Weapons/X', 'fx/vehicleexplosions/a',
                                             'sounds/horn_a', 'anims/soldier_run'])
            gs = self.parts.get(p) or [self.guid()]
            target = f'{p}/{r.choice(gs) if r.random() < 0.7 else self.guid()}'
        attrs = [('ref', target)] + ([('name', name)] if name is not None else [])
        return E(tag, T, attrs, '')

    def value(self, name: str | None, own: list[str], depth: int) -> dbx.Element:
        r = self.r
        x = r.random()
        if x < 0.3:
            return self.num(name)
        if x < 0.45:
            return self.text(name)
        if x < 0.55:
            return E('field', 6, [('name', name)] if name is not None else [], '', bytes([0, r.choice([0, 1, 2])]))
        if x < 0.7:
            return self.ref(name, own)
        if x < 0.85 and depth < 3:
            return self.array(name or 'Arr', own, depth + 1)
        if depth < 3:
            return self.struct(name or 'S', own, depth + 1)
        return self.num(name)

    def array(self, name: str, own: list[str], depth: int) -> dbx.Element:
        r = self.r
        n = r.choice([0, 0, 1, 2, 3])
        if n == 0:
            return E('array', r.choice([T, C]), [('name', name)], '', b'', None if r.random() < 0.5 else [])
        kind = r.random()
        if kind < 0.4:
            items = [self.ref(None, own, 'item') for _ in range(n)]
        elif kind < 0.8:
            items = [self.struct(None, own, depth + 1, tag='complex') for _ in range(n)]
        else:
            return self.num(name, 'array')
        return E('array', C, [('name', name)], '', b'', items)

    def struct(self, name: str | None, own: list[str], depth: int, tag: str = 'complex') -> dbx.Element:
        fields = self.r.sample(FIELDS, self.r.randint(1, 4))
        kids = [self.value(f, own, depth) for f in fields]
        if self.r.random() < 0.05 and kids:
            kids.append(kids[0])                    # a duplicate field
        return E(tag, C, [('name', name)] if name is not None else [], '', b'', kids)

    def rule_fields(self, typ: str, own: list[str]) -> list[dbx.Element]:
        """The fields a rule looks at, for the types that have them."""
        r = self.r
        short = typ.rsplit('.', 1)[-1]
        f: list[dbx.Element] = []
        if short == 'FiringFunctionData':
            f.append(E('complex', C, [('name', 'Shot')], '', b'', [
                E('field', N, [('name', 'InitialSpeed')], '', num_raw([0.0, 0.0, r.choice([24.0, 350.0, 600.0, 50.0])],
                                                                      '>f')),
                self.ref('ProjectileData', own)]))
            f.append(E('complex', C, [('name', 'Ammo')], '', b'', [
                E('field', N, [('name', 'NumberOfMagazines')], '', num_raw([r.choice([-1, 4])], '>i')),
                E('field', N, [('name', 'AmmoPickupMinAmount')], '', num_raw([r.choice([0, 3])], '>i')),
                E('field', N, [('name', 'AmmoPickupMaxAmount')], '', num_raw([r.choice([0, 6])], '>i'))]))
            if r.random() < 0.5:
                f.append(E('complex', C, [('name', 'FireLogic')], '', b'', [
                    E('field', N, [('name', 'MaxRecoilAngleX')], '', num_raw([r.choice([0.0, 2.0])], '>f')),
                    E('field', N, [('name', 'MinRecoilAngleX')], '', num_raw([1.0], '>f')),
                    E('complex', C, [('name', 'Recoil')], '', b'', [
                        E('field', N, [('name', 'MaxRecoilAngleX')], '', num_raw([0.5], '>f')),
                        E('field', N, [('name', 'MinRecoilAngleX')], '', num_raw([0.25], '>f'))])]))
        elif short in ('BulletEntityData', 'GrenadeEntityData'):
            f.append(E('field', N, [('name', 'Gravity')], '', num_raw([r.choice([0.0, -9.0, -9.81])], '>f')))
        elif short == 'WeaponFiringData':
            f.append(E('field', 6, [('name', 'UseAutoAiming')], '', bytes([0, r.choice([0, 1])])))
        elif short == 'SoldierAimingSimulationData':
            f.append(E('field', T, [('name', 'ZoomType')], r.choice(['ztHoldButtonToZoom', 'ztPressButtonToSwitchZoomLevel'])))
        elif short == 'ZoomLevelData':
            f.append(E('field', N, [('name', 'SprintLookSpeedMultiplier')], '', num_raw([r.choice([0.5, 1.0])], '>f')))
        elif short == 'SoldierWeaponData':
            f.append(E('field', T, [('name', 'AnimBaseSet')], r.choice(['wabsShg', 'wabsHg', 'wabsRif'])))
            if r.random() < 0.6:                                # a shotgun keeps wabsShg (spec WPN-WW2-8)
                f.append(E('field', T, [('name', 'WeaponClass')], r.choice(['wcShotgun', 'wcKnife', 'wcSniper'])))
            f.append(E('array', C, [('name', 'WeaponStates')], '', b'', [
                E('complex', C, [], '', b'', [E('field', 6, [('name', 'AlwaysAimHead')], '', bytes([0, r.choice([0, 1])]))])
                for _ in range(r.randint(1, 3))]))
        elif short in ('ChildRotationBodyData', 'ChildMovingBodyData'):
            fld = 'RotationInput' if short == 'ChildRotationBodyData' else 'InputAction'
            if r.random() < 0.7:
                f.append(E('field', N, [('name', fld)], '', num_raw([r.choice([0, 4, 5, 6, 7, 9, 16])], '>i')))
            else:
                f.append(E('field', T, [('name', fld)], r.choice(['EiaFire', 'EiaYaw'])))
        elif short == 'DynamicWeaponPickupEntityData':
            if r.random() < 0.6:
                f.append(E('array', N, [('name', 'WeaponSlots')], '', num_raw(r.sample([0, 1, 2, 3], r.randint(0, 3)), '>i')))
            else:
                f.append(E('array', C, [('name', 'WeaponSlots')], '', b'', [E('complex', C, [], '', b'', [
                    E('field', N, [('name', 'WeaponSlot')], '', num_raw([1], '>i')),
                    E('field', N, [('name', 'AltWeaponSlot')], '', num_raw([-1], '>i')),
                    E('field', N, [('name', 'LinkedToWeaponSlot')], '', num_raw([-1], '>i'))])]))
        elif short == 'BoneFakePhysicsData':
            f.append(E('field', T, [('name', 'BoneName')], r.choice(BONES + [' Bip01 '])))
            f.append(E('field', N, [('name', 'BoneId')], '', num_raw([r.randint(0, 6)], '>i')))
        elif short in ('SkeletonAsset', 'SoldierIKData'):
            f.append(E('field', T, [('name', 'Name')], r.choice(['characters/skeletons/ske01', 'other'])))
            f.append(E('field', N, [('name', 'WeaponBoneIndex')], '', num_raw([r.randint(-1, 8)], '>i')))
            f.append(E('field', N, [('name', 'MaxMainBoneIndex')], '', num_raw([r.randint(-1, 7)], '>i')))
            f.append(E('field', T, [('name', 'MinBoneName')], r.choice(BONES)))
            f.append(E('field', N, [('name', 'MinBoneIndex')], '', num_raw([r.randint(0, 6)], '>i')))
            f.append(E('array', C, [('name', 'BoneIndices')], '', b'', [
                E('item', N, [], '', num_raw([r.randint(0, 7)], '>i')) for _ in range(r.randint(1, 3))]))
        elif short == 'ReferenceObjectData':
            f.append(self.ref('ReferencedObject', own))
        elif short == 'WeaponStateData':
            if r.random() < 0.5:
                f.append(self.ref('Mesh1p', own))
            else:
                f.append(E('array', C, [('name', 'Meshes1p')], '', b'', [self.ref(None, own, 'item')]))
        elif short == 'PhysicsData':
            nm = r.choice(['PartMaterialDatas', 'PartInfoDatas'])
            f.append(self.array(nm, own, 2))
            f.append(self.num(r.choice(['AngularMomentumDamping', 'AngularMomentumDampingAcc', 'UpsideDownAngel',
                                        'UpsideDownAngle'])))
        elif short == 'SampleTextureData':
            row = [r.choice([0.0, 1.0]) for _ in range(4)]
            f.append(E('field', N, [('name', 'GradientData')], '', num_raw(row * r.choice([1, 2, 2]), '>f')))
        elif short == 'PartObjectData':
            f.append(E('array', C, [('name', 'Components')], '', b'', [self.ref(None, own, 'item') for _ in range(2)]))
            f.append(E('array', C, [('name', 'PartLinks')], '', b'', [self.ref(None, own, 'item') for _ in range(2)]))
        elif short == 'PartLinkData':
            f.append(E('field', N, [('name', 'PartComponentIndex1')], '', num_raw([r.randint(-1, 2)], '>i')))
            f.append(E('field', T, [('name', 'PartComponentName1')], 'x'))
            f.append(E('field', T, [('name', 'Name')], r.choice(['a', 'b'])))
        elif short == 'AnimationEntityData':
            f.append(self.ref('Animation', own))
        elif short == 'ProjectileFiringData':
            f.append(self.ref('DetonationEffect', own))
        return f

    def instances(self, n: int, own: list[str], types: list[str], twin: dict[str, list[str]] | None = None) -> list:
        r = self.r
        out = []
        for g in own:
            typ = r.choice(types)
            fields = (twin or {}).get(typ) or r.sample(FIELDS, r.randint(0, 5))
            if twin is not None and r.random() < 0.3:
                fields = [f for f in fields if r.random() < 0.8] + r.sample(FIELDS, 1)
            kids = self.rule_fields(typ, own) + [self.value(f, own, 0) for f in fields]
            r.shuffle(kids)
            attrs = [('guid', g if r.random() < 0.95 else g.upper()), ('type', typ)]
            if r.random() < 0.3:
                attrs.append(('id', 'x'))
            out.append(E('instance', C, attrs, '', b'', kids))
        return out


TYPES = ['Weapons.FiringFunctionData', 'Weapons.BulletEntityData', 'Weapons.GrenadeEntityData',
         'Weapons.WeaponFiringData', 'Weapons.SoldierAimingSimulationData', 'Weapons.ZoomLevelData',
         'Weapons.SoldierWeaponData', 'Physics.ChildRotationBodyData', 'Physics.ChildMovingBodyData',
         'Game.DynamicWeaponPickupEntityData', 'Anim.BoneFakePhysicsData', 'Anim.SkeletonAsset', 'Anim.SoldierIKData',
         'Entity.ReferenceObjectData', 'Weapons.WeaponStateData', 'Physics.PhysicsData', 'Fx.SampleTextureData',
         'Game.PartObjectData', 'Game.PartLinkData', 'Entity.AnimationEntityData', 'Weapons.ProjectileFiringData',
         'GameSharedResources.LandingGearEntryComponentData', 'GameSharedResources.PersistentWeapon',
         'GameSharedResources.PlayerEntryComponentData', 'Misc.Generic', 'Misc.Other']


def partition(g: Gen, insts: list, primary: str) -> bytes:
    p = E('partition', C, [('guid', g.guid()), ('primaryInstance', primary), ('exportMode', 'All')], '', b'', insts)
    roots = [p] + ([E('extra', T, [], 'tail')] if g.r.random() < 0.1 else [])
    return dbx.write(dbx.DbxFile([], roots))


def case(seed: int) -> tuple[str, str]:
    r = random.Random(seed)
    g = Gen(r)
    types = r.sample(TYPES, r.randint(3, 10))
    # BFBC2's template partitions
    tparts: list[tuple[str, bytes]] = []
    schemas: dict[str, list[str]] = {}
    for i in range(r.randint(1, 4)):
        name = r.choice(['objects/weapons/m1/m1', 'vehicles/jeep/jeep', 'levels/x', 'fx/a', 'sounds/b']) + str(i)
        g.parts[name] = [g.guid() for _ in range(r.randint(1, 6))]
    for name, guids in list(g.parts.items()):
        insts = g.instances(len(guids), guids, types)
        for e in insts:
            schemas.setdefault(e.attr('type') or '', [c.attr('name') or '' for c in e.children or ()])
        tparts.append((name + ('.dbx' if r.random() < 0.5 else ''), partition(g, insts, guids[0])))
    if r.random() < 0.1:                                        # templates that do not parse: skipped, reported
        tparts.append(('broken', r.choice([tparts[0][1][:60], b'not a dbx file, not at all', b''])))
    cat = None
    if r.random() < 0.7:
        cat = dbxcatalogue.Catalogue()
        seen: collections.Counter[str] = collections.Counter()
        for name, data in tparts:
            with contextlib.suppress(ValueError, IndexError):   # the broken template
                cat.add_partition(name, dbx.parse(data), seen)
        if r.random() < 0.5:
            cat.exe_names = frozenset(['EiaFire', 'EiaYaw', 'EiaThrottle', 'PlayerEntryComponentData', 'Generic',
                                       'wabsRif', 'ztPressButtonToSwitchZoomLevel'] + r.sample(TEXTS, 5))
        cat.freeze()
    try:
        T_ = dbxport.Templates(tparts, cat if r.random() < 0.5 else None)
    except IndexError:                                          # a broken template without a partition
        return 'templates IndexError', ''
    # the BF1943 partition: some guids DICE's (counterparts), some its own
    own = [g.guid() for _ in range(r.randint(1, 8))]
    pool = [x for gs in g.parts.values() for x in gs]
    if pool and r.random() < 0.5:
        own += r.sample(pool, min(len(pool), r.randint(1, 3)))
    me = 'objects/weapons/k98/k98' if r.random() < 0.5 else 'vehicles/zero/zero'
    g.inline_left = 1
    insts = g.instances(len(own), own, types, schemas if r.random() < 0.7 else None)
    data = partition(g, insts, own[0] if r.random() < 0.8 else g.guid())
    names = list(g.parts)
    kw: dict[str, Any] = {}
    if r.random() < 0.6:
        kw['catalogue'] = cat
    if r.random() < 0.5:
        kw['ours'] = {p.lower(): set(gs) for p, gs in g.parts.items() if r.random() < 0.5}
        if r.random() < 0.5:
            kw['ours'][me] = set(own)
    if r.random() < 0.5:
        si: dict[Any, Any] = {}
        for _ in range(r.randint(1, 4)):
            target = r.choice(names + ['other/missing', '*']).lower()
            key: str | tuple[str, str] = target
            if r.random() < 0.4:
                key = (r.choice(FIELDS + ['Mesh', 'ProjectileData', 'Items']), target)
            si[key] = r.choice([None, dbxport.TEMPLATE, f'{r.choice(names)}/*', f'{r.choice(names)}/{g.guid()}',
                                r.choice(names)])
        kw['stand_ins'] = si
    if r.random() < 0.3:
        kw['donors'] = r.sample(names, 1)
    for k, p in (('strict', 0.15), ('prune', 0.3), ('drop_untemplated', 0.3)):
        if r.random() < p:
            kw[k] = True
    if r.random() < 0.5:
        kw['schema'] = 'report'
    if r.random() < 0.2:
        kw['conventions'] = False
    if r.random() < 0.3:
        kw['required'] = {'Entity.ReferenceObjectData': 'ReferencedObject'}
    if r.random() < 0.3:
        kw['drop_types'] = dbxport.NOT_IN_BFBC2 | {r.choice(types)}
    if r.random() < 0.4:
        kw['bones'] = dbxport.BoneTables(r.sample(BONES, 6), r.sample(BONES, 5))
    if r.random() < 0.7:
        kw['name'] = me
    resolve = None
    if r.random() < 0.6:
        known = {p.lower() for p in names if r.random() < 0.6}
        resolve = lambda p: p.lower() in known                  # noqa: E731
    if kw.get('drop_untemplated') and kw.get('catalogue') is None and T_.catalogue is None:
        kw['drop_untemplated'] = False
    before = dict(dbxport.STATS['why'])
    try:
        dbxport.port(data, T_, resolve, **kw)
        outcome = 'ok'
    except dbxport.DualMismatch:
        raise
    except Exception as e:                                      # noqa: BLE001 - the reference's own, compared
        outcome = type(e).__name__
    after = dbxport.STATS['why']
    left = next((k for k in after if after[k] != before.get(k, 0)), '')
    return outcome, left


def game() -> list[str]:
    """Every fair port and counterpart port (rust/bench_bf1943.py's `fair`) and every effect port (rust/bench_ports.py's
    `effects`) of the user's copies, both ways. Failures (a mismatch raises DualMismatch); [] when all agree."""
    sys.path.insert(0, os.path.join(_ROOT, 'rust'))
    suite_paths.use('mesh-viewer', 'shader-tools', 'console-convert', 'anim-tools', 'model-tools')
    import bench_bf1943
    import bench_ports
    import bf1943import as bi
    import meshview
    import shaderdb
    try:
        src = bi.Source(bi.default_source())
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                      # noqa: BLE001 - only missing game data skips
        print(f'SKIP game: no game data ({type(e).__name__}: {e})')
        return []
    fails = []
    dbxport.DUAL = True
    for what, run in (('fair and counterpart ports', lambda out: bench_bf1943.run_fair(src, ix, out, {})),
                      ('effect ports', lambda out: bench_ports.run_effects(src, ix, bench_ports.effect_names(src), out))):
        before = dict(dbxport.STATS, why=collections.Counter(dbxport.STATS['why']))
        out: dict[str, str] = {}
        try:
            run(out)
        except dbxport.DualMismatch as e:
            fails.append(f'{what}: {e}')
            print(f'FAIL {what}: {e}', flush=True)
            continue
        compared = dbxport.STATS['dual'] - before['dual']
        left = dbxport.STATS['fallback'] - before['fallback']
        ok = compared > 0 and left == 0
        print(f'{"ok  " if ok else "FAIL"} {what}: {compared} ports the same both ways (bytes, report lines, records, '
              f'exceptions), {left} left to Python ({len(out)} outputs)', flush=True)
        if not ok:
            fails.append(what)
    return fails


def guards() -> list[str]:
    """Instances held inline in each other (an array item naming a local instance where BFBC2 holds structs): the
    reference recurses until its RecursionError, or grows exponentially; the Rust port stops at its depth limit or
    node budget and leaves the partition to Python - quickly, in bounded memory. Run on the Rust side alone."""
    import time
    g = Gen(random.Random(1))
    a, b = g.guid(), g.guid()

    def holder(guid: str, refs: list[str]) -> dbx.Element:
        items = [E('item', T, [('ref', r)], '') for r in refs]
        return E('instance', C, [('guid', guid), ('type', 'X.Holder')], '', b'', [E('array', C, [('name', 'Items')], '',
                                                                                    b'', items)])
    level: list[dbx.Element] = []
    for _ in range(12):                                         # BFBC2's shape: Items[] of structs, 12 deep
        level = [E('array', C, [('name', 'Items')], '', b'', [E('complex', C, [], '', b'', level)])]
    tpl = E('instance', C, [('guid', g.guid()), ('type', 'X.Holder')], '', b'', level)
    T_ = dbxport.Templates([('tpl', partition(g, [tpl], tpl.attr('guid') or ''))])
    fails = []
    for what, insts in (('an instance held inline in itself four times over, 12 deep', [holder(a, [a, a, a, a])]),
                        ('two instances held inline in each other four times over, 12 deep',
                         [holder(a, [b, b, b, b]), holder(b, [a, a, a, a])])):
        t0 = time.perf_counter()
        got = dbxport._port_rust(partition(g, insts, a), T_, None, False, (), False, None, None, None, None, (), False,
                                 'report', True, None, None)
        dt = time.perf_counter() - t0
        ok = isinstance(got, str) and 'the Rust port' in got and dt < 10
        print(f'{"ok  " if ok else "FAIL"} guard: {what} left to Python in {dt:.2f} s ({got if isinstance(got, str) else "ported"})',
              flush=True)
        if not ok:
            fails.append(what)
    return fails


def main() -> int:
    args = sys.argv[1:]
    cases = int(args[args.index('--cases') + 1]) if '--cases' in args else 600
    seed = int(args[args.index('--seed') + 1]) if '--seed' in args else 4004
    if dbxport._native() is None:
        print('ok   no Rust core built: nothing to compare (python rust\\build_native.py)')
        print('all passed')
        return 0
    dbxport.DUAL = True
    outcomes: collections.Counter[str] = collections.Counter()
    left: collections.Counter[str] = collections.Counter()
    failed = []
    for i in range(cases):
        try:
            o, why = case(seed + i)
        except dbxport.DualMismatch as e:
            failed.append(f'case {seed + i}: {e}')
            continue
        outcomes[o] += 1
        if why:
            left[why] += 1
    compared = dbxport.STATS['dual']
    for f in failed[:10]:
        print(f'FAIL {f}')
    ok = not failed and compared >= cases * 0.9
    print(f'{"ok  " if ok else "FAIL"} Rust port == Python reference on {compared} of {cases} random ports (bytes, '
          f'report lines, records, exceptions; outcomes {dict(outcomes)}); left to Python: {sum(left.values())} '
          f'{dict(left)}', flush=True)
    if not ok:
        failed = failed or ['random ports']
    dbxport.DUAL = False
    failed += guards()
    if '--game' in args:
        failed += game()
    print('all passed' if not failed else f'{len(failed)} failed')
    return 0 if not failed else 1


if __name__ == '__main__':
    sys.exit(main())
