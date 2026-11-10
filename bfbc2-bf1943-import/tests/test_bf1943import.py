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
  them; the source bundles are unchanged;
- 3p undeploys: the 22 BFBC2 shipped get DICE's 25 added tracks (88 in all, as DICE's) without a track of theirs
  changing; the 19 weapon parts reproduce DICE's exactly in at least 12 of the 22 and 380 of the 418 tracks (the
  rest DICE re-animated: M18, expack, detonator, the Shin Gunto standing, the crouching Garand's clip), the thumbs
  within 15 degrees on average (DICE's are animated); the 14 BF1943-only undeploys get their 25; thumb_donors: the
  Garand's thumbs from Vietnam's Garand undeploys are DICE's, the K98's from the M24's still give 88 tracks.

  python test_bf1943import.py [--game]
"""
import hashlib
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import')
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
                  'AimAnimation': 'aim animation to PC', 'Wave': 'not converted'}
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


def undeploys(check):
    """The added undeploy tracks against DICE's (bf1943import.Importer.undeploy_tracks)."""
    import math
    granny = bi.granny
    try:
        source = bi.Source(bi.default_source())
        imp = bi.Importer(source)
        ix = imp.index()
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP undeploys: {e}')
        return

    def tracks(b):
        a = granny.animations(granny.Granny(b))[0]
        return a['Duration'], {t['Name']: t for t in a['TrackGroups'][0]['TransformTracks']}

    def at(t, curve, time, default):
        return granny.sample(granny.curve_keys(t[curve]['CurveData']), time, default)

    def angle(a, b):
        return math.degrees(2 * math.acos(min(1.0, abs(sum(x * y for x, y in zip(a, b))))))
    keys = sorted(k for k, (_, e) in source.entries.items() if bi.UNDEPLOY.search(k) and e['type'] == 'GrannyAnimation')
    shared = [k for k in keys if k in ix.where]
    exact = tracks_ok = total = 0
    thumbs, bad = [], []
    for k in shared:
        out, _ = imp.convert(k)
        dur, d = tracks(ix.read(k))
        _, o = tracks(out)
        _, p = tracks(imp.convert(k, extra_tracks=False)[0])
        if len(o) != len(d) or any(granny.curve_keys(o[n][c]['CurveData']) != granny.curve_keys(p[n][c]['CurveData'])
                                   for n in p for c in ('PositionCurve', 'OrientationCurve')):
            bad.append(k)
        ok = n_w = 0
        for n in set(d) - set(p):
            ep = er = 0.0
            for f in (0, 0.25, 0.5, 0.75, 1):
                ep = max(ep, math.dist(at(d[n], 'PositionCurve', dur * f, (0.0, 0.0, 0.0)),
                                       at(o[n], 'PositionCurve', dur * f, (0.0, 0.0, 0.0))))
                q = angle(at(d[n], 'OrientationCurve', dur * f, (0, 0, 0, 1)), at(o[n], 'OrientationCurve', dur * f, (0, 0, 0, 1)))
                er = max(er, q)
                if 'Thumb' in n:
                    thumbs.append(q)
            if 'Thumb' not in n:
                n_w += 1
                ok += ep < 2e-3 and er < 0.2
        exact += ok == n_w
        tracks_ok += ok
        total += n_w
    mean = sum(thumbs) / max(len(thumbs), 1)
    check(len(shared) == 22 and not bad, f'undeploys: the {len(shared)} BFBC2 shipped get DICE\'s tracks, their own kept',
          str(bad[:3]))
    check(exact >= 12 and tracks_ok >= 380 and mean <= 15, f'undeploys: weapon parts as DICE\'s in {exact} of 22 '
          f'({tracks_ok} of {total} tracks), thumbs {mean:.1f} degrees from DICE\'s on average')
    own = [k for k in keys if k not in ix.where]
    counts = [len(tracks(imp.convert(k)[0])[1]) for k in own]
    check(len(own) >= 10 and set(counts) == {88}, f'undeploys: the {len(own)} BF1943-only ones get 88 tracks',
          str(counts))
    # thumb_donors: the Garand's thumbs copied from Vietnam's Garand undeploys (the same animation DICE shipped twice)
    # come out as DICE's (0 degrees), its other tracks as without a donor; the K98's from the M24's: still 88 tracks
    H = 'animations/weapons/handheld/'
    don = bi.Importer(source, thumb_donors={H + 'm1garand': H + 'vietnam/nam_rif_m1garand',
                                            H + 'beach/k98': H + 'us_sni_m24'})
    don._ix, don._bones = ix, imp._bones
    worst, same = 0.0, True
    for st in ('stand', 'crouch'):
        k = f'{H}m1garand/3p/{st}undeploy.res'
        dur, d = tracks(ix.read(k))
        out, rec = don.convert(k)
        _, o = tracks(out)
        _, plain = tracks(imp.convert(k)[0])
        same &= 'thumb tracks copied' in rec['done'] and set(o) == set(d)
        for n in d:
            if 'Thumb' in n:
                for f in (0, 0.25, 0.5, 0.75, 1):
                    worst = max(worst, angle(at(d[n], 'OrientationCurve', dur * f, (0, 0, 0, 1)),
                                             at(o[n], 'OrientationCurve', dur * f, (0, 0, 0, 1))))
            elif n in plain:
                same &= all(granny.curve_keys(o[n][c]['CurveData']) == granny.curve_keys(plain[n][c]['CurveData'])
                            for c in ('PositionCurve', 'OrientationCurve'))
    k98 = len(tracks(don.convert(H + 'beach/k98/3p/standundeploy.res')[0])[1])
    check(same and worst < 0.1 and k98 == 88, f'undeploys: thumbs from a donor undeploy - the Garand\'s from Vietnam\'s '
          f'within {worst:.2f} degrees of DICE\'s, its other tracks unchanged; the K98\'s from the M24\'s: {k98} tracks')


def main(argv):
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    units(check)
    if '--game' in argv:
        game(check)
        undeploys(check)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
