r"""culltree.py - Battlefield: Bad Company 2's static cull trees (World.StaticCullTreeData): read them, check them
against the level's placed objects, and add placed objects that have no slot (slot 0: see-through and
indestructible in game) with correctly built subtrees.

  python culltree.py info <tree.dbx> [<level.dbx>]
  python culltree.py verify <tree.dbx> <level.dbx> [--client <client tree.dbx>] [--ref <level.dbx>]...
                     [--ref-dir <folder>] [--added <guid,...>] [--report <file>]
  python culltree.py add <tree.dbx> <level.dbx> (--objects <guid,... | @file> | --auto)
                     --out-tree <tree.dbx> --out-level <level.dbx> [--ref <level.dbx>]... [--ref-dir <folder>]
                     [--misplaced <metres>] [--client <client tree.dbx>] [--report <file>]

info    the tree's size, leaves, EnumOffset, depth and root sphere; with a level, its objects by slot and the
        slot-0 objects whose blueprint is in a cull tree (candidates for add).
verify  the structure (preorder layout, Right offsets, Max, leaf numbering, 256 spare nodes, EnumOffset = client
        leaves, traversal stack depth), every object's slot (in range, not shared, its leaves) and its decoded
        spheres against the same blueprint's other instances (leave-one-out prediction); with --added, that every
        ancestor of those objects' leaves contains their spheres (the traversal reaches them).
add     gives each object a new run of leaves (as many as the blueprint has in stock trees, with its spheres moved
        and rotated to the object's transform), inserts them where the tree's spheres already contain them,
        updates the ancestors' Right offsets and Max, keeps 256 spare nodes, sets the objects' Enumeration in the
        level and writes both files (everything else byte for byte).
--ref / --ref-dir  levels (their trees found through ExclusiveStaticCullTree, beside the level's folder) to take a
        blueprint's shape from when the target tree has no instance of it; --ref-dir takes every level .dbx under
        the folder that has its tree beside it. The target level's own slotted instances come first.
--auto  every slot-0 object whose blueprint has slotted instances (in this level or a --ref level), except objects
        beyond the +-2048 m a tree can hold (distant backdrops: they stay at slot 0, outside the tree).
--misplaced  objects whose slot's leaves lie more than <metres> from where the blueprint's other instances put them
        (verify's leave-one-out prediction) are given slot 0 first, so --auto gives them new leaves; only objects
        whose blueprint has a shape. A port that keeps another object's slot (Nelson Bay Onslaught: console doors
        on PC doors' slots at the other end of the house) draws that object only when the wrong sphere is in view -
        it flickers. Their old leaves stay in the tree with no object of their own, as a stock object a port
        leaves out does.

The format is described in README.md ("Static cull trees"). The game reads the trees with the traversal at
0x0056C4E0 in BFBC2Game.exe (build 795745); the notes there come from short targeted disassembly only.
"""
import glob
import math
import os
import struct
import sys
from collections import Counter, defaultdict

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


class CullTreeError(Exception):
    pass


# ------------------------------------------------------------------------------------------------ binary DBX
# A minimal codec (bfbc2-dbx-tools/FORMAT.md sections 1, 2 and 4) that keeps every byte, so an unchanged file is
# written back exactly and a changed one differs only where values or array items were changed.

def _varint(data, pos):
    v = shift = 0
    while True:
        b = data[pos]
        pos += 1
        v |= (b & 0x7F) << shift
        if b < 0x80:
            return v, pos
        shift += 7


def _put_varint(v, out):
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

    def __init__(self, doc, tag, head, attrs):
        self.doc, self.tag, self.head, self.attrs = doc, tag, head, attrs
        self.extra = 0
        self.value = b''
        self.children = None

    @property
    def kind(self):
        return self.head >> 4

    def attr(self, name):
        s = self.doc.strings
        for k, v in self.attrs:
            if s[k] == name:
                return s[v]
        return None

    @property
    def name(self):
        return self.attr('name')

    def child(self, name):
        for c in self.children or ():
            if c.name == name:
                return c
        return None

    def numbers(self):
        """(size, [raw big-endian values]) of a NUMBERS element."""
        count, p = _varint(self.value, 0)
        size, p = _varint(self.value, p)
        return size, [self.value[p + i * size:p + (i + 1) * size] for i in range(count)]

    def set_numbers(self, size, values):
        out = bytearray()
        _put_varint(len(values), out)
        _put_varint(size, out)
        for v in values:
            if len(v) != size:
                raise CullTreeError('internal: number of the wrong size')
            out += v
        self.value = bytes(out)

    def clone(self):
        e = El(self.doc, self.tag, self.head, list(self.attrs))
        e.extra, e.value = self.extra, self.value
        if self.children is not None:
            e.children = [c.clone() for c in self.children]
        return e


class Doc:
    def __init__(self, data, name):
        if len(data) <= 24 or not data.startswith(MAGIC):
            raise CullTreeError(f'{name} is not a binary .dbx file')
        self.name = name
        self.head12 = data[12:16]
        size, count = struct.unpack_from('>II', data, 16)
        self.string_area = data[24:24 + size]
        base = 24 + 4 * count
        self.strings = []
        for i in range(count):
            off = struct.unpack_from('>I', data, 24 + 4 * i)[0]
            end = data.index(b'\0', base + off)
            self.strings.append(data[base + off:end].decode('utf-8', 'replace'))
        pos = 24 + size
        self.roots, stack = [], []
        while pos < len(data):
            tag, p = _varint(data, pos)
            if tag == 0:
                if not stack:
                    break
                stack.pop()
                pos = p
                continue
            pos = p
            head = data[pos]
            pos += 1
            attrs = []
            for _ in range(head & 0xF):
                k, pos = _varint(data, pos)
                v, pos = _varint(data, pos)
                attrs.append((k, v))
            e = El(self, tag, head, attrs)
            (stack[-1].children if stack else self.roots).append(e)
            kind = head >> 4
            if kind == CONTAINER:
                e.extra = data[pos]
                pos += 1
                e.children = []
                stack.append(e)
            elif kind == TEXT:
                start = pos
                _, pos = _varint(data, pos)
                e.value = data[start:pos]
            elif kind == NUMBERS:
                start = pos
                n, pos = _varint(data, pos)
                sz, pos = _varint(data, pos)
                pos += n * sz
                if pos > len(data):
                    raise CullTreeError(f'{name}: truncated file')
                e.value = data[start:pos]
            else:
                e.value = data[pos:pos + 2]
                pos += 2
        if stack:
            raise CullTreeError(f'{name}: truncated file (unclosed elements)')
        self.trailer = data[pos:]

    @classmethod
    def read(cls, path):
        with open(path, 'rb') as f:
            return cls(f.read(), os.path.basename(path))

    def tobytes(self):
        out = bytearray()

        def put(e):
            _put_varint(e.tag, out)
            out.append(e.head)
            for k, v in e.attrs:
                _put_varint(k, out)
                _put_varint(v, out)
            if e.kind == CONTAINER:
                out.append(e.extra)
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

    def write(self, path):
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        with open(path, 'wb') as f:
            f.write(self.tobytes())

    @property
    def instances(self):
        return [e for e in self.roots[0].children if self.strings[e.tag] == 'instance']


def _int(e):
    size, v = e.numbers()
    if size != 4 or len(v) != 1:
        raise CullTreeError('internal: not a single 4-byte number')
    return struct.unpack('>i', v[0])[0]


def _floats(e):
    size, v = e.numbers()
    return [struct.unpack('>f', x)[0] for x in v] if size == 4 else []


# ------------------------------------------------------------------------------------------------ the tree

def s16(v):
    return v - 65536 if v >= 32768 else v


def u16(v):
    return v & 0xFFFF


class Sphere:
    __slots__ = ('c', 'r')

    def __init__(self, c, r):
        self.c, self.r = tuple(c), r

    def contains(self, o, eps=1e-6):
        return _dist(self.c, o.c) + o.r <= self.r + eps


def _dist(a, b):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def decode(node):
    """The sphere a node's keys stand for, as the game decodes it (centre key / 16, radius key / 4)."""
    _, _, x, y, z, w = node
    return Sphere((s16(x) / CENTER_SCALE, s16(y) / CENTER_SCALE, s16(z) / CENTER_SCALE), s16(w) / RADIUS_SCALE)


def encode(sphere, contain=()):
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


def enclose(a, b):
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

    def __init__(self, data, left=None, right=None, leaf=None, new=False):
        self.data, self.left, self.right, self.leaf, self.new = data, left, right, leaf, new

    @property
    def sphere(self):
        return decode(self.data)

    @property
    def max(self):
        return self.data[1]


def build(nodes):
    """The preorder node list as a linked tree; checks every Right offset (left child = next node, right child =
    this node + Right, Right = 1 + size of the left subtree). Returns (root, used node count)."""
    if not nodes or nodes[0][0] == 0:
        raise CullTreeError('the tree is empty')
    root = None
    work = [(0, None, None)]
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
    # the linked tree written back in preorder must give the same Right offsets (no overlaps, no gaps)
    flat = flatten(root)
    if len(flat) != used or [f[0] for f in flat] != [nodes[i][0] for i in range(used)]:
        raise CullTreeError('the Right offsets do not describe one preorder tree (subtrees overlap or leave gaps)')
    return root, used


def flatten(root):
    """Preorder node tuples with Right recomputed from the left subtree's size."""
    out = []
    sizes = {}
    # post-order sizes
    st = [(root, False)]
    while st:
        n, done = st.pop()
        if n.leaf is not None:
            sizes[id(n)] = 1
        elif done:
            sizes[id(n)] = 1 + sizes[id(n.left)] + sizes[id(n.right)]
        else:
            st.append((n, True))
            st.append((n.right, False))
            st.append((n.left, False))
    st = [root]
    while st:
        n = st.pop()
        if n.leaf is not None:
            out.append(n.data)
        else:
            right = 1 + sizes[id(n.left)]
            n.data = (right,) + tuple(n.data[1:])
            out.append(n.data)
            st.append(n.right)
            st.append(n.left)
    return out


def walk(root):
    """(node, ancestors) for every node, preorder."""
    st = [(root, ())]
    while st:
        n, anc = st.pop()
        yield n, anc
        if n.leaf is None:
            st.append((n.right, anc + (n,)))
            st.append((n.left, anc + (n,)))


class CullTree:
    def __init__(self, path):
        self.path = path
        self.doc = Doc.read(path)
        inst = [e for e in self.doc.instances if e.attr('type') == 'World.StaticCullTreeData']
        if not inst:
            raise CullTreeError(f'{os.path.basename(path)} has no World.StaticCullTreeData instance (not a cull tree)')
        self.inst = inst[0]
        self.array = self.inst.child('Nodes')
        eo = self.inst.child('EnumOffset')
        if self.array is None or eo is None:
            raise CullTreeError(f'{os.path.basename(path)}: the cull tree has no Nodes or EnumOffset field')
        self.enum_offset = _int(eo)
        self.nodes = [self._node(c) for c in self.array.children]
        used = len(self.nodes)
        while used and self.nodes[used - 1] == (0, 0.0, 0, 0, 0, 0):
            used -= 1
        self.root, self.used = build(self.nodes[:used])
        if self.used != used:
            raise CullTreeError(f'{os.path.basename(path)}: {used - self.used} nodes before the spare ones are not '
                                'reached from the root')
        self.spare = len(self.nodes) - used
        self.leaf_node = {}
        for n, _ in walk(self.root):
            if n.leaf is not None:
                if n.leaf in self.leaf_node:
                    raise CullTreeError(f'{os.path.basename(path)}: leaf index {n.leaf} is used twice')
                self.leaf_node[n.leaf] = n

    def _node(self, c):
        v = {}
        for f in c.children:
            size, vals = f.numbers()
            v[f.name] = (size, vals[0])
        try:
            right = struct.unpack('>i', v['Right'][1])[0]
            mx = struct.unpack('>f', v['Max'][1])[0]
            keys = [struct.unpack('>H', v['SphereKey' + a][1][:2])[0] for a in 'XYZW']
        except KeyError as e:
            raise CullTreeError(f'a cull tree node has no field {e}')
        return (right, mx, *keys)

    @property
    def leaves(self):
        return len(self.leaf_node)

    def leaf_range_ok(self):
        return sorted(self.leaf_node) == list(range(1, self.leaves + 1))

    def depth_and_stack(self):
        """Deepest leaf (edges from the root) and the most stack entries the game's traversal can need when every
        sphere test passes (it pushes the right child, then the left, and pops)."""
        depth, stack, most = 0, [(self.root, 0)], 0
        while stack:
            most = max(most, len(stack))
            n, d = stack.pop()
            depth = max(depth, d)
            if n.leaf is None:
                stack.append((n.right, d + 1))
                stack.append((n.left, d + 1))
        return depth, most

    def write(self, path):
        flat = flatten(self.root)
        flat += [(0, 0.0, 0, 0, 0, 0)] * SPARE
        tmpl = self.array.children[0]
        items = []
        for d in flat:
            c = tmpl.clone()
            for f in c.children:
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

    @property
    def pos(self):
        return tuple(self.m[3][:3])

    def to_world(self, local):
        m = self.m
        return tuple(local[0] * m[0][i] + local[1] * m[1][i] + local[2] * m[2][i] + m[3][i] for i in range(3))

    def to_local(self, world):
        m = self.m
        r = [m[0][:3], m[1][:3], m[2][:3]]
        inv = _inv3(r)
        d = [world[i] - m[3][i] for i in range(3)]
        return tuple(sum(d[j] * inv[j][i] for j in range(3)) for i in range(3))

    @property
    def scale(self):
        return max(math.sqrt(sum(x * x for x in self.m[i][:3])) for i in range(3))


def _inv3(r):
    a, b, c = r
    det = (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]))
    if abs(det) < 1e-12:
        raise CullTreeError('an object has a transform that cannot be inverted (zero scale)')
    m = [[b[1] * c[2] - b[2] * c[1], a[2] * c[1] - a[1] * c[2], a[1] * b[2] - a[2] * b[1]],
         [b[2] * c[0] - b[0] * c[2], a[0] * c[2] - a[2] * c[0], a[2] * b[0] - a[0] * b[2]],
         [b[0] * c[1] - b[1] * c[0], a[1] * c[0] - a[0] * c[1], a[0] * b[1] - a[1] * b[0]]]
    return [[x / det for x in row] for row in m]


class Level:
    def __init__(self, path):
        self.path = path
        self.doc = Doc.read(path)
        self.objects = []
        self.tree_refs = {}
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
            o.guid = e.attr('guid')
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

    def by_guid(self, guid):
        g = guid.strip().lower()
        return next((o for o in self.objects if o.guid.lower() == g), None)

    def tree_path(self, field='ExclusiveStaticCullTree'):
        """The tree file a level refers to: levels/<x>/staticculltree_* beside the level's own folder."""
        ref = self.tree_refs.get(field)
        if not ref:
            return None
        parts = ref.split('/')[:-1]           # drop the instance guid
        base = os.path.dirname(os.path.abspath(self.path))
        while parts and parts[0].lower() != 'levels':
            parts = parts[1:]
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

    def set_enum(self, o, value):
        o.el.set_numbers(4, [struct.pack('>i', value)])
        o.enum = value


def leaves_of(tree, level):
    """{object: [leaf indices]} for the level's slotted objects in this tree: from its slot up to the next object's
    slot (or the last leaf)."""
    eo, last = tree.enum_offset, tree.enum_offset + tree.leaves
    slotted = sorted((o for o in level.objects if eo < o.enum <= last), key=lambda o: o.enum)
    out = {}
    for i, o in enumerate(slotted):
        nxt = slotted[i + 1].enum if i + 1 < len(slotted) else last + 1
        out[o] = list(range(o.enum - eo, max(o.enum - eo + 1, nxt - eo)))
    return out


# ------------------------------------------------------------------------------------------------ templates

class Shape:
    """A blueprint's leaves in its own space: [(local centre, radius key, Max)] per leaf, from its instances."""

    def __init__(self, blueprint, leaves, source, count, confirmed=0):
        self.blueprint, self.leaves, self.source, self.count = blueprint, leaves, source, count
        self.confirmed = confirmed             # agreeing instances in other levels

    def place(self, o):
        """[(world sphere, radius key, Max)] for object o: centre moved and rotated with its transform, radius and
        Max scaled; the radius key is the largest the instances have, plus 1 (0.25 m) for the new centre's
        rounding."""
        s = o.scale
        out = []
        for c, w, mx in self.leaves:
            wc = o.to_world(c)
            out.append((Sphere(wc, (w + 1) * s / RADIUS_SCALE), mx * s))
        return out


def _median(vals):
    v = sorted(vals)
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def collect(pairs):
    """Every slotted instance of every blueprint in (tree, level, label) pairs, in order:
    {blueprint: [(label, k, [(local centre, w, Max)], object)]}."""
    found = defaultdict(list)
    for tree, level, label in pairs:
        for o, lv in leaves_of(tree, level).items():
            sph = []
            s = o.scale or 1.0
            for li in lv:
                n = tree.leaf_node.get(li)
                if n is None:
                    break
                d = n.sphere
                sph.append((o.to_local(d.c), n.data[5] / s, n.max / s))
            found[o.blueprint].append((label, len(lv), sph, o))
    return found


def shape_for(blueprint, found, tol=0.5):
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
        good = _consensus([x for x in inst if x[0] == label], k, tol)
        if good:
            groups.append((label, good, _shape_leaves(good, k)))
    if not groups:
        return None

    def agree(a, b):
        return all(_dist(x[0], y[0]) <= tol and abs(x[2] - y[2]) <= 0.01 * max(x[2], y[2], 1e-6) for x, y in zip(a, b))
    for label, good, leaves in groups:
        others = sum(len(g) for lb, g, lv in groups if lb != label and agree(leaves, lv))
        if len(good) >= 3 or others:
            return Shape(blueprint, leaves, label, len(good), others)
    label, good, leaves = max(groups, key=lambda g: len(g[1]))
    return Shape(blueprint, leaves, label, len(good), 0)


def _shape_leaves(good, k):
    leaves = []
    for j in range(k):
        c = tuple(_median([g[2][j][0][a] for g in good]) for a in range(3))
        w = max(g[2][j][1] for g in good)
        mx = _median([g[2][j][2] for g in good])
        leaves.append((c, w, mx))
    return leaves


def _consensus(group, k, tol):
    if not group:
        return []
    med = [tuple(_median([g[2][j][0][a] for g in group]) for a in range(3)) for j in range(k)]
    good = [g for g in group if all(_dist(g[2][j][0], med[j]) <= tol for j in range(k))]
    return good if len(good) * 2 > len(group) else []


def ref_pairs(refs, ref_dirs, log):
    """(tree, level, label) for each --ref level and every level under each --ref-dir that has its tree."""
    paths = list(refs)
    for d in ref_dirs:
        for p in sorted(glob.glob(os.path.join(d, '**', '*.dbx'), recursive=True)):
            b = os.path.basename(p).lower()
            if 'staticculltree' in b or '_description' in b:
                continue
            with open(p, 'rb') as f:
                if b'ExclusiveStaticCullTree' not in f.read():      # not a level partition
                    continue
            paths.append(p)
    out, seen = [], set()
    for p in paths:
        try:
            lv = Level(p)
        except CullTreeError:
            continue
        tp = lv.tree_path()
        if not tp:
            if p in refs:
                log(f'note: {p}: its exclusive cull tree was not found beside it; not used')
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


# ------------------------------------------------------------------------------------------------ add

def insert(tree, sub, sph):
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
        cands = [c for c in (n.left, n.right) if c.sphere.contains(sph)]
        if cands:
            c = min(cands, key=lambda x: x.sphere.r)
            if c.leaf is None:
                path.append(c)
                n = c
                continue
            target, parent = c, n
        else:
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


def make_subtree(placed, first_leaf):
    """The nodes for one object's leaves: one leaf, or an internal node over a balanced split of them (two leaves:
    an internal node with the two leaves, as stock trees often hold an object)."""
    leaves = []
    for j, (s, mx) in enumerate(placed):
        keys = encode(s)
        leaves.append(Node(((LEAF | (first_leaf + j)) - (1 << 32), mx, *keys), leaf=first_leaf + j, new=True))

    def join(ns):
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


def add(tree, level, objects, found, log):
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


def _fmt(v):
    return '(' + ', '.join(f'{x:.1f}' for x in v) + ')'


def auto_objects(level, found):
    return [o for o in level.objects if o.enum == 0 and o.blueprint and o.blueprint in found]


def misplaced(tree, level, found, metres):
    """[(object, metres off)]: slotted objects whose leaves lie more than metres from where the blueprint's other
    instances (leave-one-out) put them, and whose blueprint has a shape without them."""
    out = []
    for o, ids in leaves_of(tree, level).items():
        rest = {b: [x for x in v if x[3] is not o] for b, v in found.items() if b == o.blueprint}
        sh = shape_for(o.blueprint, rest)
        if sh is None:
            continue
        k = min(len(sh.leaves), len(ids))
        off = [_dist(o.to_world(sh.leaves[j][0]), tree.leaf_node[ids[j]].sphere.c) for j in range(k)
               if tree.leaf_node.get(ids[j]) is not None]
        if off and max(off) > metres:
            out.append((o, max(off)))
    return out


# ------------------------------------------------------------------------------------------------ checks

def check_structure(tree, client=None):
    """[(ok, text)] for the tree's own rules."""
    res = []
    flat = flatten(tree.root)
    res.append((flat == tree.nodes[:tree.used] or tree.path is None,
                f'preorder layout and Right offsets: {tree.used} used nodes, every Right = 1 + left subtree size'))
    res.append((tree.spare == SPARE, f'{tree.spare} all-zero spare nodes after the used ones (stock trees: {SPARE})'))
    res.append((tree.leaf_range_ok(), f'leaf indices 1..{tree.leaves}, each once'))
    res.append((tree.used == 2 * tree.leaves - 1, f'{tree.used} used nodes = 2 x {tree.leaves} leaves - 1 (binary tree)'))
    bad_max = [n for n, _ in walk(tree.root) if n.leaf is None and n.max != max(n.left.max, n.right.max)]
    res.append((not bad_max, f'every internal node\'s Max is the larger of its children\'s ({len(bad_max)} not)'))
    depth, most = tree.depth_and_stack()
    res.append((most <= STACK_SLOTS, f'deepest leaf {depth} levels; traversal stack up to {most} of {STACK_SLOTS} entries'))
    if client is not None:
        res.append((tree.enum_offset == client.leaves and client.enum_offset == 0,
                    f'EnumOffset {tree.enum_offset} = client tree leaves {client.leaves} (client EnumOffset '
                    f'{client.enum_offset})'))
    return res


def containment(tree, leaf_ids):
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


def object_checks(tree, level, found):
    """Slots and spheres of the level's objects: [(ok, text)], details [str]."""
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
    errs, rad_short, n_cmp, worst = [], 0, 0, []
    for o, ids in lv.items():
        inst = [x for x in found.get(o.blueprint, []) if x[3] is not o]
        if len(inst) < 2:
            continue
        sh = shape_for(o.blueprint, {o.blueprint: inst})
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
    inside = sum(1 for o, ids in lv.items() if ids[0] in tree.leaf_node and tree.leaf_node[ids[0]].sphere.contains(Sphere(o.pos, 0)))
    res.append((True, f'object origin inside its first leaf\'s sphere: {inside} of {len(lv)} (models built away '
                      'from their origin, e.g. distant scenery, are not)'))
    zero = [o for o in level.objects if o.enum == 0]
    cand = auto_objects(level, found)
    res.append((not cand, f'{len(zero)} objects with slot 0; {len(cand)} of them have a blueprint that is in a cull '
                          'tree (see-through / indestructible in game: candidates for add)'))
    for o in cand:
        detail.append(f'  slot 0: {o.guid} {o.blueprint} at {_fmt(o.pos)}')
    return res, detail


# ------------------------------------------------------------------------------------------------ command line

def _arg(args, name, many=False):
    vals = []
    while name in args:
        i = args.index(name)
        if i + 1 >= len(args):
            raise CullTreeError(f'{name} needs a value')
        vals.append(args[i + 1])
        del args[i:i + 2]
    return vals if many else (vals[-1] if vals else None)


def _flag(args, name):
    if name in args:
        args.remove(name)
        return True
    return False


class Out:
    def __init__(self):
        self.lines = []

    def __call__(self, s=''):
        print(s, flush=True)
        self.lines.append(s)

    def save(self, path):
        if path:
            os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
            with open(path, 'w', encoding='utf-8') as f:
                f.write('\n'.join(self.lines) + '\n')


def _report(res, log):
    bad = 0
    for ok, text in res:
        bad += not ok
        log(f'{"ok  " if ok else "FAIL"} {text}')
    return bad


def cmd_info(args, log):
    t = CullTree(args[0])
    depth, most = t.depth_and_stack()
    r = t.root.sphere
    log(f'{args[0]}')
    log(f'  nodes {len(t.nodes)}: {t.used} used ({t.leaves} leaves, {t.used - t.leaves} internal), {t.spare} spare')
    log(f'  EnumOffset {t.enum_offset}: slots {t.enum_offset + 1}..{t.enum_offset + t.leaves}')
    log(f'  root sphere centre {_fmt(r.c)} radius {r.r:.2f} m, Max {t.root.max:.3f}; deepest leaf {depth}, stack {most}')
    if len(args) > 1:
        lv = Level(args[1])
        found = collect([(t, lv, 'level')])
        lo = leaves_of(t, lv)
        ks = Counter(len(v) for v in lo.values())
        log(f'{args[1]}: {len(lv.objects)} placed objects, {len(lo)} with a slot in this tree '
            f'(leaves per object: {dict(sorted(ks.items()))}), {sum(1 for o in lv.objects if o.enum == 0)} with slot 0')
        for o in auto_objects(lv, found):
            log(f'  slot 0, blueprint in this tree: {o.guid} {o.blueprint} at {_fmt(o.pos)}')
    return 0


def cmd_verify(args, log):
    refs, dirs = _arg(args, '--ref', True), _arg(args, '--ref-dir', True)
    client_p, added, report = _arg(args, '--client'), _arg(args, '--added'), _arg(args, '--report')
    if len(args) != 2:
        raise CullTreeError('verify needs <tree.dbx> <level.dbx>')
    t, lv = CullTree(args[0]), Level(args[1])
    client = CullTree(client_p) if client_p else None
    log(f'tree  {args[0]}\nlevel {args[1]}')
    pairs = [(t, lv, 'level')] + ref_pairs(refs, dirs, log)
    found = collect(pairs)
    bad = _report(check_structure(t, client), log)
    res, detail = object_checks(t, lv, found)
    bad += _report(res, log)
    for d in detail:
        log(d)
    if added:
        guids = [g for g in added.split(',') if g.strip()]
        for g in guids:
            o = lv.by_guid(g)
            if o is None or not o.enum:
                log(f'FAIL added object {g}: not in the level or slot 0')
                bad += 1
                continue
            ids = leaves_of(t, lv).get(o, [])
            c = containment(t, ids)
            miss = sum(m for _, m, _ in c)
            ok = c and miss == 0
            bad += not ok
            log(f'{"ok  " if ok else "FAIL"} added {o.guid} {o.blueprint.rsplit("/", 1)[-1]} slot {o.enum}: leaves '
                f'{ids}, every ancestor (root excepted) contains their spheres'
                f' (path {max((p for _, _, p in c), default=0)} nodes, {miss} not containing)')
    log('all checks passed' if not bad else f'{bad} check(s) failed')
    if isinstance(log, Out):
        log.save(report)
    return 1 if bad else 0


def cmd_add(args, log):
    refs, dirs = _arg(args, '--ref', True), _arg(args, '--ref-dir', True)
    objs_arg, auto = _arg(args, '--objects'), _flag(args, '--auto')
    away = _arg(args, '--misplaced')
    out_tree, out_level = _arg(args, '--out-tree'), _arg(args, '--out-level')
    client_p, report = _arg(args, '--client'), _arg(args, '--report')
    if len(args) != 2 or not out_tree or not out_level or not (objs_arg or auto):
        raise CullTreeError('add needs <tree.dbx> <level.dbx>, --objects or --auto, --out-tree and --out-level')
    for o in (out_tree, out_level):
        if os.path.abspath(o).lower() in (os.path.abspath(args[0]).lower(), os.path.abspath(args[1]).lower()):
            raise CullTreeError('write the results to new files (--out-tree / --out-level), not over the inputs')
    t, lv = CullTree(args[0]), Level(args[1])
    client = CullTree(client_p) if client_p else None
    pairs = [(t, lv, 'level')] + ref_pairs(refs, dirs, log)
    found = collect(pairs)
    if away is not None:
        moved = misplaced(t, lv, found, float(away))
        for o, off in moved:
            log(f'  misplaced: {o.guid} {o.blueprint.rsplit("/", 1)[-1]} at {_fmt(o.pos)} on slot {o.enum}, its '
                f'leaves {off:.2f} m from the blueprint\'s shape: slot 0, new leaves')
            lv.set_enum(o, 0)
        if moved:
            found = collect(pairs)           # without the wrong instances
    if auto:
        objs = auto_objects(lv, found)
        # objects beyond what a tree can hold (+-2048 m) stay at slot 0 - outside the tree, drawn at every distance,
        # as stock levels have them (Atacama's mountain backdrops at x 2337 / -2179)
        far = [o for o in objs if any(abs(c) > 2040 for c in o.pos)]
        for o in far:
            log(f'  beyond the tree\'s range, left at slot 0: {o.guid} {o.blueprint.rsplit("/", 1)[-1]} at {_fmt(o.pos)}')
        objs = [o for o in objs if o not in far]
    else:
        guids = objs_arg
        if guids.startswith('@'):
            with open(guids[1:], encoding='utf-8') as f:
                guids = ','.join(l.split('#')[0].strip() for l in f if l.split('#')[0].strip())
        objs = []
        for g in guids.split(','):
            if not g.strip():
                continue
            o = lv.by_guid(g)
            if o is None:
                raise CullTreeError(f'object {g} is not in {os.path.basename(args[1])}')
            objs.append(o)
    if not objs:
        log('nothing to add')
        return 0
    before = (t.used, t.leaves)
    log(f'adding {len(objs)} object(s) to {args[0]} (EnumOffset {t.enum_offset}, {t.leaves} leaves):')
    done = add(t, lv, objs, found, log)
    t.write(out_tree)
    lv.doc.write(out_level)
    log(f'tree: {before[0]} -> {flatten(t.root).__len__()} used nodes, {before[1]} -> {t.leaves} leaves, '
        f'{SPARE} spare; written {out_tree}')
    log(f'level: {len(done)} Enumeration(s) set; written {out_level}')
    # check what was written
    t2, lv2 = CullTree(out_tree), Level(out_level)
    bad = _report(check_structure(t2, client), log)
    for o, slot, ids, _ in done:
        c = containment(t2, ids)
        miss = sum(m for _, m, _ in c)
        o2 = lv2.by_guid(o.guid)
        ok = len(c) == len(ids) and miss == 0 and o2.enum == slot
        bad += not ok
        log(f'{"ok  " if ok else "FAIL"} {o.guid}: slot {o2.enum}, leaves {ids} reached through containing spheres')
    log('written and checked' if not bad else f'{bad} check(s) failed')
    if isinstance(log, Out):
        log.save(report)
    return 1 if bad else 0


def main(argv):
    args = list(argv[1:])
    if not args or args[0] in ('-h', '--help', 'help'):
        print(__doc__)
        return 0
    cmd, rest = args[0], args[1:]
    log = Out()
    if cmd == 'info':
        if not rest:
            raise CullTreeError('info needs <tree.dbx> [<level.dbx>]')
        return cmd_info(rest, log)
    if cmd == 'verify':
        return cmd_verify(rest, log)
    if cmd == 'add':
        return cmd_add(rest, log)
    raise CullTreeError(f'unknown command {cmd!r}: use info, verify or add')


if __name__ == '__main__':
    sys.path.insert(0, os.path.join(SUITE, 'core'))
    from bfbc2core import errors
    errors.setup('culltree', gui=False)
    with errors.guard():                       # an error: cause, what to do and the log's path; exit code 1
        sys.exit(main(sys.argv))
