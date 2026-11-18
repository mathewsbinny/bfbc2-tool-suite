"""The independent check of round G3 (rust/BENCHMARKS.md, "G3"): each Package-only level read the way NavKit reads a
Dist level - every bundle of its stack unpacked whole by the suite's FbrbTool (bfbc2core.fbrb.unpack) and converted
to XML (DbxEdit xml), then loaded by the suite's level loader (bfbc2core.level's Rust scene_load) - and compared with
what the editor opened from its own copy (tests/g3.rs, BC2_EDITOR_G3_OUT).

    python package_levels_check.py <editor.json> <scratch folder> [--opfor <folder>] [--keep-commons]

The stack is read from game\\Dist\\win32\\package.mft here, with its own small parser. Unpacked bundles go under the
scratch folder: a common bundle once (nam_common, ww2_common, mp2_common), each level's own bundle for its level only
(deleted after it). Bundles NavKit already unpacked in Project OPFOR BFBC2's work folder (mp_common, the Dist levels a
mode layout builds on) are read there, read only, when their .source.sha256 stamp is the bundle's. Nothing is written
to the game copy. FbrbTool's unpack does not write *deleted* markers, so where a marker hides a later bundle's file the
two reads may differ: every difference is listed.
"""
import glob
import hashlib
import json
import os
import shutil
import sys
import time

SUITE = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(SUITE, 'core'))
from bfbc2core import dbxedit, fbrb, paths  # noqa: E402
from bfbc2core import level as levelmod  # noqa: E402


def mft_stacks(game: str) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    cur = None
    with open(os.path.join(game, 'Dist', 'win32', 'package.mft'), encoding='latin-1') as f:
        for line in f:
            line = line.strip()
            if line.startswith('BundleSource '):
                cur = line.split(' ', 1)[1].lower()
                out.setdefault(cur, [])
            elif line.startswith('BundleTarget ') and cur:
                out[cur].append(line.split(' ', 1)[1])
            elif not line:
                cur = None
    return out


def bundle_file(game: str, ref: str) -> str:
    kind, rest = ref.split(':', 1)
    root = os.path.join(game, 'Dist', 'win32') if kind == 'game' else os.path.join(game, 'Package')
    return os.path.join(root, *rest.split('/'))


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def navkit_unpack(opfor: str, ref: str, bundle: str) -> str | None:
    """NavKit's unpack of this bundle in OPFOR's work folder, when current (read only)."""
    rest = ref.split(':', 1)[1].lower()
    name = rest.split('/')[1]
    if not ref.startswith('game:') or not rest.endswith('/level-00.fbrb'):
        return None
    for d in (os.path.join(opfor, 'work', 'navkit', 'common', name), os.path.join(opfor, 'work', 'levels', name, 'dist')):
        stamp = os.path.join(d, '.source.sha256')
        if os.path.isfile(stamp) and open(stamp).read().strip() == sha256(bundle):
            return d
    return None


def unpack(bundle: str, dest: str, xml: bool) -> str:
    if os.path.isfile(os.path.join(dest, '.done')):
        return dest
    shutil.rmtree(dest, ignore_errors=True)
    fbrb.unpack(bundle, dest)
    if xml:
        r = dbxedit.run_cmd(['DbxEdit', 'xml', dest], capture_output=True, text=True, errors='replace')
        if r.returncode != 0:
            raise RuntimeError(f'DbxEdit xml {dest}: {r.stdout[-300:]} {r.stderr[-300:]}')
    open(os.path.join(dest, '.done'), 'w').close()
    return dest


def find_named(dirs: list[str], name: str, last: bool) -> str | None:
    for d in dirs:
        hits = [f for f in glob.glob(os.path.join(d, '**', '*'), recursive=True)
                if os.path.basename(f).lower() == name.lower()]
        if hits:
            return hits[-1] if last else hits[0]
    return None


def du(path: str) -> int:
    return sum(os.path.getsize(os.path.join(r, f)) for r, _, fs in os.walk(path) for f in fs)


def main(argv: list[str]) -> int:
    args = argv[1:]
    opfor = None
    if '--opfor' in args:
        i = args.index('--opfor')
        opfor = args[i + 1]
        del args[i:i + 2]
    keep = '--keep-commons' in args
    args = [a for a in args if a != '--keep-commons']
    editor_json, scratch = args
    opfor = paths.find_opfor(opfor)
    game = os.path.join(opfor, 'game')
    stacks = mft_stacks(game)
    rows = json.load(open(editor_json, encoding='utf-8'))
    # the levels on one common bundle together: each common is unpacked once
    rows.sort(key=lambda r: (stacks[f'game:levels/{r["level"]}/level-00.fbrb'][1:], r['level']))
    os.makedirs(scratch, exist_ok=True)
    results = []
    commons: dict[str, str] = {}
    peak = 0
    for row in rows:
        lv = row['level']
        t0 = time.time()
        bases, own = [], []
        for ref in stacks[f'game:levels/{lv}/level-00.fbrb']:
            b = bundle_file(game, ref)
            have = navkit_unpack(opfor, ref, b)
            if have is None:
                folder = ref.split(':', 1)[1].split('/')[1].lower()
                dest = os.path.join(scratch, folder)
                have = unpack(b, dest, xml=True)
                if folder == lv:
                    own.append(dest)
                else:
                    commons[folder] = dest
            bases.append(have)
        tdirs = []
        for ref in stacks.get(f'game:levels/{lv}/terrain-00.fbrb', []):
            b = bundle_file(game, ref)
            if os.path.isfile(b):
                folder = 'terrain_' + ref.split(':', 1)[1].split('/')[1].lower()
                tdirs.append(unpack(b, os.path.join(scratch, folder), xml=False))
        part = next(p for p in glob.glob(os.path.join(bases[0], 'levels', '*.dbx'))
                    if os.path.splitext(os.path.basename(p))[0].lower() == lv)
        LV = os.path.splitext(os.path.basename(part))[0]
        groups = sorted(glob.glob(os.path.join(bases[0], 'levels', lv, '*_HavokAsset.dbx')))
        f = levelmod.read_many([part])[0]
        if isinstance(f, Exception):
            raise f
        tname = levelmod.terrain_name(f.instances) or lv
        hpath = find_named(tdirs, f'{tname}.heightfield-000.res', last=False)
        mesh = find_named(tdirs, f'{tname}.water.mesh.res', last=True)
        visual = find_named(tdirs, f'{tname}.water.res', last=True)
        s = levelmod._rust().scene_load(LV, part, groups, bases, True, hpath, 2, mesh, visual, alias_cache=None)
        ents = s['entities']
        tris, meshes, missing = s['collision']
        mine = {
            'entities': len(ents),
            'guids': [e[1] for e in ents],
            'with_blueprint_type': sum(1 for e in ents if e[11] is not None),
            'meshes': len(meshes),
            'mesh_list': [f'{i} {bp}' for i, bp in meshes],
            'missing': len(missing),
            'triangles': sum(len(tris[bp]) // 36 for _, bp in meshes if tris[bp] is not None),
            'terrain': None if s['terrain'] is None else s['terrain'][2],
            'water_triangles': None if s['water'] is None else len(s['water'][1]) // 12,
        }
        diffs = [k for k in mine if mine[k] != row.get(k)]
        size = sum(du(d) for d in set(own + list(commons.values()) + tdirs))
        peak = max(peak, size)
        results.append({'level': lv, 'same': not diffs, 'differs': diffs,
                        **{k: (mine[k], row.get(k)) for k in mine if k not in ('guids', 'mesh_list')},
                        'seconds': round(time.time() - t0, 1)})
        print(json.dumps(results[-1]), flush=True)
        for d in own + tdirs:
            shutil.rmtree(d, ignore_errors=True)
        if not keep:
            # a common is kept while the next level loads it too
            nxt = rows[rows.index(row) + 1]['level'] if rows.index(row) + 1 < len(rows) else None
            need = {r.split(':', 1)[1].split('/')[1].lower() for r in stacks.get(f'game:levels/{nxt}/level-00.fbrb', [])}
            for c in [c for c in commons if c not in need]:
                shutil.rmtree(commons.pop(c), ignore_errors=True)
    for d in commons.values():
        if not keep:
            shutil.rmtree(d, ignore_errors=True)
    same = sum(1 for r in results if r['same'])
    print(f'{same} of {len(results)} levels the same; scratch peak {peak / 1e9:.2f} GB')
    return 0 if same == len(results) else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
