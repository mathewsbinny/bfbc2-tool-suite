r"""bf1943import.py - bring Battlefield 1943 (Xbox 360) resources into Bad Company 2's PC formats, from the user's own
copy (nothing is shipped: the outputs stay in Project OPFOR BFBC2's work folder).

  python bf1943import.py list [<filter>...] [--source <folder>]
                                        the BF1943 resources whose names contain every filter (any case): type,
                                        size, bundle, and what import would do with it
  python bf1943import.py import <filter>... [--source <folder>] [--opfor <folder>] [--out <folder>]
                                        converts every matching resource into <out>\<resource name> and writes
                                        <out>\import.json (one record per resource: source bundle and SHA-256,
                                        type, what was done, output SHA-256, notes); default <out>:
                                        <OPFOR>\work\bf1943\import
  python bf1943import.py check [--source <folder>] [--opfor <folder>]
                                        converts every animation and skeleton of the source in memory and checks
                                        each output (reads as a PC file; the same object tree as the source; the
                                        bone table renumbered onto BFBC2's soldier); exit code 1 on a failure

<source>: the folder of BF1943's extracted Xbox 360 bundles (dist\xenon), default the Project Rising Sun workspace
next to the suite (operation-rising-sun\work\extracted\dist\xenon) or BF1943_XENON. Read only.

What is converted (PLAN):
- GrannyAnimation, DeltaAnimation: to little-endian (granny.to_pc). When every track is a soldier bone, DICE's
  per-bone table is renumbered from BF1943's soldier skeleton (ske01) to BFBC2's (same 147 bones and parents,
  another order); otherwise (vehicles, props) the indices are kept and the record says so.
- GrannyModel (skeletons): to little-endian.
- XenonTexture: to the PC's ITexture (bfbc2-console-convert texture.convert; DXT1/3/5, BC5, DXT5A, RGB565, L8).
  A third-person undeploy (3p/StandUnDeploy, 3p/CrouchUnDeploy) gets the tracks DICE adds to every one it ships:
  the same weapon's same-stance deploy animates 25 more bones than the undeploy (19 weapon parts, 6 thumbs) - on
  all 22 BF1943 undeploys BFBC2 has, exactly the 25 DICE added (undeploy_tracks).
- AimAnimation (third-person aim poses): to little-endian with the bone table renumbered onto BFBC2's soldier
  (bfbc2-anim-tools aimanim.to_pc).
Everything else is listed with the reason it is not converted yet: MeshData and mesh sets (need PC vertex layouts
from a shader database), Havok physics (BF1943's Havok differs from BFBC2's), DBX partitions (another game's
data), sounds.

The game copy and the source are never written; <out> may not lie inside either.
"""
import contextlib
import hashlib
import json
import os
import re
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'mesh-viewer', 'console-convert', 'anim-tools', 'core')
import aimanim  # noqa: E402
import granny  # noqa: E402
from bfbc2core import errors, paths  # noqa: E402

SOLDIER = 'characters/skeletons/ske01.res'
UNDEPLOY = re.compile(r'/3p/(stand|crouch)undeploy\.res$')
PARKED = 0.5                        # m: a deploy's weapon part this far from the HandsIkPose is parked (undeploy_tracks)
TRACK_FORMATS = 'animations/weapons/handheld/m1garand/3p/standundeploy.res'  # DICE's: the curve formats added
ANIMATIONS = ('GrannyAnimation', 'DeltaAnimation')
NOT_YET = {
    'MeshData': 'mesh data: needs PC vertex layouts from a shader database',
    'RigidMeshSet': 'mesh set: needs its meshes converted first',
    'SkinnedMeshSet': 'mesh set: needs its meshes converted first',
    'CompositeMeshSet': 'mesh set: needs its meshes converted first',
    'HavokPhysicsData': "physics: BF1943's Havok differs from BFBC2's",
    'AnimTreeInfo': 'animation tree: format not decoded',
    '<non-resource>': "DBX partition: another game's data",
}


class Bf1943ImportError(Exception):
    what_to_do = ('Check the names with "list", the --source folder (BF1943\'s extracted dist\\xenon) and --out; '
                  'python bf1943import.py --help shows the commands.')


def default_source():
    """BF1943's extracted Xbox 360 bundles: BF1943_XENON when set (unchecked), else Project Rising Sun's workspace
    beside the suite (operation-rising-sun\\work\\extracted\\dist\\xenon)."""
    return paths.sibling(paths.BF1943_WORKSPACE, 'work', 'extracted', 'dist', 'xenon', env='BF1943_XENON', suite=SUITE)


def fbrb1943():
    """The BF1943 tool suite's bundle reader: bf1943-tool-suite\\bf1943-fbrb-tool beside the suite FIRST, then
    BF1943_SUITE_FBRB (the opposite order to default_source's). Put on sys.path once."""
    p = paths.find_sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', marker='fbrb1943.py', env='BF1943_SUITE_FBRB',
                           suite=SUITE)
    if p:
        paths.use_folder(p)
        import fbrb1943 as m
        return m
    raise Bf1943ImportError('the BF1943 tool suite (bf1943-tool-suite\\bf1943-fbrb-tool\\fbrb1943.py) was not found next to '
                       'this suite: its bundle reader reads the Xbox 360 bundles')


class Source:
    """The BF1943 resources by name (lower case; the first bundle holding a name wins - its copies are identical)."""

    def __init__(self, folder):
        if not os.path.isdir(folder):
            raise Bf1943ImportError(f'no BF1943 source folder at {folder} (pass --source: the extracted dist\\xenon folder)')
        self.folder = os.path.abspath(folder)
        fb = fbrb1943()
        self.bundles: dict[str, Any]
        self.entries: dict[str, tuple[str, dict[str, Any]]]
        self.bundles, self.entries = {}, {}
        for dp, _, fs in os.walk(self.folder):
            for f in sorted(fs):
                if not f.endswith('.fbrb'):
                    continue
                path = os.path.join(dp, f)
                try:
                    bd = fb.Bundle(open(path, 'rb').read())
                except ValueError:                          # the three bundles of another layout (fbrb1943's docstring)
                    continue
                rel = os.path.relpath(path, self.folder).replace('\\', '/')
                self.bundles[rel] = bd
                for e in bd.entries:
                    self.entries.setdefault(e['name'].lower(), (rel, e))
        if not self.entries:
            raise Bf1943ImportError(f'{folder}: no BF1943 bundles in it')

    def data(self, key):
        rel, e = self.entries[key]
        return self.bundles[rel].data(e['name'])

    def find(self, filters):
        fl = [f.lower().replace('\\', '/') for f in filters]
        return sorted(k for k in self.entries if all(f in k for f in fl))


def plan(kind):
    if kind in ANIMATIONS:
        return 'animation to PC (bones renumbered onto BFBC2\'s soldier when it animates the soldier)'
    if kind == 'GrannyModel':
        return 'skeleton to PC'
    if kind == 'AimAnimation':
        return "aim animation to PC (bones renumbered onto BFBC2's soldier)"
    if kind == 'XenonTexture':
        return 'texture to PC'
    return 'not converted: ' + NOT_YET.get(kind, f'{kind}: no converter')


class Importer:
    def __init__(self, source, opfor=None):
        self.src = source
        self._bones = None
        self._opfor = opfor
        src_ske = source.entries.get(SOLDIER)
        self.src_soldier = source.data(SOLDIER) if src_ske else None

    def soldier_map(self):
        """{BF1943 bone index: BFBC2 bone index} and the BF1943 soldier's bone names."""
        if self._bones is None:
            if self.src_soldier is None:
                raise Bf1943ImportError(f'the source has no {SOLDIER} (the BF1943 soldier skeleton)')
            import meshview
            import shaderdb
            ix = meshview.Index(shaderdb.find_opfor(self._opfor))
            pc = ix.read(SOLDIER)
            if pc is None:
                raise Bf1943ImportError(f'the game copy has no {SOLDIER}')
            names = {b['Name'] for b in granny.Granny(self.src_soldier).root()['Skeletons'][0]['Bones']}
            self._bones = (granny.bone_map(self.src_soldier, pc), names)
        return self._bones

    def index(self):
        """The game copy's stock bundles (meshview.Index), opened once."""
        if getattr(self, '_ix', None) is None:
            import meshview
            import shaderdb
            self._ix = meshview.Index(shaderdb.find_opfor(self._opfor))
        return self._ix

    def undeploy_tracks(self, key, out):
        """(PC bytes, note) of a converted third-person undeploy with the tracks DICE adds: the bones its weapon's
        same-stance deploy animates and it does not (BF1943's 3p undeploys: 63 tracks, the deploys 88 - 19 weapon
        parts and 6 thumbs, the 25 DICE added to all 22 it shipped). Each holds the deploy's last frame (DICE's rule
        for the weapon parts; DICE's thumbs are animated, which no BF1943 file has - the deploy's last frame is 13 deg
        from them on average, the bind pose 20), but a weapon part the deploy has parked 0.5 m or more from the
        weapon's HandsIkPose takes that pose (the crouching Garand's clip parts). Curve formats from DICE's own
        Garand undeploy (DaIdentity, D3 / D4Constant32f). Measured on the 22: tests\\test_bf1943import.py."""
        m = UNDEPLOY.search(key)
        if not m:
            return out, None
        dep = key[:m.start()] + f'/3p/{m.group(1)}deploy.res'
        if dep not in self.src.entries:
            return out, None
        have = {t['Name'] for t in granny.animations(granny.Granny(out, key))[0]['TrackGroups'][0]['TransformTracks']}

        def tracks(k):
            g = granny.Granny(self.convert(k, extra_tracks=False)[0], k)
            return {t['Name']: t for t in granny.animations(g)[0]['TrackGroups'][0]['TransformTracks']}
        dtr = tracks(dep)
        ik_key = key[:m.start()] + '/3p/handsikpose.res'
        ik = tracks(ik_key) if ik_key in self.src.entries else {}

        def last(t, curve):
            k = granny.curve_keys(t[curve]['CurveData'])
            return k[-1][1] if k else None
        add = []
        for name in sorted(set(dtr) - have):
            pos, ori = last(dtr[name], 'PositionCurve'), last(dtr[name], 'OrientationCurve')
            if name in ik and name.startswith('jntWpn'):
                ip, io = last(ik[name], 'PositionCurve'), last(ik[name], 'OrientationCurve')
                d = sum((a - b) ** 2 for a, b in zip(pos or (0, 0, 0), ip or (0, 0, 0))) ** 0.5
                if d >= PARKED:
                    pos, ori = ip, io
            add.append({'name': name, 'position': ('constant', pos) if pos else None,
                        'orientation': ('constant', ori) if ori else None})
        if not add:
            return out, None
        donor = self.index().read(TRACK_FORMATS)
        if donor is None:
            raise Bf1943ImportError(f'the game copy has no {TRACK_FORMATS} (the curve formats for the added tracks)')
        return granny.add_tracks(out, add, donor, key), f'{len(add)} tracks added from {dep.rsplit("/", 1)[-1]} ' \
            f'(the bones its deploy animates, held at its last frame)'

    def convert(self, key, extra_tracks=True):
        """(output bytes or None, record) for one resource; extra_tracks: a 3p undeploy gets DICE's added tracks
        (undeploy_tracks)."""
        rel, e = self.src.entries[key]
        b = self.src.data(key)
        rec = {'name': e['name'], 'type': e['type'], 'bundle': rel, 'sha256': hashlib.sha256(b).hexdigest()}
        kind = e['type']
        out = None
        if kind in ANIMATIONS:
            g = granny.Granny(b, e['name'])
            tracks = {t['Name'] for a in granny.animations(g) for tg in a['TrackGroups'] for t in tg['TransformTracks']}
            words = granny.struct.unpack_from('>%dI' % (g.strings_from // 4), b, 0)
            table, at = granny.bone_table(list(words))
            bones, names = self.soldier_map()
            if tracks and tracks <= names:
                out = granny.to_pc(b, bones, e['name'])
                rec['done'] = f'animation to PC; {table} table of {len(at)} bone(s) renumbered onto BFBC2\'s soldier'
                if extra_tracks and kind == 'GrannyAnimation':
                    out, note = self.undeploy_tracks(key, out)
                    if note:
                        rec['done'] += '; ' + note
            else:
                out = granny.to_pc(b, None, e['name'])
                rec['done'] = 'animation to PC; bone indices kept (not a soldier animation)'
                if at:
                    rec['note'] = 'its bone table indexes another skeleton: check that skeleton\'s bone order'
        elif kind == 'AimAnimation':
            bones, _ = self.soldier_map()
            out = aimanim.to_pc(b, bones)
            rec['done'] = "aim animation to PC; bone table renumbered onto BFBC2's soldier"
        elif kind == 'GrannyModel':
            out = granny.to_pc(b, None, e['name'])
            rec['done'] = 'skeleton to PC'
        elif kind == 'XenonTexture':
            import texture
            try:
                out = texture.convert(b)
                rec['done'] = 'texture to PC'
            except texture.ConvertError as er:
                rec['done'] = f'not converted: {er}'
        else:
            rec['done'] = 'not converted: ' + NOT_YET.get(kind, f'{kind}: no converter')
        if out is not None:
            rec['out_sha256'] = hashlib.sha256(out).hexdigest()
        return out, rec


def _inside(path, folder):
    path, folder = os.path.normcase(os.path.abspath(path)), os.path.normcase(os.path.abspath(folder))
    return path == folder or path.startswith(folder.rstrip('\\/') + os.sep)


def target(out_dir, name):
    """<out_dir>/<name>, refused when the name would leave out_dir."""
    p = os.path.abspath(os.path.join(out_dir, *name.replace('\\', '/').split('/')))
    if not _inside(p, out_dir):
        raise Bf1943ImportError(f'{name}: the name leads outside the output folder')
    return p


def run_import(source, filters, out_dir, opfor=None, log=print):
    import shaderdb
    if not filters:
        raise Bf1943ImportError('import needs at least one filter (part of a resource name)')
    keys = source.find(filters)
    if not keys:
        raise Bf1943ImportError(f'no BF1943 resource name contains {" and ".join(filters)} (list shows the names)')
    game = None
    with contextlib.suppress(Exception):                    # no game copy: only the source is guarded
        game = os.path.join(shaderdb.find_opfor(opfor), 'game')
    for guarded in (source.folder, game):
        if guarded and _inside(out_dir, guarded):
            raise Bf1943ImportError(f'{out_dir}: the output folder may not lie inside {guarded}')
    imp = Importer(source, opfor)
    records = []
    for k in keys:
        out, rec = imp.convert(k)
        if out is not None:
            p = target(out_dir, rec['name'])
            os.makedirs(os.path.dirname(p), exist_ok=True)
            tmp = p + '.part'
            open(tmp, 'wb').write(out)
            os.replace(tmp, p)
            rec['out'] = os.path.relpath(p, out_dir).replace('\\', '/')
        records.append(rec)
        log(f'{rec["name"]}: {rec["done"]}')
    os.makedirs(out_dir, exist_ok=True)
    man = os.path.join(out_dir, 'import.json')
    old = {}
    if os.path.isfile(man):
        old = {r['name'].lower(): r for r in json.load(open(man, encoding='utf-8')).get('resources', [])}
    old.update({r['name'].lower(): r for r in records})
    tmp = man + '.part'
    json.dump({'source': source.folder, 'resources': sorted(old.values(), key=lambda r: r['name'].lower())},
              open(tmp, 'w', encoding='utf-8'), indent=1)
    os.replace(tmp, man)
    done = sum(1 for r in records if 'out' in r)
    log(f'{done} of {len(records)} resources converted into {out_dir} (records: import.json)')
    return records


def clean(v):
    if isinstance(v, dict):
        return {k: clean(x) for k, x in v.items() if not k.startswith('__') and k != 'Padding'}
    if isinstance(v, list):
        return [clean(x) for x in v]
    return v


def check(source, opfor=None, log=print):
    """Converts every animation and skeleton and checks each output; returns the failures."""
    imp = Importer(source, opfor)
    bones, names = imp.soldier_map()
    pc_names = None
    fails: list[str]
    counts: dict[str, int]
    fails, counts = [], {}
    for k, (_rel, e) in sorted(source.entries.items()):
        if e['type'] not in ANIMATIONS + ('GrannyModel',):
            continue
        try:
            out, rec = imp.convert(k)
            full = out
            if 'tracks added' in rec['done']:                   # a 3p undeploy: the tree is checked without them
                out = imp.convert(k, extra_tracks=False)[0]
                old = {t['Name']: clean(t) for t in granny.animations(granny.Granny(out))[0]['TrackGroups'][0]['TransformTracks']}
                new = {t['Name']: clean(t) for t in granny.animations(granny.Granny(full))[0]['TrackGroups'][0]['TransformTracks']}
                if any(new.get(n) != t for n, t in old.items()):
                    raise granny.GrannyError('adding tracks changed a track it had')
            src, cv = granny.Granny(source.data(k), e['name']), granny.Granny(out, e['name'])
            if cv.e != '<':
                raise granny.GrannyError('output is not little-endian')
            if clean(src.root()) != clean(cv.root()):
                raise granny.GrannyError('the object tree changed')
            if 'renumbered' in rec['done']:
                if pc_names is None:
                    import meshview
                    import shaderdb
                    pc = meshview.Index(shaderdb.find_opfor(opfor)).read(SOLDIER)
                    pc_names = [b['Name'] for b in granny.Granny(pc).root()['Skeletons'][0]['Bones']]
                src_names = [b['Name'] for b in granny.Granny(imp.src_soldier).root()['Skeletons'][0]['Bones']]
                w_src = granny.struct.unpack_from('>%dI' % (src.strings_from // 4), source.data(k), 0)
                w_out = granny.struct.unpack_from('<%dI' % (cv.strings_from // 4), out, 0)
                _, a_src = granny.bone_table(list(w_src))
                _, a_out = granny.bone_table(list(w_out))
                if sorted(src_names[w_src[i]] for i in a_src) != sorted(pc_names[w_out[i]] for i in a_out):
                    raise granny.GrannyError('the renumbered bone table names other bones')
            counts[e['type']] = counts.get(e['type'], 0) + 1
        except Exception as er:                             # noqa: BLE001 - every failure is reported
            fails.append(f'{e["name"]}: {er}')
    log(', '.join(f'{v} {k}' for k, v in sorted(counts.items())) + f' converted and checked; {len(fails)} failed')
    for f in fails[:30]:
        log('FAIL ' + f)
    return fails


def main(argv):
    if any(a in ('-h', '--help', '/?') for a in argv[1:]):
        print(__doc__)
        return 0
    args, opts = [], {}
    it = iter(argv[1:])
    for a in it:
        if a.startswith('--'):
            opts[a] = next(it, None)
            if opts[a] is None:
                raise Bf1943ImportError(f'{a} needs a value')
        else:
            args.append(a)
    if not args or args[0] not in ('list', 'import', 'check'):
        print(__doc__)
        return 1 if args else 0
    source = Source(opts.get('--source') or default_source())
    if args[0] == 'list':
        for k in source.find(args[1:]):
            rel, e = source.entries[k]
            print(f'{e["size"]:10d}  {e["type"]:20s}  {e["name"]}  [{rel}]  -> {plan(e["type"])}')
        return 0
    if args[0] == 'check':
        return 1 if check(source, opts.get('--opfor')) else 0
    out = opts.get('--out')
    if not out:
        import shaderdb
        out = os.path.join(shaderdb.find_opfor(opts.get('--opfor')), 'work', 'bf1943', 'import')
    run_import(source, args[1:], out, opts.get('--opfor'))
    return 0


if __name__ == '__main__':
    errors.setup('bf1943import', gui=False)    # a log; every error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
