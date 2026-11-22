r"""culltreelib.py - Battlefield: Bad Company 2's static cull trees (World.StaticCullTreeData): read them, check
them against the level's placed objects, and add placed objects that have no slot (slot 0: see-through and
indestructible in game) with correctly built subtrees. The library of `bfbc2 culltree info | verify | add`.

Round RT3 retired the command line culltree.py: bfbc2.exe answers it alone (bfbc2core.native.RETIRED_CLI; its usage
text, which describes the three commands, is rust\bc2-cli\src\cull\culltree_usage.txt). This module keeps the
readers, the checks and the builder the tests import (CullTree, Level, collect, ref_rows, add, check_structure,
object_checks, misplaced, auto_objects ...), F4's string-table reference (kept by ruling), and the row cache that
bfbc2.exe shares (its key is this file's hash: rust\bc2-cli\src\cull includes this file for it).

The format is described in README.md ("Static cull trees"). The game reads the trees with the traversal at
0x0056C4E0 in BFBC2Game.exe (build 795745); the notes there come from short targeted disassembly only.
"""
import contextlib
import glob
import math
import os
import struct
import sys
from collections import Counter, defaultdict
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..'))
MAGIC = b'{binary}'
CONTAINER, TEXT, NUMBERS = 0xA, 2, 7
LEAF = 0x80000000
SPARE = 256                    # all-zero nodes after the used ones in every stock tree
CENTER_SCALE = 16.0            # SphereKeyX/Y/Z = round(centre * 16), signed 16-bit
RADIUS_SCALE = 4.0             # SphereKeyW = radius * 4, signed 16-bit
STACK_SLOTS = 128              # traversal stack: dwords at +0x20 .. +0x21F of the query object
NODE_FIELDS = ('Right', 'Max', 'SphereKeyX', 'SphereKeyY', 'SphereKeyZ', 'SphereKeyW')

NodeData = tuple[Any, ...]                     # a node as stored: (Right, Max, kx, ky, kz, kw)
Vec = tuple[float, ...]                        # a point (x, y, z)
ShapeLeaf = tuple[Vec, float, float]           # a blueprint's leaf: (local centre, radius key, Max)
Instance = tuple[str, int, list[ShapeLeaf], 'Placed']    # collect(): (label, leaf count, leaves, object)
Found = Mapping[str, list[Instance]]           # collect()'s {blueprint: [instance]}
Pair = tuple['CullTree', 'Level', str]         # (tree, level, label); collect() also takes RefRows
Log = Callable[[str], object]


class CullTreeError(Exception):
    pass


# ------------------------------------------------------------------------------------------------ the Rust core
# Doc's element reader and ref_pairs' search for level partitions run in the suite's Rust core (rust/, module
# bc2_native: bc2_formats::culltree). The Python element reader was retired on 2026-11-18 (round F1a,
# rust/BENCHMARKS.md "Retired Python"); its answers on every tree and level partition of the game copy and OPFOR's
# work folders are kept as recorded digests (tests/culltree_golden.json, compared by tests/test_culltree_native.py).
# The tool needs the module: run it inside the suite (python rust/build_native.py builds it, or --download).

_native: Any = None


def _rust() -> Any:
    """The Rust core, or NativeMissing saying how to get it."""
    global _native
    if _native is None:
        if SUITE not in sys.path:
            sys.path.insert(0, SUITE)
        import suite_paths
        suite_paths.use('core')
        from bfbc2core import native
        _native = native.require('culltree.py (cull trees and level partitions)', 'culltree_elements')
    return _native


# Round F4: Doc's string table in the Rust core too (bc2_native.culltree_strings), the Python below its reference and
# fallback (a table the Rust core does not read answers None, and the Python raises its own error).
# BFBC2_PURE_PYTHON=1 or use_native(False) reads it in Python.
_STRINGS_NATIVE: list[bool] = []


def use_native(on: bool) -> bool:
    """The string table read in the Rust core (on) or in Python (off) for this process; True when Rust reads it."""
    _STRINGS_NATIVE[:] = [bool(on)]
    return bool(on) and hasattr(_rust(), 'culltree_strings')


def _strings(data: bytes, count: int) -> list[str]:
    """Doc's string table: count strings, each at its offset (the big-endian word at 24 + 4 i) from 24 + 4 count, up
    to its zero byte."""
    on = _STRINGS_NATIVE[0] if _STRINGS_NATIVE else os.environ.get('BFBC2_PURE_PYTHON', '') in ('', '0')
    fn = getattr(_rust(), 'culltree_strings', None) if on else None
    got = fn(data, count) if fn is not None and type(data) is bytes and type(count) is int else None
    return got if got is not None else _strings_reference(data, count)


def _strings_reference(data: bytes, count: int) -> list[str]:
    """The Python (the reference)."""
    base = 24 + 4 * count
    strings: list[str] = []
    for i in range(count):
        off = struct.unpack_from('>I', data, 24 + 4 * i)[0]
        end = data.index(b'\0', base + off)
        strings.append(data[base + off:end].decode('utf-8', 'replace'))
    return strings


# ------------------------------------------------------------------------------------------------ binary DBX
# A minimal codec (bfbc2-dbx-tools/FORMAT.md sections 1, 2 and 4) that keeps every byte, so an unchanged file is
# written back exactly and a changed one differs only where values or array items were changed.

def _varint(data: bytes, pos: int) -> tuple[int, int]:
    v = shift = 0
    while True:
        b = data[pos]
        pos += 1
        v |= (b & 0x7F) << shift
        if b < 0x80:
            return v, pos
        shift += 7


def _put_varint(v: int, out: bytearray) -> None:
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            out.append(b | 0x80)
        else:
            out.append(b)
            return


class El:
    __slots__ = ('tag', 'head', 'attrs', 'extra', 'value', 'children', 'doc')
    doc: 'Doc'
    tag: int
    head: int
    attrs: list[tuple[int, int]]
    extra: int
    value: bytes
    children: 'list[El] | None'               # a container's elements; None for a value

    def __init__(self, doc: 'Doc', tag: int, head: int, attrs: list[tuple[int, int]]) -> None:
        self.doc, self.tag, self.head, self.attrs = doc, tag, head, attrs
        self.extra = 0
        self.value = b''
        self.children = None

    @property
    def kind(self) -> int:
        return self.head >> 4

    def attr(self, name: str) -> str | None:
        s = self.doc.strings
        for k, v in self.attrs:
            if s[k] == name:
                return s[v]
        return None

    @property
    def name(self) -> str | None:
        return self.attr('name')

    def child(self, name: str) -> 'El | None':
        for c in self.children or ():
            if c.name == name:
                return c
        return None

    def numbers(self) -> tuple[int, list[bytes]]:
        """(size, [raw big-endian values]) of a NUMBERS element."""
        count, p = _varint(self.value, 0)
        size, p = _varint(self.value, p)
        return size, [self.value[p + i * size:p + (i + 1) * size] for i in range(count)]

    def set_numbers(self, size: int, values: Sequence[bytes]) -> None:
        out = bytearray()
        _put_varint(len(values), out)
        _put_varint(size, out)
        for v in values:
            if len(v) != size:
                raise CullTreeError('internal: number of the wrong size')
            out += v
        self.value = bytes(out)

    def clone(self) -> 'El':
        e = El(self.doc, self.tag, self.head, list(self.attrs))
        e.extra, e.value = self.extra, self.value
        if self.children is not None:
            e.children = [c.clone() for c in self.children]
        return e


class Doc:
    def __init__(self, data: bytes | bytearray, name: str) -> None:
        data = bytes(data)
        if len(data) <= 24 or not data.startswith(MAGIC):
            raise CullTreeError(f'{name} is not a binary .dbx file')
        self.name = name
        self.head12 = data[12:16]
        size, count = struct.unpack_from('>II', data, 16)
        self.string_area = data[24:24 + size]
        self.strings: list[str] = _strings(data, count)
        pos = 24 + size
        self.roots: list[El]
        try:
            self.roots, end = _rust().culltree_elements(data, pos, self, El)
        except ValueError as e:               # the stream cut short, unclosed, or a number past 64 bits
            raise CullTreeError(f'{name}: {e}') from None
        self.trailer = data[end:]

    @classmethod
    def read(cls, path: str) -> 'Doc':
        with open(path, 'rb') as f:
            return cls(f.read(), os.path.basename(path))

    def tobytes(self) -> bytes:
        out = bytearray()

        def put(e: El) -> None:
            _put_varint(e.tag, out)
            out.append(e.head)
            for k, v in e.attrs:
                _put_varint(k, out)
                _put_varint(v, out)
            if e.kind == CONTAINER:
                out.append(e.extra)
                assert e.children is not None          # the reader gives every container a list
                for c in e.children:
                    put(c)
                out.append(0)
            else:
                out.extend(e.value)
        for r in self.roots:
            put(r)
        payload = bytes(out) + self.trailer
        n = len(self.strings)
        header = MAGIC + struct.pack('>I', 24 + len(self.string_area)) + self.head12 + struct.pack('>II', len(self.string_area), n)
        return header + self.string_area + payload

    def write(self, path: str) -> None:
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(self.tobytes())

    @property
    def instances(self) -> list[El]:
        return [e for e in self.roots[0].children or () if self.strings[e.tag] == 'instance']


def _int(e: El) -> int:
    size, v = e.numbers()
    if size != 4 or len(v) != 1:
        raise CullTreeError('internal: not a single 4-byte number')
    return struct.unpack('>i', v[0])[0]


def _floats(e: El) -> list[float]:
    size, v = e.numbers()
    return [struct.unpack('>f', x)[0] for x in v] if size == 4 else []


# ------------------------------------------------------------------------------------------------ the tree

def s16(v: int) -> int:
    return v - 65536 if v >= 32768 else v


def u16(v: int) -> int:
    return v & 0xFFFF


class Sphere:
    __slots__ = ('c', 'r')
    c: Vec
    r: float

    def __init__(self, c: Iterable[float], r: float) -> None:
        self.c, self.r = tuple(c), r

    def contains(self, o: 'Sphere', eps: float = 1e-6) -> bool:
        return _dist(self.c, o.c) + o.r <= self.r + eps


def _dist(a: Iterable[float], b: Iterable[float]) -> float:
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def decode(node: NodeData) -> Sphere:
    """The sphere a node's keys stand for, as the game decodes it (centre key / 16, radius key / 4)."""
    _, _, x, y, z, w = node
    return Sphere((s16(x) / CENTER_SCALE, s16(y) / CENTER_SCALE, s16(z) / CENTER_SCALE), s16(w) / RADIUS_SCALE)


def encode(sphere: Sphere, contain: Iterable[Sphere] = ()) -> list[int]:
    """Keys for a sphere: centre rounded to 1/16, radius rounded up to 1/4 so that the decoded sphere contains the
    given sphere and every sphere in contain (decoded spheres of children)."""
    keys = [int(round(v * CENTER_SCALE)) for v in sphere.c]
    if any(not -32768 <= k <= 32767 for k in keys):
        raise CullTreeError(f'sphere centre {sphere.c} is outside the range a cull tree can hold (+-2048 m)')
    qc = tuple(k / CENTER_SCALE for k in keys)
    need = max([_dist(qc, sphere.c) + sphere.r] + [_dist(qc, s.c) + s.r for s in contain])
    w = int(math.ceil(need * RADIUS_SCALE - 1e-9))
    if w > 32767:
        raise CullTreeError(f'sphere radius {need:.1f} m is more than a cull tree can hold (8191 m)')
    return [u16(k) for k in keys] + [w]


def enclose(a: Sphere, b: Sphere) -> Sphere:
    """The smallest sphere containing spheres a and b."""
    d = _dist(a.c, b.c)
    if d + b.r <= a.r:
        return Sphere(a.c, a.r)
    if d + a.r <= b.r:
        return Sphere(b.c, b.r)
    r = (d + a.r + b.r) / 2
    t = (r - a.r) / d
    return Sphere(tuple(x + (y - x) * t for x, y in zip(a.c, b.c)), r)


class Node:
    """A tree node: leaf (leaf index) or internal (left, right); data = (Right, Max, kx, ky, kz, kw) as stored."""
    __slots__ = ('data', 'left', 'right', 'leaf', 'new')
    data: NodeData
    left: 'Node | None'
    right: 'Node | None'
    leaf: int | None
    new: bool

    def __init__(self, data: NodeData, left: 'Node | None' = None, right: 'Node | None' = None,
                 leaf: int | None = None, new: bool = False) -> None:
        self.data, self.left, self.right, self.leaf, self.new = data, left, right, leaf, new

    @property
    def sphere(self) -> Sphere:
        return decode(self.data)

    @property
    def max(self) -> float:
        return self.data[1]


def build(nodes: Sequence[NodeData]) -> tuple[Node, int]:
    """The preorder node list as a linked tree; checks every Right offset (left child = next node, right child =
    this node + Right, Right = 1 + size of the left subtree). Returns (root, used node count)."""
    if not nodes or nodes[0][0] == 0:
        raise CullTreeError('the tree is empty')
    root: Node | None = None
    work: list[tuple[int, Node | None, str]] = [(0, None, '')]    # (node index, parent, the parent's side)
    order = []
    while work:
        i, parent, side = work.pop()
        if i >= len(nodes):
            raise CullTreeError(f'node {i} is past the end of the tree')
        d = nodes[i]
        if d[0] < 0:
            n = Node(d, leaf=d[0] & 0x7FFFFFFF)
        else:
            if d[0] < 2:
                raise CullTreeError(f'node {i}: Right {d[0]} cannot hold a left subtree')
            n = Node(d)
            work.append((i + d[0], n, 'right'))
            work.append((i + 1, n, 'left'))
        order.append(i)
        if len(order) > len(nodes):
            raise CullTreeError('the Right offsets loop back into the tree')
        if parent is None:
            root = n
        else:
            setattr(parent, side, n)
    used = len(order)
    assert root is not None                    # node 0, set on the first pass
    # the linked tree written back in preorder must give the same Right offsets (no overlaps, no gaps)
    flat = flatten(root)
    if len(flat) != used or [f[0] for f in flat] != [nodes[i][0] for i in range(used)]:
        raise CullTreeError('the Right offsets do not describe one preorder tree (subtrees overlap or leave gaps)')
    return root, used


def flatten(root: Node) -> list[NodeData]:
    """Preorder node tuples with Right recomputed from the left subtree's size."""
    out = []
    sizes: dict[int, int] = {}
    # post-order sizes
    st = [(root, False)]
    while st:
        n, done = st.pop()
        if n.leaf is not None:
            sizes[id(n)] = 1
        elif done:
            sizes[id(n)] = 1 + sizes[id(n.left)] + sizes[id(n.right)]
        else:
            assert n.left is not None and n.right is not None    # an internal node has both children
            st.append((n, True))
            st.append((n.right, False))
            st.append((n.left, False))
    pre = [root]
    while pre:
        n = pre.pop()
        if n.leaf is not None:
            out.append(n.data)
        else:
            assert n.left is not None and n.right is not None
            right = 1 + sizes[id(n.left)]
            n.data = (right,) + tuple(n.data[1:])
            out.append(n.data)
            pre.append(n.right)
            pre.append(n.left)
    return out


def walk(root: Node) -> Iterator[tuple[Node, tuple[Node, ...]]]:
    """(node, ancestors) for every node, preorder."""
    st: list[tuple[Node, tuple[Node, ...]]] = [(root, ())]
    while st:
        n, anc = st.pop()
        yield n, anc
        if n.leaf is None:
            assert n.left is not None and n.right is not None
            st.append((n.right, anc + (n,)))
            st.append((n.left, anc + (n,)))


class CullTree:
    def __init__(self, path: str) -> None:
        self.path = path
        self.doc = Doc.read(path)
        inst = [e for e in self.doc.instances if e.attr('type') == 'World.StaticCullTreeData']
        if not inst:
            raise CullTreeError(f'{os.path.basename(path)} has no World.StaticCullTreeData instance (not a cull tree)')
        self.inst = inst[0]
        array = self.inst.child('Nodes')
        eo = self.inst.child('EnumOffset')
        if array is None or eo is None:
            raise CullTreeError(f'{os.path.basename(path)}: the cull tree has no Nodes or EnumOffset field')
        self.array: El = array
        self.enum_offset = _int(eo)
        self.nodes = [self._node(c) for c in self.array.children or ()]
        used = len(self.nodes)
        while used and self.nodes[used - 1] == (0, 0.0, 0, 0, 0, 0):
            used -= 1
        self.root, self.used = build(self.nodes[:used])
        if self.used != used:
            raise CullTreeError(f'{os.path.basename(path)}: {used - self.used} nodes before the spare ones are not '
                                'reached from the root')
        self.spare = len(self.nodes) - used
        self.leaf_node: dict[int, Node | None] = {}    # add() holds a new leaf's index with None until it is linked
        for n, _ in walk(self.root):
            if n.leaf is not None:
                if n.leaf in self.leaf_node:
                    raise CullTreeError(f'{os.path.basename(path)}: leaf index {n.leaf} is used twice')
                self.leaf_node[n.leaf] = n

    def _node(self, c: El) -> NodeData:
        v = {}
        for f in c.children or ():
            size, vals = f.numbers()
            v[f.name] = (size, vals[0])
        try:
            right = struct.unpack('>i', v['Right'][1])[0]
            mx = struct.unpack('>f', v['Max'][1])[0]
            keys = [struct.unpack('>H', v['SphereKey' + a][1][:2])[0] for a in 'XYZW']
        except KeyError as e:
            raise CullTreeError(f'a cull tree node has no field {e}') from e
        return (right, mx, *keys)

    @property
    def leaves(self) -> int:
        return len(self.leaf_node)

    def leaf_range_ok(self) -> bool:
        return sorted(self.leaf_node) == list(range(1, self.leaves + 1))

    def depth_and_stack(self) -> tuple[int, int]:
        """Deepest leaf (edges from the root) and the most stack entries the game's traversal can need when every
        sphere test passes (it pushes the right child, then the left, and pops)."""
        depth, stack, most = 0, [(self.root, 0)], 0
        while stack:
            most = max(most, len(stack))
            n, d = stack.pop()
            depth = max(depth, d)
            if n.leaf is None:
                assert n.left is not None and n.right is not None
                stack.append((n.right, d + 1))
                stack.append((n.left, d + 1))
        return depth, most

    def write(self, path: str) -> None:
        flat = flatten(self.root)
        flat += [(0, 0.0, 0, 0, 0, 0)] * SPARE
        assert self.array.children is not None
        tmpl = self.array.children[0]
        items = []
        for d in flat:
            c = tmpl.clone()
            for f in c.children or ():
                nm = f.name
                size, _ = f.numbers()
                if nm == 'Right':
                    f.set_numbers(4, [struct.pack('>i', d[0])])
                elif nm == 'Max':
                    f.set_numbers(4, [struct.pack('>f', d[1])])
                elif nm in NODE_FIELDS:
                    k = d[2 + 'XYZW'.index(nm[-1])]
                    f.set_numbers(size, [struct.pack('>H', k) + b'\0' * (size - 2)])
            items.append(c)
        # values are written as they were read (big-endian, same sizes), so unchanged nodes keep their bytes
        old = self.array.children
        self.array.children = items
        try:
            self.doc.write(path)
        finally:
            self.array.children = old


# ------------------------------------------------------------------------------------------------ levels

class Placed:
    __slots__ = ('guid', 'blueprint', 'm', 'enum', 'el', 'level')
    guid: str
    blueprint: str                             # the blueprint's path, lower case
    m: list[list[float]]                       # the transform's four rows
    enum: int                                  # Enumeration: the slot (0: none)
    el: El                                     # the Enumeration field
    level: 'Level'

    @property
    def pos(self) -> Vec:
        return tuple(self.m[3][:3])

    def to_world(self, local: Sequence[float]) -> Vec:
        m = self.m
        return tuple(local[0] * m[0][i] + local[1] * m[1][i] + local[2] * m[2][i] + m[3][i] for i in range(3))

    def to_local(self, world: Sequence[float]) -> Vec:
        m = self.m
        r = [m[0][:3], m[1][:3], m[2][:3]]
        inv = _inv3(r)
        d = [world[i] - m[3][i] for i in range(3)]
        return tuple(sum(d[j] * inv[j][i] for j in range(3)) for i in range(3))

    @property
    def scale(self) -> float:
        return max(math.sqrt(sum(x * x for x in self.m[i][:3])) for i in range(3))


def _inv3(r: Sequence[Sequence[float]]) -> list[list[float]]:
    a, b, c = r
    det = (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]))
    if abs(det) < 1e-12:
        raise CullTreeError('an object has a transform that cannot be inverted (zero scale)')
    m = [[b[1] * c[2] - b[2] * c[1], a[2] * c[1] - a[1] * c[2], a[1] * b[2] - a[2] * b[1]],
         [b[2] * c[0] - b[0] * c[2], a[0] * c[2] - a[2] * c[0], a[2] * b[0] - a[0] * b[2]],
         [b[0] * c[1] - b[1] * c[0], a[1] * c[0] - a[0] * c[1], a[0] * b[1] - a[1] * b[0]]]
    return [[x / det for x in row] for row in m]


class Level:
    def __init__(self, path: str) -> None:
        self.path = path
        self.doc = Doc.read(path)
        self.objects: list[Placed] = []
        self.tree_refs: dict[str, str | None] = {}
        for e in self.doc.instances:
            for f in ('ExclusiveStaticCullTree', 'ClientStaticCullTree'):
                c = e.child(f)
                if c is not None and c.attr('ref'):
                    self.tree_refs[f] = c.attr('ref')
            if e.attr('type') != 'Entity.ReferenceObjectData':
                continue
            en, tr, ro = e.child('Enumeration'), e.child('Transform'), e.child('ReferencedObject')
            if en is None or tr is None:
                continue
            o = Placed()
            o.guid = e.attr('guid') or ''              # (an instance always has one)
            ref = ro.attr('ref') if ro is not None else None
            o.blueprint = ref.rsplit('/', 1)[0].lower() if ref and '/' in ref else (ref or '')
            v = _floats(tr)
            if len(v) != 16:
                continue
            o.m = [v[0:4], v[4:8], v[8:12], v[12:16]]
            o.enum = _int(en)
            o.el = en
            o.level = self
            self.objects.append(o)
        if not self.objects:
            raise CullTreeError(f'{os.path.basename(path)} has no placed objects (Entity.ReferenceObjectData): '
                                'not a level partition')

    def by_guid(self, guid: str) -> Placed | None:
        g = guid.strip().lower()
        return next((o for o in self.objects if o.guid.lower() == g), None)

    def tree_path(self, field: str = 'ExclusiveStaticCullTree') -> str | None:
        """The tree file a level refers to: levels/<x>/staticculltree_* beside the level's own folder."""
        return tree_path_of(self.path, self.tree_refs.get(field))

    def set_enum(self, o: Placed, value: int) -> None:
        o.el.set_numbers(4, [struct.pack('>i', value)])
        o.enum = value


def tree_path_of(path: str, ref: str | None) -> str | None:
    """Level.tree_path for a level file at path whose tree reference is ref."""
    if not ref:
        return None
    parts = ref.split('/')[:-1]           # drop the instance guid
    base = os.path.dirname(os.path.abspath(path))
    while parts and parts[0].lower() != 'levels':
        parts = parts[1:]
    if not parts:                         # no levels/ segment: the walk below would return the level's folder
        return None
    p = base
    rest = parts[1:]
    for i, q in enumerate(rest):
        last = i == len(rest) - 1
        want, isit = (q.lower() + '.dbx', os.path.isfile) if last else (q.lower(), os.path.isdir)
        try:
            m = [f for f in os.listdir(p) if f.lower() == want and isit(os.path.join(p, f))]
        except OSError:
            return None
        if not m:
            return None
        p = os.path.join(p, sorted(m)[0])
    return p


def leaves_of(tree: 'CullTree', level: Level) -> dict[Placed, list[int]]:
    """{object: [leaf indices]} for the level's slotted objects in this tree: from its slot up to the next object's
    slot (or the last leaf)."""
    eo, last = tree.enum_offset, tree.enum_offset + tree.leaves
    slotted = sorted((o for o in level.objects if eo < o.enum <= last), key=lambda o: o.enum)
    out: dict[Placed, list[int]] = {}
    for i, o in enumerate(slotted):
        nxt = slotted[i + 1].enum if i + 1 < len(slotted) else last + 1
        out[o] = list(range(o.enum - eo, max(o.enum - eo + 1, nxt - eo)))
    return out


# ------------------------------------------------------------------------------------------------ templates

class Shape:
    """A blueprint's leaves in its own space: [(local centre, radius key, Max)] per leaf, from its instances."""

    def __init__(self, blueprint: str, leaves: list[ShapeLeaf], source: str, count: int, confirmed: int = 0) -> None:
        self.blueprint, self.leaves, self.source, self.count = blueprint, leaves, source, count
        self.confirmed = confirmed             # agreeing instances in other levels

    def place(self, o: Placed) -> list[tuple[Sphere, float]]:
        """[(world sphere, radius key, Max)] for object o: centre moved and rotated with its transform, radius and
        Max scaled; the radius key is the largest the instances have, plus 1 (0.25 m) for the new centre's
        rounding."""
        s = o.scale
        out = []
        for c, w, mx in self.leaves:
            wc = o.to_world(c)
            out.append((Sphere(wc, (w + 1) * s / RADIUS_SCALE), mx * s))
        return out


def _median(vals: Iterable[float]) -> float:
    v = sorted(vals)
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def collect(pairs: Iterable['Pair | RefRows']) -> defaultdict[str, list[Instance]]:
    """Every slotted instance of every blueprint in (tree, level, label) pairs, in order:
    {blueprint: [(label, k, [(local centre, w, Max)], object)]}."""
    found: defaultdict[str, list[Instance]] = defaultdict(list)
    for pair in pairs:
        if isinstance(pair, RefRows):        # a reference level's rows, read or from the cache (objects None)
            for bp, rows in pair.rows.items():
                found[bp].extend(rows)
            continue
        tree, level, label = pair
        for o, lv in leaves_of(tree, level).items():
            sph: list[ShapeLeaf] = []
            s = o.scale or 1.0
            for li in lv:
                n = tree.leaf_node.get(li)
                if n is None:
                    break
                d = n.sphere
                sph.append((o.to_local(d.c), n.data[5] / s, n.max / s))
            found[o.blueprint].append((label, len(lv), sph, o))
    return found


def shape_for(blueprint: str, found: Found, tol: float = 0.5, memo: dict[Any, Any] | None = None) -> 'Shape | None':
    """The blueprint's shape from its instances: the most common leaf count; per level (label), the instances whose
    local centres lie within tol m of that level's median (a port with shuffled slots gives wrong ones). The
    first level (the target's own instances come first) whose shape is confirmed wins: by 3 or more agreeing
    instances, or by another level's shape (same centres within tol, same Max within 1%). When no level is
    confirmed, the level with the most agreeing instances is used."""
    inst = found.get(blueprint.lower())
    if not inst:
        return None
    k = Counter(x[1] for x in inst).most_common(1)[0][0]
    inst = [x for x in inst if x[1] == k and len(x[2]) == k]
    groups = []
    for label in dict.fromkeys(x[0] for x in inst):
        group = [x for x in inst if x[0] == label]
        key = (label, k, tol, tuple(map(id, group)))
        if memo is not None and key in memo:     # the same instances (leave-one-out: every level but the object's)
            good, leaves = memo[key][1:]
        else:
            good = _consensus(group, k, tol)
            leaves = _shape_leaves(good, k) if good else []
            if memo is not None:
                memo[key] = (group, good, leaves)    # the group kept: its ids stay its own while the memo lives
        if good:
            groups.append((label, good, leaves))
    if not groups:
        return None

    def agree(a: Sequence[ShapeLeaf], b: Sequence[ShapeLeaf]) -> bool:
        return all(_dist(x[0], y[0]) <= tol and abs(x[2] - y[2]) <= 0.01 * max(x[2], y[2], 1e-6) for x, y in zip(a, b))
    for label, good, leaves in groups:
        others = sum(len(g) for lb, g, lv in groups if lb != label and agree(leaves, lv))
        if len(good) >= 3 or others:
            return Shape(blueprint, leaves, label, len(good), others)
    label, good, leaves = max(groups, key=lambda g: len(g[1]))
    return Shape(blueprint, leaves, label, len(good), 0)


def _shape_leaves(good: Sequence[Instance], k: int) -> list[ShapeLeaf]:
    leaves: list[ShapeLeaf] = []
    for j in range(k):
        c = tuple(_median([g[2][j][0][a] for g in good]) for a in range(3))
        w = max(g[2][j][1] for g in good)
        mx = _median([g[2][j][2] for g in good])
        leaves.append((c, w, mx))
    return leaves


def _consensus(group: Sequence[Instance], k: int, tol: float) -> list[Instance]:
    if not group:
        return []
    med = [tuple(_median([g[2][j][0][a] for g in group]) for a in range(3)) for j in range(k)]
    good = [g for g in group if all(_dist(g[2][j][0], med[j]) <= tol for j in range(k))]
    return good if len(good) * 2 > len(group) else []


LEVEL_MARK = b'ExclusiveStaticCullTree'


def level_partitions(folder: str) -> list[str]:
    """The level partitions under a folder: every .dbx below it (glob's '**/*.dbx', sorted) that is not a tree or a
    description and holds ExclusiveStaticCullTree. With the Rust core the folder is listed and the files searched on
    all cores (2026-11-16: --ref-dir over work\\levels' 37 levels read 109,660 files, 136 of a 200 s add run)."""
    m = _rust()
    rels = m.culltree_dbx_files(folder)               # None: a name that is not valid Unicode (glob lists it)
    files = sorted(os.path.join(folder, r) for r in rels) if rels is not None else \
        sorted(glob.glob(os.path.join(folder, '**', '*.dbx'), recursive=True))
    cands = [p for p in files if 'staticculltree' not in os.path.basename(p).lower()
             and '_description' not in os.path.basename(p).lower()]
    hits = m.culltree_contains(cands, LEVEL_MARK)
    out = []
    for p, hit in zip(cands, hits):
        if hit is None:                             # a file that cannot be read: raises here
            with open(p, 'rb') as f:
                hit = LEVEL_MARK in f.read()
        if hit:
            out.append(p)
    return out


def ref_pairs(refs: Sequence[str], ref_dirs: Iterable[str], log: Log,
              target: tuple[str, str] | None = None) -> list[Pair]:
    """(tree, level, label) for each --ref level and every level under each --ref-dir that has its tree. target: the
    (tree path, level path) being checked - never its own reference (a --ref-dir holding the whole levels folder read
    it a second time, and its instances confirmed themselves)."""
    paths = list(refs)
    for d in ref_dirs:
        paths += level_partitions(d)

    def same(a: str) -> str:
        return os.path.normcase(os.path.abspath(a))
    out: list[Pair]
    seen: set[tuple[str, int]]
    out, seen = [], set()
    skip = {same(x) for x in target} if target is not None else set()    # the target's tree and level files
    for p in paths:
        if same(p) in skip:
            continue
        try:
            lv = Level(p)
        except CullTreeError:
            continue
        tp = lv.tree_path()
        if not tp:
            if p in refs:
                log(f'note: {p}: its exclusive cull tree was not found beside it; not used')
            continue
        if same(tp) in skip:                  # another level file naming the target's own tree
            continue
        key = (os.path.basename(p).lower(), os.path.getsize(tp))
        if key in seen:
            continue
        seen.add(key)
        try:
            out.append((CullTree(tp), lv, os.path.splitext(os.path.basename(p))[0]))
        except CullTreeError as e:
            log(f'note: {tp}: {e}; not used')
    return out


# ref_rows (round E3m): ref_pairs' choices, with each reference level's collect() rows kept on disk per level file
# and tree file (by content since round PR1b-1c: size and SHA-256, _stat; before, size and modification time) and
# this module's code, so a second add or verify against the same
# --ref-dir parses no reference level again (console_cull's add on Onslaught MP_005 against work\levels: 89 level
# partitions and 46 trees parsed, 66 of its 128 s, 2.4 GB). The rows hold no objects (None where collect() puts the
# Placed: only the target level's objects are ever compared by identity). REFERENCE (BFBC2_CULLTREE_REFERENCE=1)
# runs ref_pairs and shape_for without its memo, as before; BFBC2_CULLTREE_CACHE: the cache folder, or 0 for none.
REFERENCE = os.environ.get('BFBC2_CULLTREE_REFERENCE', '') not in ('', '0')
CACHE_VERSION = 2
_CODE_ID: str | None = None


class RefRows:
    """A reference level's rows for collect(): {blueprint: [(label, leaf count, leaves, None)]}."""
    __slots__ = ('label', 'rows')

    def __init__(self, label: str, rows: dict[str, list[Instance]]) -> None:
        self.label, self.rows = label, rows


def cache_folder() -> str | None:
    """Where ref_rows keeps its rows: BFBC2_CULLTREE_CACHE (a folder; 0: none), else
    %LOCALAPPDATA%\\bfbc2-tool-suite\\cache\\culltree. None: no disk cache."""
    v = os.environ.get('BFBC2_CULLTREE_CACHE', '')
    if v == '0':
        return None
    return v or os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'cache',
                             'culltree')


def _code_id() -> str:
    """CACHE_VERSION and a digest of this file: rows made by other code are not used."""
    global _CODE_ID
    if _CODE_ID is None:
        import hashlib
        with open(os.path.abspath(__file__), 'rb') as f:
            _CODE_ID = f'{CACHE_VERSION}-{hashlib.sha1(f.read()).hexdigest()[:16]}'
    return _CODE_ID


def _stat(path: str) -> tuple[int, int]:
    """A file's content key: its size and the first 128 bits of its SHA-256 (a signed little-endian int), as
    bfbc2.exe's culltool computes it (bc2_levelwrite::culltool::content_stat): the rows are reused while the files
    hold the same bytes, whatever their times say."""
    import hashlib
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        size = os.fstat(f.fileno()).st_size
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return size, int.from_bytes(h.digest()[:16], 'little', signed=True)


class _RowCache:
    """One pickle per reference level file: {'code', 'level': (size, key), 'ref': its tree reference or
    'notlevel': True, 'trees': {tree path: {'stat': (size, key), 'rows' or 'error'}}} (_stat's keys)."""

    def __init__(self, folder: str | None) -> None:
        self.folder = folder

    def _file(self, path: str) -> str | None:
        if not self.folder:
            return None
        import hashlib
        return os.path.join(self.folder, hashlib.sha1(os.path.normcase(os.path.abspath(path)).encode('utf-8'))
                            .hexdigest()[:24] + '.pkl')

    def get(self, path: str, st: tuple[int, int]) -> dict[str, Any] | None:
        f = self._file(path)
        if not f or not os.path.isfile(f):
            return None
        import pickle
        try:
            with open(f, 'rb') as fh:
                e = pickle.load(fh)
        except Exception:                      # a damaged or foreign file: read the level again
            return None
        ok = isinstance(e, dict) and e.get('code') == _code_id() and e.get('level') == st \
            and e.get('path') == os.path.normcase(os.path.abspath(path))
        if ok:
            c = _caches()
            if c is not None:
                c.touch(f)                     # used now: the pruning deletes the least recently used
        return e if ok else None

    def put(self, path: str, e: dict[str, Any]) -> None:
        f = self._file(path)
        if not f:
            return
        import pickle
        tmp = f'{f}.{os.getpid()}.tmp'
        try:
            os.makedirs(os.path.dirname(f), exist_ok=True)
            with open(tmp, 'wb') as fh:
                pickle.dump(e, fh, protocol=pickle.HIGHEST_PROTOCOL)
            os.replace(tmp, f)
        except OSError:                        # a cache that cannot be written is not used
            with contextlib.suppress(OSError):
                os.remove(tmp)
            return
        c = _caches()
        if c is not None:                      # kept under BFBC2_CULLTREE_CACHE_MB (default 2048)
            c.maybe_prune('culltree', self.folder)


def _caches() -> Any:
    """bfbc2core.caches (the cache's size cap and pruning), or None when the tool runs outside the suite."""
    try:
        if SUITE not in sys.path:
            sys.path.insert(0, SUITE)
        import suite_paths
        suite_paths.use('core')
        from bfbc2core import caches
        return caches
    except ImportError:
        return None


def ref_rows(refs: Sequence[str], ref_dirs: Iterable[str], log: Log,
             target: tuple[str, str] | None = None) -> list['Pair | RefRows']:
    """ref_pairs' levels as RefRows, in its order, with its notes: the same collect() result (but for the objects,
    None here) with the rows from the cache where the level file and its tree are unchanged."""
    if REFERENCE:
        return list(ref_pairs(refs, ref_dirs, log, target))
    paths = list(refs)
    for d in ref_dirs:
        paths += level_partitions(d)

    def same(a: str) -> str:
        return os.path.normcase(os.path.abspath(a))
    cache = _RowCache(cache_folder())
    out: list[Pair | RefRows] = []
    seen: set[tuple[str, int]] = set()
    skip = {same(x) for x in target} if target is not None else set()
    for p in paths:
        if same(p) in skip:
            continue
        lv: Level | None = None
        try:
            st = _stat(p)
        except OSError:
            Level(p)                           # raises as ref_pairs' read does
            raise
        e = cache.get(p, st)
        if e is None:
            e = {'code': _code_id(), 'path': same(p), 'level': st, 'trees': {}}
            try:
                lv = Level(p)
                e['ref'] = lv.tree_refs.get('ExclusiveStaticCullTree')
            except CullTreeError:
                e['notlevel'] = True
            cache.put(p, e)
        if e.get('notlevel'):
            continue
        tp = tree_path_of(p, e['ref'])
        if not tp:
            if p in refs:
                log(f'note: {p}: its exclusive cull tree was not found beside it; not used')
            continue
        if same(tp) in skip:
            continue
        key = (os.path.basename(p).lower(), os.path.getsize(tp))
        if key in seen:
            continue
        seen.add(key)
        label = os.path.splitext(os.path.basename(p))[0]
        tst = _stat(tp)
        t = e['trees'].get(same(tp))
        if t is None or t['stat'] != tst:
            if lv is None:
                lv = Level(p)
            try:
                tree = CullTree(tp)
                rows = {bp: [(x[0], x[1], x[2], None) for x in xs] for bp, xs in collect([(tree, lv, label)]).items()}
                t = {'stat': tst, 'rows': rows}
            except CullTreeError as err:
                t = {'stat': tst, 'error': str(err)}
            e['trees'][same(tp)] = t
            cache.put(p, e)
        if 'error' in t:
            log(f'note: {tp}: {t["error"]}; not used')
            continue
        out.append(RefRows(label, t['rows']))
    return out


# ------------------------------------------------------------------------------------------------ add

def insert(tree: CullTree, sub: Node, sph: Sphere) -> Node:
    """Inserts subtree sub (sphere sph) where the tree's spheres already contain it: down from the root, into a
    child whose decoded sphere contains sph (the smaller one); where neither does, a new internal node takes the
    place of the child that grows least. That child stays on its side under the new node (the game tests left and
    right children differently, so it keeps the test it had) and sub takes the other side. Every ancestor's Max is
    raised to the subtree's; the root's sphere is enlarged if needed (it is never tested by the traversal).
    Returns the new internal node."""
    path = [tree.root]
    n = tree.root
    while True:
        if n.leaf is not None:
            raise CullTreeError('internal: descended into a leaf')
        assert n.left is not None and n.right is not None
        cands = [c for c in (n.left, n.right) if c.sphere.contains(sph)]
        if cands:
            c = min(cands, key=lambda x: x.sphere.r)
            if c.leaf is None:
                path.append(c)
                n = c
                continue
            target, parent = c, n
        else:
            assert n.left is not None and n.right is not None    # an internal node has both children
            grow = lambda x: enclose(x.sphere, sph).r - x.sphere.r  # noqa: E731
            target, parent = min((n.left, n.right), key=grow), n
        break
    s = enclose(target.sphere, sph)
    keys = encode(s, contain=(target.sphere, sph))
    if parent.left is target:
        p = Node((0, max(target.max, sub.max), *keys), left=target, right=sub, new=True)
        parent.left = p
    else:
        p = Node((0, max(target.max, sub.max), *keys), left=sub, right=target, new=True)
        parent.right = p
    for a in path:
        if a.max < sub.max:
            a.data = (a.data[0], sub.max) + tuple(a.data[2:])
    root = tree.root
    if not root.sphere.contains(sph):
        rs = enclose(root.sphere, sph)
        root.data = root.data[:2] + tuple(encode(rs, contain=(root.sphere, sph)))
    return p


def make_subtree(placed: Sequence[tuple[Sphere, float]], first_leaf: int) -> tuple[Node, Sphere]:
    """The nodes for one object's leaves: one leaf, or an internal node over a balanced split of them (two leaves:
    an internal node with the two leaves, as stock trees often hold an object)."""
    leaves = []
    for j, (s, mx) in enumerate(placed):
        keys = encode(s)
        leaves.append(Node(((LEAF | (first_leaf + j)) - (1 << 32), mx, *keys), leaf=first_leaf + j, new=True))

    def join(ns: list[Node]) -> Node:
        if len(ns) == 1:
            return ns[0]
        h = len(ns) // 2
        a, b = join(ns[:h]), join(ns[h:])
        s = enclose(a.sphere, b.sphere)
        return Node((0, max(a.max, b.max), *encode(s, contain=(a.sphere, b.sphere))), left=a, right=b, new=True)
    top = join(leaves)
    whole = leaves[0].sphere
    for x in leaves[1:]:
        whole = enclose(whole, x.sphere)
    return top, top.sphere if top.leaf is None else whole


def add(tree: CullTree, level: Level, objects: Iterable[Placed], found: Found, log: Log
        ) -> list[tuple[Placed, int, list[int], Shape]]:
    """Gives each object (slot 0) new leaves; returns [(object, slot, leaves, shape)]."""
    done = []
    for o in objects:
        if o.enum:
            raise CullTreeError(f'object {o.guid} already has slot {o.enum}; add only takes slot-0 objects')
        shape = shape_for(o.blueprint, found)
        if shape is None:
            raise CullTreeError(f'object {o.guid}: no slotted instance of {o.blueprint} in the tree or the --ref '
                                'levels to take its shape from; add a --ref level that has one')
        placed = shape.place(o)
        first = tree.leaves + 1
        sub, sph = make_subtree(placed, first)
        insert(tree, sub, sph)
        for j in range(len(placed)):
            tree.leaf_node[first + j] = None
        for n, _ in walk(sub):
            if n.leaf is not None:
                tree.leaf_node[n.leaf] = n
        slot = tree.enum_offset + first
        level.set_enum(o, slot)
        done.append((o, slot, list(range(first, first + len(placed))), shape))
        log(f'  {o.guid} {o.blueprint.rsplit("/", 1)[-1]:32s} at {_fmt(o.pos)}: slot {slot}, {len(placed)} leaves '
            f'(shape from {shape.source}: {shape.count} instance(s), {shape.confirmed} agreeing elsewhere), '
            f'radius {placed[0][0].r:.2f} m')
        if shape.count < 3 and not shape.confirmed:
            log(f'  warning: the shape of {o.blueprint} rests on {shape.count} unconfirmed instance(s); add a --ref '
                'level with more of them to check it')
    return done


def _fmt(v: Iterable[float]) -> str:
    return '(' + ', '.join(f'{x:.1f}' for x in v) + ')'


def auto_objects(level: Level, found: Found) -> list[Placed]:
    return [o for o in level.objects if o.enum == 0 and o.blueprint and o.blueprint in found]


def misplaced(tree: CullTree, level: Level, found: Found, metres: float) -> list[tuple[Placed, float]]:
    """[(object, metres off)]: slotted objects whose leaves lie more than metres from where the blueprint's other
    instances (leave-one-out) put them, and whose blueprint has a shape without them."""
    out = []
    memo: dict[Any, Any] | None = None if REFERENCE else {}
    for o, ids in leaves_of(tree, level).items():
        rest = {b: [x for x in v if x[3] is not o] for b, v in found.items() if b == o.blueprint}
        sh = shape_for(o.blueprint, rest, memo=memo)
        if sh is None:
            continue
        k = min(len(sh.leaves), len(ids))
        off = [_dist(o.to_world(sh.leaves[j][0]), n.sphere.c) for j in range(k)
               if (n := tree.leaf_node.get(ids[j])) is not None]
        if off and max(off) > metres:
            out.append((o, max(off)))
    return out


# ------------------------------------------------------------------------------------------------ checks

def check_structure(tree: CullTree, client: CullTree | None = None) -> list[tuple[bool, str]]:
    """[(ok, text)] for the tree's own rules."""
    res = []
    flat = flatten(tree.root)
    res.append((flat == tree.nodes[:tree.used] or tree.path is None,
                f'preorder layout and Right offsets: {tree.used} used nodes, every Right = 1 + left subtree size'))
    res.append((tree.spare == SPARE, f'{tree.spare} all-zero spare nodes after the used ones (stock trees: {SPARE})'))
    res.append((tree.leaf_range_ok(), f'leaf indices 1..{tree.leaves}, each once'))
    res.append((tree.used == 2 * tree.leaves - 1, f'{tree.used} used nodes = 2 x {tree.leaves} leaves - 1 (binary tree)'))
    bad_max = [n for n, _ in walk(tree.root) if n.leaf is None and n.left is not None and n.right is not None
               and n.max != max(n.left.max, n.right.max)]
    res.append((not bad_max, f'every internal node\'s Max is the larger of its children\'s ({len(bad_max)} not)'))
    depth, most = tree.depth_and_stack()
    res.append((most <= STACK_SLOTS, f'deepest leaf {depth} levels; traversal stack up to {most} of {STACK_SLOTS} entries'))
    if client is not None:
        res.append((tree.enum_offset == client.leaves and client.enum_offset == 0,
                    f'EnumOffset {tree.enum_offset} = client tree leaves {client.leaves} (client EnumOffset '
                    f'{client.enum_offset})'))
    return res


def containment(tree: CullTree, leaf_ids: Iterable[int]) -> list[tuple[int, int, int]]:
    """For the given leaves: (leaf, ancestors whose decoded sphere does not contain the leaf's sphere, path length),
    the root excepted (the traversal tests children only)."""
    want = set(leaf_ids)
    out = []
    for n, anc in walk(tree.root):
        if n.leaf in want:
            s = n.sphere
            miss = [a for a in anc[1:] if not a.sphere.contains(s)]
            out.append((n.leaf, len(miss), len(anc)))
    return out


def object_checks(tree: CullTree, level: Level, found: Found) -> tuple[list[tuple[bool, str]], list[str]]:
    """Slots and spheres of the level's objects: [(ok, text)], details [str]."""
    res: list[tuple[bool, str]]
    detail: list[str]
    res, detail = [], []
    eo, last = tree.enum_offset, tree.enum_offset + tree.leaves
    cnt = Counter(o.enum for o in level.objects if o.enum)
    shared = [s for s, c in cnt.items() if c > 1]
    res.append((not shared, f'no slot shared by two objects ({len(shared)} shared)'))
    out_of = [o for o in level.objects if o.enum and not eo < o.enum <= last]
    res.append((not out_of, f'every slot is a leaf of this tree: {eo + 1}..{last} ({len(out_of)} outside)'))
    lv = leaves_of(tree, level)
    ks = Counter(len(v) for v in lv.values())
    res.append((True, 'leaves per object: ' + ', '.join(f'{k}: {c}' for k, c in sorted(ks.items()))))
    used = sum(len(v) for v in lv.values())
    res.append((True, f'{used} of {tree.leaves} leaves start at or follow an object\'s slot'))
    # leave-one-out prediction against the same blueprint's other instances
    errs: list[float]
    worst: list[tuple[float, Placed, int]]
    errs, rad_short, n_cmp, worst = [], 0, 0, []
    memo: dict[Any, Any] | None = None if REFERENCE else {}
    for o, ids in lv.items():
        inst = [x for x in found.get(o.blueprint, []) if x[3] is not o]
        if len(inst) < 2:
            continue
        sh = shape_for(o.blueprint, {o.blueprint: inst}, memo=memo)
        if sh is None or len(sh.leaves) != len(ids):
            continue
        for j, li in enumerate(ids):
            n = tree.leaf_node.get(li)
            if n is None:
                continue
            d = n.sphere
            pc = o.to_world(sh.leaves[j][0])
            e = _dist(pc, d.c)
            errs.append(e)
            n_cmp += 1
            if d.r + 1e-6 < sh.leaves[j][2] * o.scale:
                rad_short += 1
            worst.append((e, o, li))
    if errs:
        errs.sort()
        res.append((errs[len(errs) * 99 // 100] < 1.0,
                    f'decoded leaf centres vs the blueprint\'s other instances (leave-one-out): {n_cmp} leaves, '
                    f'median {errs[len(errs) // 2]:.3f} m, 99% within {errs[len(errs) * 99 // 100]:.3f} m, '
                    f'largest {errs[-1]:.2f} m'))
        res.append((rad_short == 0, f'decoded radius at least the leaf\'s Max (unquantized radius): '
                                    f'{n_cmp - rad_short} of {n_cmp}'))
        for e, o, li in sorted(worst, key=lambda x: -x[0])[:5]:
            if e > 1.0:
                detail.append(f'  leaf {li} of {o.guid} ({o.blueprint.rsplit("/", 1)[-1]}) is {e:.2f} m from where '
                              'the blueprint\'s other instances put it')
    inside = sum(1 for o, ids in lv.items()
                 if (first := tree.leaf_node.get(ids[0])) is not None and first.sphere.contains(Sphere(o.pos, 0)))
    res.append((True, f'object origin inside its first leaf\'s sphere: {inside} of {len(lv)} (models built away '
                      'from their origin, e.g. distant scenery, are not)'))
    zero = [o for o in level.objects if o.enum == 0]
    cand = auto_objects(level, found)
    res.append((not cand, f'{len(zero)} objects with slot 0; {len(cand)} of them have a blueprint that is in a cull '
                          'tree (see-through / indestructible in game: candidates for add)'))
    for o in cand:
        detail.append(f'  slot 0: {o.guid} {o.blueprint} at {_fmt(o.pos)}')
    return res, detail
