r"""granny.py on the game's files (game tier; skipped without them):

- the soldier skeleton (characters/skeletons/ske01): 147 bones; jntWpn_6 and jntWpn_7 are children of jntWpn_1; every
  weapon bone's bind pose is the weapon root;
- every animation of the WW2 rifles (M1 Garand, its rifle grenade, Type 5 and its rifle grenade): parses, has
  tracks, every curve format is read, every quantized orientation key is a unit quaternion (float-control
  curves hold spline control points, which need not be);
- shift: a constant track (1p_lower), a quantized one moved by its offsets (1p_fire) and a re-quantized ramp
  (Type 5 1p_reload) - the bone's keys land where asked, every other track of the file reads exactly as before,
  the file keeps its size and its Granny CRC is right (it is recomputed and checked against zlib).

  python test_granny.py
"""
import math
import os
import re
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import granny  # noqa: E402

ANIMS = re.compile(r'animations/weapons/handheld/(m1garand|m1garand_rgl|beach/jap_type5|beach/jap_type5_rgl)/.*\.res$')


def index():
    sys.path.insert(0, os.path.join(HERE, '..', '..', 'bfbc2-mesh-viewer'))
    import meshview
    import shaderdb
    try:
        return meshview.Index(shaderdb.find_opfor(None))
    except shaderdb.ShaderDbError:
        return None


def tracks(g):
    out = {}
    for an in granny.animations(g):
        for tg in an['TrackGroups']:
            for tr in tg['TransformTracks']:
                out[tr['Name']] = (granny.curve_keys(tr['OrientationCurve']['CurveData']),
                                   granny.curve_keys(tr['PositionCurve']['CurveData']))
    return out


def crc_ok(b, g):
    stored = struct.unpack_from('<I', b, g.gr2 + 40)[0]
    return stored == zlib.crc32(b[g.gr2 + g.crc_from:g.gr2 + g.total]) & 0xFFFFFFFF


def main():
    ix = index()
    if ix is None:
        print('SKIP: no Project OPFOR BFBC2 game copy')
        return 0
    fails = []
    sk = granny.Granny(ix.read('characters/skeletons/ske01.res'), 'ske01').root()['Skeletons'][0]['Bones']
    names = [b['Name'] for b in sk]
    if len(sk) != 147:
        fails.append(f'skeleton: {len(sk)} bones, expected 147')
    w1 = names.index('jntWpn_1')
    for b in ('jntWpn_6', 'jntWpn_7'):
        if sk[names.index(b)]['ParentIndex'] != w1:
            fails.append(f'{b}: parent {sk[names.index(b)]["ParentIndex"]}, expected jntWpn_1 ({w1})')
    roots = {tuple(round(v, 4) for v in sk[i]['InverseWorldTransform'][12:15]) for i, n in enumerate(names) if n.startswith('jntWpn_')}
    if len(roots) != 1:
        fails.append(f'weapon bones bind at {len(roots)} places, expected one')

    files = sorted(n for n in ix.where if ANIMS.search(n))
    parsed = formats = 0
    seen = set()
    for n in files:
        b = ix.read(n)
        try:
            g = granny.Granny(b, n)
        except granny.GrannyError as e:
            if 'generatedaimanimation' in n:                     # another format: generated aim poses
                continue
            fails.append(f'{n}: {e}')
            continue
        tr = tracks(g)
        if not tr:
            fails.append(f'{n}: no tracks')
        parsed += 1
        for an in granny.animations(g):
            for tg in an['TrackGroups']:
                for t in tg['TransformTracks']:
                    for c in ('OrientationCurve', 'PositionCurve', 'ScaleShearCurve'):
                        seen.add(t[c]['CurveData'].get('__type__'))
        for an in granny.animations(g):                     # quantized quaternions decode to unit length
            for tg in an['TrackGroups']:
                for t in tg['TransformTracks']:
                    cd = t['OrientationCurve']['CurveData']
                    if not cd.get('__type__', '').endswith(('D4nK16uC15u', 'D4nK8uC7u')):
                        continue
                    for _, q in granny.curve_keys(cd):
                        if abs(math.sqrt(sum(x * x for x in q)) - 1) > 1e-4:
                            fails.append(f'{n} {t["Name"]}: orientation {q} is not a unit quaternion')
                            break
        if not crc_ok(b, g):
            fails.append(f'{n}: the stored Granny CRC does not match (reader assumption)')
    formats = len(seen)
    print(f'{parsed} of {len(files)} animations parsed; curve formats seen: {sorted(s.replace("CurveDataHeader_", "") for s in seen if s)}')
    aim = sum('generatedaimanimation' in n for n in files)
    if parsed != len(files) - aim or parsed < 50:
        fails.append(f'{parsed} animations parsed of {len(files) - aim} (and at least 50 expected)')

    for name, bone, kw, check in (
            ('animations/weapons/handheld/m1garand_rgl/1p/1p_lower.res', 'jntWpn_7', {'by': '0,0,0.1336'}, 'all'),
            ('animations/weapons/handheld/m1garand_rgl/1p/1p_fire.res', 'jntWpn_7', {'by': '0,0,0.1336'}, 'all'),
            ('animations/weapons/handheld/beach/jap_type5_rgl/1p/1p_reload.res', 'jntWpn_7',
             {'rest': '0,0.0973,1.2826', 'ramp': True}, 'ends')):
        b = ix.read(name)
        g0 = granny.Granny(b, name)
        before = tracks(g0)
        g = granny.Granny(b, name)
        granny.shift(g, bone, kw.get('rest'), kw.get('by'), kw.get('ramp', False))
        out = g.to_bytes()
        g2 = granny.Granny(out, name)
        after = tracks(g2)
        if len(out) != len(b):
            fails.append(f'{name}: size changed')
        if not crc_ok(out, g2):
            fails.append(f'{name}: CRC not recomputed right')
        for t in before:
            if t != bone and before[t] != after[t]:
                fails.append(f'{name}: track {t} changed')
        old, new = before[bone][1], after[bone][1]
        if check == 'all':
            d = [float(x) for x in kw['by'].split(',')]
            worst = max(abs(nv[i] - ov[i] - d[i]) for (_, ov), (_, nv) in zip(old, new) for i in range(3))
            if worst > 2e-3:
                fails.append(f'{name}: keys moved wrong by up to {worst:.4f}')
        else:
            target = [float(x) for x in kw['rest'].split(',')]
            if max(abs(new[-1][1][i] - target[i]) for i in range(3)) > 3e-3:
                fails.append(f'{name}: last key {new[-1][1]}, expected {target}')
            if max(abs(new[0][1][i] - old[0][1][i]) for i in range(3)) > 3e-3:
                fails.append(f'{name}: first key moved ({old[0][1]} -> {new[0][1]})')
        print(f'shift {name.split("/")[-1]} ({check}): ok' if not any(name in f for f in fails) else f'shift {name}: FAILED')
    for f in fails[:20]:
        print('FAIL', f)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
