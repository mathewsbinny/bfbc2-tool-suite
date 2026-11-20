# mypy: disable-error-code="attr-defined"
# (round RT2: the retired references are named here; at run time tests/replay.py stands in for them)
r"""Round F0: mesh resource headers in the Rust core - MeshData (mesh.parse / write: bc2_native.mesh_parse /
mesh_write) and the mesh sets (meshset.parse / write: meshset_parse / meshset_write) - against their Python, the
reference that stays until the port passes a full test tier (parse_reference, write_reference in both modules).

Compared: the parsed dicts (keys in order, every value and type, floats bit for bit), the PC bytes written; for
damaged inputs and changed dicts the same result or the same error.

  python test_meshdata_native.py          quick: headers and sets of every kind built here (both byte orders, with
                                          and without the flag byte), 3,000 damaged copies, 800 changed dicts
                                          written both ways; 60 of Battlefield 1943's mesh resources when found
  python test_meshdata_native.py --game   + every MeshData and mesh set of Battlefield 1943, of BFBC2's unpacked
                                          Xbox bundles (OPFOR work\xbox) and of the game copy (PC), read both ways
                                          and written both ways (the Xbox ones as the converter writes them); 2
                                          damaged copies of each; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.

The mesh.parse / write and meshset.parse references were retired in round RT2 (meshset.write_reference stays:
compared live): their answers were recorded first (--record, and --game --record, while they were there:
f0_meshdata_replay*.json beside this test) and are replayed by tests/replay.py where they are gone.
"""
import copy
import glob
import os
import random
import struct
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core')
import mesh  # noqa: E402
import meshset  # noqa: E402
from bfbc2core import fbrb, native, paths  # noqa: E402
from f0_parity import Checks, outcome, stdout_utf8  # noqa: E402
from replay import Replay  # noqa: E402

N = native.require('mesh.py / meshset.py headers', 'mesh_parse')
SETS = meshset.KINDS
# round RT2: the references' answers recorded (--record) and replayed once they are retired
R = Replay('f0_meshdata', HERE, [(mesh, 'parse_reference'), (mesh, 'write_reference'),
                                 (meshset, 'parse_reference'), (meshset, 'write_reference')])


class Tally:
    """Counts; fallback_ok: the reference answered and the Rust core left it to the reference."""

    def __init__(self) -> None:
        self.same = self.ndiff = self.native = self.fallback = self.fallback_ok = 0
        self.differ: list[str] = []

    def add(self, ok: bool, label: str, answered: bool, ref_ok: bool) -> None:
        self.native += answered
        self.fallback += not answered
        self.fallback_ok += (not answered) and ref_ok
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)

    def text(self) -> str:
        return (f'{self.same} identical ({self.native} answered in Rust, {self.fallback} left to the reference, '
                f'{self.fallback_ok} of them answered by it)')


def both(t: Tally, label: str, fn: Any, ref: Any, nat: Any, *args: Any) -> Any:
    a, r = outcome(fn, *args), outcome(ref, *args)
    t.add(a == r, f'{label}: {str(a)[:90]} / {str(r)[:90]}', nat(*args) is not None, r[0] == 'ok')
    return fn(*args) if a[0] == 'ok' else None


def mesh_all(t: Tally, label: str, b: bytes, big: bool) -> None:
    m = both(t, label, mesh.parse, mesh.parse_reference, N.mesh_parse, b, big)
    if m is not None:
        d = m['data']
        verts, idx = b[d:d + m['vertex_bytes']], b[d + m['vertex_bytes']:]
        both(t, label + ' write', mesh.write, mesh.write_reference, N.mesh_write, m, verts, idx)


def set_all(t: Tally, label: str, b: bytes, kind: str, big: bool, flag: Any = None) -> None:
    s = both(t, label, meshset.parse, meshset.parse_reference, lambda *a: N.meshset_parse(*a), b, kind, big, flag)
    if s is not None:
        both(t, label + ' write', meshset.write, meshset.write_reference, N.meshset_write, s, kind)


# ------------------------------------------------------------------------------------------------ built inputs

def f32(rng: random.Random) -> float:
    if rng.random() < 0.05:
        return rng.choice([0.0, -0.0, float('inf'), 1e-45])
    return struct.unpack('<f', struct.pack('<f', rng.uniform(-1e4, 1e4)))[0]


def name(rng: random.Random) -> bytes:
    return bytes(rng.randrange(1, 256) for _ in range(rng.randrange(12)))


def build_mesh(rng: random.Random) -> dict[str, Any]:
    return {'h0': rng.randrange(1 << 32), 'h1': bytes([rng.randrange(256), rng.randrange(256)]),
            'parts': [{'name': name(rng), 'triangles': rng.randrange(1 << 20), 'vertices': rng.randrange(1 << 20),
                       'first_index': rng.randrange(1 << 20), 'vertex_offset': rng.randrange(1 << 20),
                       'stride': rng.randrange(256), 'k': rng.randrange(256), 'fa': rng.randrange(256),
                       'fb': rng.randrange(256), 'bones': [rng.randrange(1 << 16) for _ in range(rng.randrange(5))]}
                      for _ in range(rng.randrange(5))],
            'lists': [[rng.randrange(256) for _ in range(rng.randrange(4))] for _ in range(4)]}


def to_big(m: dict[str, Any], verts: bytes, idx: bytes) -> bytes:
    """A MeshData in the Xbox's byte order."""
    out = bytearray(struct.pack('>I', m['h0']) + m['h1'] + bytes([len(m['parts'])]))
    for p in m['parts']:
        out += p['name'] + b'\0' + struct.pack('>4I4B', p['triangles'], p['vertices'], p['first_index'],
                                               p['vertex_offset'], p['stride'], p['k'], p['fa'], p['fb'])
        out += struct.pack('>H%dH' % len(p['bones']), len(p['bones']), *p['bones'])
    for li in m['lists']:
        out += struct.pack('>I', len(li)) + bytes(li)
    return bytes(out + struct.pack('>3I', len(verts), len(idx), 1) + verts + idx)


def build_set(rng: random.Random, kind: str) -> dict[str, Any]:
    lods = []
    for _ in range(rng.randrange(1, 4)):
        n = rng.randrange(3)
        L: dict[str, Any] = {'x': rng.randrange(256), 'shaders': n,
                             'sets': [(name(rng), [name(rng) for _ in range(n)]) for _ in range(rng.randrange(3))],
                             'bounds': tuple(f32(rng) for _ in range(6)), 'size': rng.randrange(1 << 32),
                             'extra': rng.randrange(1 << 32), 'flag': None if kind == 'SkinnedMeshSet' else rng.randrange(256)}
        if kind in meshset.COMPOSITE_LIKE:
            L['parts'] = [(rng.randrange(1 << 32), name(rng), tuple(f32(rng) for _ in range(6)),
                           tuple(f32(rng) for _ in range(12))) for _ in range(rng.randrange(3))]
            L['masks'] = [tuple(rng.randrange(1 << 64) for _ in range(3)) for _ in range(n)]
        if kind == 'TreeMeshSet':
            L['billboard'], L['tree_value'] = name(rng), rng.randrange(1 << 32)
        if kind == 'SkinnedMeshSet':
            nb = rng.randrange(4)
            L['bones'] = [rng.randrange(1 << 32) for _ in range(nb)]
            L['bone_hashes'] = [rng.randrange(1 << 32) for _ in range(nb)]
        lods.append(L)
    return {'a': rng.randrange(1 << 32), 'z': rng.randrange(256), 'lod_settings': name(rng), 'lods': lods}


def damaged(b: bytes, rng: random.Random) -> bytes:
    x = bytearray(b)
    k = rng.randrange(3)
    if k in (1, 2) and x:
        for _ in range(rng.randint(1, 3)):
            x[rng.randrange(len(x))] = rng.choice([0, 255, rng.randrange(256)])
    if k in (0, 2) and x:
        del x[rng.randrange(len(x)):]
    return bytes(x)


BAD = [-1, 256, 1 << 32, 1.5, None, True, 'x', b'x', 0]


def change(d: Any, rng: random.Random, depth: int = 0) -> Any:
    """A copy of a parsed dict with one value somewhere replaced (a dict's value, a list's item, a tuple's item)."""
    g = copy.deepcopy(d)
    node: Any = g
    for _ in range(rng.randrange(1, 5)):
        if isinstance(node, dict) and node:
            k = rng.choice(list(node))
            if rng.random() < 0.4 or not isinstance(node[k], dict | list | tuple) or not node[k]:
                node[k] = rng.choice(BAD + [[], (), 1e39, float('nan'), [1, 2], (b'a', [b'b'])])
                return g
            if isinstance(node[k], tuple):
                node[k] = list(node[k]) if rng.random() < 0.5 else node[k]
            node = node[k]
        elif isinstance(node, list) and node:
            i = rng.randrange(len(node))
            if rng.random() < 0.4 or not isinstance(node[i], dict | list | tuple) or not node[i]:
                node[i] = rng.choice(BAD + [1e39, float('nan')])
                return g
            if isinstance(node[i], tuple):
                node[i] = list(node[i])
            node = node[i]
        else:
            return g
    return g


# ------------------------------------------------------------------------------------------------ the game's meshes

KIND_NAMES = ('MeshData',) + SETS


def bf1943_items(limit: int | None) -> list[tuple[str, bytes, str, bool]]:
    try:
        suite_paths.use('bf1943-import')
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'skip Battlefield 1943 ({e})')
        return []
    ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') in KIND_NAMES)
    if limit:
        ks = ks[::max(len(ks) // limit, 1)][:limit]
    return [(k, src.data(k), str(src.entries[k][1].get('type')), True) for k in ks]


def bundle_items(pattern: str, big: bool) -> list[tuple[str, bytes, str, bool]]:
    where: dict[str, tuple[str, str, str]] = {}
    for p in sorted(glob.glob(pattern, recursive=True)):
        try:
            entries = fbrb.entries(p)
        except Exception:                                        # noqa: BLE001
            continue
        for e in entries:
            if e.type in KIND_NAMES and not e.deleted:
                where.setdefault(e.name.lower(), (p, e.name, e.type))
    by: dict[str, list[tuple[str, str]]] = {}
    for p, n, ty in where.values():
        by.setdefault(p, []).append((n, ty))
    out = []
    for p, nts in sorted(by.items()):
        types = dict(nts)
        for n, b in sorted(fbrb.read(p, [n for n, _ in nts]).items()):
            out.append((n.lower(), b, types[n], big))
    return out


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    if hasattr(mesh, 'use_native'):
        c.check(mesh.use_native(True), 'the Rust core runs mesh.py\'s and meshset.py\'s headers')
    rng = random.Random(0xF04)

    t = Tally()
    built_meshes = []
    for i in range(300):
        m = build_mesh(rng)
        verts, idx = bytes(rng.randrange(256) for _ in range(rng.randrange(40))), bytes(rng.randrange(8))
        pc = mesh.write(m, verts, idx)
        xb = to_big(m, verts, idx)
        built_meshes += [(pc, False), (xb, True)]
        mesh_all(t, f'built mesh {i} PC', pc, False)
        mesh_all(t, f'built mesh {i} Xbox', xb, True)
    built_sets = []
    for i in range(300):
        kind = SETS[i % 4]
        s = build_set(rng, kind)
        b = meshset.write(s, kind)
        built_sets.append((b, kind))
        set_all(t, f'built {kind} {i}', b, kind, False)
        if kind == 'RigidMeshSet':
            set_all(t, f'built {kind} {i} without the flag', b, kind, False, False)
        set_all(t, f'built {kind} {i} as big-endian', b, kind, True)
    c.check(not t.ndiff, f'built headers and sets of every kind both ways: {t.text()}', '; '.join(t.differ))

    td = Tally()
    for i in range(1500):
        b, big = rng.choice(built_meshes)
        both(td, f'damaged mesh {i}', mesh.parse, mesh.parse_reference, N.mesh_parse, damaged(b, rng), big)
        b, kind = rng.choice(built_sets)
        k = rng.choice([kind, kind, rng.choice(SETS), 'Mesh'])
        set_all(td, f'damaged set {i}', damaged(b, rng), k, False, rng.choice([None, None, False, True, 0, 'x']))
    c.check(not td.ndiff, f'damaged headers and sets: {td.text()}', '; '.join(td.differ))

    tc = Tally()
    for i in range(400):
        b, big = rng.choice(built_meshes)
        m = mesh.parse(b, big)
        d = m['data']
        both(tc, f'changed mesh {i}', mesh.write, mesh.write_reference, N.mesh_write, change(m, rng),
             rng.choice([b[d:d + m['vertex_bytes']], bytearray(b'ab'), memoryview(b'ab'), 'ab']), b'')
        b, kind = rng.choice(built_sets)
        s = meshset.parse(b, kind, False)
        both(tc, f'changed set {i}', meshset.write, meshset.write_reference, N.meshset_write, change(s, rng),
             rng.choice([kind, kind, kind, rng.choice(SETS), 'Mesh']))
    c.check(not tc.ndiff, f'changed dicts written: {tc.text()}', '; '.join(tc.differ))

    items = bf1943_items(None if game else 60)
    if game:
        opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
        items += bundle_items(os.path.join(opfor, 'work', 'xbox', '**', '*.fbrb'), True)
        items += bundle_items(os.path.join(opfor, 'game', '**', '*.fbrb'), False)
    if items:
        tg, tgd = Tally(), Tally()
        tn = tr = 0.0
        kinds: dict[str, int] = {}
        for nm, b, kind, big in items:
            kinds[kind] = kinds.get(kind, 0) + 1
            flag = False if (big and kind == 'RigidMeshSet' and nm.startswith('bf1943')) else None
            for use, cost in ((True, 'n'), (False, 'r'))[:1 + R.live(mesh, 'parse_reference')]:
                t0 = time.perf_counter()
                try:
                    if kind == 'MeshData':
                        (mesh.parse if use else mesh.parse_reference)(b, big)
                    else:
                        (meshset.parse if use else meshset.parse_reference)(b, kind, big)
                except Exception:                                # noqa: BLE001
                    pass
                if cost == 'n':
                    tn += time.perf_counter() - t0
                else:
                    tr += time.perf_counter() - t0
            if kind == 'MeshData':
                mesh_all(tg, nm, b, big)
            else:
                set_all(tg, nm, b, kind, big, flag)
                if big and kind == 'RigidMeshSet':
                    set_all(tg, nm + ' (no flag byte)', b, kind, big, False)
            frng = random.Random(len(b))
            for j in range(2 if game else 1):
                if kind == 'MeshData':
                    both(tgd, f'{nm} damaged {j}', mesh.parse, mesh.parse_reference, N.mesh_parse, damaged(b, frng), big)
                else:
                    set_all(tgd, f'{nm} damaged {j}', damaged(b, frng), kind, big)
        what = ', '.join(f'{v:,} {k}' for k, v in sorted(kinds.items()))
        c.check(not tg.ndiff and not tg.fallback_ok, f'game mesh resources ({what}) read and written both ways: {tg.text()}',
                '; '.join(tg.differ))
        c.check(not tgd.ndiff, f'damaged game mesh resources: {tgd.text()}', '; '.join(tgd.differ))
        print(f'timing (parse): Python {tr:.2f} s, Rust {tn:.2f} s')
    c.check(R.finish(), 'the references: recorded, or replayed as recorded')
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
