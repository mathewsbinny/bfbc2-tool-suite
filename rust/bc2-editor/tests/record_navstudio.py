r"""Records what the Navigation Studio showed for a level, for the map editor's AI workspace test (nav_parity.rs), before
the Studio was retired (round GR2). Digests only - no game content: each list's length and the SHA-1 of its compact
JSON (separators , and :), floats written as fixed-point text.

Per level, for each path-data file the Studio opened (NavKit's newest build and DICE's original, studio.default_files):
navdata.summary, the areas (sector, index, tile, layer, pitch, heights, cells, island, ladder, boundary pieces), the
cells (grid i, j, area; height in cm), the route graph's islands, the ground's colour classes in each of the Studio's
colourings (island, sector, pitch, difference, anomaly: Studio.cell_colors, each colour numbered by first appearance),
the difference figures, the soldier spawns' islands (Studio.spawn_islands), the islands' distances from the main network
(Studio.island_gaps), the anomaly overlay (overlay.anomalies on the level's own AI layer) and path tests between soldier
spawns (navpath.route). Then the compare overlays (overlay.reference_report): ons_mp_004 against level:ons_mp_002, mp_002
against level:ons_mp_002 (no path data of its own), sp_009 against level:sp_01 (its patrols), and the WW2 level guadal_canal's own AI layer (its partition in the
editor's Package copy, written as XML in a scratch folder) against nothing.

  python rust\bc2-editor\tests\record_navstudio.py [level ...]     default: sp_01 ons_mp_004 ons_mp_002 sp_009

Read only. Needs the suite's Rust core module, the game copy and the levels prepared.
"""
import gzip
import hashlib
import json
import os
import sys
import tempfile
from types import SimpleNamespace
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools')
sys.path.insert(0, os.path.join(SUITE, 'bfbc2-navigation-studio'))
import overlay  # noqa: E402
import studio  # noqa: E402
from bfbc2core import dbx, navdata, navpath  # noqa: E402
from bfbc2core import level as levelmod  # noqa: E402

DEFAULT = ['sp_01', 'ons_mp_004', 'ons_mp_002', 'sp_009']
COMPARES = [('ons_mp_004', 'level:ons_mp_002'), ('mp_002', 'level:ons_mp_002'), ('sp_009', 'level:sp_01')]
WW2 = 'guadal_canal'


def f3(v: float) -> str:
    return '%.3f' % float(v)


def f2(v: float) -> str:
    return '%.2f' % float(v)


def digest(v: Any, count: int | None = None) -> list[Any]:
    text = json.dumps(v, separators=(',', ':'), ensure_ascii=False)
    return [len(v) if count is None else count, hashlib.sha1(text.encode('utf-8')).hexdigest()]


def plain(v: Any) -> Any:
    """Floats as 3-decimal text, recursively (the digests' rule)."""
    if isinstance(v, bool) or v is None or isinstance(v, (int, str)):
        return v
    if isinstance(v, (float, np.floating)):
        return f3(v)
    if isinstance(v, (np.integer,)):
        return int(v)
    if isinstance(v, dict):
        return {k: plain(x) for k, x in v.items()}
    if isinstance(v, (list, tuple, np.ndarray)):
        return [plain(x) for x in v]
    return str(v)


def classes(colours: np.ndarray) -> list[int]:
    """Each cell's colour numbered by first appearance."""
    seen: dict[tuple[float, ...], int] = {}
    out = []
    for c in map(tuple, np.asarray(colours, np.float64).round(6).tolist()):
        out.append(seen.setdefault(c, len(seen)))
    return out


def record_file(st: Any, name: str, nav: navdata.NavData) -> dict[str, Any]:
    ij = np.round(nav.cell_xz / navdata.GRID).astype(np.int64)
    out: dict[str, Any] = {'file': os.path.basename(nav.path), 'summary': navdata.summary(nav)}
    out['areas'] = digest([[a.sector, a.index, a.tile[0], a.tile[1], a.layer, '%.4f' % a.pitch, f3(a.min_y), f3(a.max_y), a.cells,
                            a.island, int(a.ladder), len(a.outline), len(a.portals), len(a.borders)] for a in nav.areas])
    out['cells'] = digest([[int(i), int(j), int(k)] for (i, j), k in zip(ij.tolist(), nav.cell_area.tolist())])
    out['cell_y'] = digest([int(v) for v in np.round(np.asarray(nav.cell_y, np.float64) * 100.0).astype(np.int64).tolist()])
    out['node_island'] = digest([int(v) for v in nav.node_island.tolist()])
    out['islands'] = digest([[i.nodes, i.cells, f2(i.centre[0]), f2(i.centre[1]), f2(i.centre[2])] for i in nav.islands])
    out['colours'] = {}
    for m in studio.COLOR_MODES:
        st.color = m
        out['colours'][m] = digest(classes(st.cell_colors(name)))
    out['spawns'] = digest(sorted([[e.guid, isl, None if d is None else f2(d)] for e, isl, d in st.spawn_islands(nav)]))
    out['gaps'] = digest([f2(g) for g in studio.Studio.island_gaps(st, name)])
    return out


def path_tests(st: Any, files: dict[str, navdata.NavData]) -> list[dict[str, Any]]:
    sp = sorted(st.spawns, key=lambda e: e.guid)
    pairs = []
    if len(sp) >= 2:
        pairs += [(sp[0].position, sp[-1].position), (sp[1].position, sp[len(sp) // 2].position)]
    nav = files.get('NavKit') or files.get('DICE')
    if nav is not None and len(nav.islands) > 1:
        off = np.array([0, navdata.Y_OFFSET, 0])
        pairs.append((nav.islands[1].centre - off, nav.islands[0].centre - off))
    out = []
    for a, b in pairs:
        row: dict[str, Any] = {'a': [float(v) for v in a], 'b': [float(v) for v in b], 'routes': {}}
        for name, f in files.items():
            r = navpath.route(f, a, b)
            row['routes'][name] = {'found': r.found, 'nodes': digest(list(map(int, r.nodes))), 'cost': f3(r.cost),
                                   'length': f3(r.length), 'straight': f3(r.straight), 'start_node': r.start_node,
                                   'end_node': r.end_node, 'start_join': f2(min(r.start_join, 1e9)),
                                   'end_join': f2(min(r.end_join, 1e9)), 'start_island': r.start_island,
                                   'end_island': r.end_island, 'reason': r.reason,
                                   'gap': None if r.gap is None else [r.gap[0], r.gap[1], f2(r.gap[2])]}
        change = studio.route_cost_change({k: navpath.route(f, a, b) for k, f in files.items()})
        row['change'] = None if change is None else f2(change)
        out.append(row)
    return out


def record(lv: str) -> dict[str, Any]:
    lvl = levelmod.load(lv, log=lambda *_: None)
    cand, ref = studio.default_files(lvl.opfor, lvl.lv)
    files = {}
    for name, p in (('NavKit', cand), ('DICE', ref)):
        if p and os.path.isfile(p):
            files[name] = navdata.load(p, lvl.heightfield, None)
    spawns = [e for e in lvl.entities if e.category == 'soldier spawn' and e.position is not None]
    st = SimpleNamespace(files=files, color='island', same={}, spawns=spawns)
    st.spawn_islands = lambda nav: studio.Studio.spawn_islands(st, nav)
    st.cell_colors = lambda name: studio.Studio.cell_colors(st, name)
    out: dict[str, Any] = {'level': lvl.name, 'spawns': len(spawns), 'files': {}}
    diff = None
    if len(files) == 2:
        same_c, same_r = navdata.compare_cells(files['NavKit'], files['DICE'])
        st.same = {'NavKit': same_c, 'DICE': same_r}
        lad = studio.Studio._ladder_cells(files['NavKit'])
        diff = {'reproduced': f3(100.0 * same_r.mean() if len(same_r) else 0.0),
                'extra': f3(100.0 * (~same_c & ~lad).sum() / max(len(same_r), 1)),
                'ladder': f3(100.0 * lad.sum() / max(len(same_r), 1)),
                'same_navkit': digest([int(v) for v in same_c.tolist()]), 'same_dice': digest([int(v) for v in same_r.tolist()])}
    out['difference'] = diff
    for name, nav in files.items():
        out['files'][name] = record_file(st, name, nav)
    if 'NavKit' in files:
        _l, xml, _h, _o = overlay.reference_source(f'level:{lvl.lv}', lvl.opfor)
        layer = overlay.read_ai_layer(xml) if xml else {'spawners': [], 'area_orders': [], 'patrols': []}
        an = overlay.anomalies(files['NavKit'], layer)
        out['anomalies'] = {k: digest(plain(v)) for k, v in an.items()}
    out['paths'] = path_tests(st, files)
    return out


def record_compare(target: str, spec: str, opfor: str, candidate: str | None) -> dict[str, Any]:
    rep = overlay.reference_report(target, spec, opfor, candidate)
    layer = overlay.read_ai_layer(rep['reference_xml'])
    return {'target': target, 'source': spec, 'candidate': os.path.basename(candidate) if candidate else None,
            'has_ai': rep['reference_has_ai'], 'sector_offsets': rep['reference_sector_offsets'],
            'summary': rep['summary'], 'rows': {k: digest(plain(rep[k])) for k in ('spawners', 'area_orders', 'patrol_points', 'objectives')},
            'patrols': digest(plain(layer['patrols']))}


def main(argv: list[str]) -> int:
    out_dir = os.path.join(HERE, 'data')
    os.makedirs(out_dir, exist_ok=True)
    result: dict[str, Any] = {'levels': {}, 'compares': [], 'ww2': None}
    for lv in argv[1:] or DEFAULT:
        r = record(lv)
        result['levels'][lv] = r
        print(f'{lv}: ' + ', '.join(f'{n} {f["summary"]["cells"]:,} cells' for n, f in r['files'].items()), flush=True)
    from bfbc2core import paths
    opfor = paths.find_opfor(None, start=SUITE)
    for target, spec in COMPARES:
        cand, _ref = studio.default_files(opfor, target)
        result['compares'].append(record_compare(target, spec, opfor, cand))
        print(f'compare {target} with {spec}: {result["compares"][-1]["summary"]}', flush=True)
    part = os.path.join(opfor, 'work', 'editor', 'levels', WW2, '00_' + WW2, 'levels', WW2 + '.dbx')
    if os.path.isfile(part):
        with tempfile.TemporaryDirectory(dir=os.environ.get('GR2_SCRATCH')) as tmp:
            xml = os.path.join(tmp, WW2 + '.xml')
            with open(xml, 'w', encoding='utf-8') as fh:
                fh.write(dbx.to_xml(open(part, 'rb').read()))
            r = record_compare(WW2, xml, opfor, None)
            r['source'] = 'partition'
            result['ww2'] = r
            print(f'compare {WW2} (its own layer): {r["summary"]}', flush=True)
    path = os.path.join(out_dir, 'navstudio.json.gz')
    with gzip.GzipFile(path, 'wb', mtime=0) as fh:
        fh.write(json.dumps(result, separators=(',', ':'), indent=None).encode('utf-8'))
    print('->', os.path.relpath(path, SUITE))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
