r"""Round O2's nav group: the command lines test_cli_o2.py replays for hpdtool, hkpack, convert.py and aimanim.

cases(): the quick tier's (synthetic inputs made in the case folder; no game data). opfor_calls(opfor): the --game
tier's (Project OPFOR's real inputs, copied to the case folder).
"""
from __future__ import annotations

import math
import os
import random
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

# Python's stdio switches the encoding / buffering cases run under too
VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    """rows, each again with stderr on stdout's pipe, and the picked ones (keys) under Python's stdio switches
    (alone and merged)."""
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- aimanim

def _aim_write(a: dict[str, Any]) -> bytes:
    """aimanim.write_reference's bytes (the Python writer, copied, so the inputs do not depend on the port)."""
    nb, n = len(a['bones']), a['poses']
    o = 0x1c + 2 * nb
    head = bytearray(o + (16 - (o - 0x0c) % 16) % 16)
    struct.pack_into('<5I', head, 0, 1, a['addr'], 0, (a['addr'] + 0x10) & 0xffffffff, nb)
    head[0x14] = n
    for i, (bone, kind, _) in enumerate(a['bones']):
        head[0x1c + 2 * i], head[0x1d + 2 * i] = bone, kind
    body = bytearray()
    v = lambda q: struct.pack('<4f', *q)                                               # noqa: E731
    for _bone, kind, block in a['bones']:
        if kind == 0:
            body += v(block[0]) + v(block[1])
        elif kind == 1:
            body += v(block[0]) + b''.join(v(q) for q in block[1])
        else:
            body += b''.join(v(p) + v(q) for p, q in block)
    out = bytes(head) + bytes(body)
    return out[:8] + struct.pack('<I', len(out) - 12) + out[12:]


def _aim_big(b: bytes) -> bytes:
    nb = struct.unpack_from('<I', b, 0x10)[0]
    out = bytearray(b)
    for o in range(0, len(b), 4):
        if 0x14 <= o < 0x1c + 2 * nb:
            continue
        out[o:o + 4] = b[o:o + 4][::-1]
    return bytes(out)


def _aim_sample(n: int = 25) -> dict[str, Any]:
    q = lambda k: (0.0, math.sin(k / 10), 0.0, math.cos(k / 10))                       # noqa: E731
    return {'addr': 0x08A255B0, 'poses': n, 'bones': [
        (0, 0, ((0.0, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0, 1.0))),
        (2, 1, ((0.1, 0.2, 0.3, 0.0), [q(k) for k in range(n)])),
        (17, 2, [((k * 0.01, 1.4, 0.2, 1.0), q(k)) for k in range(n)]),
        (255, 0, ((1e-7, -2.5e10, 3.4e38, -0.0), (1.0, 0.5, 0.25, 1e-40))),
    ]}


def _aim_many() -> bytes:
    bones: list[tuple[int, int, Any]] = []
    for i in range(300):
        if i % 3 == 0:
            bones.append((i % 256, 0, ((i * 0.1, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0, 1.0))))
        elif i % 3 == 1:
            bones.append((i % 256, 1, ((0.0, 0.0, 0.0, 0.0), [(0.0, 0.0, 0.0, 1.0)])))
        else:
            bones.append((i % 256, 2, [((0.5, 0.5, 0.5, 1.0), (0.0, 0.0, 0.0, 1.0))]))
    return _aim_write({'addr': 0, 'poses': 1, 'bones': bones})


def aimanim_cases() -> list[Case]:
    good = _aim_write(_aim_sample())
    nan = bytearray(good)
    struct.pack_into('<f', nan, 0x30, float('nan'))
    no_poses = _aim_write({'addr': 16, 'poses': 0, 'bones': [(1, 0, ((1.0, 2.0, 3.0, 1.0), (0.0, 0.0, 0.0, 1.0))),
                                                               (2, 1, ((0.0, 0.0, 0.0, 0.0), []))]})
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('a PC file', {'a.res': good}, ['{d}\\a.res']),
        ('a PC file read as a Xbox one (--big after it)', {'a.res': good}, ['{d}\\a.res', '--big']),
        ('a Xbox file --big', {'a.res': _aim_big(good)}, ['{d}\\a.res', '--big']),
        ('a Xbox file without --big', {'a.res': _aim_big(good)}, ['{d}\\a.res']),
        ('--big first (the file is named --big)', {'a.res': good}, ['--big', '{d}\\a.res']),
        ('a missing file', {}, ['{d}\\missing.res']),
        ('an empty argument', {}, ['']),
        ('a folder', {'f': None}, ['{d}\\f']),
        ('too short', {'a.res': b'\x01\x00\x00\x00'}, ['{d}\\a.res']),
        ('not the layout', {'a.res': b'\x02' + bytes(0x2f)}, ['{d}\\a.res']),
        ('a NaN', {'a.res': bytes(nan)}, ['{d}\\a.res']),
        ('no poses (IndexError)', {'a.res': no_poses}, ['{d}\\a.res']),
        ('300 bones (stdout over 8,192 bytes)', {'a.res': _aim_many()}, ['{d}\\a.res']),
        ('a path in cp1252', {'café.res': good}, ['{d}\\café.res']),
        ('a missing path outside cp1252', {}, ['{d}\\Ж.res']),
        ('extra arguments', {'a.res': good}, ['{d}\\a.res', 'x', 'y']),
    ]
    out: list[Case] = [(f'aimanim: {name}', 'aimanim', files, args, {}, False) for name, files, args in rows]
    return with_variants(out, ['aimanim: a missing path outside cp1252', 'aimanim: 300 bones (stdout over 8,192 bytes)',
                               'aimanim: a path in cp1252', 'aimanim: no poses (IndexError)'])


# ------------------------------------------------------------------------------------------- the group

# ------------------------------------------------------------------------------------------- hpdtool

def _navkit() -> tuple[Any, Any]:
    suite_paths.use('navkit-tools')
    import hpdtool
    import kymesh
    return hpdtool, kymesh


def _f32(rng: random.Random) -> float:
    r = rng.random()
    if r < 0.05:
        return rng.choice([0.0, -0.0, 1.0, float('inf'), -float('inf'), 1e-45, 3.4028234663852886e38])
    return float(struct.unpack('<f', struct.pack('<f', rng.uniform(-5000, 5000)))[0])


def _v3(rng: random.Random) -> tuple[float, float, float]:
    return (_f32(rng), _f32(rng), _f32(rng))


def _mesh(rng: random.Random) -> Any:
    """A random runtime mesh (test_pathdata_native.build_mesh's, copied)."""
    _hpd, kymesh = _navkit()
    tx, tz = rng.randint(1, 3), rng.randint(1, 3)
    n_areas = rng.randint(1, 6)
    areas = []
    for _ in range(n_areas):
        a = kymesh.Area(rng.randrange(1 << 16), rng.randrange(8), rng.randrange(8), rng.randrange(3), _f32(rng),
                        _f32(rng), _f32(rng), _f32(rng))
        a.portals = [kymesh.Portal(_v3(rng), _v3(rng), rng.randrange(n_areas)) for _ in range(rng.randint(0, 3))]
        a.border_portals = [kymesh.BorderPortal(_v3(rng), _v3(rng), _v3(rng),
                                                rng.choice([(kymesh.NULL, kymesh.NULL), (rng.randrange(1 << 32), 7)]))
                            for _ in range(rng.randint(0, 2))]
        a.d_lines = [[_v3(rng) for _ in range(rng.randint(0, 4))] for _ in range(rng.randint(0, 2))]
        a.outline = [[_v3(rng) for _ in range(rng.randint(1, 5))] for _ in range(rng.randint(0, 3))]
        a.cells = [kymesh.Cell(_v3(rng), [_v3(rng) for _ in range(rng.randint(0, 3))]) for _ in range(rng.randint(0, 2))]
        if a.cells:
            a.end_row = _v3(rng)
        for key, lst in ((0x20, None), (0x28, a.portals), (0x30, a.border_portals), (0x38, a.d_lines),
                         (0x40, a.outline), (0x48, a.cells)):
            if not lst and rng.random() < 0.7:
                a.null_offsets[key] = rng.choice([kymesh.NULL, 0, 0x50, rng.randrange(1 << 32)])
        areas.append(a)
    order = list(range(n_areas))
    rng.shuffle(order)
    tiles: list[list[Any]] = [[] for _ in range(tx * tz)]
    for i in order:
        for _ in range(rng.choice([1, 1, 2])):
            tiles[rng.randrange(tx * tz)].append(kymesh.TileItem(i, _f32(rng), _f32(rng)))
    first: list[int] = []
    for t in tiles:
        for it in t:
            if it.area not in first:
                first.append(it.area)
    renum = {old: new for new, old in enumerate(first)}
    areas = [areas[old] for old in first]
    for t in tiles:
        for it in t:
            it.area = renum[it.area]
    for a in areas:
        for p in a.portals:
            p.target = renum[p.target]
    links = []
    for _ in range(rng.choice([0, 0, 1, 2])):
        objs = [kymesh.LinkObject((rng.randrange(1 << 32), rng.randrange(9), 0),
                                  [(rng.randrange(len(areas)), rng.randrange(0, 0x50, 4)) for _ in range(rng.randint(0, 3))])
                for _ in range(rng.randint(0, 3))]
        links.append(kymesh.NeighbourLink(rng.randrange(1 << 16), objs))
    return kymesh.Mesh(rng.randrange(1 << 16), rng.randrange(1 << 16), 0, rng.randrange(1 << 16), _f32(rng), _f32(rng),
                       (_f32(rng), _f32(rng), _f32(rng), _f32(rng)), tx, tz, _f32(rng), tiles, areas, links,
                       bytes(rng.randrange(256) for _ in range(4)), rng.choice([kymesh.NULL, 0, 1234]))


def _hpd_file(seed: int, sectors: tuple[int, int] = (1, 4), nodes: int = 12) -> bytes:
    """A random path data file, written by the reference writer (deterministic per seed)."""
    hpdtool, kymesh = _navkit()
    rng = random.Random(seed)
    out = []
    for _ in range(rng.randint(*sectors)):
        n = rng.randint(0, nodes)
        nn = [hpdtool.Node(rng.randrange(1 << 32), _v3(rng), 0x80000000, _f32(rng), rng.randrange(1 << 32),
                           rng.randrange(1 << 32), rng.randrange(1 << 32)) for _ in range(n)]
        ee = [hpdtool.Edge(rng.randrange(1 << 32), rng.randrange(max(n, 1)), rng.randrange(max(n, 1)))
              for _ in range(rng.randint(0, 2 * n))]
        bounds = tuple(_f32(rng) for _ in range(6))
        mesh = kymesh.write_mesh_reference(_mesh(rng))
        out.append(hpdtool.Sector(rng.randrange(1 << 16), bounds, 0, 100007 + rng.randrange(3), rng.randrange(1 << 32),
                                  nn, ee, mesh))
    header = tuple(rng.randrange(1 << 32) for _ in range(hpdtool.HEADER_WORDS))
    return bytes(hpdtool.write_reference(hpdtool.HpdFile(header, out)))


def _damaged(data: bytes, seed: int) -> bytes:
    rng = random.Random(seed)
    b = bytearray(data)
    kind = seed % 3
    if kind == 0:
        del b[rng.randrange(200, len(b)):]
    elif kind == 1:
        for _ in range(3):
            b[rng.randrange(len(b))] = rng.randrange(256)
    else:
        b += bytes(5)
    return bytes(b)


def _nan_file() -> bytes:
    """A good file whose first sector's first bound is a NaN (the core leaves it to the reference)."""
    b = bytearray(_hpd_file(3))
    struct.pack_into('<f', b, 12 + 36 * 4 + 4, float('nan'))
    return bytes(b)


def hpdtool_cases() -> list[Case]:
    good, other, big = _hpd_file(1), _hpd_file(2), _hpd_file(7, (3, 3), 120)
    empty = _hpd_file(9, (0, 0))
    tree = {'levels\\mp_001\\pathdata.hpd': good, 'levels\\mp_002\\sub\\PathData.HPD': other,
            'levels\\mp_002\\notes.txt': 'x', 'levels\\sp_003\\pathdata.hpd': big}
    bad_tree = dict(tree, **{'levels\\mp_004\\pathdata.hpd': _damaged(good, 3), 'levels\\mp_005\\pathdata.hpd': _damaged(other, 4),
                             'levels\\mp_006\\pathdata.hpd': _damaged(good, 5)})
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('info without a file (the usage)', {}, ['info']),
        ('an unknown command', {'a.hpd': good}, ['what', '{d}\\a.hpd']),
        ('info', {'a.hpd': good}, ['info', '{d}\\a.hpd']),
        ('info, extra arguments', {'a.hpd': good}, ['info', '{d}\\a.hpd', 'more']),
        ('info of a file with no sectors', {'a.hpd': empty}, ['info', '{d}\\a.hpd']),
        ('info of a missing file', {}, ['info', '{d}\\none.hpd']),
        ('info of a folder', {'f': None}, ['info', '{d}\\f']),
        ('info of a file that is not path data', {'a.hpd': b'not a file at all' * 9}, ['info', '{d}\\a.hpd']),
        ('info of a cut file', {'a.hpd': good[:300]}, ['info', '{d}\\a.hpd']),
        ('info of a file with a NaN', {'a.hpd': _nan_file()}, ['info', '{d}\\a.hpd']),
        ('sector 0', {'a.hpd': good}, ['sector', '{d}\\a.hpd', '0']),
        ('sector 1', {'a.hpd': other}, ['sector', '{d}\\a.hpd', '1']),
        ('sector -1', {'a.hpd': good}, ['sector', '{d}\\a.hpd', '-1']),
        ('sector " +2 "', {'a.hpd': big}, ['sector', '{d}\\a.hpd', ' +2 ']),
        ('sector 1_0 (out of range)', {'a.hpd': big}, ['sector', '{d}\\a.hpd', '1_0']),
        ('sector 99 (out of range)', {'a.hpd': good}, ['sector', '{d}\\a.hpd', '99']),
        ('sector x', {'a.hpd': good}, ['sector', '{d}\\a.hpd', 'x']),
        ('sector 1.5', {'a.hpd': good}, ['sector', '{d}\\a.hpd', '1.5']),
        ('sector without an index', {'a.hpd': good}, ['sector', '{d}\\a.hpd']),
        ('sector with many nodes (stdout over 8,192 bytes)', {'a.hpd': big}, ['sector', '{d}\\a.hpd', '0']),
        ('sector of a missing file', {}, ['sector', '{d}\\none.hpd', '0']),
        ('roundtrip of a folder', tree, ['roundtrip', '{d}\\levels']),
        ('roundtrip of files and a folder', tree, ['roundtrip', '{d}\\levels\\mp_001\\pathdata.hpd', '{d}\\levels\\mp_002']),
        ('roundtrip of a folder with damaged files', bad_tree, ['roundtrip', '{d}\\levels']),
        ('roundtrip of a missing file', tree, ['roundtrip', '{d}\\levels\\mp_001\\pathdata.hpd', '{d}\\none.hpd']),
        ('roundtrip of an empty folder', {'e': None}, ['roundtrip', '{d}\\e']),
        ('roundtrip, a folder with a trailing backslash', tree, ['roundtrip', '{d}\\levels\\']),
        ('coverage of a folder', tree, ['coverage', '{d}\\levels']),
        ('coverage of a folder with damaged files', bad_tree, ['coverage', '{d}\\levels']),
        ('coverage of a missing file', {}, ['coverage', '{d}\\none.hpd']),
        ('meshtrip of a folder', tree, ['meshtrip', '{d}\\levels']),
        ('meshtrip of a folder with damaged files', bad_tree, ['meshtrip', '{d}\\levels']),
        ('meshtrip of a missing file', {}, ['meshtrip', '{d}\\none.hpd']),
        ('roundtrip, a folder named outside cp1252', {'Ж\\x\\pathdata.hpd': good, 'Ж\\y\\pathdata.hpd':
                                                      _damaged(good, 3)}, ['roundtrip', '{d}\\Ж']),
        ('coverage, a folder named in cp1252', {'café\\x\\pathdata.hpd': good}, ['coverage', '{d}\\café']),
        ('info of a missing file named outside cp1252', {}, ['info', '{d}\\Ж.hpd']),
    ]
    out: list[Case] = [(f'hpdtool: {name}', 'hpdtool', files, args, {}, False) for name, files, args in rows]
    return with_variants(out, ['hpdtool: roundtrip, a folder named outside cp1252',
                               'hpdtool: sector with many nodes (stdout over 8,192 bytes)',
                               'hpdtool: coverage, a folder named in cp1252', 'hpdtool: info of a missing file named '
                               'outside cp1252', 'hpdtool: info of a file with no sectors'])


# ------------------------------------------------------------------------------------------- hkpack

HK_MAGIC = bytes.fromhex('57e0e05710c0c010')
# hkClassMember types
(T_VOID, T_BOOL, T_CHAR, T_INT8, T_UINT8, T_INT16, T_UINT16, T_INT32, T_UINT32, T_INT64, T_UINT64, T_REAL, T_VECTOR4,
 T_QUATERNION, T_MATRIX3, T_ROTATION, T_QSTRANSFORM, T_MATRIX4, T_TRANSFORM, T_ZERO, T_POINTER, T_FUNCTIONPOINTER,
 T_ARRAY, T_INPLACEARRAY, T_ENUM, T_STRUCT, T_SIMPLEARRAY, T_HOMOGENEOUSARRAY, T_VARIANT, T_CSTRING, T_ULONG,
 T_FLAGS) = range(32)


class _Pack:
    """A little-endian Havok packfile of one section (__data__): hkClass descriptions and objects, with local
    pointer fixups and virtual fixups naming each object's class."""

    def __init__(self) -> None:
        self.body = bytearray(16)
        self.local: list[tuple[int, int]] = []
        self.virtual: list[tuple[int, int]] = []         # (object, class name offset)
        self.names: dict[str, int] = {}

    def alloc(self, n: int, align: int = 4) -> int:
        while len(self.body) % align:
            self.body.append(0)
        at = len(self.body)
        self.body += bytes(n)
        return at

    def string(self, s: str | bytes, end: bool = True) -> int:
        raw = s.encode('latin-1') if isinstance(s, str) else s
        at = self.alloc(len(raw) + (1 if end else 0), 1)
        self.body[at:at + len(raw)] = raw
        return at

    def ptr(self, slot: int, target: int) -> None:
        self.local.append((slot, target))

    def obj(self, cname: str, size: int) -> int:
        at = self.alloc(max(size, 4), 16)
        if cname not in self.names:
            self.names[cname] = self.string(cname)
        self.virtual.append((at, self.names[cname]))
        return at

    def klass(self, name: str, size: int, members: list[tuple[str, int | None, int, int, int, int]],
              parent: int | None = None) -> int:
        """An hkClass: members (name, class object or None, type, subtype, carray, offset); its offset."""
        c = self.obj('hkClass', 0x30)
        self.ptr(c, self.string(name))
        if parent is not None:
            self.ptr(c + 4, parent)
        struct.pack_into('<2i', self.body, c + 8, size, 0)
        if members:
            tab = self.alloc(24 * len(members), 16)
            self.ptr(c + 24, tab)
            for k, (mn, mc, mt, ms, ca, mo) in enumerate(members):
                m = tab + 24 * k
                self.ptr(m, self.string(mn))
                if mc is not None:
                    self.ptr(m + 4, mc)
                struct.pack_into('<BBhHH', self.body, m + 12, mt, ms, ca, 0, mo)
        struct.pack_into('<i', self.body, c + 28, len(members))
        return c

    def bytes(self, prefix: bytes = b'', version: bytes = b'Havok-5.5.0-r1') -> bytes:
        body = bytearray(self.body)
        while len(body) % 16:
            body.append(0)
        local = len(body)
        for s, d in self.local:
            body += struct.pack('<2I', s, d)
        glob = len(body)
        virt = glob
        for o, n in self.virtual:
            body += struct.pack('<3I', o, 0, n)
        exports = len(body)
        head = bytearray(0x40 + 0x30)
        struct.pack_into('<8I', head, 0, *struct.unpack('<2I', HK_MAGIC), 0, 5, 0x01010004, 1, 0, 0)
        head[0x28:0x28 + len(version)] = version
        head[0x40:0x40 + 8] = b'__data__'
        struct.pack_into('<7I', head, 0x40 + 20, len(head), local, glob, virt, exports, exports, exports)
        return prefix + bytes(head) + bytes(body)


def _hk_sample(many: int = 3) -> bytes:
    pk = _Pack()
    vec = pk.klass('hkVec', 8, [('x', None, T_REAL, 0, 0, 0), ('y', None, T_INT16, 0, 0, 4)])
    base = pk.klass('hkBase', 8, [('m_kind', None, T_ENUM, T_INT8, 0, 0), ('m_flags', None, T_FLAGS, T_UINT16, 0, 2),
                                  ('m_mask', None, T_FLAGS, 0, 0, 4)])
    members = [
        ('m_bool', None, T_BOOL, 0, 0, 8), ('m_char', None, T_CHAR, 0, 0, 9), ('m_i8', None, T_INT8, 0, 0, 10),
        ('m_u8', None, T_UINT8, 0, 0, 11), ('m_i16', None, T_INT16, 0, 0, 12), ('m_u16', None, T_UINT16, 0, 0, 14),
        ('m_i32', None, T_INT32, 0, 0, 16), ('m_u32', None, T_UINT32, 0, 0, 20), ('m_i64', None, T_INT64, 0, 0, 24),
        ('m_u64', None, T_UINT64, 0, 0, 32), ('m_real', None, T_REAL, 0, 0, 40), ('m_ulong', None, T_ULONG, 0, 0, 44),
        ('m_vec', None, T_VECTOR4, 0, 0, 48), ('m_quat', None, T_QUATERNION, 0, 0, 64),
        ('m_rot', None, T_ROTATION, 0, 0, 80), ('m_ptr', None, T_POINTER, T_STRUCT, 0, 128),
        ('m_rawptr', None, T_POINTER, 0, 0, 132), ('m_null', None, T_POINTER, 0, 0, 136),
        ('m_name', None, T_CSTRING, 0, 0, 140), ('m_noname', None, T_CSTRING, 0, 0, 144),
        ('m_ints', None, T_ARRAY, T_INT32, 0, 148), ('m_many', None, T_ARRAY, T_REAL, 0, 156),
        ('m_vecs', vec, T_ARRAY, T_STRUCT, 0, 164), ('m_odd', None, T_SIMPLEARRAY, T_VARIANT, 0, 172),
        ('m_empty', None, T_ARRAY, T_INT32, 0, 180), ('m_sub', vec, T_STRUCT, 0, 0, 188),
        ('m_carr', None, T_REAL, 0, 3, 196), ('m_zero', None, T_ZERO, 0, 0, 208), ('m_fn', None, T_FUNCTIONPOINTER, 0, 0, 212),
        ('m_var', None, T_VARIANT, 0, 0, 216), ('m_nostruct', None, T_STRUCT, 0, 0, 224),
        ('m_i32', None, T_INT32, 0, 0, 228), ('m_ptrs', None, T_ARRAY, T_POINTER, 0, 232),
    ]
    pk.klass('hkThing', 240, members, parent=base)
    rng = random.Random(5)
    things = []
    for k in range(many):
        t = pk.obj('hkThing', 240)
        things.append(t)
        b = pk.body
        struct.pack_into('<bHI', b, t, -3 + k, 0xFFFF - k, 7 * k)
        struct.pack_into('<?bbBhHiIqQfI', b, t + 8, k % 2 == 0, -5, -100 + k, 200, -30000, 60000 + k, -2_000_000_000,
                         4_000_000_000, -(1 << 40), (1 << 63) + k, [0.1, 1e-7, 123456.789, float('inf')][k % 4], 42)
        struct.pack_into('<4f', b, t + 48, 1.0, -2.5, 0.0004, 1e10)
        struct.pack_into('<4f', b, t + 64, 0.0, 0.7071, 0.0, -0.7071)
        struct.pack_into('<12f', b, t + 80, *[rng.uniform(-2, 2) for _ in range(12)])
        struct.pack_into('<3f', b, t + 196, 0.5, -0.0, 99999.0)
        struct.pack_into('<i', b, t + 228, k)
        pk.ptr(t + 140, pk.string(f'thing{k}'))
        ints = pk.alloc(4 * 5)
        struct.pack_into('<5i', pk.body, ints, *range(k, k + 5))
        pk.ptr(t + 148, ints)
        struct.pack_into('<i', pk.body, t + 152, 5)
        reals = pk.alloc(4 * 9)
        struct.pack_into('<9f', pk.body, reals, *[i / 3 for i in range(9)])
        pk.ptr(t + 156, reals)
        struct.pack_into('<i', pk.body, t + 160, 9)
        vecs = pk.alloc(8 * 2)
        struct.pack_into('<fhxxfhxx', pk.body, vecs, 1.5, -1, 2.25, 300)
        pk.ptr(t + 164, vecs)
        struct.pack_into('<i', pk.body, t + 168, 2)
        pk.ptr(t + 172, vecs)
        struct.pack_into('<i', pk.body, t + 176, 3)
        struct.pack_into('<i', pk.body, t + 184, 4)              # a count with no data pointer: []
        struct.pack_into('<fh', pk.body, t + 188, 3.75, 12)
        if k:
            pk.ptr(t + 128, things[0])
        pk.ptr(t + 132, ints + 4)
        pt = pk.alloc(8)
        pk.ptr(pt, things[0])
        pk.ptr(pt + 4, ints)
        pk.ptr(t + 232, pt)
        struct.pack_into('<i', pk.body, t + 236, 2)
    other = pk.obj('hkOther', 8)
    struct.pack_into('<I', pk.body, other, 1)
    return pk.bytes()


def _hk_bad_cstring() -> bytes:
    pk = _Pack()
    pk.klass('hkS', 8, [('m_name', None, T_CSTRING, 0, 0, 0)])
    s = pk.obj('hkS', 8)
    pk.ptr(s, pk.string(b'caf\xe9 \xff', end=True))
    return pk.bytes()


def hkpack_cases() -> list[Case]:
    sample = _hk_sample()
    wrapped = _Pack()
    wrapped.klass('hkA', 4, [('a', None, T_INT32, 0, 0, 0)])
    for _ in range(3):
        wrapped.obj('hkA', 4)
    many = _Pack()
    many.klass('hkA', 4, [('a', None, T_INT32, 0, 0, 0)])
    for k in range(700):
        many.obj(f'hkKind{k % 260:03d}', 4)
    rows: list[tuple[str, dict[str, Any], list[str]]] = [
        ('the usage', {}, []),
        ('classes without a file (the usage)', {}, ['classes']),
        ('an unknown command (the file read first)', {'a.hkx': sample}, ['what', '{d}\\a.hkx']),
        ('an unknown command, a missing file', {}, ['what', '{d}\\none.hkx']),
        ('classes', {'a.hkx': sample}, ['classes', '{d}\\a.hkx']),
        ('classes of a packfile after a wrapper', {'a.res': b'DICE wrapper bytes' * 3 + wrapped.bytes()}, ['classes', '{d}\\a.res']),
        ('classes, 260 kinds (stdout over 8,192 bytes)', {'a.hkx': many.bytes()}, ['classes', '{d}\\a.hkx']),
        ('classes of another version', {'a.hkx': _hk_sample(1)[:0x28] + b'Havok-7.1.0-r1\0\0' + _hk_sample(1)[0x38:]},
         ['classes', '{d}\\a.hkx']),
        ('classes of a missing file', {}, ['classes', '{d}\\none.hkx']),
        ('classes of a file without a packfile', {'a.hkx': b'nothing here' * 20}, ['classes', '{d}\\a.hkx']),
        ('classes of a cut packfile', {'a.hkx': sample[:0x60]}, ['classes', '{d}\\a.hkx']),
        ('dump', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkThing']),
        ('dump 3', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkThing', '3']),
        ('dump 0', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkThing', '0']),
        ('dump -1', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkThing', '-1']),
        ('dump x', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkThing', 'x']),
        ('dump the classes themselves', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkClass', '9']),
        ('dump a class with no description', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkOther']),
        ('dump a class no object has', {'a.hkx': sample}, ['dump', '{d}\\a.hkx', 'hkNothing']),
        ('dump without a class', {'a.hkx': sample}, ['dump', '{d}\\a.hkx']),
        ('dump many (stdout over 8,192 bytes)', {'a.hkx': _hk_sample(40)}, ['dump', '{d}\\a.hkx', 'hkThing', '40']),
        ('dump a string outside ASCII', {'a.hkx': _hk_bad_cstring()}, ['dump', '{d}\\a.hkx', 'hkS']),
        ('classes, a path outside cp1252', {'Ж.hkx': sample}, ['classes', '{d}\\Ж.hkx']),
    ]
    out: list[Case] = [(f'hkpack: {name}', 'hkpack', files, args, {}, False) for name, files, args in rows]
    return with_variants(out, ['hkpack: dump a string outside ASCII', 'hkpack: dump many (stdout over 8,192 bytes)',
                               'hkpack: classes of a missing file'])


# ------------------------------------------------------------------------------------------- convert.py

def _pe(classes: list[tuple[str, str | None, int, list[tuple[str, str | None, int, int, int, int]]]]) -> bytes:
    """A 32-bit PE image whose hkClass initializers describe classes: (name, parent, size, members (name, class,
    type, subtype, carray, offset)) - test_havok_classes_native.build_pe's layout, deterministic."""
    base, sec_va, sec_raw = 0x400000, 0x1000, 0x400
    sec = bytearray(0x3000)
    strings: dict[str, int] = {}
    spos = [0x10]

    def string(s: str) -> int:
        if s not in strings:
            at = spos[0]
            sec[at:at + len(s) + 1] = s.encode() + b'\0'
            strings[s] = base + sec_va + at
            spos[0] += len(s) + 1
        return strings[s]
    objs = {name: base + sec_va + 0x2000 + 0x40 * k for k, (name, *_r) in enumerate(classes)}
    pos = 0x400
    tables = []
    for _name, _parent, _size, members in classes:
        tab = pos
        for mn, mc, mt, ms, ca, mo in members:
            sec[pos:pos + 24] = struct.pack('<3IBBhHHI', string(mn), objs.get(mc or '', 0), 0, mt, ms, ca, 0, mo, 0)
            pos += 24
        tables.append((base + sec_va + tab, len(members)))
    ctor = base + sec_va + 0x2F00
    pos = 0x1000
    for k, (name, parent, size, _m) in enumerate(classes):
        sec[pos] = 0xCC
        pos += 1
        args = [string(name), objs.get(parent or '', 0), size, 0, 0, 0, 0, tables[k][0], tables[k][1], 0, 0, 0]
        for a in reversed(args):
            sec[pos:pos + 5] = b'\x68' + struct.pack('<I', a)
            pos += 5
        here = base + sec_va + pos
        sec[pos:pos + 10] = b'\xB9' + struct.pack('<I', objs[name]) + b'\xE8' + struct.pack('<i', ctor - (here + 10))
        pos += 10
    pe = 0x80
    hdr = bytearray(sec_raw)
    hdr[0:2] = b'MZ'
    struct.pack_into('<I', hdr, 0x3C, pe)
    hdr[pe:pe + 4] = b'PE\0\0'
    struct.pack_into('<H', hdr, pe + 6, 1)
    struct.pack_into('<H', hdr, pe + 20, 0xE0)
    struct.pack_into('<I', hdr, pe + 52, base)
    o = pe + 24 + 0xE0
    hdr[o:o + 8] = b'.text\0\0\0'
    struct.pack_into('<IIII', hdr, o + 8, len(sec), sec_va, len(sec), sec_raw)
    return bytes(hdr + sec)


def _exe() -> bytes:
    return _pe([('hkBaseObject', None, 4, []), ('hkReferencedObject', 'hkBaseObject', 8,
                                                 [('memSizeAndFlags', None, T_UINT16, 0, 0, 4)]),
                ('hkpShape', 'hkReferencedObject', 16, [('userData', None, T_ULONG, 0, 0, 8),
                                                         ('type', None, T_ENUM, T_INT32, 0, 12)])])


def _wrapper(packfile: bytes) -> bytes:
    """DICE's wrapper (big-endian) before a packfile: one part, no vectors, no boxes, a few bytes, then 1, 1, a hash."""
    head = struct.pack('>3I', 1, 1, 0xABCDEF01) + struct.pack('>H', 0) + struct.pack('>H', 0) + b'\x01\x02\x03'
    return head + struct.pack('>3I', 1, 1, 0x12345678) + packfile


def _console_pack(version: bytes = b'Havok-5.5.0-r1', layout: int = 0x04000001) -> bytes:
    """A big-endian-looking packfile header (enough for the wrapper and the walk's first refusal)."""
    head = bytearray(0x40 + 0x30 * 3)
    head[0:8] = HK_MAGIC
    struct.pack_into('>6I', head, 8, 0, 5, layout, 3, 2, 0)
    head[0x28:0x28 + len(version)] = version
    for i, name in enumerate((b'__classnames__', b'__types__', b'__data__')):
        head[0x40 + 0x30 * i:0x40 + 0x30 * i + len(name)] = name
        struct.pack_into('>7I', head, 0x40 + 0x30 * i + 20, len(head), 0, 0, 0, 0, 0, 0)
    return bytes(head)


def _manifest(entries: list[tuple[str, str]]) -> str:
    head = '# fbrb.manifest - entry order matters. name|type|flags(hex)|size2  zipped=1\n'
    rows = ''.join(f'{name}.res|{kind}|00010000|12\n' for name, kind in entries)
    return head + rows + 'Other/Thing.dbx|<non-resource>|00010000|9\r\nbroken line\n'


def _console_texture(fmt: int, w: int, h: int, mips: int, flags: int, texel: Any) -> bytes:
    """tests/test_converter_units.console_texture (texture.layout / tiled: the Python of the console layout)."""
    suite_paths.use('console-convert')
    import texture
    bpb, edge, _ = texture.FORMATS[fmt]
    where, size = texture.layout_reference(w, h, mips, fmt, 1)
    data = bytearray(size)
    for m in range(mips):
        start, pitch, ox, oy = where[0][m]
        bw, bh = max((max(w >> m, 1) + edge - 1) // edge, 1), max((max(h >> m, 1) + edge - 1) // edge, 1)
        for y in range(bh):
            for x in range(bw):
                at = start + texture.tiled_reference(x + ox, y + oy, pitch, {1: 0, 8: 3}[bpb]) * bpb
                data[at:at + bpb] = texel(m, x, y)
    if bpb > 1:
        for i in range(0, len(data) - 1, 2):
            data[i], data[i + 1] = data[i + 1], data[i]
    sizes = [size] + [0] * 14
    return struct.pack('>8I', 3, 0, fmt, flags, w, h, 1, mips) + struct.pack('>15I', *sizes) + bytes(data)


def _pc_texture(console: bytes, cmd: str = 'texture', exe: bytes | None = None) -> bytes:
    """The PC file of a console texture or physics resource (the inputs of the verify cases): the reference suite's
    convert.py (BFBC2_O2_REF, else this checkout; its Rust core module matches its sources)."""
    import subprocess
    import tempfile
    ref = os.environ.get('BFBC2_O2_REF') or SUITE
    with tempfile.TemporaryDirectory() as t:
        src, dst, x = os.path.join(t, 'c.res'), os.path.join(t, 'p.res'), os.path.join(t, 'x.exe')
        with open(src, 'wb') as f:
            f.write(console)
        extra = []
        if exe is not None:
            with open(x, 'wb') as f:
                f.write(exe)
            extra = ['--exe', x]
        subprocess.run([sys.executable, os.path.join(ref, 'bfbc2-console-convert', 'convert.py'), cmd, src, dst]
                       + extra, check=True, capture_output=True)
        with open(dst, 'rb') as f:
            return f.read()


def _smooth(m: int, x: int, y: int) -> bytes:
    return bytes([(x * 13 + y * 7 + m * 3) % 200 + 20])


def convert_cases() -> list[Case]:
    exe = _exe()
    pack = _console_pack()
    con_tex = _console_texture(10, 16, 16, 5, 0, _smooth)
    pc_tex = _pc_texture(con_tex)
    other_art = _pc_texture(_console_texture(10, 16, 16, 5, 0, lambda m, x, y: bytes([(x * y * 37 + m) % 251])))
    near = bytearray(pc_tex)
    near[92 + 5] ^= 1                                           # one texel off: the same picture
    garbage = _console_texture(10, 16, 16, 5, 0, lambda m, x, y: _smooth(0, x, y) if m == 0 else
                               bytes([(x * 97 + y * 31 + m * 59) % 256]))
    big_tex = _console_texture(10, 32, 16, 1, 0, _smooth)
    con = {
        'con\\fbrb.manifest': _manifest([('Physics/A', 'HavokPhysicsData'), ('Physics/B', 'HavokPhysicsData'),
                                         ('Physics/C', 'HavokPhysicsData'), ('Physics/Sub/D', 'HavokPhysicsData'),
                                         ('Physics/E', 'HavokPhysicsData'), ('Physics/X', 'HavokPhysicsData'), ('Textures/T1', 'XenonTexture'), ('Textures/T2', 'XenonTexture'),
                                         ('Textures/T3', 'XenonTexture'), ('Textures/T4', 'XenonTexture'),
                                         ('Textures/T5', 'XenonTexture'), ('Textures/T6', 'XenonTexture'),
                                         ('Textures/T7', 'XenonTexture'), ('Textures/Only', 'XenonTexture'),
                                         ('Textures/Gone', 'XenonTexture')]),
        'con\\Physics\\A.res': _wrapper(pack), 'con\\physics\\b.res': b'no packfile here' * 4,
        'con\\Physics\\C.res': _wrapper(_console_pack(b'Havok-4.0.0')), 'con\\Physics\\Sub\\D.res': _wrapper(pack),
        'con\\Physics\\E.res': _wrapper(_console_pack(layout=4)), 'con\\Physics\\X.res': _wrapper(pack),
        'con\\Textures\\T1.res': con_tex, 'con\\Textures\\T2.res': con_tex, 'con\\Textures\\T3.res': con_tex,
        'con\\Textures\\T4.res': garbage, 'con\\Textures\\T5.res': big_tex, 'con\\Textures\\T6.res': con_tex[:200],
        'con\\Textures\\T7.res': con_tex, 'con\\Textures\\Only.res': con_tex,
    }
    pc = {
        'pc\\fbrb.manifest': _manifest([('Physics/A', 'HavokPhysicsData'), ('physics/b', 'HavokPhysicsData'),
                                        ('Physics/C', 'HavokPhysicsData'), ('Physics/Sub/D', 'HavokPhysicsData'),
                                        ('Physics/E', 'HavokPhysicsData'), ('Physics/X', 'HavokPhysicsData'), ('Textures/T1', 'ITexture'), ('Textures/T2', 'ITexture'),
                                        ('Textures/T3', 'ITexture'), ('Textures/T4', 'ITexture'),
                                        ('Textures/T5', 'ITexture'), ('Textures/T6', 'ITexture'),
                                        ('Textures/T7', 'ITexture'), ('Textures/Gone', 'ITexture')]),
        'pc\\Physics\\A.res': _wrapper(pack)[:-4] + b'abcd', 'pc\\Physics\\B.res': b'no packfile here' * 4,
        'pc\\Physics\\C.res': _wrapper(_console_pack(b'Havok-4.0.0')), 'pc\\Physics\\Sub\\D.res': b'short',
        'pc\\Physics\\E.res': _wrapper(_console_pack(layout=4)), 'pc\\Physics\\X.res': _pc_texture(_wrapper(pack), 'havok', exe),
        'pc\\Textures\\T1.res': pc_tex, 'pc\\Textures\\T2.res': other_art, 'pc\\Textures\\T3.res': bytes(near),
        'pc\\Textures\\T4.res': pc_tex, 'pc\\Textures\\T5.res': pc_tex, 'pc\\Textures\\T6.res': pc_tex[:200],
        'pc\\Textures\\T7.res': pc_tex,
    }
    folders = dict(con, **pc, **{'exe\\BFBC2Game.exe': exe, 'notexe.exe': b'MZ' + bytes(300)})
    E = ['--exe', '{d}\\exe\\BFBC2Game.exe']
    rows: list[tuple[str, dict[str, Any], list[str], dict[str, str]]] = [
        ('no arguments', {}, [], {}),
        ('-h', {}, ['-h'], {}),
        ('--help', {}, ['--help'], {}),
        ('an unknown command', {}, ['what'], {}),
        ('havok -h', {}, ['havok', '-h'], {}),
        ('verify-havok -h', {}, ['verify-havok', '-h'], {}),
        ('texture -h', {}, ['texture', '-h'], {}),
        ('verify-texture --help', {}, ['verify-texture', '--help'], {}),
        ('havok without arguments', {}, ['havok'], {}),
        ('havok with one argument', {}, ['havok', 'x'], {}),
        ('havok with three arguments', {}, ['havok', 'x', 'y', 'z'], {}),
        ('havok --exe without a value', {}, ['havok', 'x', 'y', '--exe'], {}),
        ('verify-havok without a console folder', {}, ['verify-havok', 'pc'], {}),
        ('texture with an unknown option', {}, ['texture', 'a', 'b', '--what'], {}),
        ('-h after the arguments', {}, ['texture', 'a', 'b', '-h'], {}),
        ('havok: a missing exe', folders, ['havok', '{d}\\con\\Physics\\A.res', '{d}\\o.res', '--exe', '{d}\\none.exe'], {}),
        ('havok: an exe without class initializers', folders, ['havok', '{d}\\con\\Physics\\A.res', '{d}\\o.res', '--exe',
                                                               '{d}\\notexe.exe'], {}),
        # (no "no game copy" case: without OPFOR_ROOT's exe, default_exe looks beside the suite, so its answer
        # depends on where the checkout sits - a checkout beside project-opfor-bfbc2 finds the real exe)
        ('havok: a missing input', folders, ['havok', '{d}\\none.res', '{d}\\o.res'] + E, {}),
        ('havok: no packfile in the resource', folders, ['havok', '{d}\\con\\physics\\b.res', '{d}\\o.res'] + E, {}),
        ('havok: a packfile the walk refuses', folders, ['havok', '{d}\\con\\Physics\\A.res', '{d}\\o.res'] + E, {}),
        ('havok: other layout rules', folders, ['havok', '{d}\\con\\Physics\\E.res', '{d}\\o.res'] + E, {}),
        ('havok: another Havok version', folders, ['havok', '{d}\\con\\Physics\\C.res', '{d}\\o.res'] + E, {}),
        ('havok: --exe=', folders, ['havok', '{d}\\con\\Physics\\A.res', '{d}\\o.res', '--exe={d}\\exe\\BFBC2Game.exe'], {}),
        ('havok: --exe first, given twice', folders, ['havok', '--exe', '{d}\\none.exe', '{d}\\con\\Physics\\A.res'] + E
         + ['{d}\\o.res'], {}),
        ('havok: the exe from OPFOR_ROOT', dict(folders, **{'op\\game\\BFBC2Game.exe': exe}),
         ['havok', '{d}\\con\\Physics\\A.res', '{d}\\o.res'], {'OPFOR_ROOT': '{d}\\op'}),
        ('verify-havok', folders, ['verify-havok', '{d}\\pc', '{d}\\con'] + E, {}),
        ('verify-havok with a report', folders, ['verify-havok', '{d}\\pc', '{d}\\con', '--report', '{d}\\r.txt'] + E, {}),
        ('verify-havok, the console folder twice', folders, ['verify-havok', '{d}\\pc', '{d}\\con', '{d}\\con'] + E, {}),
        ('verify-havok, nothing in common', folders, ['verify-havok', '{d}\\pc', '{d}\\pc'] + E, {}),
        ('verify-havok, a folder without a manifest', folders, ['verify-havok', '{d}\\pc', '{d}\\exe'] + E, {}),
        ('verify-havok, a missing exe', folders, ['verify-havok', '{d}\\pc', '{d}\\con', '--exe', '{d}\\none.exe'], {}),
        ('texture', folders, ['texture', '{d}\\con\\Textures\\T1.res', '{d}\\out.res'], {}),
        ('texture of a big one', folders, ['texture', '{d}\\con\\Textures\\T5.res', '{d}\\out.res'], {}),
        ('texture of a cut one', folders, ['texture', '{d}\\con\\Textures\\T6.res', '{d}\\out.res'], {}),
        ('texture of a missing file', folders, ['texture', '{d}\\none.res', '{d}\\out.res'], {}),
        ('texture into a missing folder', folders, ['texture', '{d}\\con\\Textures\\T1.res', '{d}\\no\\out.res'], {}),
        ('texture of a havok resource', folders, ['texture', '{d}\\con\\Physics\\A.res', '{d}\\out.res'], {}),
        ('verify-texture', folders, ['verify-texture', '{d}\\pc', '{d}\\con'], {}),
        ('verify-texture with a report', folders, ['verify-texture', '{d}\\pc', '{d}\\con', '--report', '{d}\\r.txt'], {}),
        ('verify-texture, --report before the folders', folders, ['verify-texture', '--report', '{d}\\r.txt', '{d}\\pc',
                                                                  '{d}\\con'], {}),
        ('verify-texture, a missing manifest', folders, ['verify-texture', '{d}\\pc', '{d}\\none'], {}),
        ('texture, a path outside cp1252', dict(folders, **{'Ж.res': con_tex}), ['texture', '{d}\\Ж.res',
                                                                                     '{d}\\Жo.res'], {}),
        ('verify-havok, names outside cp1252', {
            'c\\fbrb.manifest': _manifest([('Physics/Ж', 'HavokPhysicsData')]), 'c\\Physics\\Ж.res': _wrapper(pack),
            'p\\fbrb.manifest': _manifest([('Physics/Ж', 'HavokPhysicsData')]), 'p\\Physics\\Ж.res': _wrapper(pack),
            'e.exe': exe}, ['verify-havok', '{d}\\p', '{d}\\c', '--exe', '{d}\\e.exe'], {}),
    ]
    out: list[Case] = [(f'convert: {name}', 'convert', files, args, env, False) for name, files, args, env in rows]
    return with_variants(out, ['convert: texture, a path outside cp1252', 'convert: verify-havok, names outside cp1252',
                               'convert: -h', 'convert: verify-texture'])


# ------------------------------------------------------------------------------------------- the group

# Round RT1: the lines whose answer changed on purpose when the Python reader bfbc2.exe hands them to lost its
# reference (aimanim.parse_reference): what they answer now, in place of the recording's fields named (the recording
# stays as the record of the retired answer). A NaN in an aim animation is refused by the Rust core, so the command
# line now ends with its error.
_AIM_REFUSED = 'error: not an aim animation the reader takes (a cut or damaged file, or a value it does not read)\r\n'
NOW: dict[str, dict[str, object]] = {
    'aimanim: a NaN': {'code': 1, 'out': '', 'err': _AIM_REFUSED},
    'aimanim: a NaN (merged)': {'code': 1, 'out': _AIM_REFUSED},
}


def cases() -> list[Case]:
    return aimanim_cases() + hpdtool_cases() + hkpack_cases() + convert_cases()


def _man_rows(folder: str, kind: str) -> dict[str, str]:
    """{lower-case name: name as written} of a real manifest's entries of a kind."""
    out: dict[str, str] = {}
    with open(os.path.join(folder, 'fbrb.manifest'), encoding='utf-8', errors='replace') as f:
        for line in f:
            q = line.rstrip('\n').split('|')
            if len(q) >= 2 and q[1] == kind and os.path.isfile(os.path.join(folder, *q[0].split('/'))):
                out[q[0].lower()] = q[0]
    return out


def opfor_calls(opfor: str) -> list[Call]:
    """Project OPFOR's real inputs, copied into the case folder (outputs there too; the exe read where it is):
    hpdtool on NavKit's built path data (work\\navkit), hkpack on MP_005's Havok assets (work\\audit), aimanim on
    BFBC2's aim poses (work\\audit\\mp_common), convert on Onslaught's console MP_002 (work\\onslaught: its console
    level bundle and the PC level it was built beside: the physics files both have, the 10 smallest textures)."""
    work = os.path.join(opfor, 'work')
    exe = os.path.join(opfor, 'game', 'BFBC2Game.exe')
    calls: list[Call] = []
    for lv in ('sp_009', 'ons_mp_002', 'sp_01'):
        p = os.path.join(work, 'navkit', lv, 'pathdata.hpd')
        if not os.path.isfile(p):
            continue
        src = {f'levels\\{lv}\\pathdata.hpd': p}
        for args in (['info'], ['sector', '0'], ['sector', '-1'], ['roundtrip'], ['coverage'], ['meshtrip']):
            argv = [args[0], f'{{d}}\\levels\\{lv}\\pathdata.hpd'] + args[1:]
            calls.append((f'hpdtool {" ".join(args)} {lv}', 'hpdtool', argv, src, {}, args[0] in ('info', 'sector')))
    level = os.path.join(work, 'audit', 'mp_005', 'Levels', 'MP_005')
    for name in ('StaticModelGroupEntity', 'RigidModelGroupEntity', 'ForestEntity', 'MiddlegrowthModelGroupEntity'):
        p = os.path.join(level, f'{name}_HavokAsset.res')
        if not os.path.isfile(p):
            continue
        src = {'a.res': p}
        calls.append((f'hkpack classes {name}', 'hkpack', ['classes', '{d}\\a.res'], src, {}, True))
        for cls in ('hkpStorageExtendedMeshShapeMeshSubpartStorage', 'hkpConvexVerticesShape', 'hkpRigidBody',
                    'hkRootLevelContainer'):
            calls.append((f'hkpack dump {name} {cls}', 'hkpack', ['dump', '{d}\\a.res', cls, '3'], src, {}, True))
    poses = os.path.join(work, 'audit', 'mp_common', 'Animations', 'Weapons', 'Handheld')
    if os.path.isdir(poses):
        found = sorted(os.path.join(dp, f) for dp, _, fs in os.walk(poses) for f in fs
                       if f.lower().endswith('generatedaimanimation.res'))
        for p in found[:: max(1, len(found) // 6)][:6]:
            calls.append((f'aimanim {os.path.basename(p)}', 'aimanim', ['{d}\\a.res'], {'a.res': p}, {}, True))
    build = os.path.join(work, 'onslaught', 'build_ons_mp_002')
    con_dir, pc_dir = os.path.join(build, 'con_level'), os.path.join(build, 'pc', 'mp_002_level_dist')
    if os.path.isfile(os.path.join(con_dir, 'fbrb.manifest')) and os.path.isfile(os.path.join(pc_dir, 'fbrb.manifest')) \
            and os.path.isfile(exe):
        files: dict[str, Any] = {}
        rows: dict[str, list[str]] = {'con': [], 'pc': []}
        ch, ph = _man_rows(con_dir, 'HavokPhysicsData'), _man_rows(pc_dir, 'HavokPhysicsData')
        ct, pt = _man_rows(con_dir, 'XenonTexture'), _man_rows(pc_dir, 'ITexture')
        texs = sorted(set(ct) & set(pt), key=lambda k: os.path.getsize(os.path.join(con_dir, *ct[k].split('/'))))[:10]
        for key in sorted(set(ch) & set(ph)):
            rows['con'].append(f'{ch[key]}|HavokPhysicsData|00010000|0')
            rows['pc'].append(f'{ph[key]}|HavokPhysicsData|00010000|0')
            files['con\\' + ch[key].replace('/', '\\')] = os.path.join(con_dir, *ch[key].split('/'))
            files['pc\\' + ph[key].replace('/', '\\')] = os.path.join(pc_dir, *ph[key].split('/'))
        for key in texs:
            rows['con'].append(f'{ct[key]}|XenonTexture|00010000|0')
            rows['pc'].append(f'{pt[key]}|ITexture|00010000|0')
            files['con\\' + ct[key].replace('/', '\\')] = os.path.join(con_dir, *ct[key].split('/'))
            files['pc\\' + pt[key].replace('/', '\\')] = os.path.join(pc_dir, *pt[key].split('/'))
        for side in ('con', 'pc'):
            files[f'{side}\\fbrb.manifest'] = (('\n'.join(rows[side]) + '\n').encode('utf-8'), 0)
        calls.append(('convert verify-havok (MP_002 console / PC)', 'convert',
                      ['verify-havok', '{d}\\pc', '{d}\\con', '--exe', exe, '--report', '{d}\\r.txt'], files, {}, False))
        calls.append(('convert verify-texture (MP_002 console / PC)', 'convert',
                      ['verify-texture', '{d}\\pc', '{d}\\con', '--report', '{d}\\r.txt'], files, {}, False))
        # the whole folders, read where they are (only the report written, into the case folder)
        for sub in ('mp_002_level_dist', 'mp_common_level_dist', 'sp_common_level_dist'):
            pcs = os.path.join(build, 'pc', sub)
            if os.path.isfile(os.path.join(pcs, 'fbrb.manifest')):
                calls.append((f'convert verify-havok {sub} / con_level', 'convert',
                              ['verify-havok', pcs, con_dir, '--exe', exe, '--report', '{d}\\r.txt'], {}, {}, False))
                calls.append((f'convert verify-texture {sub} / con_level', 'convert',
                              ['verify-texture', pcs, con_dir, '--report', '{d}\\r.txt'], {}, {}, False))
        for key in sorted(set(ch) & set(ph))[:4]:
            src = {'c.res': os.path.join(con_dir, *ch[key].split('/'))}
            calls.append((f'convert havok {key}', 'convert', ['havok', '{d}\\c.res', '{d}\\o.res', '--exe', exe], src,
                          {}, True))
        for key in texs[:4]:
            src = {'c.res': os.path.join(con_dir, *ct[key].split('/'))}
            calls.append((f'convert texture {key}', 'convert', ['texture', '{d}\\c.res', '{d}\\o.res'], src, {}, True))
    return calls
