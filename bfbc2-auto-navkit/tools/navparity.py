"""NavKit: DICE-parity report over many levels, saved per named run so runs can be compared.

  navparity.py run <name> <level>=<candidate.hpd> ... [--opfor <folder>]
  navparity.py run <name> --pattern "<path with {lv}>" [--levels sp_01,sp_02,...]
  navparity.py show <name> [<other name>]        a table; with two names, the second against the first

Each level is compared with its original (DICE) path data in OPFOR's game copy, on everything:
  sectors        sector ids in each file
  walkable       DICE ground reproduced, extra ground, areas exactly the same, detailed (0.2 m) areas
  boundaries     outline, portals and border portals of the areas that match
  routing        route nodes and edges in the same place (0.05 m), edge costs of those edges (the old measures,
                 nodes_same / edges_same / costs); and over every DICE node and edge (round NKS-deep E): each DICE
                 node paired one to one with its nearest NavKit node of the sector within half a propagation step
                 (1.675 m coarse, 0.425 m detailed; 1 m in height): nodes_paired, edges_paired (DICE edges whose
                 partners hold the edge), costs_paired (those with the cost within a tenth of DICE's; all three
                 shares of DICE's), nodes_paired_candidate (the paired share of NavKit's nodes), node_distance (the nearest NavKit node's plan distance histogram, nd_far beyond 4 m)
  islands        separately connected regions (the seeded islands): count, large ones (>= 25 nodes), and over
                 DICE's route nodes (each paired with a NavKit node within 2 m): of the pairs DICE connects,
                 the share NavKit connects (connectivity), and of the pairs NavKit connects, the share DICE does
                 not (over-connection)
  directed       the route graph as the runtime walks it (KYRT-PATH-3: a node's edges are those of all its copies;
                 an edge into a border node with no owner copy is dropped): each file's strongly connected share of
                 its weakly connected node pairs (strong_share, strong_share_reference), and of the DICE node pairs
                 joined both ways, the share NavKit joins both ways (runtime_connectivity)
  ai positions   the earlier figures on the level's AI order positions: coverage, pairwise route check,
                 main route network
  D and F        D lines and F cells (navdf.py): precision / recall of D sides and F cells, all and per pitch
                 (columns df_*)

Runs are saved in OPFOR's work\\navkit\\parity\\<name>.json.
"""
import json
import math
import os
import sys
import time
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit', 'navkit-tools')
import navcompare  # noqa: E402
import navdf  # noqa: E402
import navdiff  # noqa: E402
import ordercheck  # noqa: E402

LEVELS = ['sp_01', 'sp_02', 'sp_03', 'sp_04', 'sp_003_b', 'sp_004_b', 'sp_005', 'sp_005_b', 'sp_006', 'sp_007',
          'sp_008', 'sp_008_b', 'sp_009']
ONSLAUGHT = ['ons_mp_002', 'ons_mp_004']       # Napisal's ports with DICE's console path data (OPFOR's Onslaught import)
NEAR = 2.0
NEAR_DY = 1.0                   # height tolerance of a node match (both files' nodes sit 0.95 m over their ground). Was
                                # 2.5 m: a street node then matched a deck node 2.5 m above it, and ONS_MP_002's detailed
                                # build showed 119 'missing' DICE links - 12 with 1 m (2026-10-21)


def opfor_root(given=None):
    import navkit
    return navkit.find_opfor(given)


def reference_of(root, lv):
    """DICE's infantry path data of a level (the earliest kept original where an OPFOR stage - NavKit, stock-MP AI -
    installed its own), or None: no file, or only OPFOR's (installed where the level had none; navkit.original_file)."""
    import navkit
    return navkit.original_file(pathdata_of(root, lv))


def pathdata_of(root, lv):
    """The infantry path data installed in the game copy (whoever wrote it)."""
    return os.path.join(root, 'game', 'Output', 'win32', 'ai', 'levels', lv, 'infantry', 'pathdata.hpd')


def _sha1(path):
    import hashlib
    return hashlib.sha1(open(path, 'rb').read()).hexdigest()


def need_reference(root, lv):
    ref = reference_of(root, lv)
    if ref is None:
        import navkit
        raise RuntimeError(f'{lv}: no DICE path data to compare with (none in the game copy, or only OPFOR\'s)'
                           + navkit.opfor_write_note(pathdata_of(root, lv)))
    return ref


def level_xml(root, lv):
    d = os.path.join(root, 'work', 'levels', lv, 'dist', 'levels')
    for f in os.listdir(d):
        if f.lower() == lv + '.xml':
            return os.path.join(d, f)
    raise FileNotFoundError(f'{lv}: converted level partition not found in {d} (navkit.py prepare {lv})')


def terrain_bounds(root, lv):
    r"""(minX, minZ, maxX, maxZ) of the level's terrain heightfield (work\levels\<lv>\terrain), or None. DICE's console
    path data can reach past it (ONS_MP_002: ground to z -527, the PC terrain ends at -512 - 3,600 m2 and a route
    that do not exist on PC), so the reference is clipped to it: ground and connectivity are measured on PC ground."""
    suite_paths.use('core')
    from bfbc2core import terrain
    d = os.path.join(root, 'work', 'levels', lv, 'terrain')
    for r, _dirs, files in os.walk(d):
        for f in files:
            if f.lower().endswith('.heightfield-000.res'):
                hf = terrain.read(os.path.join(r, f))
                return (float(hf.min[0]), float(hf.min[2]), float(hf.max[0]), float(hf.max[2]))
    return None


STALE_DY = 1.0     # a reference route node with no sampled PC surface within this of its ground height is stale


def stale_nodes(root, lv, reference, clip=None):
    """Ids of the reference's route nodes on ground the PC level does not have: no surface of NavKit's 0.8 m samples
    (the column and its 8 neighbours) within STALE_DY of the node's ground (node height - 0.95). DICE's data was built
    before some late geometry - sp_03: its only valley-to-plateau links run 1.5-5 m under the cliff mesh sp03_cliff_2
    and raised terrain (85 nodes; 99.8 % of its nodes are on PC ground) - and a route there is no route on PC, so
    connectivity leaves those nodes out, as it leaves out ground past the terrain (terrain_bounds). Empty without
    0.8 m samples."""
    import hpdtool
    import navsample
    import numpy as np
    sp = next((p for p in (os.path.join(root, 'work', 'navsample', f'{lv}_navkit.bin'),
                           os.path.join(root, 'work', 'navsample', f'{lv}_full.bin')) if os.path.isfile(p)), None)
    if sp is None:
        return set()
    s, _ = navsample.load(sp)
    out = set()
    for sec in hpdtool.read(reference).sectors:
        for n in sec.nodes:
            x, y, z = n.pos
            if clip and not (clip[0] <= x <= clip[2] and clip[1] <= z <= clip[3]):
                continue
            i, j = int(round((x - s.min_x) / s.pitch)), int(round((z - s.min_z) / s.pitch))
            if not (1 <= i < s.nx - 1 and 1 <= j < s.nz - 1):
                continue
            g = y - 0.95
            ok = False
            for dj in (-1, 0, 1):
                c0 = (j + dj) * s.nx + i
                a, b = s.start[c0 - 1], s.start[c0 + 2]
                if b > a and np.min(np.abs(s.y[a:b] - g)) <= STALE_DY:
                    ok = True
                    break
            if not ok:
                out.add(n.id)
    return out


def ladder_ground(reference, candidate, reference_cells):
    """Ground reached only through ladder links (navdata: ladder edges, ladder areas): its share of the reference's
    ground, and how many of its cells the reference lacks (kept out of 'extra ground')."""
    suite_paths.use('core')
    import numpy as np
    from bfbc2core import navdata
    cn = navdata.load(candidate)
    if not len(cn.cell_area) or not any(a.ladder for a in cn.areas):
        return {'percent': 0.0, 'extra_cells': 0, 'cells': 0}
    rn = navdata.load(reference)                 # only when there is ladder ground to compare (round E3h)
    same_c, _ = navdata.compare_cells(cn, rn)
    lad = np.array([a.ladder for a in cn.areas])[cn.cell_area]
    return {'percent': round(100.0 * int(lad.sum()) / max(reference_cells, 1), 2), 'extra_cells': int((lad & ~same_c).sum()),
            'cells': int(lad.sum())}


def pairs(n):
    return n * (n - 1) // 2


def islands(reference, candidate, clip=None, drop=None):
    R, C = ordercheck.PathData(reference, clip, drop), ordercheck.PathData(candidate)
    res: dict[str, float] = {'reference': len(R.group_size), 'candidate': len(C.group_size),
           'large_reference': sum(1 for v in R.group_size.values() if v >= 25),
           'large_candidate': sum(1 for v in C.group_size.values() if v >= 25)}
    grid = defaultdict(list)
    for k, (_nid, p) in enumerate(C.nodes):
        grid[(math.floor(p[0] / NEAR), math.floor(p[2] / NEAR))].append(k)
    table: Counter[tuple[int, int]] = Counter()
    matched = 0
    for nid, p in R.nodes:
        gx, gz = math.floor(p[0] / NEAR), math.floor(p[2] / NEAR)
        best, best_d = None, math.inf
        for dx in (-1, 0, 1):
            for dz in (-1, 0, 1):
                for k in grid.get((gx + dx, gz + dz), ()):
                    q = C.nodes[k][1]
                    if abs(q[1] - p[1]) > NEAR_DY:
                        continue
                    d = (q[0] - p[0]) ** 2 + (q[2] - p[2]) ** 2
                    if d <= NEAR * NEAR and (best is None or d < best_d):
                        best, best_d = k, d
        if best is not None:
            matched += 1
            table[(R.group[nid], C.group[C.nodes[best][0]])] += 1
    by_ref: Counter[int]
    by_cand: Counter[int]
    by_ref, by_cand = Counter(), Counter()
    for (gr, gc), n in table.items():
        by_ref[gr] += n
        by_cand[gc] += n
    both = sum(pairs(n) for n in table.values())
    ref_pairs = sum(pairs(n) for n in by_ref.values())
    cand_pairs = sum(pairs(n) for n in by_cand.values())
    res['nodes_near'] = round(100.0 * matched / max(len(R.nodes), 1), 2)
    res['connectivity'] = round(100.0 * both / max(ref_pairs, 1), 2)
    res['over_connection'] = round(100.0 * (cand_pairs - both) / max(cand_pairs, 1), 2)
    return res


def runtime_components(path, clip=None, drop=None):
    """The route graph's strongly connected components as the runtime's edge walk reads it (KYRT-PATH-3): a border
    node is one node over its copies (ids compared with bit 31 masked) and its out-edges are those of every copy; an
    edge into a border node none of whose copies is an owner (bit 16 of field c set on all) is dropped. Returns ({masked id: component}, positions
    [(masked id, pos)], strong pairs, weak pairs): the pairs count node pairs in one strong / one weak component.
    clip and drop leave nodes out as PathData does."""
    import hpdtool
    f = hpdtool.read(path)
    dropped = {d & 0x7FFFFFFF for d in (drop or ())}

    def inside(n):
        p = n.pos
        return (n.id & 0x7FFFFFFF) not in dropped and (not clip or (clip[0] <= p[0] <= clip[2] and clip[1] <= p[2] <= clip[3]))
    index: dict[int, int] = {}
    nodes = []
    for s in f.sectors:
        for n in s.nodes:
            if inside(n) and (n.id & 0x7FFFFFFF) not in index:
                index[n.id & 0x7FFFFFFF] = len(index)
                nodes.append((n.id & 0x7FFFFFFF, n.pos))
    owned = {n.id for s in f.sectors for n in s.nodes if not n.id & 0x80000000 or not n.c & 0x10000}
    adj: list[list[int]] = [[] for _ in index]
    for s in f.sectors:
        for e in s.edges:
            a, b = s.nodes[e.src], s.nodes[e.dst]
            if b.id not in owned:
                continue                      # into a border node with no owner copy: dropped (0x0105C8C0)
            ia, ib = index.get(a.id & 0x7FFFFFFF), index.get(b.id & 0x7FFFFFFF)
            if ia is not None and ib is not None and inside(a) and inside(b):
                adj[ia].append(ib)
    comp = _tarjan(adj)
    parent = list(range(len(adj)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for src, out in enumerate(adj):
        for dst in out:
            ra, rb = find(src), find(dst)
            if ra != rb:
                parent[ra] = rb
    strong = sum(pairs(n) for n in Counter(comp).values())
    weak = sum(pairs(n) for n in Counter(find(x) for x in range(len(adj))).values())
    return {nid: comp[k] for k, (nid, _) in enumerate(nodes)}, nodes, strong, weak


def _tarjan(adj):
    """Strongly connected components (Tarjan's, iterative): a component number per node."""
    n = len(adj)
    index, low, comp = [-1] * n, [0] * n, [-1] * n
    on = [False] * n
    stack: list[int] = []
    nxt = ncomp = 0
    for root in range(n):
        if index[root] >= 0:
            continue
        index[root] = low[root] = nxt
        nxt += 1
        stack.append(root)
        on[root] = True
        call = [(root, 0)]
        while call:
            v, i = call[-1]
            if i < len(adj[v]):
                call[-1] = (v, i + 1)
                w = adj[v][i]
                if index[w] < 0:
                    index[w] = low[w] = nxt
                    nxt += 1
                    stack.append(w)
                    on[w] = True
                    call.append((w, 0))
                elif on[w]:
                    low[v] = min(low[v], index[w])
                continue
            call.pop()
            if call:
                u = call[-1][0]
                low[u] = min(low[u], low[v])
            if low[v] == index[v]:
                while True:
                    w = stack.pop()
                    on[w] = False
                    comp[w] = ncomp
                    if w == v:
                        break
                ncomp += 1
    return comp


def runtime_islands(reference, candidate, clip=None, drop=None):
    """The directed measures as the runtime walks the graph (runtime_components): each file's strong share (node
    pairs in one strongly connected component over those in one weakly connected one), and over DICE's route nodes
    paired with a candidate node as islands() pairs them, of the pairs DICE's graph joins both ways, the share the
    candidate's joins both ways (runtime_connectivity)."""
    rc, rnodes, rs, rw = runtime_components(reference, clip, drop)
    cc, cnodes, cs, cw = runtime_components(candidate)
    grid = defaultdict(list)
    for nid, p in cnodes:
        grid[(math.floor(p[0] / NEAR), math.floor(p[2] / NEAR))].append((nid, p))
    table: Counter[tuple[int, int]] = Counter()
    for nid, p in rnodes:
        gx, gz = math.floor(p[0] / NEAR), math.floor(p[2] / NEAR)
        best, best_d = None, math.inf
        for dx in (-1, 0, 1):
            for dz in (-1, 0, 1):
                for cid, q in grid.get((gx + dx, gz + dz), ()):
                    if abs(q[1] - p[1]) > NEAR_DY:
                        continue
                    d = (q[0] - p[0]) ** 2 + (q[2] - p[2]) ** 2
                    if d <= NEAR * NEAR and (best is None or d < best_d):
                        best, best_d = cid, d
        if best is not None:
            table[(rc[nid], cc[best])] += 1
    by_ref: Counter[int] = Counter()
    for (gr, _gc), n in table.items():
        by_ref[gr] += n
    both = sum(pairs(n) for n in table.values())
    return {'runtime_connectivity': round(100.0 * both / max(sum(pairs(n) for n in by_ref.values()), 1), 2),
            'strong_share': round(100.0 * cs / max(cw, 1), 2),
            'strong_share_reference': round(100.0 * rs / max(rw, 1), 2)}


def run_level(root, lv, candidate, log):
    import hpdtool
    with hpdtool.shared_reads():         # each file parsed once for all the measures (round E4e)
        return _run_level(root, lv, candidate, log)


def _run_level(root, lv, candidate, log):
    ref = need_reference(root, lv)
    xml = level_xml(root, lv)
    t = time.time()
    clip = terrain_bounds(root, lv)
    d, _, R, C = navdiff.diff(ref, candidate, log=lambda *a: None, clip=clip)
    df = navdf.compare_sectors(R, C, clip)          # in the Rust core when built (round E4e)
    a = navcompare.compare(xml, ref, candidate)
    stale = stale_nodes(root, lv, ref, clip)
    isl = islands(ref, candidate, clip, stale)
    lad = ladder_ground(ref, candidate, d['cells']['reference'])
    directed = runtime_islands(ref, candidate, clip, stale)
    row = {
        'level': lv, 'candidate': candidate, 'candidate_sha1': _sha1(candidate), 'reference_clip': clip, 'stale_reference_nodes': len(stale),
        'sectors_reference': d['sectors']['reference'], 'sectors_candidate': d['sectors']['candidate'],
        'sectors_missing': len(d['sectors']['only_reference']), 'sectors_extra': len(d['sectors']['only_candidate']),
        'ground': d['cells']['parity'],
        # extra ground without the ground reached by ladder links (its own column: DICE has no ladder links)
        'extra_ground': round(100.0 * max(d['cells']['only_candidate'] - lad['extra_cells'], 0) / max(d['cells']['reference'], 1), 2),
        'ladder_ground': lad['percent'],
        'areas_same': d['areas']['parity'], 'areas_reference': d['areas']['reference'], 'areas_candidate': d['areas']['candidate'],
        'detailed_reference': d['areas']['detailed_reference'], 'detailed_candidate': d['areas']['detailed_candidate'],
        'outline': d['bounds_of_same_areas']['outline'], 'portals': d['bounds_of_same_areas']['portals'],
        'borders': d['bounds_of_same_areas']['borders'],
        'nodes_same': d['graph']['node_parity'], 'edges_same': d['graph']['edge_parity'], 'costs': d['graph']['costs_same_of_same_edges'],
        # the nearest-node pairing over every DICE node and edge (round NKS-deep E; navdiff.paired_graph)
        'nodes_paired': d['graph']['node_pairing'], 'edges_paired': d['graph']['edge_pairing'],
        'nodes_paired_candidate': d['graph']['node_pairing_candidate'],      # of NavKit's nodes: more nodes earn no credit
        'nodes_reference': d['graph']['nodes_reference'], 'nodes_candidate': d['graph']['nodes_candidate'],
        'costs_paired': d['graph']['cost_pairing'], 'node_distance': d['graph']['node_distance'],
        'islands_reference': isl['reference'], 'islands_candidate': isl['candidate'],
        'large_islands_reference': isl['large_reference'], 'large_islands_candidate': isl['large_candidate'],
        'nodes_near': isl['nodes_near'], 'connectivity': isl['connectivity'], 'over_connection': isl['over_connection'],
        'ai_coverage': a['coverage_percent'], 'ai_pairwise': a['pairwise']['percent'],
        'ai_main_reference': a['main_network']['reference'], 'ai_main_candidate': a['main_network']['candidate'],
    }
    row.update(directed)    # the directed measures, as the runtime walks the graph (KYRT-PATH-3)
    row.update({k: v for k, v in df.items() if k in DF_KEYS})     # D lines and F cells (navdf.py)
    log(f"  {lv:9s} ground {row['ground']:6.2f} % (+{row['extra_ground']:.1f} % extra)  areas {row['areas_same']:6.2f} %  nodes {row['nodes_same']:6.2f} %  "
        f"connectivity {row['connectivity']:6.2f} %  AI positions {row['ai_coverage']:5.1f} %  ({time.time() - t:.0f} s)")
    return row


DF_KEYS = ['df_d_precision', 'df_d_recall', 'df_d_recall_near', 'df_d_recall_shared', 'df_f_precision', 'df_f_recall',
           'df_d_precision_n', 'df_d_recall_n', 'df_f_precision_n', 'df_f_recall_n',
           'df_d_precision_d', 'df_d_recall_d', 'df_f_precision_d', 'df_f_recall_d', 'df_d_dy',
           'df_d_reference', 'df_d_candidate', 'df_f_reference', 'df_f_candidate']
COLUMNS = [
    ('ground', 'ground %', '{:.1f}'), ('extra_ground', 'extra %', '{:.1f}'), ('areas_same', 'areas %', '{:.1f}'),
    ('detailed_reference', 'det.ref', '{}'), ('detailed_candidate', 'det.ours', '{}'),
    ('portals', 'portals %', '{:.1f}'), ('nodes_same', 'nodes %', '{:.1f}'), ('edges_same', 'edges %', '{:.1f}'),
    ('nodes_paired', 'n.pair %', '{:.1f}'), ('edges_paired', 'e.pair %', '{:.1f}'), ('costs_paired', 'c.pair %', '{:.1f}'),
    ('islands_reference', 'isl.ref', '{}'), ('islands_candidate', 'isl.ours', '{}'), ('connectivity', 'connect %', '{:.1f}'),
    ('over_connection', 'over %', '{:.1f}'), ('runtime_connectivity', 'runtime %', '{:.1f}'), ('ai_coverage', 'AI pos %', '{:.1f}'), ('ai_pairwise', 'pairwise %', '{:.1f}'),
    ('ladder_ground', 'ladder %', '{:.1f}'),
    ('df_d_precision', 'D prec %', '{:.1f}'), ('df_d_recall', 'D rec %', '{:.1f}'), ('df_f_precision', 'F prec %', '{:.1f}'),
    ('df_f_recall', 'F rec %', '{:.1f}'),
]
TOTALS = ['ground', 'extra_ground', 'areas_same', 'portals', 'nodes_same', 'edges_same', 'nodes_paired', 'edges_paired',
          'costs_paired', 'connectivity', 'over_connection', 'runtime_connectivity', 'ai_coverage',
          'ai_pairwise', 'ladder_ground', 'df_d_precision', 'df_d_recall', 'df_f_precision', 'df_f_recall']


def table(run, other=None, log=print):
    rows = {r['level']: r for r in run['levels']}
    base = {r['level']: r for r in other['levels']} if other else {}
    head = f"{'level':9s} " + ' '.join(f'{h:>10s}' for _, h, _ in COLUMNS)
    log(f"run '{run['name']}' ({run['date']})" + (f" against '{other['name']}' ({other['date']}): value (change)" if other else ''))
    log(head)
    for lv in [l for l in LEVELS if l in rows] + sorted(set(rows) - set(LEVELS)):
        r = rows[lv]
        cells = []
        for key, _, f in COLUMNS:
            if key in ('portals',) and r['areas_same'] == 0:
                cells.append(f"{'n/a':>10s}")   # a share of matching areas, and none match
                continue
            v = f.format(r.get(key, 0))
            if lv in base and isinstance(r.get(key, 0), (int, float)) and r.get(key, 0) != base[lv].get(key, 0):
                delta = r.get(key, 0) - base[lv].get(key, 0)
                v += f"({'+' if delta > 0 else ''}{f.format(delta) if isinstance(delta, float) else delta})"
            cells.append(f'{v:>10s}')
        log(f'{lv:9s} ' + ' '.join(cells))
    avg = {k: sum(r.get(k, 0) for r in rows.values()) / len(rows) for k in TOTALS}
    if all(r['areas_same'] == 0 for r in rows.values()):
        avg.pop('portals')
    line = f"{'mean':9s} " + ' '.join(f"{(f.format(avg[k]) if k in avg else ''):>10s}" for k, _, f in COLUMNS)
    log(line)
    if other:
        bavg = {k: sum(r.get(k, 0) for r in other['levels']) / len(other['levels']) for k in TOTALS}
        log(f"{'change':9s} " + ' '.join(f"{(('+' if avg[k] - bavg[k] > 0 else '') + f.format(avg[k] - bavg[k]) if k in avg and k in bavg else ''):>10s}" for k, _, f in COLUMNS))


def save_path(root, name):
    d = os.path.join(root, 'work', 'navkit', 'parity')
    os.makedirs(d, exist_ok=True)
    return os.path.join(d, name.replace(' ', '_') + '.json')


def main(argv):
    args = argv[1:]
    opfor = None
    if '--opfor' in args:
        i = args.index('--opfor'); opfor = args[i + 1]; del args[i:i + 2]
    if len(args) < 2 or args[0] not in ('run', 'show'):
        print(__doc__)
        return 1
    root = opfor_root(opfor)
    if args[0] == 'show':
        runs = [json.load(open(save_path(root, n))) for n in args[1:3]]
        table(runs[0], runs[1] if len(runs) > 1 else None) if len(runs) == 1 else table(runs[1], runs[0])
        return 0
    name = args[1]
    targets = []
    if '--pattern' in args:
        pattern = args[args.index('--pattern') + 1]
        levels = args[args.index('--levels') + 1].split(',') if '--levels' in args else LEVELS
        targets = [(lv, pattern.replace('{lv}', lv)) for lv in levels]
    else:
        targets = [tuple(a.split('=', 1)) for a in args[2:] if '=' in a]
    print(f"parity run '{name}': {len(targets)} level(s)")
    rows = [run_level(root, lv, path, print) for lv, path in targets]
    run = {'name': name, 'date': time.strftime('%Y-%m-%d %H:%M'), 'levels': rows}
    json.dump(run, open(save_path(root, name), 'w'), indent=1)
    print()
    table(run)
    print(f"\nsaved: {save_path(root, name)}")
    return 0


if __name__ == '__main__':
    suite_paths.use('core')
    from bfbc2core import errors
    errors.setup('navparity', gui=False)       # a log; an unexpected error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
