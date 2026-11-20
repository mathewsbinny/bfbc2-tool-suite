r"""Round O3's cull group: the command lines test_cli_o3.py replays for culltree.py (bfbc2-culltree: static cull trees
- info, verify, add).

cases(): the quick tier's (synthetic trees and level partitions built in code, with the DBX writer and the sphere
keys copied from the reference so the inputs do not depend on the port; no game data). opfor_calls(opfor): the --game
tier's: console_cull.py's add (Onslaught levels' exclusive trees, OPFOR's --keep unpack in work\console_cull, their
Linux levels and every unpacked stock level of work\levels as references, read in place), verify and info.

LAD_SKIP: culltree.py keeps each reference level's rows in a pickle cache (%LOCALAPPDATA%\bfbc2-tool-suite\cache\
culltree, with caches.maybe_prune's stamps); bfbc2.exe reads and writes the same entries since round CT1, plus its
own listing entries, and pickles them with protocol 4 rather than 5, so that folder is not compared byte for byte.
The cache never changes an answer (rows from it equal the read ones); cases run with it on (cold: each case starts
with an empty local app data) and off.
"""
from __future__ import annotations

import math
import os
import struct
from typing import Any

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

LAD_SKIP = r'^bfbc2-tool-suite/cache/'
VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}

MAGIC = b'{binary}'
CONTAINER, TEXT, NUMBERS = 0xA, 2, 7
LEAF = 0x80000000
SPARE = 256


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- DBX files

class Builder:
    """A binary DBX file from nested tuples (tag, kind, [(name, value)], payload): children, a text, (size, [bytes])
    or the two-byte kinds' int (test_culltree.py's)."""

    def __init__(self) -> None:
        self.strings = ['']

    def s(self, text: str) -> int:
        if text not in self.strings:
            self.strings.append(text)
        return self.strings.index(text)

    @staticmethod
    def varint(v: int, out: bytearray) -> None:
        while True:
            b = v & 0x7F
            v >>= 7
            out.append(b | (0x80 if v else 0))
            if not v:
                return

    def element(self, e: Any, out: bytearray) -> None:
        tag, kind, attrs, payload = e
        self.varint(self.s(tag), out)
        out.append(kind << 4 | len(attrs))
        for k, v in attrs:
            self.varint(self.s(k), out)
            self.varint(self.s(v), out)
        if kind == CONTAINER:
            out.append(0)
            for c in payload:
                self.element(c, out)
            out.append(0)
        elif kind == TEXT:
            self.varint(self.s(payload), out)
        elif kind == NUMBERS:
            size, vals = payload
            self.varint(len(vals), out)
            self.varint(size, out)
            for v in vals:
                out.extend(v)
        else:
            out.extend(bytes([0, payload]))

    def build(self, root: Any, head12: bytes = b'\0\0\0\0') -> bytes:
        body = bytearray()
        self.element(root, body)
        body.append(0)
        table, text = bytearray(), bytearray()
        for s in self.strings:
            table += struct.pack('>I', len(text))
            text += s.encode('utf-8') + b'\0'
        area = bytes(table) + bytes(text)
        return MAGIC + struct.pack('>I', 24 + len(area)) + head12 + struct.pack('>II', len(area), len(self.strings)) \
            + area + bytes(body)


def num(name: str, size: int, vals: list[bytes]) -> Any:
    return ('field', NUMBERS, [('name', name)], (size, vals))


def num4i(name: str, v: int) -> Any:
    return num(name, 4, [struct.pack('>i', v)])


Node = tuple[int, float, int, int, int, int]


def tree_file(nodes: list[Node], enum_offset: int, key_size: int = 2, drop: str = '', extra: bool = False,
              guid: str = '11111111-0000-0000-0000-000000000001') -> bytes:
    items: list[Any] = []
    for r, mx, x, y, z, w in nodes:
        fields = [num4i('Right', r), num('Max', 4, [struct.pack('>f', mx)])]
        fields += [num('SphereKey' + a, key_size, [struct.pack('>H', k) + b'\0' * (key_size - 2)])
                   for a, k in zip('XYZW', (x, y, z, w))]
        if extra:                                   # a field the tool does not know: kept as it is
            fields.append(('field', TEXT, [('name', 'Note')], 'n'))
        items.append(('complex', CONTAINER, [], [f for f in fields if f[2][0][1] != drop]))
    root = ('partition', CONTAINER, [('guid', guid), ('primaryInstance', guid), ('exportMode', 'All')], [
        ('instance', CONTAINER, [('guid', guid), ('type', 'World.StaticCullTreeData'), ('id', 'StaticCullTree')], [
            ('array', CONTAINER, [('name', 'Nodes')], items),
            num4i('EnumOffset', enum_offset)])])
    return Builder().build(root, b'\x00\x01\x02\x03')


def level_file(objects: list[tuple[str, str, list[float], int]], tree_ref: str, client_ref: str = 'null') -> bytes:
    """objects: (guid, blueprint, transform 16 floats, enumeration)."""
    insts: list[Any] = [('instance', CONTAINER, [('guid', 'AAAAAAAA-0000-0000-0000-000000000000'),
                                                ('type', 'Levels.LevelData')], [
        ('field', TEXT, [('ref', tree_ref), ('name', 'ExclusiveStaticCullTree')], ''),
        ('field', TEXT, [('ref', client_ref), ('name', 'ClientStaticCullTree')], '')])]
    for guid, bp, m, en in objects:
        insts.append(('instance', CONTAINER, [('guid', guid), ('type', 'Entity.ReferenceObjectData')], [
            num4i('Enumeration', en),
            ('complex', NUMBERS, [('name', 'Transform')], (4, [struct.pack('>f', v) for v in m])),
            ('field', TEXT, [('ref', f'{bp}/{guid.lower()}'), ('name', 'ReferencedObject')], ''),
            ('field', 6, [('name', 'IsInObjectSet')], 0)]))
    root = ('partition', CONTAINER, [('guid', 'BBBBBBBB-0000-0000-0000-000000000000'), ('exportMode', 'All')], insts)
    return Builder().build(root)


# ------------------------------------------------------------------------------------------- spheres (the reference's)

def _dist(a: Any, b: Any) -> float:
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def encode(c: tuple[float, ...], r: float, contain: list[tuple[tuple[float, ...], float]] = ()) -> list[int]:  # type: ignore[assignment]
    keys = [int(round(v * 16)) for v in c]
    qc = tuple(k / 16 for k in keys)
    need = max([_dist(qc, c) + r] + [_dist(qc, sc) + sr for sc, sr in contain])
    return [k & 0xFFFF for k in keys] + [int(math.ceil(need * 4 - 1e-9))]


def decode(keys: list[int]) -> tuple[tuple[float, ...], float]:
    s = [k - 65536 if k >= 32768 else k for k in keys]
    return (s[0] / 16, s[1] / 16, s[2] / 16), s[3] / 4


def enclose(a: tuple[tuple[float, ...], float], b: tuple[tuple[float, ...], float]) -> tuple[tuple[float, ...], float]:
    (ac, ar), (bc, br) = a, b
    d = _dist(ac, bc)
    if d + br <= ar:
        return a
    if d + ar <= br:
        return b
    r = (d + ar + br) / 2
    t = (r - ar) / d
    return tuple(x + (y - x) * t for x, y in zip(ac, bc)), r


def build_nodes(leaves: list[tuple[int, float, list[int]]]) -> list[Node]:
    """Preorder nodes over leaves (index, Max, keys), joined in halves as add does; 256 spare nodes after."""
    def join(ns: list[Any]) -> Any:
        if len(ns) == 1:
            return ns[0]
        h = len(ns) // 2
        a, b = join(ns[:h]), join(ns[h:])
        sa, sb = decode(a['keys']), decode(b['keys'])
        s = enclose(sa, sb)
        return {'keys': encode(s[0], s[1], [sa, sb]), 'max': max(a['max'], b['max']), 'l': a, 'r': b}
    root = join([{'keys': k, 'max': mx, 'leaf': i} for i, mx, k in leaves])
    out: list[Node] = []

    def size(n: Any) -> int:
        return 1 if 'leaf' in n else 1 + size(n['l']) + size(n['r'])

    def pre(n: Any) -> None:
        if 'leaf' in n:
            out.append(((LEAF | n['leaf']) - (1 << 32), n['max'], *n['keys']))
        else:
            out.append((1 + size(n['l']), n['max'], *n['keys']))
            pre(n['l'])
            pre(n['r'])
    pre(root)
    return out + [(0, 0.0, 0, 0, 0, 0)] * SPARE


def rot_y(deg: float, pos: tuple[float, float, float], scale: float = 1.0) -> list[float]:
    a = math.radians(deg)
    c, s = math.cos(a) * scale, math.sin(a) * scale
    return [c, 0, -s, 0, 0, scale, 0, 0, s, 0, c, 0, pos[0], pos[1], pos[2], 1]


# a blueprint's leaves in its own space: (local centre, true radius)
SHAPES = {'objects/test/hut/hut': [((1.5, 2.0, 0.5), 6.2)] * 2,
          'objects/test/lamp/lamp': [((0.0, 0.0, 0.4), 0.47)] * 2,
          'objects/test/rock/rock': [((0.0, 0.5, 0.0), 2.0)],
          'objects/test/tower/tower': [((0.0, 4.0, 0.0), 5.0), ((0.0, 12.0, 0.0), 3.0), ((1.0, 18.0, 1.0), 1.25)],
          'objects/test/wall/wall': [((-3.0, 1.0, 0.0), 3.1), ((3.0, 1.0, 0.0), 3.1)]}


def world(bp: str, m: list[float]) -> list[tuple[tuple[float, ...], float]]:
    return [(tuple(c[0] * m[0 + i] + c[1] * m[4 + i] + c[2] * m[8 + i] + m[12 + i] for i in range(3)), r)
            for c, r in SHAPES[bp]]


Obj = tuple[str, str, list[float], int, list[float] | None]     # guid, blueprint, transform, slotted, leaves' transform


def make(objs: list[Obj], eo: int, **tree_kw: Any) -> tuple[bytes, list[tuple[str, str, list[float], int]]]:
    """The tree over the slotted objects' leaves (an object's leaves may stand where another transform puts them: a
    misplaced slot) and the level's objects with their slots."""
    leaves: list[tuple[int, float, list[int]]] = []
    placed = []
    nxt = 1
    for guid, bp, m, slot, at in objs:
        if not slot:
            placed.append((guid, bp, m, 0))
            continue
        placed.append((guid, bp, m, eo + nxt))
        for c, r in world(bp, at or m):
            leaves.append((nxt, r, encode(c, r)))
            nxt += 1
    return tree_file(build_nodes(leaves), eo, **tree_kw), placed


def guid(n: int, prefix: str = '0') -> str:
    return f'{prefix * 8}-0000-0000-0000-{n:012d}'


TARGET: list[Obj] = [
    (guid(1), 'objects/test/hut/hut', rot_y(0, (10, 5, 20)), 1, None),
    (guid(2), 'objects/test/hut/hut', rot_y(90, (-40, 6, 60)), 1, None),
    (guid(3), 'objects/test/rock/rock', rot_y(30, (5, 1, -70)), 1, None),
    (guid(4), 'objects/test/hut/hut', rot_y(180, (80, 4, -10)), 1, None),
    (guid(5), 'objects/test/rock/rock', rot_y(10, (-60, 2, -30)), 1, None),
    (guid(6), 'objects/test/hut/hut', rot_y(237, (30, 7, 90)), 0, None),
    (guid(7), 'objects/test/lamp/lamp', rot_y(45, (35, 7, 95)), 0, None),
    (guid(8), 'objects/test/tower/tower', rot_y(15, (100, 0, 100), 1.5), 1, None),
    (guid(9), 'objects/test/tower/tower', rot_y(75, (-100, 0, 100)), 1, None),
    (guid(10), 'objects/test/tower/tower', rot_y(200, (0, 3, -150)), 1, None),
    (guid(11), 'objects/test/tower/tower', rot_y(300, (50, 1, 150)), 0, None),
    (guid(12), 'objects/test/hut/hut', rot_y(45, (-20, 5, -90)), 1, rot_y(45, (-29, 5, -90))),      # misplaced 9 m
    (guid(13), 'objects/test/lamp/lamp', rot_y(0, (2045.5, 0, 10)), 0, None),       # beyond the tree's range
    (guid(14), 'objects/test/wall/wall', rot_y(0, (60, 0, 60)), 0, None),           # no shape anywhere
    (guid(15), 'objects/test/rock/rock', rot_y(120, (-5, 1, 5)), 1, None),
    (guid(16), 'objects/test/rock/rock', rot_y(300, (15, 1, -5)), 0, None),
]
TREE_REF = 'Levels/Test/staticculltree_exclusive/11111111-0000-0000-0000-000000000001'
EO = 100


def target_files(objs: list[Obj] | None = None, eo: int = EO, folder: str = 'lv', **kw: Any) -> dict[str, Any]:
    tree, placed = make(objs or TARGET, eo, **kw)
    return {f'{folder}/Levels/Test/staticculltree_exclusive.dbx': tree,
            f'{folder}/Levels/Test.dbx': level_file(placed, TREE_REF)}


def ref_files() -> dict[str, Any]:
    """A --ref-dir: two reference levels (lamps and huts; towers), a non-level partition, a level whose tree is
    missing, one whose tree is broken, one named like the first with a tree of the same size (taken once), a
    description and a tree among them (left out)."""
    out: dict[str, Any] = {}
    lamps: list[Obj] = [(guid(i, '1'), 'objects/test/lamp/lamp', rot_y(20 * i, (i * 7, 3, -i * 5)), 1, None)
                        for i in range(1, 5)]
    lamps += [(guid(9, '1'), 'objects/test/hut/hut', rot_y(33, (300, 2, 300)), 1, None)]
    t, p = make(lamps, 40)
    out['refs/a/Levels/RefA/staticculltree_exclusive.dbx'] = t
    out['refs/a/Levels/RefA.dbx'] = level_file(p, 'levels/RefA/StaticCullTree_Exclusive/1111')
    out['refs/a/Levels/RefA_description.dbx'] = level_file(p, 'levels/RefA/StaticCullTree_Exclusive/1111')
    towers: list[Obj] = [(guid(i, '2'), 'objects/test/tower/tower', rot_y(40 * i, (i * 30, 0, 9)), 1, None)
                         for i in range(1, 4)]
    t, p = make(towers, 7)
    out['refs/b/levels/RefB/staticculltree_exclusive.dbx'] = t
    out['refs/b/levels/RefB.dbx'] = level_file(p, 'Levels/RefB/staticculltree_exclusive/2222')
    out['refs/b/levels/RefB/other.dbx'] = Builder().build(('partition', CONTAINER, [], [('instance', CONTAINER,
                                                                                        [('type', 'X.Y')], [])]))
    out['refs/c/Levels/NoTree.dbx'] = level_file(p, 'Levels/NoTree/staticculltree_exclusive/3333')
    out['refs/c/Levels/Broken.dbx'] = level_file(p, 'Levels/Broken/staticculltree_exclusive/4444')
    out['refs/c/Levels/Broken/staticculltree_exclusive.dbx'] = level_file(p, 'x')         # a level, not a tree
    out['refs/d/Levels/RefA.dbx'] = out['refs/a/Levels/RefA.dbx']
    out['refs/d/Levels/RefA/staticculltree_exclusive.dbx'] = out['refs/a/Levels/RefA/staticculltree_exclusive.dbx']
    out['refs/d/notes.txt'] = 'not a partition'
    return out


def client_file(leaves: int, eo: int = 0) -> bytes:
    lf = [(i, 1.0, encode((float(i), 0.0, 0.0), 1.0)) for i in range(1, leaves + 1)]
    return tree_file(build_nodes(lf), eo)


# ------------------------------------------------------------------------------------------- the cases

def cases() -> list[Case]:
    rows: list[Case] = []

    def add(key: str, files: dict[str, Any], args: list[str], env: dict[str, str] | None = None) -> None:
        rows.append((key, 'culltree', files, args, env or {}, False))

    T, L = '{d}\\lv\\Levels\\Test\\staticculltree_exclusive.dbx', '{d}\\lv\\Levels\\Test.dbx'
    OT, OL = '{d}\\out\\tree.dbx', '{d}\\out\\level.dbx'
    tf = target_files()
    rf = ref_files()
    both = dict(tf, **rf)
    refdirs = ['--ref-dir', '{d}\\refs\\a', '--ref-dir', '{d}\\refs\\b', '--ref-dir', '{d}\\refs\\c',
               '--ref-dir', '{d}\\refs\\d']
    # the command line
    add('usage', {}, [])
    add('help', {}, ['-h'])
    add('help word', {}, ['help'])
    add('unknown command', {}, ['bogus', 'x'])
    add('info no tree', {}, ['info'])
    add('verify no args', {}, ['verify'])
    add('verify one arg', tf, ['verify', T])
    add('add no args', {}, ['add'])
    add('flag without value', tf, ['verify', T, L, '--ref'])
    add('add without outputs', tf, ['add', T, L, '--auto'])
    add('add without objects', tf, ['add', T, L, '--out-tree', OT, '--out-level', OL])
    add('add empty objects', tf, ['add', T, L, '--objects', '', '--out-tree', OT, '--out-level', OL])
    add('add over input', tf, ['add', T, L, '--auto', '--out-tree', '{d}' + T[3:].upper(), '--out-level', OL])
    add('add over level', tf, ['add', T, L, '--auto', '--out-tree', OT, '--out-level', '{d}\\lv\\Levels\\..\\Levels\\test.dbx'])
    add('add bad misplaced', tf, ['add', T, L, '--auto', '--misplaced', 'two', '--out-tree', OT, '--out-level', OL])
    add('add misplaced underscore', tf, ['add', T, L, '--auto', '--misplaced', ' 1_0 ', '--out-tree', OT, '--out-level', OL])
    # info
    add('info tree', tf, ['info', T])
    add('info tree level', tf, ['info', T, L])
    add('info extra args', tf, ['info', T, L, 'more'])
    add('info missing', {}, ['info', '{d}\\missing.dbx'])
    add('info folder', {'sub': None}, ['info', '{d}\\sub'])
    add('info text file', {'a.dbx': 'hello, not binary at all'}, ['info', '{d}\\a.dbx'])
    add('info level as tree', tf, ['info', L])
    cut = tf['lv/Levels/Test/staticculltree_exclusive.dbx']
    add('info truncated', {'t.dbx': cut[:len(cut) // 2]}, ['info', '{d}\\t.dbx'])
    add('info unclosed', {'t.dbx': cut[:-(SPARE * 20)]}, ['info', '{d}\\t.dbx'])
    add('info key size 4', target_files(key_size=4), ['info', T, L])
    add('info extra field', target_files(extra=True), ['info', T])
    add('info no Max', target_files(drop='Max'), ['info', T])
    add('info no SphereKeyW', target_files(drop='SphereKeyW'), ['info', T])
    nodes = [(0, 0.0, 0, 0, 0, 0)] * 4
    add('info empty tree', {'t.dbx': tree_file(nodes, 3)}, ['info', '{d}\\t.dbx'])
    lf = [(i, 1.0, encode((float(i), 0.0, 0.0), 1.0)) for i in range(1, 6)]
    good = build_nodes(lf)
    bad_right = [(good[0][0] + 1,) + good[0][1:]] + good[1:]
    add('info bad right', {'t.dbx': tree_file(bad_right, 3)}, ['info', '{d}\\t.dbx'])
    loop = [(1,) + good[0][1:]] + good[1:]
    add('info right too small', {'t.dbx': tree_file(loop, 3)}, ['info', '{d}\\t.dbx'])
    past = [(500,) + good[0][1:]] + good[1:]
    add('info right past end', {'t.dbx': tree_file(past, 3)}, ['info', '{d}\\t.dbx'])
    dup = list(good)
    dup[2] = (dup[1][0],) + dup[2][1:]
    add('info leaf twice', {'t.dbx': tree_file(dup, 3)}, ['info', '{d}\\t.dbx'])
    unreached2 = good[:9] + [((LEAF | 7) - (1 << 32), 1.0, 1, 2, 3, 4)] + good[9:]
    add('info unreached node', {'t.dbx': tree_file(unreached2, 3)}, ['info', '{d}\\t.dbx'])
    gaps = [g for g in good[:9]] + [(0, 0.0, 0, 0, 0, 0)] * 3
    add('info few spare', {'t.dbx': tree_file(gaps, 3)}, ['info', '{d}\\t.dbx'])
    odd = [(i, 1.0, encode((float(i) * 400, 50.0 * i, -80.0 * i), 0.3 + i)) for i in (3, 1, 2, 9)]
    add('info leaf numbers', {'t.dbx': tree_file(build_nodes(odd), 0)}, ['info', '{d}\\t.dbx'])
    # non-ASCII paths: cp1252 (é) prints; Ω does not (UnicodeEncodeError at the first line), nor an emoji
    for label, folder in (('e-acute', 'café'), ('omega', 'Ωmega'), ('emoji', 'map\U0001F600')):
        files = {f'{folder}/t.dbx': cut}
        add(f'info path {label}', files, ['info', f'{{d}}\\{folder}\\t.dbx'])
    # verify
    add('verify', tf, ['verify', T, L])
    add('verify refs', both, ['verify', T, L] + refdirs)
    add('verify ref level', both, ['verify', T, L, '--ref', '{d}\\refs\\a\\Levels\\RefA.dbx', '--ref',
                                   '{d}\\refs\\c\\Levels\\NoTree.dbx', '--ref', '{d}\\refs\\c\\Levels\\Broken.dbx'])
    add('verify ref missing', tf, ['verify', T, L, '--ref', '{d}\\nothing.dbx'])
    add('verify ref not a level', dict(tf, **{'x.dbx': 'text'}), ['verify', T, L, '--ref', '{d}\\x.dbx'])
    one = [(guid(1, '3'), 'objects/test/rock/rock', rot_y(0, (0, 0, 0)), 1)]
    add('verify ref damaged tree', dict(tf, **{'r/Levels/D.dbx': level_file(one, 'Levels/D/staticculltree_exclusive/5'),
                                               'r/Levels/D/staticculltree_exclusive.dbx': b'{binary}' + bytes(30)}),
        ['verify', T, L, '--ref-dir', '{d}\\r'])
    add('verify ref dir empty', dict(tf, **{'empty': None}), ['verify', T, L, '--ref-dir', '{d}\\empty'])
    add('verify ref dir self', both, ['verify', T, L, '--ref-dir', '{d}\\lv', '--ref-dir', '{d}\\refs\\a'])
    add('verify client good', dict(tf, **{'client.dbx': client_file(EO)}), ['verify', T, L, '--client', '{d}\\client.dbx'])
    add('verify client bad', dict(tf, **{'client.dbx': client_file(7, 2)}), ['verify', T, L, '--client', '{d}\\client.dbx'])
    add('verify added', both, ['verify', T, L, '--added', f'{guid(1)},{guid(8).lower()}, ,{guid(6)},nope,'] + refdirs)
    add('verify report', both, ['verify', T, L, '--report', '{d}\\rep\\sub\\verify.txt'] + refdirs)
    add('verify cache off', both, ['verify', T, L] + refdirs, {'BFBC2_CULLTREE_CACHE': '0'})
    add('verify reference', both, ['verify', T, L] + refdirs, {'BFBC2_CULLTREE_REFERENCE': '1'})
    add('verify level as level', tf, ['verify', T, T])
    add('verify key size 4', target_files(key_size=4), ['verify', T, L])
    # add
    add('add auto', both, ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL] + refdirs)
    add('add auto misplaced', both, ['add', T, L, '--auto', '--misplaced', '2.0', '--out-tree', OT, '--out-level', OL]
        + refdirs)
    add('add auto misplaced report client', dict(both, **{'client.dbx': client_file(EO)}),
        ['add', T, L, '--auto', '--misplaced', '2', '--client', '{d}\\client.dbx', '--report', '{d}\\r\\add.txt',
         '--out-tree', OT, '--out-level', OL] + refdirs)
    add('add auto misplaced far', both, ['add', T, L, '--auto', '--misplaced', '0.05', '--out-tree', OT, '--out-level',
                                         OL] + refdirs)
    add('add auto no refs', tf, ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL])
    add('add auto reference', both, ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL] + refdirs,
        {'BFBC2_CULLTREE_REFERENCE': '1'})
    add('add auto cache off', both, ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL] + refdirs,
        {'BFBC2_CULLTREE_CACHE': '0'})
    add('add auto key size 4', dict(target_files(key_size=4), **rf),
        ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL] + refdirs)
    add('add auto extra field', dict(target_files(extra=True), **rf),
        ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL] + refdirs)
    add('add objects', both, ['add', T, L, '--objects', f'{guid(6)},{guid(16).lower()} ', '--out-tree', OT,
                              '--out-level', OL, '--ref-dir', '{d}\\refs\\a'])
    add('add objects file', dict(both, **{'objs.txt': f'# the huts\n{guid(6)}  # first\n\n{guid(11)}\r\n# end\n'}),
        ['add', T, L, '--objects', '@{d}\\objs.txt', '--out-tree', OT, '--out-level', OL] + refdirs)
    add('add objects file missing', tf, ['add', T, L, '--objects', '@{d}\\none.txt', '--out-tree', OT, '--out-level', OL])
    add('add object unknown', tf, ['add', T, L, '--objects', 'abc', '--out-tree', OT, '--out-level', OL])
    add('add object slotted', tf, ['add', T, L, '--objects', guid(1), '--out-tree', OT, '--out-level', OL])
    add('add object no shape', tf, ['add', T, L, '--objects', guid(14), '--out-tree', OT, '--out-level', OL])
    add('add object lamp no ref', tf, ['add', T, L, '--objects', guid(7), '--out-tree', OT, '--out-level', OL])
    add('add object far', both, ['add', T, L, '--objects', guid(13), '--out-tree', OT, '--out-level', OL] + refdirs)
    add('add twice', both, ['add', T, L, '--objects', f'{guid(6)},{guid(6)}', '--out-tree', OT, '--out-level', OL])
    full = [o for o in TARGET if o[3] or o[1] == 'objects/test/wall/wall']
    add('add nothing', target_files(full), ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL])
    add('add flags repeated', both, ['add', T, L, '--auto', '--out-tree', '{d}\\x.dbx', '--out-tree', OT, '--out-level',
                                     OL, '--ref-dir', '{d}\\refs\\a', '--auto'])
    add('add path e-acute', {f'café/{k}': v for k, v in both.items()},
        ['add', T.replace('{d}', '{d}\\café'), L.replace('{d}', '{d}\\café'), '--auto', '--out-tree', OT,
         '--out-level', OL, '--ref-dir', '{d}\\café\\refs\\a'])
    add('add path omega', {f'Ω/{k}': v for k, v in tf.items()},
        ['add', T.replace('{d}', '{d}\\Ω'), L.replace('{d}', '{d}\\Ω'), '--auto', '--out-tree', OT,
         '--out-level', OL])
    big = list(TARGET) + [(guid(100 + i), 'objects/test/rock/rock', rot_y(i * 7, (i * 3.0 - 600, 1, i * 2.0)), 0, None)
                          for i in range(400)]
    add('add many', dict(target_files(big), **rf), ['add', T, L, '--auto', '--out-tree', OT, '--out-level', OL]
        + refdirs)
    add('add out to folder', both, ['add', T, L, '--auto', '--out-tree', '{d}\\lv', '--out-level', OL])
    rows = with_variants(rows, ['usage', 'add auto misplaced', 'info path omega', 'unknown command', 'add many'])
    return rows


# ------------------------------------------------------------------------------------------- the game (--game)

def opfor_calls(opfor: str) -> list[Call]:
    r"""console_cull.py's command line (add --auto --misplaced 2.0, --ref <its Linux level>, --ref-dir every unpacked
    stock level of work\levels, read in place) on each Onslaught level as console_cull ran it: the exclusive tree
    and the level partition from the bundle before its install (game\Package\levels\<lv>\level-00.fbrb
    .opfor-cull-orig, read in memory; the case folder holds them as Levels\<LV>\...); verify of the outputs
    OPFOR kept (work\console_cull\<lv>\tree.dbx, level.dbx, copied) with the same references; info of each
    tree with its level."""
    import sys
    suite = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
    if suite not in sys.path:
        sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core')
    from pathlib import Path

    from bfbc2core import fbrb
    calls: list[Call] = []
    work = os.path.join(opfor, 'work', 'console_cull')
    if not os.path.isdir(work):
        return calls
    levels = os.path.join(opfor, 'work', 'levels')
    refdirs: list[str] = []
    for d in sorted(os.listdir(levels)) if os.path.isdir(levels) else []:
        if os.path.isdir(os.path.join(levels, d)):
            refdirs += ['--ref-dir', os.path.join(levels, d)]
    pc_map = {'ons_mp_002': 'mp_002', 'ons_mp_004': 'mp_004', 'ons_mp_005': 'mp_005', 'ons_mp_008': 'mp_008'}
    for n, lv in enumerate(sorted(os.listdir(work))):
        bundle = os.path.join(opfor, 'game', 'Package', 'levels', lv, 'level-00.fbrb.opfor-cull-orig')
        if not os.path.isfile(bundle):
            continue
        name = lv.upper()
        tree_e, level_e = f'levels/{lv}/staticculltree_exclusive.dbx', f'levels/{lv}.dbx'
        got = fbrb.read_uncached(Path(bundle), [tree_e, level_e])
        mtime = os.path.getmtime(bundle)
        files: dict[str, Any] = {f'Levels/{name}/staticculltree_exclusive.dbx': (got[tree_e], mtime),
                                 f'Levels/{name}.dbx': (got[level_e], mtime)}
        refs: list[str] = []
        pc = pc_map.get(lv)
        if pc:
            linux = os.path.join(opfor, 'work', 'onslaught', f'linux_{pc}', 'level', 'Levels', pc.upper() + '.dbx')
            if os.path.isfile(linux):
                refs = ['--ref', linux]
        T, L = f'{{d}}\\Levels\\{name}\\staticculltree_exclusive.dbx', f'{{d}}\\Levels\\{name}.dbx'
        calls.append((f'add {lv}', 'culltree', ['add', T, L, '--auto', '--misplaced', '2.0', *refs, *refdirs,
                                                '--out-tree', '{d}\\tree.dbx', '--out-level', '{d}\\level.dbx'],
                      files, {}, True))
        calls.append((f'info {lv}', 'culltree', ['info', T, L], files, {}, True))
        kept_t, kept_l = os.path.join(work, lv, 'tree.dbx'), os.path.join(work, lv, 'level.dbx')
        if n < 2 and os.path.isfile(kept_t) and os.path.isfile(kept_l):
            calls.append((f'verify {lv} (its add output)', 'culltree',
                          ['verify', '{d}\\tree.dbx', '{d}\\level.dbx', *refs, *refdirs],
                          {'tree.dbx': kept_t, 'level.dbx': kept_l}, {}, False))
    return calls
