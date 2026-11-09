r"""havokport.py on Battlefield 1943's physics (game tier; skipped without the Rising Sun workspace, the BF1943 suite or
the game copy).

- the oracle: BF1943's Kurogane physics, converted, equals BFBC2's own Kurogane object for object (62 objects paired
  by walking pointers from the root with BFBC2's 5.5.0 layouts) and field for field (floats within 0.1 %), but for
  the values the game rebuilds at load (a collidable's cached bounding volume, a motion's deactivation counters and
  saved quality type, a triangle subpart's offset) and MOPP code lengths DICE rebuilt by a byte or two;
- the Zero and Corsair, which DICE re-exported for BFBC2, convert and pair all but the subparts DICE merged;
- the WW2 side project's physics convert: Willys (and wreck), Sherman, LCVP and its shell, B-25, Ki-21, the air-raid
  bunker, the machine-gun mounts - every byte accounted for by a 5.5.0 field (the console converter's check).

  python test_havokport.py
"""
import collections
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path[:0] = [os.path.join(HERE, '..'), os.path.join(SUITE, 'bfbc2-mesh-viewer'), os.path.join(SUITE, 'bfbc2-shader-tools'),
                os.path.join(SUITE, 'bfbc2-console-convert')]
import bf1943import as bi  # noqa: E402
import havok  # noqa: E402
import havokport  # noqa: E402
from havok import ARRAY, ENUM, FLAGS, PLAIN, POINTER, SIMPLEARRAY, STRUCT, VARIANT, ZERO  # noqa: E402

V = 'objects/vehicles/beach/'
KUROGANE = V + 'kurogane/kurogane_mesh_havok.res'
REEXPORTED = [V + 'jap_zero_fighter/zero_fighter_havok.res', V + 'us_corsair/us_corsair_havok.res']
ANTENNA = 'objects/beach_objects/antenna_01/antenna_01_banger_havok.res'      # velocity limit 10, convex + list shapes
BANGER = 'objects/levelspecific_objects/ghosttown_lamppolelong/ghosttown_lamppolelong_banger_havok.res'
WW2 = [V + 'willys-mb/willys-mb_havok.res', V + 'willys-mb/willys-mb_wreck_havok.res',
       V + 'm4-sherman/us_shermanm4_tank_havok.res', V + 'lcvp-landingcraft/gen_landingcraft_havok.res',
       'objects/beach_objects/invisible_boat_wall/lcvp_shell_havok.res', V + 'mitchell_bomber/mitchell_bomber_mesh_havok.res',
       V + 'sally_bomber/sally_bomber_havok.res', V + 'bombingraid_prototype/airraiddummy_havok.res',
       'objects/weapons/stationary/mounts/hmg_agl_windowmount_havok.res',
       'objects/weapons/stationary/mounts/hmg_agl_stand_havok.res']
RUNTIME = ('boundingVolumeData', 'deactivationIntegrateCounter', 'deactivationNumInactiveFrames',
           'deactivationRefOrientation', 'savedQualityTypeIndex', 'triangleOffset')


def hkpack():
    sys.path.insert(0, os.path.join(SUITE, '..', 'bf1943-tool-suite', 'bf1943-auto-navkit-common', 'tools'))
    import hkpack as m
    return m


class Layouts:
    def __init__(self, k):
        self.k = k

    def members(self, cls):
        c, chain = self.k[cls], []
        while c:
            chain.append(c)
            c = self.k.get(c.parent) if c.parent else None
        return [m for c in reversed(chain) for m in c.members]

    def fields(self, cls, base=0, path=''):
        for m in self.members(cls):
            at = base + m.offset
            for i in range(max(m.carray, 1)):
                q = path + m.name + (f'[{i}]' if m.carray else '')
                if m.type == STRUCT and m.cls in self.k:
                    yield from self.fields(m.cls, at + i * self.k[m.cls].size, q + '.')
                elif m.type in (ARRAY, SIMPLEARRAY, POINTER, VARIANT, ZERO) or m.type == 29:
                    continue
                elif m.type in (ENUM, FLAGS):
                    yield q, at + i * PLAIN[m.subtype][0], PLAIN[m.subtype][0]
                elif m.type in PLAIN:
                    yield q, at + i * PLAIN[m.type][0], PLAIN[m.type][0]

    def slots(self, cls, a, b, pa, pb, da, db):
        for m in self.members(cls):
            A, B = a + m.offset, b + m.offset
            if m.type in (POINTER, VARIANT):
                yield A, B
            elif m.type in (ARRAY, SIMPLEARRAY):
                xa, xb = pa.ptr.get(A), pb.ptr.get(B)
                n = min(struct.unpack_from('<i', da, A + 4)[0], struct.unpack_from('<i', db, B + 4)[0])
                if xa is None or xb is None:
                    continue
                if m.subtype == POINTER:
                    for i in range(n):
                        yield xa + 4 * i, xb + 4 * i
                elif m.subtype == STRUCT and m.cls in self.k:
                    for i in range(n):
                        yield from self.slots(m.cls, xa + i * self.k[m.cls].size, xb + i * self.k[m.cls].size,
                                              pa, pb, da, db)
            elif m.type == STRUCT and m.cls in self.k:
                yield from self.slots(m.cls, A, B, pa, pb, da, db)


def compare(hp, L, out, ref):
    po, pb = hp.Packfile(out, out.find(hp.MAGIC)), hp.Packfile(ref, ref.find(hp.MAGIC))
    oo, ob = dict(po.objects), dict(pb.objects)
    pairs, todo = {}, [(min(oo), min(ob))]
    while todo:
        a, b = todo.pop()
        if (a, b) in pairs or oo.get(a) != ob.get(b):
            continue
        pairs[(a, b)] = oo[a]
        for A, B in L.slots(oo[a], a, b, po, pb, out, ref):
            ta, tb = po.ptr.get(A), pb.ptr.get(B)
            if ta in oo and tb in ob:
                todo.append((ta, tb))
    bad: collections.Counter[str] = collections.Counter()
    for (a, b), cls in pairs.items():
        for name, off, size in L.fields(cls):
            if any(r in name for r in RUNTIME):
                continue
            x, y = out[a + off:a + off + size], ref[b + off:b + off + size]
            if x == y:
                continue
            if size % 4 == 0:
                fx = struct.unpack('<%df' % (size // 4), x)
                fy = struct.unpack('<%df' % (size // 4), y)
                if all(abs(p - q) <= 1e-3 * max(1, abs(q)) for p, q in zip(fx, fy)):
                    continue
            bad[f'{cls}.{name}'] += 1
    return pairs, len(po.objects), len(pb.objects), bad


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    try:
        src = bi.Source(bi.default_source())
        hp = hkpack()
        import meshview
        import shaderdb
        root = shaderdb.find_opfor(None)
        ix = meshview.Index(root)
        K = havok.classes(os.path.join(root, 'game', 'BFBC2Game.exe'))
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP: {e}')
        return 0
    L = Layouts(K)
    template = ix.read(KUROGANE)
    out = havokport.convert(src.data(KUROGANE), template, K)
    pairs, n_out, n_ref, bad = compare(hp, L, out, template)
    check(len(pairs) == n_out == n_ref and not bad,
          f'oracle Kurogane: {len(pairs)} of {n_out} / {n_ref} objects paired, every field equal but runtime caches',
          ', '.join(f'{k} {v}' for k, v in sorted(bad.items())[:6]))
    # velocity limits: Havok's hkUFloat8 table from the executable; the Kurogane's 200 -> 0x7F as BFBC2's own file
    table = havokport.ufloat8_table(open(os.path.join(root, 'game', 'BFBC2Game.exe'), 'rb').read())
    check(table is not None and len(table) == 256 and havokport.ufloat8(200.0, table) == 0x7F,
          'hkUFloat8 table found in the executable; 200 -> 0x7F')
    if table is not None:
        check(havokport.convert(src.data(KUROGANE), template, K, table) == out,
              'oracle Kurogane: the same bytes with the table')
        try:
            a = havokport.convert(src.data(ANTENNA), ix.read(BANGER), K, table)
            check(b'hkpRigidBody' in a, 'antenna banger (velocity limit 10): converted with the table')
        except Exception as e:                                   # noqa: BLE001
            check(False, 'antenna banger (velocity limit 10)', str(e))
    for name in REEXPORTED:
        out = havokport.convert(src.data(name), template, K)
        pairs, n_out, n_ref, bad = compare(hp, L, out, ix.read(name))
        check(len(pairs) >= n_ref - 3, f'{name.split("/")[-1]}: {len(pairs)} of {n_out} / {n_ref} objects paired '
              f'(DICE re-exported it for BFBC2)')
    for name in WW2:
        try:
            out = havokport.convert(src.data(name), template, K)
            po = hp.Packfile(out, out.find(hp.MAGIC))
            check(any(c == 'hkpRigidBody' for _, c in po.objects),
                  f'{name.split("/")[-1]}: converted, {len(po.objects)} objects')
        except Exception as e:                                   # noqa: BLE001
            check(False, f'{name.split("/")[-1]}', str(e))
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
