r"""Round O3's meshview group: the command lines test_cli_o3.py replays for meshview.py (its `find`; drawing a mesh
and building a missing index are handed to the Python).

cases(): the quick tier's, on a fake game copy made in the case folder (dummy bundles, and the index's cache
work\tools_out\meshview_index.bin written as meshview.Index writes it, naming the case folder; no game data).
opfor_calls(opfor): the --game tier's, on Project OPFOR's game copy and index cache, read in place (only when that
cache is this game copy's: a stale one would be rebuilt into OPFOR's work\, so then the calls are left out).
"""
from __future__ import annotations

import os
import sys
from collections.abc import Callable
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


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    """rows, each again with stderr on stdout's pipe, and the picked ones under Python's stdio switches."""
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


def _meshview() -> Any:
    suite_paths.use('mesh-viewer')
    import meshview
    return meshview


# ------------------------------------------------------------------------------------------- the fake game copy

T0 = 1_700_000_000
# (path under the copy, modification time): bundles the index takes, and those it leaves out
BUNDLES = [
    ('game/Package/startup-00.fbrb', T0 + 1),
    ('game/Package/levels/mp_001/level-00.fbrb', T0 + 2),
    ('game/Package/levels/mp_001/Level-01.FBRB', T0 + 3),
    ('game/Package/levels/ons_mp_002/level-00.fbrb', T0 + 4),          # an Onslaught level: left out
    ('game/Package/levels/ons_mp_002/sub/x-00.fbrb', T0 + 5),
    ('game/Package/levels/wake/level-00.fbrb', T0 + 6),                # an OPFOR install's level: left out
    ('game/Package/levels/wake/vo/vo-00.fbrb', T0 + 7),
    ('game/Package/async/weapons-00.fbrb', T0 + 8),
    ('game/Package/async/Zeta-00.fbrb', T0 + 9),
    ('game/Package/async/alpha-00.fbrb', T0 + 10),
    ('game/Dist/win32/levels/mp_001/level-00.fbrb', T0 + 11),
    ('game/Dist/win32/menu/main/mainmenu-00.fbrb', T0 + 12),
    ('game/Dist/win32/levels/OPFOR_test/x-00.fbrb', T0 + 13),
]
OTHER = [('game/Package/levels/wake/opfor-wake-installed.json', T0), ('game/Package/async/notes.txt', T0)]

# index entries: (entry name as the bundle holds it, which bundle)
ENTRIES = [
    ('Objects/Weapons/Handheld/M1Garand/M1Garand_Mesh_lod0_data.res', 7),
    ('Objects/Weapons/Handheld/M1Garand/M1Garand_Mesh_lod1_data.res', 7),
    ('Objects/Weapons/Handheld/M1Garand/M1Garand_Mesh.dbx', 7),
    ('Objects/Vehicles/Air/Zero/Zero_Mesh_lod1_data.res', 1),          # no lod0: the lower-case key is printed
    ('Objects/Props/Café/Café_Mesh_lod0_data.res', 8),
    ('Objects/Props/Δelta/Δelta_Mesh_lod0_data.res', 8),               # outside cp1252
    ('X_lod0_data.res', 9),                                             # shorter than the ending's 14 characters
    ('Objects/Rocks/Rock٣_lod٣_data.res', 2),                 # Arabic-Indic digits: re's \d
    ('Objects/Rocks/Rock02_lod0_data.resx', 2),
    ('Objects/Trees/Palm_Mesh_lod0_data.res', 10),
    ('Objects/Trees/Palm_Mesh_lod2_data.res', 10),
]


def _root(d: str) -> str:
    return os.path.abspath(os.path.join(d, 'opfor'))


def _where(root: str, extra: int = 0) -> dict[str, tuple[str, str]]:
    bundles = [os.path.join(root, *p.split('/')) for p, _ in BUNDLES]
    where = {n.lower(): (bundles[b], n) for n, b in ENTRIES}
    for k in range(extra):                                  # many trees: more than 8,192 bytes printed
        n = f'Objects/Trees/Forest/Tree_{k:04d}_Mesh_lod0_data.res'
        where[n.lower()] = (bundles[10], n)
    return where


def cache_bytes(sig: list[tuple[str, int, int]], where: dict[str, tuple[str, str]], version: int = 4) -> bytes:
    """meshview_index.bin as bc2_gamefs::meshindex writes it (round PR1b-2-rest), written here from the format: the
    magic, the version, the signature (path, size, mtime_ns as i128), where (key, bundle's place in the signature,
    name), no layouts, parts or entries, no databases read, then the SHA-1 of all before it."""
    import hashlib
    import struct

    def text(s: str) -> bytes:
        b = s.encode('utf-8')
        return struct.pack('<I', len(b)) + b
    at = {b: i for i, (b, _, _) in reversed(list(enumerate(sig)))}
    out = bytearray(b'BC2MESHIX\0' + struct.pack('<II', version, len(sig)))
    for b, size, ns in sig:
        out += text(b) + struct.pack('<Q', size) + ns.to_bytes(16, 'little', signed=True)
    out += struct.pack('<I', len(where))
    for k, (b, n) in where.items():
        out += text(k) + struct.pack('<I', at.get(b, len(sig))) + text(n)   # not a bundle of the copy: never read
    out += struct.pack('<IIIQ', 0, 0, 0, 0)
    return bytes(out) + hashlib.sha1(out).digest()


def rekey_index(d: str, rel: str, data: bytes) -> bytes:
    """REKEY (round PR1b-2-rest): meshview_index.bin without its SHA-1 trailer, which hashes the paths before the case
    folder is normalized out of them (the rest compares as normalized)."""
    if rel.replace(os.sep, '/').endswith('work/tools_out/meshview_index.bin') and data.startswith(b'BC2MESHIX'):
        return data[:-20]
    return data


REKEY = [rekey_index]


def index_cache(extra: int = 0, version: int = 4, stale: bool = False) -> Callable[[str], bytes]:
    """meshview_index.bin as the index writes it for the copy in d (its bundles' sizes and times read then)."""
    def make(d: str) -> bytes:
        mv = _meshview()
        root = _root(d)
        sig = [(b, os.stat(b).st_size, os.stat(b).st_mtime_ns + (1 if stale else 0)) for _, b in mv._bundles(root)]
        return cache_bytes(sig, _where(root, extra), version)
    return make


def _database(seed: int, entries: list[tuple[str, int]], textures: dict[str, list[tuple[int, str]]],
               twin_layout: bool = False) -> bytes:
    """A Dx9 shader database built as test_shaderdb.build builds them (made-up names, GUIDs and records), its entries'
    programs naming textures (with_textures); twin_layout: a second record with the parts' layout GUID (the last
    wins), and a part named in upper case with a latin-1 letter."""
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests'))
    import test_shaderdb as tsd
    db = tsd.with_textures(tsd.build(9, entries, seed), textures)
    if twin_layout:
        lay = db.parts[0][1]
        db.layouts.append(lay + bytes(range(1, 0x4a)))
        db.parts.append(('Objects/Props/CAFÉ/Café_Mesh_lod0/PART|X/Standard', lay))
        db.parts.append(('objects/props/café/café_mesh_lod0/part|x/standard', lay))     # the same name lower: first kept
    return db.to_bytes()


def built_files(cache: Any = None, twin_db: bool = False) -> dict[str, Any]:
    """A fake copy whose bundles are real ones (fbrb.build: entries with made-up bytes, Dx9 shader databases, a deleted
    marker, a name in two bundles), the index built from them: without a cache file unless one is given.
    twin_db: a bundle with two databases, one named as the folder of the other (shaderdb.pick_database refuses that
    name: the reference leaves it out of its index and says so)."""
    from bfbc2core import fbrb
    g = 'Objects/Weapons/Handheld/M1Garand/M1Garand_Mesh'
    level = [
        ('Levels/MP_001/MP_001.dbx', '<non-resource>', b'dbx'),
        (g + '_lod0_data.res', 'MeshData', b'mesh0'),
        ('Levels/MP_001/Shaders/Dx9', 'Dx9ShaderDatabase',
         _database(1, [(g + '/M1Garand_Body', 2), ('Objects/Props/Crate/Crate_Mesh/Crate_Wood', 1)],
                   {g + '/M1Garand_Body': [(0, 'Objects/Weapons/Handheld/M1Garand/M1Garand_D'), (1, 'Short/x'),
                                           (2, 'Objects/Weapons/Handheld/M1Garand/M1Garand_N')]},
                   twin_layout=True)),
        ('Levels/MP_001/Shaders/Dx10', 'Dx10ShaderDatabase', b'not read'),
        ('Gone/Removed.dbx', '*deleted*', b''),
    ]
    common = [
        (g + '_lod0_data.res', 'MeshData', b'a later copy: not the index\'s'),
        ('Objects/Trees/Palm_Mesh_lod0_data.res', 'MeshData', b'palm'),
        ('Gone/Removed.dbx', '<non-resource>', b'here'),
        ('Shaders/Common', 'Dx9ShaderDatabase',
         _database(2, [(g + '/M1Garand_Body', 1), ('Objects/Trees/Palm_Mesh/Palm_Leaves', 1)],
                   {'Objects/Trees/Palm_Mesh/Palm_Leaves': [(0, 'Textures/Trees/Palm_D'), (1, 'Textures/Trees/Palm_D')]})),
    ]
    if twin_db:
        common.append(('Shaders/Common/Extra', 'Dx9ShaderDatabase', _database(3, [('Objects/X/Y/Z', 1)], {})))
    files: dict[str, Any] = {
        'opfor/game/Package/levels/mp_001/level-00.fbrb': (fbrb.build(level), T0 + 1),
        'opfor/game/Package/async/common-00.fbrb': (fbrb.build(common), T0 + 2),
        'opfor/game/Dist/win32/menu/main/mainmenu-00.fbrb': (fbrb.build([('Menu/Main.res', 'SwfMovie', b'm')], False),
                                                             T0 + 3),
        'opfor/game/BFBC2Game.exe': (b'MZ' + bytes(62), T0 + 0.75),
    }
    if cache is not None:
        files['opfor/work/tools_out/meshview_index.bin'] = cache
    return files


def game_files(cache: Any = None, exe: bool = True) -> dict[str, Any]:
    """The fake copy, the index cache last (it is made from the files before it)."""
    files: dict[str, Any] = {}
    for i, (p, t) in enumerate(BUNDLES):
        files['opfor/' + p] = (b'FBRB-dummy-%d' % i * (i + 1), t)
    for p, t in OTHER:
        files['opfor/' + p] = (b'{}', t)
    if exe:
        files['opfor/game/BFBC2Game.exe'] = (b'MZ' + bytes(62), T0 + 0.75)
    files['opfor/work/tools_out/meshview_index.bin'] = cache if cache is not None else (index_cache(), T0)
    return files


def ref_beside_opfor() -> bool:
    """Whether the suite whose command lines answer (test_cli_o2.REF) sits beside a Project OPFOR copy: then the 'no
    game copy' cases would find it (shaderdb.find_opfor looks beside the suite), so they are left out; they are
    recorded and checked against a reference copy elsewhere (--ref)."""
    base = sys.modules.get('test_cli_o2')
    ref = getattr(base, 'REF', SUITE)
    return os.path.isdir(os.path.join(os.path.dirname(ref), 'project-opfor-bfbc2', 'game'))


ROOT_ENV = {'OPFOR_ROOT': '{d}\\opfor'}


def cases() -> list[Case]:
    good = game_files()
    big = game_files((index_cache(extra=400), T0))
    rows: list[Case] = []

    def add(key: str, args: list[str], files: dict[str, Any] | None = None, env: dict[str, str] | None = None) -> None:
        rows.append((key, 'meshview', good if files is None else files, args, ROOT_ENV if env is None else env,
                     False))

    add('usage', [])
    add('usage --png', ['--png'])
    add('usage --opfor only', ['--opfor', '{d}\\opfor'])
    add('find garand', ['find', 'garand'])
    add('find everything', ['find'])
    add('find two words', ['find', 'M1Garand/M1GARAND', 'mesh'])
    add('find nothing', ['find', 'no such thing'])
    add('find cafe', ['find', 'café'])
    add('find delta (not cp1252)', ['find', 'δelta'])
    add('find lod', ['find', '_lod'])
    add('find arabic digits', ['find', 'rock'])
    add('find tree', ['find', 'trees/'])
    add('find short name', ['find', 'x_lod'])
    add('find --level', ['--level', 'mp_001', 'find', 'garand'])
    add('find --png', ['find', '--png', 'zero'])
    add('find --opfor last', ['find', 'zero', '--opfor'])
    add('find --opfor given', ['--opfor', '{d}\\opfor', 'find', 'palm'], env={'OPFOR_ROOT': ''})
    add('find --opfor wrong', ['--opfor', '{d}\\nowhere', 'find', 'palm'])
    if not ref_beside_opfor():                  # else both sides find that copy: the answer depends on the place
        add('no game copy', ['find', 'palm'], env={'OPFOR_ROOT': ''})
        add('no game copy, given nowhere', ['--opfor', '{d}\\nowhere', 'find', 'palm'], env={'OPFOR_ROOT': ''})
    add('find everything, long output', ['find'], big)
    add('find trees, long output', ['find', 'forest/'], big)
    # handed to the Python: an index that is not this copy's (rebuilt from the dummy bundles), a damaged cache,
    # another cache version, drawing a mesh
    add('stale cache', ['find', 'garand'], game_files((index_cache(stale=True), T0)))
    add('damaged cache', ['find', 'garand'], game_files((b'BC2MESHIX\0damaged', T0)))
    add('old cache version', ['find', 'garand'], game_files((index_cache(version=2), T0)))
    add('draw a mesh not there', ['Objects/Nowhere/Nothing_Mesh'])
    # the index built (no cache, or one that is not this copy's) from real bundles: the cache written byte for byte
    built = built_files()
    add('build: find garand', ['find', 'garand'], built)
    add('build: find everything', ['find'], built)
    add('build: stale cache', ['find', 'palm'], built_files((index_cache(stale=True), T0)))
    add('build: damaged cache', ['find', 'palm'], built_files((b'BC2MESHIX\0\x04broken', T0)))
    # handed to the Python: a database the reference cannot pick (left out of its index, not cached, said)
    add('build: a database it leaves out', ['find', 'garand'], built_files(twin_db=True))
    return with_variants(rows, ['usage', 'find garand', 'find delta (not cp1252)', 'find everything, long output',
                                'no game copy', 'build: find garand'])


def cache_sig(data: bytes) -> list[tuple[str, int, int]] | None:
    """The signature of a meshview_index.bin (round PR1b-2-rest's format, as cache_bytes writes it), or None when it
    is not whole or not this format's version."""
    import hashlib
    import struct
    if len(data) < 38 or hashlib.sha1(data[:-20]).digest() != data[-20:] or data[:10] != b'BC2MESHIX\0':
        return None
    version, n = struct.unpack_from('<II', data, 10)
    if version != 4:
        return None
    at, sig = 18, []
    for _ in range(n):
        (k,) = struct.unpack_from('<I', data, at)
        path = data[at + 4:at + 4 + k].decode('utf-8')
        at += 4 + k
        (size,) = struct.unpack_from('<Q', data, at)
        sig.append((path, size, int.from_bytes(data[at + 8:at + 24], 'little', signed=True)))
        at += 24
    return sig


def index_current(opfor: str) -> bool:
    """Whether OPFOR's index cache is its game copy's (then meshview reads it and writes nothing)."""
    mv = _meshview()
    try:
        with open(os.path.join(opfor, 'work', 'tools_out', mv.CACHE_NAME), 'rb') as f:
            data = f.read()
        sig = [(b, os.stat(b).st_size, os.stat(b).st_mtime_ns) for _, b in mv._bundles(opfor)]
        return cache_sig(data) == sig
    except Exception:                                        # noqa: BLE001 - missing or damaged: not current
        return False


def opfor_calls(opfor: str) -> list[Call]:
    if not index_current(opfor):
        print('     (OPFOR\'s meshview index cache is not current: its calls left out, nothing rebuilt)')
        return []
    env = {'OPFOR_ROOT': opfor}
    texts = ['garand', 'zero', 'corsair', 'kurogane', 'chi_ha', 'bofors', 'palm', 'm1a1', 'soldier', 'zzz none']
    out: list[Call] = [(f'find {t}', 'meshview', ['find', *t.split()], {}, env, True) for t in texts]
    out.append(('find _lod1 (a large list)', 'meshview', ['find', '_lod1'], {}, env, True))
    out.append(('find --opfor', 'meshview', ['--opfor', opfor, 'find', 'garand'], {}, {'OPFOR_ROOT': ''}, True))
    return out
