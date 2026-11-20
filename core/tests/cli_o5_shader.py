r"""Round O5's shader group: the command lines test_cli_o5.py replays for the shader tools - shaderdb.py (info, list,
textures, cover, needs, find, roundtrip, merge), renderqa.py and xdiff.py (their argument parsing and the errors before
any drawing; the drawing itself, on the GPU through shaderview.exe, is handed to the Python).

cases(): the quick tier's, on databases, bundles, unpacked level folders and a fake game copy built in the case folder
(test_shaderdb.build's made-up records, binary .dbx partitions written by bfbc2core.dbx; no game data).
opfor_calls(opfor): the --game tier's - Project OPFOR's console_shaders.py calls (`needs <unpacked level> -o <file>`,
`merge <base> <donors...> --need <file> -o <out>`) on the Onslaught levels: each level's installed Package bundle
unpacked into a scratch folder (read only by the calls), the donors read in place from the game copy (level:<name>),
every output in the case folder.
"""
from __future__ import annotations

import atexit
import os
import shutil
import sys
import tempfile
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}

T0 = 1_700_000_000
A = 'Objects/A/A_Mesh/Objects/A/Shaders/Metal'
A2 = 'Objects/A/A_Mesh/Objects/A/Shaders/MetalSnow'
B = 'Objects/B/B_Mesh/Objects/B/Shaders/Wood'
B2 = 'Objects/B/B_Mesh/Objects/B/Shaders/WoodSnow'
C = 'Objects/C/C_Mesh/Objects/C/Shaders/Glass'
D = 'Objects/Café/Café_Mesh/Objects/Café/Shaders/Tile\x81'      # latin-1, and a byte cp1252 has no character for
E = 'Shaders/Decals/Decal_1'


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    """rows, each again with stderr on stdout's pipe, and the picked ones under Python's stdio switches."""
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


def _tsd() -> Any:
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests'))
    import test_shaderdb as tsd
    return tsd


def database(dx: int, entries: list[tuple[str, int]], seed: int, shared: bytes = b'', big: bool = False,
             textures: dict[str, list[tuple[int, str]]] | None = None) -> Any:
    tsd = _tsd()
    db = tsd.build(dx, entries, seed, shared, big)
    if textures:
        db = tsd.with_textures(db, textures)
    return db


def _el(tag: str, kind: int, attrs: list[tuple[str, str]], text: str = '', children: list[Any] | None = None) -> Any:
    from bfbc2core import dbx
    return dbx.Element(tag, kind, attrs, text, b'', children)


def partition(guid: str, instances: list[Any]) -> bytes:
    from bfbc2core import dbx
    return bytes(dbx.write(dbx.DbxFile([], [_el('partition', dbx.CONTAINER, [('guid', guid)],
                                                 children=instances)])))


def mesh_asset(kind: str, name: str | None, extra: list[Any] | None = None) -> Any:
    from bfbc2core import dbx
    kids = [_el('field', dbx.TEXT, [('name', 'Name')], name)] if name is not None else []
    kids += extra or []
    return _el('instance', dbx.CONTAINER, [('guid', '11111111-0000-0000-0000-000000000001'), ('type', kind)],
               children=kids)


def ref(name: str, target: str) -> Any:
    from bfbc2core import dbx
    return _el('field', dbx.TEXT, [('name', name), ('ref', target)])


def level_folder(prefix: str, bad: bool = False) -> dict[str, Any]:
    """An unpacked level bundle: mesh assets (two kinds, one without a Name field), a level partition whose fields
    reference meshes inside and outside the folder, shader sets (unresolved), a Systems/Mesh reference (skipped),
    a LodSettings field (not a mesh field), items of an array, a deleted entry (empty file), a non-.dbx file."""
    from bfbc2core import dbx
    G = '22222222-0000-0000-0000-000000000002'
    items = _el('array', dbx.CONTAINER, [('name', 'Meshes3p')], children=[
        _el('item', dbx.TEXT, [('ref', 'objects/b/b_mesh/' + G)]),
        _el('item', dbx.TEXT, [('ref', 'Objects/Outside/Gun_Mesh/' + G)]),
        _el('item', dbx.TEXT, [('ref', 'null')])])
    comp = _el('complex', dbx.CONTAINER, [('name', 'WeaponData')], children=[
        ref('WeaponMesh', 'objects/a/a_mesh/' + G),
        ref('MeshShaderSet', 'Objects/ShaderSets/Snow_Set/' + G),
        ref('AnimatedMesh', 'Systems/Mesh/Default/' + G),
        ref('LodSettings', 'objects/c/c_mesh/' + G),
        ref('Mesh', 'NoSlash'),
        ref('Shader', 'objects/d/d_mesh/' + G),
        items])
    level = _el('instance', dbx.CONTAINER, [('guid', G), ('type', 'Entity.LevelData')], children=[
        ref('Mesh', 'Objects/A/A_Mesh/' + G),
        ref('MeshZoom1p', 'Objects/Props/Barrel_Mesh/' + G),
        ref('Mesh', 'Objects/Props/Barrel_Mesh/' + G),
        ref('Meshes1p', 'objects/blueprint/house/' + G),
        ref('Name', 'objects/x/y_mesh/' + G),
        comp])
    files: dict[str, Any] = {
        f'{prefix}/Objects/A/A_Mesh.dbx': partition(G, [mesh_asset('Render.RigidMeshAsset', 'Objects/A/A_Mesh')]),
        f'{prefix}/Objects/B/B_Mesh.dbx': partition(G, [mesh_asset('Render.SkinnedMeshAsset', 'Objects/B/B_MESH')]),
        f'{prefix}/Objects/Tree/Tree_Mesh.dbx': partition(G, [mesh_asset('Render.TreeMeshAsset', None)]),
        f'{prefix}/Objects/Café/Café_Mesh.dbx': partition(G, [mesh_asset('Render.CompositeMeshAsset', 'Objects/Café/Café_Mesh')]),
        f'{prefix}/Objects/ShaderSets/Snow_Set.dbx': partition(G, [mesh_asset('Render.MeshShaderSet', 'x')]),
        f'{prefix}/Levels/LV/LV.dbx': partition(G, [level]),
        f'{prefix}/Gone/Removed.dbx': b'',
        f'{prefix}/fbrb.manifest': '# a manifest\n',
        f'{prefix}/Levels/LV/notes.txt': 'not a partition',
    }
    if bad:
        files[f'{prefix}/Levels/LV/Broken.dbx'] = b'this is not a binary dbx file'
    return files


def _bundle(items: list[tuple[str, str, bytes]]) -> bytes:
    from bfbc2core import fbrb
    return bytes(fbrb.build(items))


_FILES: dict[str, Any] | None = None


def built() -> dict[str, Any]:
    """Every input the cases share (made once)."""
    global _FILES
    if _FILES is not None:
        return _FILES
    tex = {A: [(1, 'Textures/A/Metal_Nm'), (2, 'Textures/A/Metal_C'), (3, 'Textures/A/Metal_RGBM')],
           B: [(1, 'Textures/B/Wood_Df')], D: [(0, 'Textures/Café/Tile_Sp')]}
    base = database(9, [(A, 3), (D, 1), (E, 1)], 20, textures=tex)
    donor = database(9, [(A, 1), (B, 2), (C, 2)], 21, shared=b'', textures={B: [(1, 'Textures/B/Wood_Df')]})
    donor2 = database(9, [(B, 1), (B2, 2), (A2, 1)], 22, shared=b'other')
    dx10 = database(10, [(A, 2), (B, 1)], 23)
    dx10_donor = database(10, [(B, 2), (C, 1)], 24)
    xbox = database(9, [(A, 3), (B, 2)], 30, big=True, textures=tex)
    xbox_donor = database(9, [(B, 2)], 31, big=True)
    broken = database(9, [(A, 3), (B, 1)], 32)
    broken.entries[0] = ('Objects/A/A_Mesh/Objects/A/Shaders/Renamed', broken.entries[0][1])
    broken.programs.reverse()
    many = database(9, [(f'Objects/Many/M{k:03d}_Mesh/Objects/Many/Shaders/Long_Shader_Name_{k:03d}', 1)
                        for k in range(260)], 33)
    good = base.to_bytes()
    files: dict[str, Any] = {
        'base/Dx9_Single.res': good,
        'donor/Dx9_Single.res': donor.to_bytes(),
        'donor2/Dx9_Single.res': donor2.to_bytes(),
        'dx10/Dx10_Single.res': dx10.to_bytes(),
        'dx10/Dx11_Single.res': dx10.to_bytes(),
        'dx10donor/Dx10_Single.res': dx10_donor.to_bytes(),
        'xbox/Single.res': xbox.to_bytes(),
        'xboxdonor/Single.res': xbox_donor.to_bytes(),
        'broken/Dx9_Single.res': broken.to_bytes(),
        'many/Dx9_Single.res': many.to_bytes(),
        'plain.res': good,
        'junk.res': b'this is not a shader database',
        'short.res': b'\x78\0\0\0\0',
        'trunc/Dx9_Single.res': good[:-3],
        'padded/Dx9_Single.res': good + b'\0',
        'need.txt': '# needed meshes\nObjects/B/B_Mesh   # the wood\n\n/objects/c/c_mesh/\nOBJECTS/Café/Café_Mesh\n',
        'need_b.txt': 'objects/b/b_mesh\n',
        'need_empty.txt': '# nothing\n\n   \n',
        'need_bad.txt': b'objects/b/b_mesh\n\xff\xfe junk\n',
        'empty_folder/x.txt': 'no partitions',
        'bundle.fbrb': _bundle([('Levels/X/Shaders/Dx9_Single', 'Dx9ShaderDatabase', donor.to_bytes()),
                                ('Levels/X/Shaders/Dx10_Single', 'Dx10ShaderDatabase', dx10_donor.to_bytes()),
                                ('Levels/X/X.dbx', '<non-resource>', b'dbx')]),
        'twodb.fbrb': _bundle([('Levels/X/Shaders/Dx9_Single', 'Dx9ShaderDatabase', donor.to_bytes()),
                               ('Levels/Y/Shaders/Dx9_Single', 'Dx9ShaderDatabase', donor2.to_bytes())]),
        # a fake game copy: three levels' bundles (Package over Dist), one without databases
        'opfor/game/Package/levels/mp_001/level-00.fbrb': (_bundle([
            ('Levels/MP_001/Shaders/Dx9_Single', 'Dx9ShaderDatabase', donor.to_bytes()),
            ('Levels/MP_001/Shaders/Dx10_Single', 'Dx10ShaderDatabase', dx10_donor.to_bytes()),
            ('Levels/MP_001/Shaders/Dx11_Single', 'Dx11ShaderDatabase', dx10_donor.to_bytes()),
            ('Levels/MP_001/MP_001.dbx', '<non-resource>', b'dbx')]), T0),
        'opfor/game/Dist/win32/levels/mp_001/level-00.fbrb': (_bundle([
            ('Levels/MP_001/Shaders/Dx9_Single', 'Dx9ShaderDatabase', base.to_bytes())]), T0),
        'opfor/game/Dist/win32/levels/MP_002/level-00.fbrb': (_bundle([
            ('Levels/MP_002/Shaders/Dx9_Single', 'Dx9ShaderDatabase', donor2.to_bytes()),
            ('Levels/MP_001/Shaders/Dx9_Single', 'Dx9ShaderDatabase', donor.to_bytes())]), T0),
        'opfor/game/Package/levels/sp_009/level-00.fbrb': (_bundle([('Levels/SP_009/SP_009.dbx', '<non-resource>',
                                                                     b'dbx')]), T0),
    }
    files.update(level_folder('lvl'))
    files.update(level_folder('lvl_bad', bad=True))
    _FILES = files
    return files


def ref_beside_opfor() -> bool:
    """Whether the reference suite sits beside a Project OPFOR copy (shaderdb.find_opfor looks beside the suite): then
    the 'no game copy' cases would find it, and are left out."""
    base = sys.modules.get('test_cli_o2')
    ref_ = getattr(base, 'REF', SUITE)
    return os.path.isdir(os.path.join(os.path.dirname(ref_), 'project-opfor-bfbc2', 'game'))


ROOT = {'OPFOR_ROOT': '{d}\\opfor'}
NO_ROOT = {'OPFOR_ROOT': ''}


def cases() -> list[Case]:
    files = built()
    rows: list[Case] = []

    def add(key: str, tool: str, args: list[str], env: dict[str, str] | None = None) -> None:
        rows.append((key, tool, files, args, ROOT if env is None else env, False))

    def sdb(key: str, args: list[str], env: dict[str, str] | None = None) -> None:
        add('shaderdb ' + key, 'shaderdb', args, env)

    d = '{d}\\'
    # ---- argparse
    sdb('usage', [])
    sdb('-h', ['-h'])
    sdb('--help', ['--help'])
    sdb('info -h', ['info', '-h'])
    sdb('merge -h', ['merge', '-h'])
    sdb('unknown command', ['frobnicate', 'x'])
    sdb('info without a file', ['info'])
    sdb('info two files', ['info', d + 'plain.res', d + 'junk.res'])
    sdb('merge without --need', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res', '-o', d + 'o.res'])
    sdb('merge without donors', ['merge', d + 'base\\Dx9_Single.res', '--need', d + 'need.txt', '-o', d + 'o.res'])
    sdb('--dx 12', ['--dx', '12', 'info', d + 'plain.res'])
    sdb('--dx x', ['--dx', 'x', 'info', d + 'plain.res'])
    sdb('unknown option', ['info', d + 'plain.res', '--frob'])
    sdb('-o missing value', ['needs', d + 'lvl', '-o'])
    # ---- info
    for name in ('base\\Dx9_Single.res', 'donor\\Dx9_Single.res', 'dx10\\Dx10_Single.res', 'dx10\\Dx11_Single.res',
                 'xbox\\Single.res', 'plain.res', 'broken\\Dx9_Single.res', 'many\\Dx9_Single.res'):
        sdb(f'info {name}', ['info', d + name])
    sdb('info --dx 10 plain (a Dx9 file read as Dx10)', ['--dx', '10', 'info', d + 'plain.res'])
    sdb('info --dx 9 named Dx10', ['--dx', '9', 'info', d + 'dx10\\Dx10_Single.res'])
    sdb('info --dx 11 named Dx10', ['--dx', '11', 'info', d + 'dx10\\Dx10_Single.res'])
    sdb('info junk', ['info', d + 'junk.res'])
    sdb('info short', ['info', d + 'short.res'])
    sdb('info truncated', ['info', d + 'trunc\\Dx9_Single.res'])
    sdb('info padded', ['info', d + 'padded\\Dx9_Single.res'])
    sdb('info missing', ['info', d + 'nowhere.res'])
    sdb('info a folder', ['info', d + 'base'])
    sdb('info bundle', ['info', d + 'bundle.fbrb'])
    sdb('info bundle --dx 10', ['--dx', '10', 'info', d + 'bundle.fbrb'])
    sdb('info bundle:entry', ['info', d + 'bundle.fbrb:Levels/X/Shaders/Dx10_Single'])
    sdb('info bundle:folder', ['info', d + 'bundle.fbrb:levels/x'])
    sdb('info bundle:no such entry', ['info', d + 'bundle.fbrb:Levels/Q'])
    sdb('info bundle with two', ['info', d + 'twodb.fbrb'])
    sdb('info bundle with two, named', ['info', d + 'twodb.fbrb:Levels/Y/Shaders/Dx9_Single'])
    sdb('info missing bundle', ['info', d + 'nobundle.fbrb'])
    sdb('info missing bundle:entry', ['info', d + 'nobundle.fbrb:x'])
    sdb('info level:mp_001', ['info', 'level:mp_001'])
    sdb('info level:MP_001 --dx 11', ['--dx', '11', 'info', 'level:MP_001'])
    sdb('info level:mp_002 (Dist)', ['info', 'level:mp_002'])
    sdb('info level:sp_009 (no database)', ['info', 'level:sp_009'])
    sdb('info level:nowhere', ['info', 'level:nowhere'])
    sdb('info level --opfor', ['--opfor', d + 'opfor', 'info', 'level:mp_001'], NO_ROOT)
    sdb('info level --opfor wrong', ['--opfor', d + 'nowhere', 'info', 'level:mp_001'], NO_ROOT)
    if not ref_beside_opfor():
        sdb('info level, no game copy', ['info', 'level:mp_001'], NO_ROOT)
    # ---- list / textures
    sdb('list base', ['list', d + 'base\\Dx9_Single.res'])
    sdb('list donor filter', ['list', d + 'donor\\Dx9_Single.res', 'B_MESH'])
    sdb('list filter nothing', ['list', d + 'donor\\Dx9_Single.res', 'zzz'])
    sdb('list --parts', ['list', d + 'base\\Dx9_Single.res', '--parts'])
    sdb('list --parts filter', ['list', '--parts', d + 'donor\\Dx9_Single.res', 'b_mesh'])
    sdb('list xbox', ['list', d + 'xbox\\Single.res'])
    sdb('list dx10', ['list', d + 'dx10\\Dx10_Single.res'])
    sdb('list many (long output)', ['list', d + 'many\\Dx9_Single.res'])
    sdb('list broken', ['list', d + 'broken\\Dx9_Single.res'])
    sdb('textures base', ['textures', d + 'base\\Dx9_Single.res'])
    sdb('textures filter', ['textures', d + 'base\\Dx9_Single.res', 'a_mesh'])
    sdb('textures --constants', ['textures', d + 'base\\Dx9_Single.res', '--constants'])
    sdb('textures xbox --constants', ['textures', d + 'xbox\\Single.res', '--constants'])
    sdb('textures donor', ['textures', d + 'donor\\Dx9_Single.res'])
    sdb('textures many', ['textures', d + 'many\\Dx9_Single.res'])
    # ---- cover / needs
    sdb('cover', ['cover', d + 'base\\Dx9_Single.res', d + 'lvl'])
    sdb('cover --all', ['cover', d + 'donor\\Dx9_Single.res', d + 'lvl', '--all'])
    sdb('cover a broken partition', ['cover', d + 'base\\Dx9_Single.res', d + 'lvl_bad'])
    sdb('cover no folder', ['cover', d + 'base\\Dx9_Single.res', d + 'nowhere'])
    sdb('cover folder without partitions', ['cover', d + 'base\\Dx9_Single.res', d + 'empty_folder'])
    sdb('needs', ['needs', d + 'lvl'])
    sdb('needs -o', ['needs', d + 'lvl', '-o', d + 'needs.txt'])
    sdb('needs -o in a new folder', ['needs', d + 'lvl', '-o', d + 'new\\needs.txt'])
    sdb('needs no folder', ['needs', d + 'nowhere'])
    sdb('needs a file', ['needs', d + 'need.txt'])
    # ---- roundtrip
    sdb('roundtrip', ['roundtrip', d + 'base\\Dx9_Single.res', d + 'xbox\\Single.res', d + 'dx10\\Dx11_Single.res'])
    sdb('roundtrip broken', ['roundtrip', d + 'broken\\Dx9_Single.res', d + 'plain.res'])
    sdb('roundtrip junk', ['roundtrip', d + 'plain.res', d + 'junk.res'])
    sdb('roundtrip nothing', ['roundtrip'])
    sdb('roundtrip --levels', ['roundtrip', '--levels'])
    sdb('roundtrip --levels and a file', ['roundtrip', d + 'plain.res', '--levels'])
    sdb('roundtrip bundle', ['roundtrip', d + 'bundle.fbrb'])
    # ---- find
    sdb('find', ['find', d + 'need.txt'])
    sdb('find --base', ['find', d + 'need.txt', '--base', d + 'base\\Dx9_Single.res'])
    sdb('find --dx 10', ['--dx', '10', 'find', d + 'need.txt'])
    sdb('find from a level folder', ['find', d + 'lvl'])
    sdb('find nothing needed', ['find', d + 'need_empty.txt'])
    sdb('find missing need', ['find', d + 'nowhere.txt'])
    # ---- merge
    out = d + 'out\\Dx9_Single.res'
    sdb('merge', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res', '--need', d + 'need.txt', '-o', out])
    sdb('merge two donors', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res',
                             d + 'donor2\\Dx9_Single.res', '--need', d + 'need.txt', '-o', out])
    sdb('merge --all-names', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res',
                              d + 'donor2\\Dx9_Single.res', '--need', d + 'need.txt', '-o', out, '--all-names'])
    sdb('merge need from a folder', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res',
                                     '--need', d + 'lvl', '-o', d + 'merged.res'])
    sdb('merge need with bad bytes', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res',
                                      '--need', d + 'need_bad.txt', '-o', out])
    sdb('merge need empty', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res',
                             '--need', d + 'need_empty.txt', '-o', out])
    sdb('merge need missing', ['merge', d + 'base\\Dx9_Single.res', d + 'donor\\Dx9_Single.res',
                               '--need', d + 'nowhere.txt', '-o', out])
    sdb('merge Dx10', ['merge', d + 'dx10\\Dx10_Single.res', d + 'dx10donor\\Dx10_Single.res', '--need',
                       d + 'need.txt', '-o', d + 'out10\\Dx10_Single.res'])
    sdb('merge Dx11 base, Dx10 donor', ['merge', d + 'dx10\\Dx11_Single.res', d + 'dx10donor\\Dx10_Single.res',
                                        '--need', d + 'need.txt', '-o', d + 'out11\\Dx11_Single.res'])
    sdb('merge Dx9 base, Dx10 donor', ['merge', d + 'base\\Dx9_Single.res', d + 'dx10donor\\Dx10_Single.res',
                                       '--need', d + 'need.txt', '-o', out])
    sdb('merge xbox', ['merge', d + 'xbox\\Single.res', d + 'xboxdonor\\Single.res', '--need', d + 'need_b.txt',
                       '-o', d + 'outx\\Single.res'])
    sdb('merge xbox donor, PC base', ['merge', d + 'base\\Dx9_Single.res', d + 'xboxdonor\\Single.res', '--need',
                                      d + 'need_b.txt', '-o', out])
    sdb('merge level donors', ['merge', d + 'base\\Dx9_Single.res', 'level:mp_002', 'level:mp_001', '--need',
                               d + 'need.txt', '-o', out])
    sdb('merge bundle donor', ['merge', d + 'dx10\\Dx10_Single.res', d + 'bundle.fbrb', '--need', d + 'need.txt',
                               '-o', out])
    sdb('merge nothing to add', ['merge', d + 'base\\Dx9_Single.res', d + 'donor2\\Dx9_Single.res', '--need',
                                 d + 'need_b.txt', '-o', d + 'same.res'])
    sdb('merge donor missing', ['merge', d + 'base\\Dx9_Single.res', d + 'nowhere.res', '--need', d + 'need.txt',
                                '-o', out])
    sdb('merge junk donor', ['merge', d + 'base\\Dx9_Single.res', d + 'junk.res', '--need', d + 'need.txt', '-o', out])
    sdb('merge broken base', ['merge', d + 'broken\\Dx9_Single.res', d + 'donor\\Dx9_Single.res', '--need',
                              d + 'need.txt', '-o', out])
    sdb('merge many', ['merge', d + 'many\\Dx9_Single.res', d + 'donor\\Dx9_Single.res', d + 'donor2\\Dx9_Single.res',
                       '--need', d + 'lvl', '-o', out])

    # ---- renderqa: the argument parsing and the errors before any drawing (the drawing is handed off)
    def rqa(key: str, args: list[str], env: dict[str, str] | None = None) -> None:
        add('renderqa ' + key, 'renderqa', args, env)
    rqa('usage', [])
    rqa('-h', ['-h'])
    rqa('unknown command', ['paint'])
    rqa('render, no mesh', ['render', '-o', d + 'rq'])
    rqa('render, no --db', ['render', 'Objects/A/A_Mesh', '-o', d + 'rq'])
    rqa('compare, no --db', ['compare', 'Objects/A/A_Mesh', '-o', d + 'rq'])
    rqa('patches, no --db', ['patches', '-o', d + 'rq'])
    rqa('strips, no --db', ['strips', '-o', d + 'rq\\new\\deeper'])
    rqa('--stage missing', ['render', 'x', '--stage', d + 'nowhere', '-o', d + 'rq'])
    rqa('--list', ['render', '--list', d + 'need.txt', '-o', d + 'rq'])
    rqa('--list missing', ['render', '--list', d + 'nowhere.txt', '-o', d + 'rq'])
    rqa('--sun bad', ['render', 'x', '--sun', 'dusk'])
    rqa('--limit 0, no mesh', ['render', '--limit', '0', '-o', d + 'rq'])
    rqa('--limit x', ['render', 'x', '--limit', 'x'])
    rqa('unknown option', ['render', 'x', '--frob'])
    rqa('--db missing value', ['render', 'x', '--db'])
    if not ref_beside_opfor():
        rqa('validate, no game copy', ['validate', 'x', '-o', d + 'rq'], NO_ROOT)
        rqa('wake, no game copy', ['wake', '-o', d + 'rq'], NO_ROOT)
    rqa('validate, no mesh', ['validate', '-o', d + 'rq'])
    # handed to the Python: drawing (here a database that is not there, refused by the Python's loading)
    rqa('render a missing database', ['render', 'Objects/A/A_Mesh', '--db', d + 'nowhere.res', '-o', d + 'rq'])

    # ---- xdiff: the argument parsing, an unknown database, no BF1943 tool suite (each before any drawing)
    def xd(key: str, args: list[str], env: dict[str, str] | None = None) -> None:
        add('xdiff ' + key, 'xdiff', args, dict({'BF1943_SUITE_FBRB': '{d}\\nowhere', 'BF1943_XENON': '{d}\\nowhere'},
                                                 **(env or {})))
    xd('-h', ['-h'])
    xd('--n x', ['--n', 'x'])
    xd('unknown argument', ['extra'])
    xd('--db other', ['--db', 'mp_005', '-o', d + 'xd'])
    xd('no BF1943 tool suite', ['--ps', '1', '2', '-o', d + 'xd'])
    xd('no BF1943 tool suite, defaults', [])
    return with_variants(rows, ['shaderdb usage', 'shaderdb info base\\Dx9_Single.res', 'shaderdb list base',
                                'shaderdb list many (long output)', 'shaderdb info junk', 'shaderdb merge two donors',
                                'shaderdb textures base', 'renderqa -h', 'renderqa render, no mesh', 'xdiff -h'])


# ------------------------------------------------------------------------------------------- the game (--game)

ONSLAUGHT = {'ons_mp_002': 'ONS_MP_002', 'ons_mp_004': 'ONS_MP_004', 'ons_mp_005': 'ONS_MP_005',
             'ons_mp_008': 'ONS_MP_008'}
EXTRA = ['objects/gameplay/flag_neutral', 'objects/gameplay/flag_rus', 'objects/gameplay/flag_us',
         'objects/objects/mp_flagpole_mobile_01', 'objects/weapons/handheld/mec_shg_toz194',
         'objects/vehicles/land/bmd3', 'objects/vehicles/land/m3a3', 'objects/vehicles/land/cav',
         'objects/weapons/stationary/us_gmg_xm307']
GENERAL = ['level:mp_012cq', 'level:mp_001', 'level:sp_005_b', 'level:sp_04', 'level:sp_02', 'level:mp_012gr',
           'level:sp_005']
OWN = {'ons_mp_002': ['level:mp_002sr'], 'ons_mp_004': ['level:mp_004sdm'],
       'ons_mp_005': ['level:mp_005gr', 'level:mp_005sr'], 'ons_mp_008': ['level:mp_008cq']}


def opfor_calls(opfor: str) -> list[Call]:
    """console_shaders.build's calls on each Onslaught level: needs of the unpacked Package bundle, and merge of its
    Dx9 / Dx10 / Dx11 databases with the level's donors (its own other modes, then GENERAL; mp_008cq last but on
    ons_mp_008) and a needs file as build writes it (the needs and EXTRA; the spawned vehicles' meshes are left out:
    console_shader_audit's index is OPFOR's)."""
    suite_paths.use('core', 'shader-tools')
    import shaderdb
    from bfbc2core import fbrb
    scratch = tempfile.mkdtemp(prefix='o5shader_')
    atexit.register(shutil.rmtree, scratch, True)
    out: list[Call] = []
    env = {'OPFOR_ROOT': opfor}
    for lv, name in ONSLAUGHT.items():
        bundle = os.path.join(opfor, 'game', 'Package', 'levels', lv, 'level-00.fbrb')
        if not os.path.isfile(bundle):
            continue
        src = os.path.join(scratch, lv)
        if not fbrb.unpack_in_process(bundle, src):
            fbrb.unpack(bundle, src)
        meshes = shaderdb.level_meshes(src)
        needs = ''.join(f'{meshes[m][0]}\n' for m in sorted(meshes)) + '\n' + '\n'.join(EXTRA) + '\n'
        donors = OWN[lv] + [g for g in GENERAL if g not in OWN[lv]] + (['level:mp_008cq'] if lv != 'ons_mp_008' else [])
        out.append((f'needs {lv}', 'shaderdb', ['needs', src, '-o', '{d}\\needs.txt'], {}, env, True))
        out.append((f'needs {lv} (stdout)', 'shaderdb', ['needs', src], {}, env, True))
        for dx in ('Dx9', 'Dx10', 'Dx11'):
            base = os.path.join(src, 'Levels', name, 'Shaders', f'{dx}_Single.res')
            out.append((f'merge {lv} {dx}', 'shaderdb', ['merge', base, *donors, '--need', '{d}\\needs.txt', '-o',
                                                         f'{{d}}\\merged\\{dx}_Single.res'],
                        {'needs.txt': (needs.encode('utf-8'), T0)}, env, True))
        out.append((f'info {lv} Dx10', 'shaderdb', ['--dx', '10', 'info', f'level:{lv}'], {}, env, True))
        out.append((f'cover {lv}', 'shaderdb', ['cover', f'level:{lv}', src], {}, env, True))
    out.append(('roundtrip --levels', 'shaderdb', ['roundtrip', '--levels'], {}, env, True))
    out.append(('find the Onslaught extras', 'shaderdb', ['find', '{d}\\need.txt'],
                {'need.txt': ('\n'.join(EXTRA).encode('utf-8'), T0)}, env, True))
    return out
