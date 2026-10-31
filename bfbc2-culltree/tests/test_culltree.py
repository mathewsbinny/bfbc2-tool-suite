r"""culltree.py on cull trees and levels built here (no game data):

- the DBX codec: a built file read and written back byte for byte;
- sphere keys: centre / 16 and radius / 4 as the game decodes them, encode() always containing what it is given;
- the tree: preorder layout and Right offsets read and written back unchanged, a broken Right offset and a file
  that is not a cull tree reported, the checks (spare nodes, leaf numbering, Max, EnumOffset = client leaves);
- verify: slots, leaves per object, decoded spheres against the blueprint's other instances;
- add: slot-0 objects get new leaves (two per object, as their blueprint has) at the end of the leaf numbering,
  spheres moved and rotated with the object, every ancestor's sphere containing them, Right offsets of the
  ancestors updated, Max kept, 256 spare nodes kept, the level changed only in those Enumeration values; a shape
  taken from a --ref level; an object that already has a slot refused; the command line.

  python test_culltree.py
"""
import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..'))
sys.path.insert(0, ROOT)
import culltree as ct  # noqa: E402


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, ok, what, detail=''):
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


# ------------------------------------------------------------------------------------------------ building files

class Builder:
    """Writes a binary DBX file (FORMAT.md sections 1-2) from nested tuples:
    (tag, kind, [(name, value)...], payload) - payload: children for containers, str for text, (size, [bytes]) for
    numbers, an int for the two-byte kinds."""

    def __init__(self):
        self.strings = ['']

    def s(self, text):
        if text not in self.strings:
            self.strings.append(text)
        return self.strings.index(text)

    @staticmethod
    def varint(v, out):
        while True:
            b = v & 0x7F
            v >>= 7
            out.append(b | (0x80 if v else 0))
            if not v:
                return

    def element(self, e, out):
        tag, kind, attrs, payload = e
        self.varint(self.s(tag), out)
        out.append(kind << 4 | len(attrs))
        for k, v in attrs:
            self.varint(self.s(k), out)
            self.varint(self.s(v), out)
        if kind == ct.CONTAINER:
            out.append(0)
            for c in payload:
                self.element(c, out)
            out.append(0)
        elif kind == ct.TEXT:
            self.varint(self.s(payload), out)
        elif kind == ct.NUMBERS:
            size, vals = payload
            self.varint(len(vals), out)
            self.varint(size, out)
            for v in vals:
                out.extend(v)
        else:
            out.extend(bytes([0, payload]))

    def build(self, root):
        body = bytearray()
        self.element(root, body)
        body.append(0)
        table = bytearray()
        text = bytearray()
        for s in self.strings:
            table += struct.pack('>I', len(text))
            text += s.encode('utf-8') + b'\0'
        area = bytes(table) + bytes(text)
        return (ct.MAGIC + struct.pack('>I', 24 + len(area)) + b'\0\0\0\0' + struct.pack('>II', len(area), len(self.strings))
                + area + bytes(body))


def num4i(name, v):
    return ('field', ct.NUMBERS, [('name', name)], (4, [struct.pack('>i', v)]))


def tree_file(nodes, enum_offset, guid='11111111-0000-0000-0000-000000000001'):
    items = []
    for r, mx, x, y, z, w in nodes:
        items.append(('complex', ct.CONTAINER, [], [
            num4i('Right', r),
            ('field', ct.NUMBERS, [('name', 'Max')], (4, [struct.pack('>f', mx)])),
            ('field', ct.NUMBERS, [('name', 'SphereKeyX')], (2, [struct.pack('>H', x)])),
            ('field', ct.NUMBERS, [('name', 'SphereKeyY')], (2, [struct.pack('>H', y)])),
            ('field', ct.NUMBERS, [('name', 'SphereKeyZ')], (2, [struct.pack('>H', z)])),
            ('field', ct.NUMBERS, [('name', 'SphereKeyW')], (2, [struct.pack('>H', w)]))]))
    root = ('partition', ct.CONTAINER, [('guid', guid), ('primaryInstance', guid), ('exportMode', 'All')], [
        ('instance', ct.CONTAINER, [('guid', guid), ('type', 'World.StaticCullTreeData'), ('id', 'StaticCullTree_Exclusive')], [
            ('array', ct.CONTAINER, [('name', 'Nodes')], items),
            num4i('EnumOffset', enum_offset)])])
    return Builder().build(root)


def rot_y(deg, pos, scale=1.0):
    a = math.radians(deg)
    c, s = math.cos(a) * scale, math.sin(a) * scale
    return [c, 0, -s, 0, 0, scale, 0, 0, s, 0, c, 0, pos[0], pos[1], pos[2], 1]


def level_file(objects, tree_ref='levels/test/staticculltree_exclusive/11111111-0000-0000-0000-000000000001'):
    """objects: (guid, blueprint, transform 16 floats, enumeration)."""
    insts = [('instance', ct.CONTAINER, [('guid', 'AAAAAAAA-0000-0000-0000-000000000000'), ('type', 'Levels.LevelData')], [
        ('field', ct.TEXT, [('ref', tree_ref), ('name', 'ExclusiveStaticCullTree')], ''),
        ('field', ct.TEXT, [('ref', 'null'), ('name', 'ClientStaticCullTree')], '')])]
    for guid, bp, m, en in objects:
        insts.append(('instance', ct.CONTAINER, [('guid', guid), ('type', 'Entity.ReferenceObjectData')], [
            num4i('Enumeration', en),
            ('complex', ct.NUMBERS, [('name', 'Transform')], (4, [struct.pack('>f', v) for v in m])),
            ('field', ct.TEXT, [('ref', f'{bp}/{guid.lower()}'), ('name', 'ReferencedObject')], ''),
            ('field', 6, [('name', 'IsInObjectSet')], 0)]))
    root = ('partition', ct.CONTAINER, [('guid', 'BBBBBBBB-0000-0000-0000-000000000000'), ('exportMode', 'All')], insts)
    return Builder().build(root)


# A blueprint's leaves in its own space: (local centre, true radius); two leaves with the same sphere, as stock
# objects have them.
SHAPES = {'objects/test/hut/hut': [((1.5, 2.0, 0.5), 6.2)] * 2,
          'objects/test/lamp/lamp': [((0.0, 0.0, 0.4), 0.47)] * 2,
          'objects/test/rock/rock': [((0.0, 0.5, 0.0), 2.0)]}


def world_leaves(bp, m):
    out = []
    for c, r in SHAPES[bp]:
        w = tuple(c[0] * m[0 + i] + c[1] * m[4 + i] + c[2] * m[8 + i] + m[12 + i] for i in range(3))
        out.append((ct.Sphere(w, r), r))
    return out


def make_tree(objs, enum_offset):
    """A tree over the slotted objects' leaves (built as add would, bottom-up), and the objects with their slots."""
    leaves, placed, nxt = [], [], 1
    for guid, bp, m, slot in objs:
        if slot == 0:
            placed.append((guid, bp, m, 0))
            continue
        placed.append((guid, bp, m, enum_offset + nxt))
        for s, r in world_leaves(bp, m):
            leaves.append(ct.Node(((ct.LEAF | nxt) - (1 << 32), r, *ct.encode(s)), leaf=nxt))
            nxt += 1

    def join(ns):
        if len(ns) == 1:
            return ns[0]
        h = len(ns) // 2
        a, b = join(ns[:h]), join(ns[h:])
        s = ct.enclose(a.sphere, b.sphere)
        return ct.Node((0, max(a.max, b.max), *ct.encode(s, contain=(a.sphere, b.sphere))), left=a, right=b)
    root = join(leaves)
    return ct.flatten(root) + [(0, 0.0, 0, 0, 0, 0)] * ct.SPARE, placed


def run(args):
    return subprocess.run([sys.executable, os.path.join(ROOT, 'culltree.py')] + args, capture_output=True, text=True,
                          errors='replace', env={**os.environ, 'LOCALAPPDATA': TMP})


# ------------------------------------------------------------------------------------------------ the tests

def main(argv):
    global TMP
    c = Checks()
    TMP = tempfile.mkdtemp(prefix='culltree_')
    try:
        # sphere keys
        n = (0, 1.0, ct.u16(-3499), 9432, ct.u16(-11703), 10661)
        d = ct.decode(n)
        c.check(abs(d.c[0] + 218.6875) < 1e-9 and abs(d.c[1] - 589.5) < 1e-9 and abs(d.r - 2665.25) < 1e-9,
                'keys decode as centre / 16 and radius / 4 (signed 16-bit)')
        ok = True
        for i in range(200):
            s = ct.Sphere((math.sin(i) * 900, math.cos(i * 3) * 300, i * 3.1 - 300), 0.05 + i * 0.37)
            q = ct.decode((0, 0, *ct.encode(s)))
            ok &= q.contains(s) and q.r - s.r < 0.25 + 0.06
        c.check(ok, 'encode() gives keys whose decoded sphere contains the sphere, at most 0.31 m larger')
        try:
            ct.encode(ct.Sphere((3000, 0, 0), 1))
            c.check(False, 'a centre beyond 2048 m refused')
        except ct.CullTreeError:
            c.check(True, 'a centre beyond 2048 m refused')

        # a target level: two huts, a lamp and rocks slotted; a rotated hut and a lamp at slot 0
        eo = 100
        objs = [('00000000-0000-0000-0000-000000000001', 'objects/test/hut/hut', rot_y(0, (10, 5, 20)), 1),
                ('00000000-0000-0000-0000-000000000002', 'objects/test/hut/hut', rot_y(90, (-40, 6, 60)), 1),
                ('00000000-0000-0000-0000-000000000003', 'objects/test/rock/rock', rot_y(30, (5, 1, -70)), 1),
                ('00000000-0000-0000-0000-000000000004', 'objects/test/hut/hut', rot_y(180, (80, 4, -10)), 1),
                ('00000000-0000-0000-0000-000000000005', 'objects/test/rock/rock', rot_y(10, (-60, 2, -30)), 1),
                ('00000000-0000-0000-0000-000000000006', 'objects/test/hut/hut', rot_y(237, (30, 7, 90)), 0),
                ('00000000-0000-0000-0000-000000000007', 'objects/test/lamp/lamp', rot_y(45, (35, 7, 95)), 0)]
        nodes, placed = make_tree(objs, eo)
        lvdir = os.path.join(TMP, 'target', 'levels')
        os.makedirs(os.path.join(lvdir, 'test'))
        tree_p = os.path.join(lvdir, 'test', 'staticculltree_exclusive.dbx')
        level_p = os.path.join(lvdir, 'test.dbx')
        tdata = tree_file(nodes, eo)
        open(tree_p, 'wb').write(tdata)
        ldata = level_file(placed)
        open(level_p, 'wb').write(ldata)
        c.check(ct.Doc(tdata, 'x').tobytes() == tdata and ct.Doc(ldata, 'x').tobytes() == ldata,
                'DBX codec: built tree and level written back byte for byte')

        t = ct.CullTree(tree_p)
        c.check(t.leaves == 8 and t.used == 15 and t.spare == 256 and t.enum_offset == eo,
                'tree read: 8 leaves, 15 used nodes, 256 spare, EnumOffset', f'{t.leaves} {t.used} {t.spare}')
        out = os.path.join(TMP, 'rt.dbx')
        t.write(out)
        c.check(open(out, 'rb').read() == tdata, 'an unchanged tree is written back byte for byte')
        c.check(all(ok for ok, _ in ct.check_structure(t)), 'structure checks pass on the built tree')
        lv = ct.Level(level_p)
        c.check(lv.tree_path() and os.path.samefile(lv.tree_path(), tree_p), 'the level\'s tree found through its reference')
        lo = ct.leaves_of(t, lv)
        c.check(sorted(len(v) for v in lo.values()) == [1, 1, 2, 2, 2], 'leaves per object from the slots')

        # broken trees
        bad = list(nodes)
        bad[0] = (bad[0][0] + 1,) + bad[0][1:]
        p = os.path.join(TMP, 'bad.dbx')
        open(p, 'wb').write(tree_file(bad, eo))
        try:
            ct.CullTree(p)
            c.check(False, 'a wrong Right offset reported')
        except ct.CullTreeError:
            c.check(True, 'a wrong Right offset reported')
        try:
            ct.CullTree(level_p)
            c.check(False, 'a level given as a tree reported')
        except ct.CullTreeError as e:
            c.check('not a cull tree' in str(e), 'a level given as a tree reported')

        # a reference level with lamps (the target has none slotted)
        refdir = os.path.join(TMP, 'ref', 'levels')
        os.makedirs(os.path.join(refdir, 'test'))
        robjs = [(f'10000000-0000-0000-0000-00000000000{i}', 'objects/test/lamp/lamp', rot_y(20 * i, (i * 7, 3, -i * 5)), 1)
                 for i in range(1, 5)]
        rnodes, rplaced = make_tree(robjs, 40)
        open(os.path.join(refdir, 'test', 'staticculltree_exclusive.dbx'), 'wb').write(tree_file(rnodes, 40))
        open(os.path.join(refdir, 'test.dbx'), 'wb').write(level_file(rplaced))

        # verify before: two candidates
        r = run(['verify', tree_p, level_p, '--ref-dir', os.path.join(TMP, 'ref')])
        c.check(r.returncode == 1 and '2 of them have a blueprint' in r.stdout, 'verify lists the 2 slot-0 objects as candidates',
                r.stdout[-400:] + r.stderr[-300:])
        c.check('leave-one-out' in r.stdout and 'FAIL decoded' not in r.stdout, 'verify: decoded spheres agree with the blueprint\'s other instances')

        # add
        ot, ol = os.path.join(TMP, 'out', 'tree.dbx'), os.path.join(TMP, 'out', 'level.dbx')
        r = run(['add', tree_p, level_p, '--auto', '--ref-dir', os.path.join(TMP, 'ref'), '--out-tree', ot, '--out-level', ol])
        c.check(r.returncode == 0 and 'written and checked' in r.stdout, 'add --auto runs and checks what it wrote',
                r.stdout[-600:] + r.stderr[-600:])
        t2, lv2 = ct.CullTree(ot), ct.Level(ol)
        hut, lamp = lv2.by_guid('00000000-0000-0000-0000-000000000006'), lv2.by_guid('00000000-0000-0000-0000-000000000007')
        c.check(hut.enum == eo + 9 and lamp.enum == eo + 11, 'new slots follow the last leaf (EnumOffset + 9, + 11)',
                f'{hut.enum} {lamp.enum}')
        c.check(t2.leaves == 12 and t2.used == 23 and t2.spare == 256, 'tree: 4 new leaves, 8 new nodes, 256 spare kept',
                f'{t2.leaves} {t2.used} {t2.spare}')
        c.check(all(ok for ok, _ in ct.check_structure(t2)), 'structure checks pass after add (Right offsets, Max, leaves)')
        want = world_leaves('objects/test/hut/hut', rot_y(237, (30, 7, 90)))[0][0]
        got = t2.leaf_node[9].sphere
        c.check(ct._dist(got.c, want.c) < 0.1 and got.contains(want), 'the new hut\'s sphere moved and rotated with the object',
                f'{got.c} {want.c}')
        lw = world_leaves('objects/test/lamp/lamp', rot_y(45, (35, 7, 95)))[0][0]
        c.check(ct._dist(t2.leaf_node[11].sphere.c, lw.c) < 0.1, 'the lamp\'s shape taken from the --ref level')
        miss = [m for _, m, _ in ct.containment(t2, [9, 10, 11, 12])]
        c.check(miss == [0, 0, 0, 0], 'every ancestor of the new leaves contains their spheres')
        c.check(all(t2.leaf_node[i].data == t.leaf_node[i].data for i in range(1, 9)), 'old leaves unchanged')
        a, b = open(level_p, 'rb').read(), open(ol, 'rb').read()
        c.check(len(a) == len(b) and sum(x != y for x, y in zip(a, b)) <= 4, 'the level changed only in the two Enumeration values')
        r = run(['verify', ot, ol, '--ref-dir', os.path.join(TMP, 'ref'), '--added',
                 '00000000-0000-0000-0000-000000000006,00000000-0000-0000-0000-000000000007'])
        c.check(r.returncode == 0 and 'all checks passed' in r.stdout, 'verify passes on the result, with --added', r.stdout[-600:])

        # refused cases
        r = run(['add', ot, ol, '--objects', '00000000-0000-0000-0000-000000000006', '--out-tree', ot + '2', '--out-level', ol + '2'])
        c.check(r.returncode == 1 and 'already has slot' in r.stderr, 'add refuses an object that has a slot', r.stderr[-300:])
        r = run(['add', tree_p, level_p, '--objects', '00000000-0000-0000-0000-000000000007', '--out-tree', ot + '3', '--out-level', ol + '3'])
        c.check(r.returncode == 1 and 'no slotted instance' in r.stderr, 'add without a shape for the blueprint says what to add', r.stderr[-300:])
        r = run(['info', os.path.join(TMP, 'missing.dbx')])
        c.check(r.returncode == 1 and 'what to do' in r.stderr.lower(), 'a missing file: exit code 1, cause and what to do')
        r = run(['add', tree_p, level_p, '--auto', '--out-tree', tree_p, '--out-level', ol])
        c.check(r.returncode == 1 and 'new files' in r.stderr, 'add does not write over its inputs')
    finally:
        shutil.rmtree(TMP, ignore_errors=True)
    print('all passed' if not c.failed else f'{c.failed} check(s) failed')
    return 1 if c.failed else 0


TMP = None

if __name__ == '__main__':
    sys.exit(main(sys.argv))
