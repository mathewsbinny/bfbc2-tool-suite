"""The asset dependency graph over DICE's bundles, and the WW2 rebuild's step graph as Project OPFOR BFBC2's step
records hold it (read-only; spec/transforms/dependencies.md DEP-*, round SPG3).

    python spec/probes/dependencies.py [--opfor <folder>] [--steps]

The asset graph: every `.dbx` entry of DICE's bundles in the game copy (`game/**/*.fbrb` dated before 2013) is a
partition node, each name once (the first bundle in path order that holds it); an edge runs from a partition to every
partition its string table names as an external reference (`<partition>/<guid>`, REF-FORM-2: the string table holds
every reference a partition makes, DBX-STR-1). Resources (`.res` entries) are nodes too; each is tied to the partition
of the same name, else of its name less a last `_<word>` or `.<word>` (PART-NAME-2). Prints node and edge counts, the
edges that resolve in no bundle (REF-RES-3) by their target's top folder, and the strongly connected components.
Round SPG3-rest added the references held inside resources (XDEP-RES-2): the names the mesh sets, the Dx9 shader
databases, the Flash movies and the terrains' and water's visual resources hold, by what they resolve to, and the
voice-over waves the line banks name (about 20 seconds more: the shader databases are read whole).

`--steps` (Project OPFOR BFBC2 only, its `work/ww2/_rebuild/track` records as the last rebuild left them; they change
with every rebuild, so `checks()` does not hold them): the steps of `rebuild_all.STEPS` in order, and for each file a
step read, the last earlier step that wrote it (a read-after-write edge), later steps writing the same file
(write-after-write) and reads of a file only a later step writes (a read of the previous rebuild's output).

Prints counts and names (partition paths and step names; no content). `checks(game)` recomputes the asset graph's
stable counts for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import re
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import fbrb, paths  # noqa: E402

EXT = re.compile(rb'^([a-z0-9_\-./ ]+)/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$', re.I)
SUFFIX = re.compile(r'[_.][^_./]+$')
# DEP-RES-1: how a resource names its partition (tried in this order)
OWNER_RULES = (('same name', lambda r: r), ('less _wavedatasns_<n>', lambda r: re.sub(r'_wavedatasns_\d+$', '', r)),
               ('less _lod<n>_data', lambda r: re.sub(r'_lod\d+_data$', '', r)),
               ('less a last _<word> or .<word>', lambda r: SUFFIX.sub('', r)))

# XDEP-RES-2: the resource types whose bytes name other assets, and the name forms
NAME_BEARING = {'RigidMeshSet', 'CompositeMeshSet', 'SkinnedMeshSet', 'TreeMeshSet', 'Dx9ShaderDatabase', 'SwfMovie',
                'VisualTerrain', 'VisualWater'}
NAME = re.compile(rb'(?<=[\x00-\x1f])[A-Za-z0-9_][A-Za-z0-9_\-. /]{3,199}(?=\x00)')
INSTANCE = re.compile(r'^(.+)/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
VO_BANK = re.compile(r'^sound/vo/vo_[a-z_]+\.dbx$')
WAVE = re.compile(r'^(.*?)(?:_\d+)?_wave$')

EXPECTED: dict[str, Any] = {
    'bundles': 1021, 'dbx_entries': 124458, 'partitions': 84641, 'resources': 80991,
    'edges': 266618, 'dangling_edges': 113597, 'dangling_targets': 7652,
    'no_out': 5254, 'no_in': 60372, 'cycles': 11, 'largest_cycle': 10,
    'res_owner': {'less _lod<n>_data': 4119, 'less _wavedatasns_<n>': 479, 'less a last _<word> or .<word>': 840,
                  'none: level shader database': 222, 'none: other': 1500, 'none: texture': 3064,
                  'none: voice-over wave': 54404, 'same name': 16363},
    'res_files': {'CompositeMeshSet': 353, 'Dx9ShaderDatabase': 76, 'RigidMeshSet': 1741, 'SkinnedMeshSet': 326,
                  'SwfMovie': 55, 'TreeMeshSet': 92, 'VisualTerrain': 30, 'VisualWater': 30},
    'res_refs': {
        'CompositeMeshSet': {'instance': 587, 'instance of an unshipped partition': 4, 'partition': 353,
                             'unresolved': 4, 'unshipped partition': 1945},
        'Dx9ShaderDatabase': {'partition': 625, 'resource ITexture': 37210, 'unresolved': 47055},
        'RigidMeshSet': {'instance': 1927, 'instance of an unshipped partition': 179, 'partition': 1741,
                         'unresolved': 179, 'unshipped partition': 3192},
        'SkinnedMeshSet': {'instance': 323, 'partition': 326, 'unresolved': 146, 'unshipped partition': 1472},
        'SwfMovie': {'partition': 103, 'resource ITexture': 3, 'unresolved': 92},
        'TreeMeshSet': {'instance': 92, 'partition': 92, 'unresolved': 60, 'unshipped partition': 293},
        'VisualTerrain': {'instance of an unshipped partition': 1, 'partition': 1, 'resource ITexture': 30,
                          'resource RigidMeshSet': 167, 'resource TerrainHeightfield': 318, 'unresolved': 2731},
        'VisualWater': {'unresolved': 145},
    },
    'vo_waves': {'named by a bank': 41393, 'no bank names it': 70},
}


def strings(b: bytes) -> list[bytes]:
    """A binary DBX's string table (DBX-STR-1: big-endian count at 20, offsets from 24, strings after them)."""
    if len(b) <= 24 or b[:8] != b'{binary}':
        return []
    n = struct.unpack_from('>I', b, 20)[0]
    base = 24 + 4 * n
    out = []
    for i in range(n):
        o = base + struct.unpack_from('>I', b, 24 + 4 * i)[0]
        out.append(b[o:b.find(b'\0', o)])
    return out


def dice(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def classify(name: str, nodes: set[str], res: dict[str, str], unshipped: set[str]) -> str:
    """XDEP-RES-2: what a name held inside a resource resolves to."""
    if name in nodes:
        return 'partition'
    m = INSTANCE.match(name)
    if m:
        return 'instance' if m.group(1) in nodes else 'instance of an unshipped partition'
    if name in res:
        return 'resource ' + res[name]
    return 'unshipped partition' if name in unshipped else 'unresolved'


def resource_refs(bundles: list[Path], nodes: set[str], res: dict[str, str], unshipped: set[str]) -> dict[str, Any]:
    """XDEP-RES-2: the names the name-bearing resources hold (a C string with a '/' in the resource's bytes), each
    resource once (the first bundle in path order), per resource type and resolution; and the voice-over waves the
    line banks name (a wave `.../<node>[_<n>]_wave` whose `<node>` is a string of a `sound/vo/vo_*` bank)."""
    refs: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
    files: collections.Counter[str] = collections.Counter()
    seen: set[str] = set()
    bank: set[str] = set()
    for p in bundles:
        es = [e for e in fbrb.entries(p) if not e.deleted]
        want = [e for e in es if e.type in NAME_BEARING and e.name.lower() not in seen]
        banks = [e.name for e in es if VO_BANK.match(e.name.lower())]
        if not want and not banks:
            continue
        types = {e.name: e.type for e in want}
        for n, b in fbrb.read(p, [e.name for e in want] + banks).items():
            if n in types:
                seen.add(n.lower())
                files[types[n]] += 1
                for name in {m.group().decode('latin-1').lower() for m in NAME.finditer(b)}:
                    if '/' in name:
                        refs[types[n]][classify(name, nodes, res, unshipped)] += 1
            else:
                bank.update(s.decode('latin-1').lower() for s in strings(b))
    waves: collections.Counter[str] = collections.Counter()
    for r, ty in res.items():
        if r.startswith('sound/vo/') and ty == 'Wave':
            m = WAVE.match(r.rsplit('/', 1)[-1])
            waves['named by a bank' if m and m.group(1) in bank else 'no bank names it'] += 1
    return {'files': dict(sorted(files.items())), 'refs': {k: dict(sorted(v.items())) for k, v in sorted(refs.items())},
            'vo_waves': dict(sorted(waves.items()))}


def cycles(nodes: set[str], adj: dict[str, list[str]]) -> list[list[str]]:
    """Strongly connected components of more than one node (iterative Tarjan)."""
    index: dict[str, int] = {}
    low: dict[str, int] = {}
    on: set[str] = set()
    stack: list[str] = []
    out: list[list[str]] = []
    c = 0
    for v0 in sorted(nodes):
        if v0 in index:
            continue
        index[v0] = low[v0] = c
        c += 1
        stack.append(v0)
        on.add(v0)
        work = [(v0, iter(adj.get(v0, ())))]
        while work:
            v, it = work[-1]
            for w in it:
                if w not in index:
                    index[w] = low[w] = c
                    c += 1
                    stack.append(w)
                    on.add(w)
                    work.append((w, iter(adj.get(w, ()))))
                    break
                if w in on:
                    low[v] = min(low[v], index[w])
            else:
                work.pop()
                if work:
                    low[work[-1][0]] = min(low[work[-1][0]], low[v])
                if low[v] == index[v]:
                    comp = []
                    while True:
                        w = stack.pop()
                        on.discard(w)
                        comp.append(w)
                        if w == v:
                            break
                    if len(comp) > 1:
                        out.append(sorted(comp))
    return out


def analyse(game: Path) -> dict[str, Any]:
    bundles = [p for p in sorted(game.glob('**/*.fbrb')) if dice(p)]
    nodes: set[str] = set()
    res: dict[str, str] = {}
    edges: set[tuple[str, str]] = set()
    entries = 0
    for p in bundles:
        es = [e for e in fbrb.entries(p) if not e.deleted]
        dbxs = [e.name for e in es if e.name.lower().endswith('.dbx')]
        for e in es:
            if e.name.lower().endswith('.res'):
                res.setdefault(e.name.lower().rsplit('.', 1)[0], e.type)
        entries += len(dbxs)
        new = [n for n in dbxs if n.lower()[:-4] not in nodes]
        if not new:
            continue
        for n, b in fbrb.read(p, new).items():
            src = n.lower()[:-4]
            nodes.add(src)
            for text in strings(b):
                m = EXT.match(text)
                if m:
                    edges.add((src, m.group(1).decode('latin-1').lower()))
    dangling = [d for _, d in edges if d not in nodes]
    adj: dict[str, list[str]] = collections.defaultdict(list)
    for a, d in edges:
        if d in nodes:
            adj[a].append(d)
    sccs = cycles(nodes, adj)
    owner: collections.Counter[str] = collections.Counter()
    for r, ty in res.items():
        for rule, f in OWNER_RULES:
            if f(r) in nodes:
                owner[rule] += 1
                break
        else:
            owner['none: ' + ('level shader database' if r.startswith('levels/') and '/shaders/' in r else
                              'voice-over wave' if '/vo/' in r and ty in ('Wave', '<non-resource>') else
                              'texture' if ty == 'ITexture' else 'other')] += 1
    top = collections.Counter(d.split('/')[0] for d in dangling)
    inside = resource_refs(bundles, nodes, res, set(dangling))
    return {
        'res_files': inside['files'], 'res_refs': inside['refs'], 'vo_waves': inside['vo_waves'],
        'bundles': len(bundles), 'dbx_entries': entries, 'partitions': len(nodes), 'resources': len(res),
        'edges': len(edges), 'dangling_edges': len(dangling), 'dangling_targets': len(set(dangling)),
        'no_out': len(nodes - {s for s, _ in edges}), 'no_in': len(nodes - {d for _, d in edges}),
        'cycles': len(sccs), 'largest_cycle': max((len(x) for x in sccs), default=0),
        'res_owner': dict(sorted(owner.items())),
        '_dangling_top': top.most_common(8), '_cycles': sccs,
        '_in': collections.Counter(d for _, d in edges if d in nodes).most_common(5),
    }


def steps(opfor: Path) -> None:
    """The WW2 rebuild's step graph from the step records (print only)."""
    ww2 = opfor / 'src' / 'tools' / 'ww2'
    sys.path.insert(0, os.fspath(ww2))
    sys.dont_write_bytecode = True                     # nothing written into OPFOR's checkout
    import rebuild_all  # noqa: PLC0415 - OPFOR's module, read only
    import stepstamps  # noqa: PLC0415
    track = opfor / 'work' / 'ww2' / '_rebuild' / 'track'
    work = stepstamps.norm(os.fspath(opfor / 'work' / 'ww2'))
    names = [rebuild_all.name(s) for s in rebuild_all.STEPS]
    recs: dict[int, dict[str, set[str]]] = {}
    for i, n in enumerate(names):
        try:
            with open(track / stepstamps.key(n), encoding='utf-8') as f:
                r = json.load(f)
        except OSError:
            continue
        recs[i] = {'inputs': set(r['inputs']), 'outputs': set(r['outputs'])}
    writers: dict[str, list[int]] = collections.defaultdict(list)
    for i in sorted(recs):
        for p in recs[i]['outputs']:
            writers[p].append(i)
    raw: collections.Counter[tuple[int, int]] = collections.Counter()
    back: collections.Counter[tuple[int, int]] = collections.Counter()
    outside = inside = 0
    for i, r in sorted(recs.items()):
        for p in r['inputs']:
            ws = writers.get(p, [])
            before = [j for j in ws if j < i]
            if before:
                raw[(max(before), i)] += 1
            elif ws:
                back[(i, min(ws))] += 1
            elif p.startswith(work + os.sep):
                inside += 1
            else:
                outside += 1
    waw: collections.Counter[tuple[int, int]] = collections.Counter()
    for ws in writers.values():
        for a, b in zip(ws, ws[1:]):
            waw[(a, b)] += 1
    print(f'steps {len(names)}, records {len(recs)}; outputs {sum(len(r["outputs"]) for r in recs.values())}, '
          f'inputs {sum(len(r["inputs"]) for r in recs.values())} (outside work\\ww2 {outside}, inside and no '
          f'step writes them {inside})')
    print(f'read-after-write edges {len(raw)} ({sum(raw.values())} files); write-after-write {len(waw)} '
          f'({sum(waw.values())} files); reads of a later step\'s output {len(back)} ({sum(back.values())} files)')
    for (a, b), c in sorted(waw.items()):
        print(f'  WAW {a:2d} {names[a]} -> {b:2d} {names[b]}: {c}')
    for (a, b), c in sorted(back.items()):
        print(f'  READ-BEFORE-WRITE {a:2d} {names[a]} reads {c} file(s) {b:2d} {names[b]} writes later')
    reach: dict[int, set[int]] = {i: set() for i in recs}
    for (a, b) in sorted(raw) + sorted(waw):
        reach[b].add(a)
    depth: dict[int, int] = {}
    for i in sorted(recs):
        depth[i] = 1 + max((depth[j] for j in reach[i] if j in depth), default=0)
    width = collections.Counter(depth.values())
    print(f'longest chain {max(depth.values())} steps; steps per depth {dict(sorted(width.items()))}')
    print('steps reading no earlier step:', [names[i] for i in sorted(recs) if not reach[i]])


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    ap.add_argument('--steps', action='store_true', help='the WW2 rebuild\'s step graph instead of the asset graph')
    a = ap.parse_args()
    found = paths.find_opfor(a.opfor)
    opfor = Path(found)
    t = time.time()
    if a.steps:
        steps(opfor)
    else:
        for k, v in sorted(analyse(Path(paths.game_dir(found))).items()):
            print(f'{k}: {v}')
    print(f'({time.time() - t:.1f} s)')


def checks(game: Path) -> list[tuple[bool, str]]:
    got = analyse(game)
    return [(got.get(k) == want, f'dependencies probe: {k} = {want!r} (got {got.get(k)!r})')
            for k, want in EXPECTED.items()]


if __name__ == '__main__':
    main()
