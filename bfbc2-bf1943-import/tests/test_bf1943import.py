r"""bf1943import.py, the Battlefield 1943 import pipeline.

Without game data (quick tier):
- an output name that would leave the output folder is refused; the containment test is case- and
  separator-blind and does not take a sibling folder with a common prefix for a subfolder;
- each resource type is routed to its converter, or listed with the reason it is not converted;
- an import into a folder inside the source, or with no filter, or with a filter that matches nothing, is refused
  before anything is written.

With --game (game tier; skipped without the Rising Sun workspace and the BF1943 suite next to this one):
- check: every BF1943 animation, delta animation and skeleton converts, reads as a PC file with the source's object
  tree, and every renumbered bone table names the same bones in BFBC2's soldier;
- import of the Garand bayonet's resources into a temporary folder: the animations land under their names and read
  as little-endian with the bayonet bone (jntWpn_6) at the muzzle in the 1P thrust; import.json records each
  resource (converted or why not); a second import with another filter adds to the records instead of replacing
  them; the source bundles are unchanged.

  python test_bf1943import.py [--game]
"""
import hashlib
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))
import bf1943import as bi  # noqa: E402


class FakeSource:
    def __init__(self, folder):
        self.folder = folder
        self.entries = {'a/b.res': ('x.fbrb', {'name': 'A/B.res', 'type': 'GrannyAnimation', 'size': 1})}

    def find(self, filters):
        return [k for k in self.entries if all(f.lower() in k for f in filters)]


def units(check):
    out = tempfile.mkdtemp(prefix='bf1943import_')
    try:
        check(bi.target(out, 'Animations/X/1P_Melee1.res').startswith(os.path.abspath(out)),
              'target: a resource name lands inside the output folder')
        for bad in ('../escape.res', 'a/../../escape.res', '..\\escape.res'):
            try:
                bi.target(out, bad)
                check(False, f'target: {bad} is refused')
            except bi.Bf1943ImportError:
                check(True, f'target: {bad} is refused')
        check(bi._inside(os.path.join(out, 'Sub', 'f'), out.upper()), '_inside: case-blind')
        check(not bi._inside(out + '_sibling', out), '_inside: a sibling with a common prefix is not inside')
        routes = {'GrannyAnimation': 'animation to PC', 'DeltaAnimation': 'animation to PC',
                  'GrannyModel': 'skeleton to PC', 'XenonTexture': 'texture to PC',
                  'MeshData': 'not converted', 'HavokPhysicsData': 'not converted', '<non-resource>': 'not converted',
                  'AimAnimation': 'not converted', 'Wave': 'not converted'}
        check(all(bi.plan(k).startswith(v) for k, v in routes.items()), 'plan: every type routed or explained')
        src = FakeSource(os.path.join(out, 'source'))
        for args, what in ((([], os.path.join(out, 'o')), 'no filter'),
                           ((['nothing-matches'], os.path.join(out, 'o')), 'a filter that matches nothing'),
                           ((['a/b'], os.path.join(src.folder, 'inside')), 'an output folder inside the source')):
            try:
                bi.run_import(src, args[0], args[1], log=lambda s: None)
                check(False, f'run_import: {what} is refused')
            except bi.Bf1943ImportError:
                check(True, f'run_import: {what} is refused')
        check(not os.path.exists(os.path.join(out, 'o')) and not os.path.exists(src.folder),
              'run_import: nothing written by a refused import')
    finally:
        shutil.rmtree(out, ignore_errors=True)


def game(check):
    folder = bi.default_source()
    try:
        source = bi.Source(folder)
    except bi.Bf1943ImportError as e:
        print(f'SKIP game part: {e}')
        return
    before = {r: hashlib.sha256(open(os.path.join(folder, *r.split('/')), 'rb').read()).hexdigest()
              for r in source.bundles}
    fails = bi.check(source, log=lambda s: None)
    check(not fails, 'check: every BF1943 animation and skeleton converts and reads back', '; '.join(fails[:3]))
    out = tempfile.mkdtemp(prefix='bf1943import_')
    try:
        recs = bi.run_import(source, ['M1Garand_Bayonet/'], out, log=lambda s: None)
        conv = [r for r in recs if 'out' in r]
        check(len(conv) >= 15 and all(os.path.isfile(os.path.join(out, *r['out'].split('/'))) for r in conv),
              f'import: {len(conv)} of {len(recs)} bayonet resources written under their names')
        thrust = os.path.join(out, 'Animations', 'Weapons', 'Handheld', 'Beach', 'M1Garand_Bayonet', '1P', '1P_Melee1.res')
        g = bi.granny.Granny(open(thrust, 'rb').read(), thrust)
        pos = None
        for an in bi.granny.animations(g):
            for tg in an['TrackGroups']:
                for tr in tg['TransformTracks']:
                    if tr['Name'] == 'jntWpn_6':
                        pos = bi.granny.curve_keys(tr['PositionCurve']['CurveData'])[0][1]
        check(g.e == '<' and pos is not None and 1.1 < pos[2] < 1.35,
              f'import: the 1P thrust reads as a PC file, bayonet bone at z {pos[2] if pos else None}')
        man = json.load(open(os.path.join(out, 'import.json'), encoding='utf-8'))
        check(len(man['resources']) == len(recs) and all('done' in r and 'sha256' in r for r in man['resources']),
              'import.json: one record per resource, each saying what was done')
        bi.run_import(source, ['US_LongKnife_Knife/1P/'], out, log=lambda s: None)
        man2 = json.load(open(os.path.join(out, 'import.json'), encoding='utf-8'))
        check(len(man2['resources']) > len(man['resources']) and
              any('longknife' in r['name'].lower() for r in man2['resources']) and
              any('bayonet' in r['name'].lower() for r in man2['resources']),
              'import.json: a second import adds its records to the first one\'s')
    finally:
        shutil.rmtree(out, ignore_errors=True)
    after = {r: hashlib.sha256(open(os.path.join(folder, *r.split('/')), 'rb').read()).hexdigest()
             for r in source.bundles}
    check(before == after, 'the source bundles are unchanged')


def main(argv):
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    units(check)
    if '--game' in argv:
        game(check)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
