r"""Round O5's stockmp group: the full pipeline of bfbc2-stockmp-ai\stockmp.py - generate (the bots placed on the path
data), build (authored, packed, staged), validate (the offline checks) and all - through test_cli_o5.py.

cases(): the quick tier's command lines on synthetic inputs made in the case folder (no game data): a fake Project
OPFOR folder whose game copy holds a Conquest and a Rush level built with test_stockmp's DBX builders, mp_common's
kits and path-finding settings, sp_common's AI template, the donor SP_004_B with its SectorOffsets assets, and path
data made here (route nodes on a grid over the level, one area under them), each bundle made with
bfbc2core.fbrb.build. The variants: Conquest and Rush, --bots, --keep-stock-spawners, --minimal, a variant, the
Package copy beside the Dist one, a level that loads sp_common (nothing packed), sparse path data (candidates dropped,
spawn points moved, too few found), two route groups (a small island), and the errors (no donor, no path data, a
configuration with problems, not JSON, no build report, ...).
opfor_calls(opfor): the --game tier's: `generate` (full) of real levels, the game copy read in place, the unpacks and
the configuration written to the case folder's work folder.
game_check(check, exe, ref): generate + build + validate (`all`) of real levels in a stand-in Project OPFOR folder
(its game\ a junction to the real game copy, its own work\ and local app data), the Python's command line, then
bfbc2.exe's on the same path: every file under work\ by SHA-256, both streams, the exit codes and the log records
the same. The junction is removed with rmdir before the folder.
"""
from __future__ import annotations

import hashlib
import json
import os
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import cli_o3_stockmp as o3s  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

VARIANTS = o3s.VARIANTS
PATHFINDING = ('ai/pathfinding/infantry/3096da77-3146-479c-ae35-c822273a54a7',
               'ai/pathfinding/vehicles/de958285-789b-4273-8a8d-a35e95c2612e')
TEMPLATE_GUID = '2eedbef1-52de-4b20-819c-e71097c15cbe'
TEMPLATE_PARTS = ('Objects/Soldiers/Templates/SpawnTemplate_Enemy_Grunt.dbx',
                  'Objects/Soldiers/Templates/AI/Enemy_Grunt_Sense.dbx',
                  'Objects/Soldiers/Templates/AI/Enemy_Grunt_Bhv.dbx')


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


# ------------------------------------------------------------------------------------------- synthetic inputs

def _navkit() -> tuple[Any, Any]:
    import cli_o2_nav
    return cli_o2_nav._navkit()


def _f32(x: float) -> float:
    return float(struct.unpack('<f', struct.pack('<f', x))[0])


def path_data(regions: tuple[tuple[float, float, float, float], ...] = ((-8, 212, -8, 62),), step: float = 3.0,
              area: tuple[float, float, float, float] | None = (-10, 215, -10, 65), islands: int = 0,
              sector: int = 0x0505, seed: int = 0) -> bytes:
    """Path data over the synthetic level: route nodes 0.95 m above the ground (y 0) on a grid in each region
    (x0, x1, z0, z1), each region one chain of edges (one route group each), and one area under them (area: its
    outline's x0, x1, z0, z1; None: no area). islands: that many more two-node groups far off."""
    hpdtool, kymesh = _navkit()
    nodes: list[Any] = []
    edges: list[Any] = []
    nid = 1000 + seed
    for x0, x1, z0, z1 in regions:
        first = len(nodes)
        x = x0
        while x <= x1:
            z = z0
            while z <= z1:
                nodes.append(hpdtool.Node(nid, (_f32(x), _f32(0.95), _f32(z)), 0, 1.0, 0, 0, 0))
                nid += 1
                z += step
            x += step
        for k in range(first, len(nodes) - 1):
            edges.append(hpdtool.Edge(0, k, k + 1))
    for i in range(islands):
        a = len(nodes)
        for d in (0.0, 2.0):
            nodes.append(hpdtool.Node(nid, (_f32(-300.0 - 40 * i + d), _f32(0.95), _f32(-300.0)), 0, 1.0, 0, 0, 0))
            nid += 1
        edges.append(hpdtool.Edge(0, a, a + 1))
    areas = []
    if area is not None:
        x0, x1, z0, z1 = area
        a = kymesh.Area(sector, 0, 0, 0, 0.4, 0.8, 1.0, -1.0)
        ring = [(_f32(x0), 0.95, _f32(z0)), (_f32(x1), 0.95, _f32(z0)), (_f32(x1), 0.95, _f32(z1)),
                (_f32(x0), 0.95, _f32(z1)), (_f32(x0), 0.95, _f32(z0))]
        a.outline = [ring]
        areas.append(a)
    tiles = [[kymesh.TileItem(0, -1.0, 1.0)] if areas else []]
    mesh = kymesh.Mesh(2, 0, 0, sector, 0.2, 0.8, (-100.0, 300.0, -100.0, 200.0), 1, 1, 0.4, tiles, areas, [],
                       b'\0\0\0\0')
    sec = hpdtool.Sector(sector, (-100.0, 300.0, -1.0, 2.0, -100.0, 200.0), 0, 100007, 0, nodes, edges,
                         kymesh.write_mesh_reference(mesh))
    return bytes(hpdtool.write_reference(hpdtool.HpdFile(tuple([0] * hpdtool.HEADER_WORDS), [sec])))


def _partition(ts: Any, part_guid: str, guids: list[str], typ: str = 'Test.ObjectData') -> bytes:
    b = ts.B()
    insts = [b.inst(g, typ, [b.text('Name', 'x')]) for g in guids]
    return bytes(b.done(part_guid, insts))


def _sector_asset(ts: Any, kind: str, n: int) -> bytes:
    b = ts.B()
    so = b.inst(ts.G(810 + n), 'GameSharedResources.PathdataSectorOffsets', [
        b.text('Name', f'Levels/SP_004_B/Precomputed/{kind}/SectorOffsets'),
        b.text('MemoryReportGroup', 'MemoryReportGroup_default'), b.num('XOffset', [-2], 'i'), b.num('ZOffset', [-5], 'i')])
    return bytes(b.done(ts.G(820 + n), [so]))


def _mp_common(ts: Any) -> bytes:
    """mp_common: the eight kits (cli_o3_stockmp) and the two PathfindingSettings partitions."""
    entries: list[tuple[str, str, bytes]] = []
    n = 0
    entries.append(('Levels/MP_Common.dbx', '<non-resource>', b'{binary}'))
    for fac in ('US', 'RU'):
        for kit in ('Assault', 'Demolition', 'Support', 'Recon'):
            n += 1
            b = ts.B()
            insts = [b.inst(ts.G(700 + n), 'GameSharedResources.SoldierAsset', [b.text('Name', f'{fac} {kit}')]),
                     b.inst(ts.G(780 + n), 'GameSharedResources.SoldierKit', [b.text('Name', 'kit')])]
            entries.append((f'Objects/Soldiers/{fac}/{kit}.dbx', '<non-resource>', b.done(ts.G(800 + n), insts)))
    for i, ref in enumerate(PATHFINDING):
        part, guid = ref.rsplit('/', 1)
        entries.append((part.replace('ai/pathfinding/infantry', 'AI/Pathfinding/Infantry')
                        .replace('ai/pathfinding/vehicles', 'AI/Pathfinding/Vehicles') + '.dbx', '<non-resource>',
                        _partition(ts, ts.G(860 + i), [guid], 'GameSharedResources.PathfindingSettings')))
    return o3s._bundle(entries)


def _sp_common(ts: Any) -> bytes:
    entries = [('Levels/SP_Common.dbx', '<non-resource>', b'{binary}')]
    for i, name in enumerate(TEMPLATE_PARTS):
        guids = [TEMPLATE_GUID] if i == 0 else [ts.G(870 + i)]
        entries.append((name, '<non-resource>', _partition(ts, ts.G(880 + i), guids, 'Entity.SpawnTemplateData')))
    return o3s._bundle(entries)


def _donor(ts: Any) -> bytes:
    return o3s._bundle([('Levels/SP_004_B.dbx', '<non-resource>', ts.synthetic_donor()),
                        ('Levels/SP_004_B/Precomputed/Infantry/SectorOffsets.dbx', '<non-resource>',
                         _sector_asset(ts, 'Infantry', 0)),
                        ('Levels/SP_004_B/Precomputed/Vehicle/SectorOffsets.dbx', '<non-resource>',
                         _sector_asset(ts, 'Vehicle', 1))])


_CACHE: dict[str, Any] = {}


def _parts() -> dict[str, Any]:
    if not _CACHE:
        ts = o3s._synthetic()
        _CACHE.update(ts=ts, conquest=ts.synthetic_level('conquest'), rush=ts.synthetic_level('rush'),
                      mp=_mp_common(ts), sp=_sp_common(ts), donor=_donor(ts), hpd=path_data(),
                      hpd_veh=path_data(seed=7))
    return _CACHE


def level_bundle(data: bytes, name: str) -> bytes:
    return o3s._bundle([(f'Levels/{name}.dbx', '<non-resource>', data),
                        (f'Levels/{name}/Precomputed/x.res', 'mesh', b'mesh bytes'),
                        (f'Levels/{name}/Gone.dbx', '*deleted*', b'')])


def game(lv: str = 'mp_005', mode: str = 'conquest', name: str = 'MP_005', pkg: bool = False, dist: bool = True,
         hpd: bytes | None = None, hpd_veh: bytes | None = None, pathdata: str | None = 'own', donor: bool = True,
         sp_common: bool = True, mft: str | None = None) -> dict[str, Any]:
    p = _parts()
    data = p['conquest'] if mode == 'conquest' else p['rush']
    g = 'opfor\\game\\'
    files: dict[str, Any] = {}
    if dist:
        files[g + f'Dist\\win32\\levels\\{lv}\\level-00.fbrb'] = level_bundle(data, name)
    if pkg:
        files[g + f'Package\\levels\\{lv}\\level-00.fbrb'] = level_bundle(data, name)
    folder = lv if pathdata == 'own' else pathdata
    if folder:
        files[g + f'Output\\win32\\ai\\levels\\{folder}\\infantry\\pathdata.hpd'] = hpd if hpd is not None else p['hpd']
        files[g + f'Output\\win32\\ai\\levels\\{folder}\\vehicle\\pathdata.hpd'] = \
            hpd_veh if hpd_veh is not None else p['hpd_veh']
    files[g + 'Dist\\win32\\levels\\mp_common\\level-00.fbrb'] = p['mp']
    if sp_common:
        files[g + 'Dist\\win32\\levels\\sp_common\\level-00.fbrb'] = p['sp']
    if donor:
        files[g + 'Dist\\win32\\levels\\sp_004_B\\level-00.fbrb'] = p['donor']
    if mft is not None:
        files[g + 'Dist\\win32\\package.mft'] = mft
    return files


def config_of(files: dict[str, Any], lv: str, change: Any) -> dict[str, Any]:
    """files plus a configuration at work\\<lv>\\config.json: change(the generated one) - made by running the Python's
    generate here would need the suite; instead a configuration written by hand in the generator's shape."""
    c = {'format': 'bfbc2-stockmp-ai', 'version': 1, 'lv': lv, 'level': 'Levels/TEST_01', 'mode': 'conquest',
         'layer': 'full', 'stock_spawners': 'human',
         'pathdata': {'infantry': '{d}\\opfor\\game\\Output\\win32\\ai\\levels\\' + lv + '\\infantry\\pathdata.hpd',
                      'vehicle': '{d}\\opfor\\game\\Output\\win32\\ai\\levels\\' + lv + '\\vehicle\\pathdata.hpd',
                      'source': "the level's own path data"},
         'template': 'objects/soldiers/templates/spawntemplate_enemy_grunt/' + TEMPLATE_GUID,
         'template_parts': list(TEMPLATE_PARTS),
         'teams': {'Team1': {'faction': 'FactionUS', 'kits': {k: f'objects/soldiers/us/{k}/x' for k in
                                                              ('assault', 'demolition', 'support', 'recon')}},
                   'Team2': {'faction': 'FactionRUS', 'kits': {k: f'objects/soldiers/ru/{k}/x' for k in
                                                               ('assault', 'demolition', 'support', 'recon')}}},
         'defaults': {'spawner': {'Amount': 0}, 'order': {'IsUrgent': True}, 'issue_on': ['spawn']},
         'spawners': [{'id': 't1_00', 'team': 'Team1', 'pos': [2.0, 0.3, 2.0], 'yaw': 10.0, 'kit': 'assault',
                       'stage': None},
                      {'id': 't1_01', 'team': 'Team1', 'pos': [5.0, 0.3, 2.0], 'yaw': 10.0, 'kit': 'recon',
                       'stage': None}],
         'orders': [{'id': 't1_flag_A', 'team': 'Team1', 'type': 'AreaMoveTo', 'objective': 'A',
                     'pos': [100.0, 0.0, 50.0], 'half_extents': [8.0, 3.0, 8.0], 'soldiers': ['t1_00', 't1_01'],
                     'stage': None, 'trigger_radius': 100.0}],
         'stages': [], 'notes': []}
    c = change(c)
    text = c if isinstance(c, str) else json.dumps(c, indent=1)
    return dict(files, **{f'opfor\\work\\stockmp\\{lv}\\config.json': (lambda d: text.replace('{d}', d.replace('\\', '\\\\')))
                          if '{d}' in text else text})


# ------------------------------------------------------------------------------------------- the cases

def pipeline_cases() -> list[Case]:
    rows: list[Case] = []

    def add(key: str, files: dict[str, Any], argv: list[str], env: dict[str, str] | None = None) -> None:
        rows.append((key, 'stockmp', files, argv + ['--opfor', '{d}\\opfor'], env or {}, False))

    g = game()
    r = game('mp_002', 'rush', 'MP_002')
    # generate: the full layer placed on the path data
    add('generate conquest', g, ['generate', 'mp_005'])
    add('generate conquest --bots 3', g, ['generate', 'mp_005', '--bots', '3'])
    add('generate conquest --bots 0', g, ['generate', 'mp_005', '--bots', '0'])
    add('generate conquest --keep-stock-spawners', g, ['generate', 'mp_005', '--keep-stock-spawners'])
    add('generate conquest variant', g, ['generate', 'mp_005', '--variant', 'exp1'])
    add('generate conquest same world path data', game(pathdata='ons_mp_005gr'), ['generate', 'mp_005'])
    add('generate rush', r, ['generate', 'mp_002'])
    add('generate rush --bots 2', r, ['generate', 'mp_002', '--bots', '2'])
    add('generate conquest sparse path data (candidates dropped, moved, too few)',
        game(hpd=path_data(((-4, 8, -4, 8), (96, 104, 46, 54)), step=1.5, area=(-1, 1, -1, 1))),
        ['generate', 'mp_005', '--bots', '6'])
    add('generate conquest without an area (moved to route nodes)', game(hpd=path_data(area=None)),
        ['generate', 'mp_005'])
    add('generate conquest small islands', game(hpd=path_data(((-8, 60, -8, 62), (150, 212, -8, 62)), step=4.0)),
        ['generate', 'mp_005'])
    add('generate conquest far islands only', game(hpd=path_data(((-8, 8, -8, 8),), islands=3, area=None)),
        ['generate', 'mp_005'])
    add('generate rush sparse', game('mp_002', 'rush', 'MP_002', hpd=path_data(((-6, 60, -6, 20),), step=2.0)),
        ['generate', 'mp_002'])
    add('generate the package copy read', game(pkg=True), ['generate', 'mp_005'])
    add('generate no level bundle', game(dist=False), ['generate', 'mp_005'])
    add('generate path data damaged', game(hpd=b'junk' * 50), ['generate', 'mp_005'])
    # all: generate, build, validate
    add('all conquest', g, ['all', 'mp_005'])
    add('all conquest --bots 2', g, ['all', 'mp_005', '--bots', '2'])
    add('all rush', r, ['all', 'mp_002'])
    add('all minimal', g, ['all', 'mp_005', '--minimal'])
    add('all minimal variant exp0', g, ['all', 'mp_005', '--minimal', '--variant', 'exp0'])
    add('all dist and package', game(pkg=True), ['all', 'mp_005'])
    add('all package only', game(pkg=True, dist=False), ['all', 'mp_005'])
    add('all level loads sp_common (nothing packed)',
        game(mft='BundleSource game:levels/mp_005/level-00.fbrb\nBundleTarget game:levels/sp_common/level-00.fbrb\n'),
        ['all', 'mp_005'])
    add('all package.mft without sp_common',
        game(mft='BundleSource game:levels/mp_005/level-00.fbrb\nBundleTarget game:levels/mp_common/level-00.fbrb\n'),
        ['all', 'mp_005'])
    add('all no donor', game(donor=False), ['all', 'mp_005'])
    add('all no sp_common', game(sp_common=False), ['all', 'mp_005'])
    add('all sparse path data', game(hpd=path_data(((-4, 8, -4, 8), (96, 104, 46, 54)), step=1.5, area=(-1, 1, -1, 1))),
        ['all', 'mp_005', '--bots', '4'])
    add('all no path data', game(pathdata=None), ['all', 'mp_005'])
    add('all PYTHONUNBUFFERED=1', g, ['all', 'mp_005'], {'PYTHONUNBUFFERED': '1'})
    add('all PYTHONUTF8=1', r, ['all', 'mp_002'], {'PYTHONUTF8': '1'})
    # build and validate on a given configuration
    add('build a hand-written configuration', config_of(g, 'mp_005', lambda c: c), ['build', 'mp_005'])
    add('build --config elsewhere', dict(config_of(g, 'mp_005', lambda c: c),
                                         **{'mine.json': o3s_text(config_of(g, 'mp_005', lambda c: c))}),
        ['build', 'mp_005', '--config', '{d}\\mine.json'])
    add('build configuration with problems', config_of(g, 'mp_005', lambda c: dict(c, orders=[
        dict(c['orders'][0], soldiers=['t1_00', 'nobody'])])), ['build', 'mp_005'])
    add('build configuration not ours', config_of(g, 'mp_005', lambda c: dict(c, format='x')), ['build', 'mp_005'])
    add('build configuration missing keys', config_of(g, 'mp_005', lambda c: {'format': 'bfbc2-stockmp-ai',
                                                                               'version': 1}), ['build', 'mp_005'])
    add('build configuration not JSON', config_of(g, 'mp_005', lambda c: '{not json'), ['build', 'mp_005'])
    add('build no configuration', g, ['build', 'mp_005'])
    add('build missing path data', config_of(g, 'mp_005', lambda c: dict(c, pathdata={'infantry': 'nowhere.hpd'})),
        ['build', 'mp_005'])
    add('build path data null', config_of(g, 'mp_005', lambda c: dict(c, pathdata=None)), ['build', 'mp_005'])
    add('build stage ids', config_of(g, 'mp_005', lambda c: dict(c, spawners=[dict(s, stage=1) for s in c['spawners']])),
        ['build', 'mp_005'])
    add('build duplicate ids and an unknown kit', config_of(g, 'mp_005', lambda c: dict(c, spawners=[
        dict(c['spawners'][0], kit='pilot'), c['spawners'][0]])), ['build', 'mp_005'])
    add('build an order of another type', config_of(g, 'mp_005', lambda c: dict(c, orders=[
        dict(c['orders'][0], type='AreaPatrol')])), ['build', 'mp_005'])
    add('build a stage without its source', config_of(g, 'mp_005', lambda c: dict(c, stages=[{'index': 0}])),
        ['build', 'mp_005'])
    add('build a version 1.0 configuration', config_of(g, 'mp_005', lambda c: dict(c, version=1.0)), ['build', 'mp_005'])
    add('validate no build report', config_of(g, 'mp_005', lambda c: c), ['validate', 'mp_005'])
    add('validate no configuration', g, ['validate', 'mp_005'])
    return rows


def o3s_text(files: dict[str, Any]) -> Any:
    return next(v for k, v in files.items() if k.endswith('config.json'))


def cases() -> list[Case]:
    rows = pipeline_cases()
    picked = ['generate conquest', 'all conquest', 'build configuration with problems']
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows if k in picked or k.startswith('all ')]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in rows if k in picked]
    return out


# ------------------------------------------------------------------------------------------- the game (--game)

LEVELS: tuple[tuple[str, list[str]], ...] = (('mp_005', ['--bots', '8']), ('mp_002', []), ('mp_008cq', []), ('mp_004', []))


def opfor_calls(opfor: str) -> list[Call]:
    """`generate` (the full layer) of real levels: the game copy and OPFOR's prepared common unpacks read in place,
    the level's unpack and the configuration written under the case folder's work folder (--work)."""
    out: list[Call] = []
    real = ['--opfor', opfor, '--work', '{d}\\work']
    for lv, more in LEVELS:
        if os.path.isdir(os.path.join(opfor, 'game', 'Dist', 'win32', 'levels', lv)):
            out.append((f'generate {lv} {" ".join(more)} (the real game)', 'stockmp', ['generate', lv, *more, *real],
                        {}, {}, True))
    return out


def _tree(top: str) -> dict[str, str]:
    out = {}
    for d, _ds, fs in os.walk(top):
        for f in fs:
            p = os.path.join(d, f)
            h = hashlib.sha256()
            with open(p, 'rb') as fh:
                while b := fh.read(1 << 20):
                    h.update(b)
            out[os.path.relpath(p, top)] = h.hexdigest()
    return out


def game_check(check: Any, exe: str, ref: str) -> None:
    """generate + build + validate (`all`) of real levels in a stand-in OPFOR folder (module docstring)."""
    import re
    import shutil
    import subprocess
    import time

    import test_cli_o2 as o2
    try:
        opfor: str | None = o2.paths.find_opfor()
    except o2.paths.PathsError:
        opfor = None
    if not opfor:
        print('     (no Project OPFOR BFBC2 copy: the stockmp builds skipped)')
        return
    root = os.path.abspath(os.environ.get('BFBC2_O5_STANDIN') or os.path.join(os.environ.get('TEMP', '.'), 'o5s'))
    if os.path.exists(root):
        check(False, f'stockmp builds: the stand-in folder {root} exists already (remove it)')
        return
    script = os.path.join(ref, 'bfbc2-stockmp-ai', 'stockmp.py')
    retired = 'stockmp' in o2.RETIRED or not os.path.isfile(script)
    levels = [x for x in (os.environ.get('BFBC2_O5_LEVELS') or 'mp_005,mp_002,mp_008cq').split(',') if x]
    stand = os.path.join(root, 'opfor')
    lad = os.path.join(root, 'lad')
    try:
        os.makedirs(stand)
        r = subprocess.run(['cmd', '/c', 'mklink', '/J', os.path.join(stand, 'game'), os.path.join(opfor, 'game')],
                           capture_output=True, text=True)
        if r.returncode:
            check(False, 'stockmp builds: the game junction', r.stdout + r.stderr)
            return
        for lv in levels:
            got: dict[str, Any] = {}
            # round RT3 retired stockmp.py: bfbc2.exe alone then, its run must succeed and be its own
            sides = [('rust', [exe, 'stockmp'])] if retired else [('python', [sys.executable, script]),
                                                                   ('rust', [exe, 'stockmp'])]
            for side, cmd in sides:
                work = os.path.join(stand, 'work')
                shutil.rmtree(work, ignore_errors=True)
                shutil.rmtree(lad, ignore_errors=True)
                os.makedirs(lad)
                env = {k: v for k, v in os.environ.items() if k.upper() not in o2.PY_ENV + o2.O3_ISOLATE}
                env.update(LOCALAPPDATA=lad, BFBC2_CLI_SUITE=ref, BFBC2_CLI_TRACE=os.path.join(root, 'trace.txt'))
                t = time.perf_counter()
                p = subprocess.run(cmd + ['all', lv, '--opfor', stand], capture_output=True, env=env, cwd=ref)
                took = time.perf_counter() - t
                logs = os.path.join(lad, 'bfbc2-tool-suite', 'logs')
                records = []
                for n in sorted(os.listdir(logs)) if os.path.isdir(logs) else []:
                    if n.endswith('.log'):
                        with open(os.path.join(logs, n), 'rb') as f:
                            rows = re.findall(rb'(?m)^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d,\d{3} ([^\r\n]*)', f.read())
                        rows = [re.sub(rb' started \((Python [^,]*|bfbc2\.exe), win32\) ---$', b' started ---', x)
                                for x in rows]
                        rows = [re.sub(rb'stockmp_validate_[a-z0-9_]+', b'stockmp_validate_N', x) for x in rows
                                if not (x.startswith(b'WARNING ') and b'ResourceWarning: unclosed file' in x)]
                        records.append((re.sub(r'\d{4}-\d\d-\d\d', '<date>', n), rows))
                files = _tree(work)
                got[side] = {'code': p.returncode, 'out': p.stdout, 'err': p.stderr, 'files': files, 'log': records,
                             'took': took}
                print(f'     stockmp all {lv}: {side} {took:.1f} s, exit {p.returncode}, {len(files)} files under '
                      'work\\', flush=True)
            b = got['rust']
            a = got.get('python', b)
            why = ''
            for k in ('code', 'out', 'err', 'log'):
                if a[k] != b[k]:
                    if k == 'log' and len(a[k]) == len(b[k]):
                        for (na, ra), (nb, rb) in zip(a[k], b[k]):
                            pairs = [(x, y) for x, y in zip(ra, rb) if x != y]
                            if na != nb or len(ra) != len(rb) or pairs:
                                why = f'log {na} / {nb}: {len(ra)} / {len(rb)} records; first differing ' \
                                      f'{pairs[:1]!r}'[:900]
                                break
                    why = why or f'{k}: {str(a[k])[-300:]!r} / {str(b[k])[-300:]!r}'
                    break
            if not why and a['files'] != b['files']:
                diff = sorted(set(a['files'].items()) ^ set(b['files'].items()))
                why = f'files: {len(diff)} differ, e.g. {diff[:4]}'
            who = ''
            try:
                with open(os.path.join(root, 'trace.txt'), encoding='utf-8') as f:
                    who = f.read().split()[-1]
            except (OSError, IndexError):
                pass
            what = (f'stockmp all {lv} in a stand-in OPFOR folder: {len(b["files"])} files under work\\, built by '
                    f'bfbc2.exe alone (stockmp.py retired in round RT3; {b["took"]:.1f} s; answered by {who or "?"})'
                    if retired else
                    f'stockmp all {lv} in a stand-in OPFOR folder: {len(a["files"])} files under work\\ identical by '
                    f'SHA-256, both streams, exit codes and log records the same (Python {a["took"]:.1f} s, Rust '
                    f'{b["took"]:.1f} s; answered by {who or "?"})')
            check(not why and a['code'] == 0 and who == 'rust' and bool(b['files']), what,
                  why or f'exit {a["code"]}, who {who}')
            shutil.rmtree(os.path.join(stand, 'work'), ignore_errors=True)
    finally:
        junction = os.path.join(stand, 'game')
        if os.path.lexists(junction) or os.path.isdir(junction):
            subprocess.run(['cmd', '/c', 'rmdir', junction], capture_output=True)
        if os.path.exists(junction):
            print(f'     the junction {junction} could not be removed: {root} left in place')
        else:
            shutil.rmtree(root, ignore_errors=True)
