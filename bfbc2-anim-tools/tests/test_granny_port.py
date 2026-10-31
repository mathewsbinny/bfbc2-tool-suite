r"""granny.py's Xbox 360 -> PC conversion (to_pc) and DICE's bone table.

Without game data (quick tier):
- bone_table reads the three layouts (skeleton; animation records {bone, k, k time pairs}; delta bones + records)
  and refuses one that does not add up;
- renumber maps the bones and sorts a delta's bone list and the records by the new index, values travelling
  with their bone; a bone missing from the map is an error.

With --game (game tier; each part skipped when its files are not on the PC):
- every Granny resource (animations, delta animations, skeletons) of BFBC2's Xbox mp_common and sp_common, as the
  console readers unpack them into OPFOR's work\xbox, that has a PC twin of the same size in the game copy: the
  converted file has the PC file's DICE table and string table byte for byte, a correct Granny CRC, and every value
  of the PC file's object tree (Granny's unused Padding members aside);
- Battlefield 1943's (the Rising Sun workspace next to the suite): its soldier animations converted with the bone
  map onto BFBC2's soldier equal BFBC2's own copy of the same animation for at least 31 of them (DICE edited or
  re-compressed the rest for BFBC2).

  python test_granny_port.py [--game]
"""
import os
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path.insert(0, os.path.join(HERE, '..'))
sys.path.insert(0, os.path.join(SUITE, 'core'))
import granny  # noqa: E402

KINDS = ('GrannyAnimation', 'DeltaAnimation', 'GrannyModel')
SAME_AS_BFBC2_AT_LEAST = 31


def clean(v):
    if isinstance(v, dict):
        return {k: clean(x) for k, x in v.items() if not k.startswith('__') and k != 'Padding'}
    if isinstance(v, list):
        return [clean(x) for x in v]
    return v


def f32(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def units(check):
    check(granny.bone_table([10]) == ('none', []), 'bone_table: a skeleton has no table')
    anim = [10, 2, 40, 1, f32(0.1), f32(0.9), 7, 2, f32(0.0), f32(0.2), f32(0.5), f32(0.7)]
    check(granny.bone_table(anim) == ('animation', [2, 6]), 'bone_table: animation records (k = 1 and k = 2)')
    delta = [10, 3, 4, 2, 9, 1, 40, 1, f32(0.0), f32(1.0)]
    check(granny.bone_table(delta) == ('delta', [2, 3, 4, 6]), 'bone_table: delta bones, then a record')
    check(granny.bone_table([10, 0]) == ('animation', []), 'bone_table: an empty animation table')
    check(granny.bone_table([10, 2, 5, 6, 0]) == ('delta', [2, 3]), 'bone_table: delta bones, no records')
    try:
        granny.bone_table([10, 3, 1, 2])
        check(False, 'bone_table: a table that does not add up is refused')
    except granny.GrannyError:
        check(True, 'bone_table: a table that does not add up is refused')
    m = {40: 3, 7: 50, 4: 9, 2: 1, 9: 2}
    check(granny.renumber(anim, m) == [10, 2, 3, 1, f32(0.1), f32(0.9), 50, 2, f32(0.0), f32(0.2), f32(0.5), f32(0.7)],
          'renumber: animation records renumbered, sorted, values kept with their bone')
    check(granny.renumber(delta, m) == [10, 3, 1, 2, 9, 1, 3, 1, f32(0.0), f32(1.0)],
          'renumber: a delta\'s bones renumbered and sorted, its record renumbered')
    try:
        granny.renumber(delta, {4: 1})
        check(False, 'renumber: a bone missing from the map is an error')
    except granny.GrannyError:
        check(True, 'renumber: a bone missing from the map is an error')


def opfor():
    for root in (os.environ.get('OPFOR_ROOT'), os.path.join(SUITE, '..', 'project-opfor-bfbc2')):
        if root and os.path.isdir(os.path.join(root, 'game')):
            return os.path.abspath(root)
    return None


def xbox_corpus(check, root):
    from bfbc2core import fbrb
    done = 0
    for lv in ('mp_common', 'sp_common'):
        xdir = os.path.join(root, 'work', 'xbox', lv)
        bundles = [os.path.join(root, 'game', 'Dist', 'win32', 'levels', lv, f)
                   for f in ('level-00.fbrb',)]
        man = os.path.join(xdir, 'fbrb.manifest')
        if not os.path.isfile(man) or not all(os.path.isfile(b) for b in bundles):
            print(f'SKIP {lv}: no unpacked Xbox copy (work\\xbox\\{lv}) or no PC bundle')
            continue
        xb = {}
        for line in open(man, encoding='utf-8', errors='replace'):
            f = line.rstrip('\n').split('|')
            if len(f) > 1 and f[1] in KINDS:
                xb[f[0].lower()] = (f[0], f[1])
        pc_names = fbrb.names(bundles[0])
        common = sorted(k for k in xb if k in pc_names)
        pcs = fbrb.read(bundles[0], common)
        counts, fails, skipped = {}, [], 0
        for k in common:
            x = open(os.path.join(xdir, *xb[k][0].split('/')), 'rb').read()
            p = pcs[k]
            if len(x) != len(p):
                skipped += 1
                continue
            try:
                cv = granny.to_pc(x, where=xb[k][0])
                gc, gp = granny.Granny(cv, k), granny.Granny(p, k)
                crc = zlib.crc32(cv[gc.gr2 + gc.crc_from:gc.gr2 + gc.total]) & 0xFFFFFFFF
                if cv[:gc.gr2] != p[:gp.gr2]:
                    raise granny.GrannyError('DICE table or strings differ')
                if crc != struct.unpack_from('<I', cv, gc.gr2 + 40)[0]:
                    raise granny.GrannyError('Granny CRC wrong')
                if clean(gc.root()) != clean(gp.root()):
                    raise granny.GrannyError('object tree differs')
                counts[xb[k][1]] = counts.get(xb[k][1], 0) + 1
            except Exception as e:                       # noqa: BLE001
                fails.append(f'{k}: {e}')
        done += sum(counts.values())
        check(not fails and counts, f'{lv}: Xbox -> PC equals the PC file: '
              + ', '.join(f'{v} {t}' for t, v in sorted(counts.items()))
              + f' ({skipped} of another size skipped)', '; '.join(fails[:5]))
    return done


def bf1943(check, root):
    xenon = os.environ.get('BF1943_XENON') or os.path.join(SUITE, '..', 'operation-rising-sun', 'work', 'extracted',
                                                           'dist', 'xenon')
    common = os.path.join(xenon, 'levels', 'levels-common-00.fbrb')
    fb = os.path.join(SUITE, '..', 'bf1943-tool-suite', 'bf1943-fbrb-tool')
    if not os.path.isfile(common) or not os.path.isfile(os.path.join(fb, 'fbrb1943.py')):
        print('SKIP Battlefield 1943: no extracted copy (Rising Sun workspace) or no BF1943 bundle reader')
        return
    sys.path.insert(0, fb)
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-mesh-viewer'))
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools'))
    import fbrb1943
    import meshview
    import shaderdb
    bd = fbrb1943.Bundle(open(common, 'rb').read())
    ix = meshview.Index(shaderdb.find_opfor(root))
    bones = granny.bone_map(bd.data('Characters/Skeletons/ske01.res'), ix.read('characters/skeletons/ske01.res'))
    same = tried = 0
    for e in bd.entries:
        if e['type'] not in ('GrannyAnimation', 'DeltaAnimation'):
            continue
        pc = ix.read(e['name'].lower())
        if pc is None:
            continue
        tried += 1
        cv = granny.to_pc(bd.data(e['name']), bones, e['name'])
        gc, gp = granny.Granny(cv), granny.Granny(pc)
        same += cv[:gc.gr2] == pc[:gp.gr2] and clean(gc.root()) == clean(gp.root())
    check(same >= SAME_AS_BFBC2_AT_LEAST, f'Battlefield 1943: {same} of {tried} animations that BFBC2 also has, '
          f'converted with the bone map, equal BFBC2\'s own copy (at least {SAME_AS_BFBC2_AT_LEAST} expected)')


def main(argv):
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    units(check)
    if '--game' in argv:
        root = opfor()
        if root is None:
            print('SKIP game part: no Project OPFOR BFBC2 game copy')
        else:
            xbox_corpus(check, root)
            bf1943(check, root)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
