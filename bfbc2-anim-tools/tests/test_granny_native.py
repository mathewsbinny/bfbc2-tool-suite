r"""granny.py's Rust core (bc2_native: bc2_formats::granny, bc2_convert::granny): the reader against granny.py's
Python reader, the format reference that stays (the same tables, the same object trees: every value, its type,
every dict's key order, floats bit for bit, the same exceptions), and the Xbox 360 -> PC conversion (to_pc) against
results recorded from its retired Python code.

The Python to_pc (_to_pc_python, _marshal, _swap) was the reference until 2026-11-18, when the port had passed the
full test tier and it was retired (rust/BENCHMARKS.md, "Retired Python"). Its results were recorded first, from the
Python code itself, in granny_golden.json beside this file: each conversion's output SHA-1 or its exception, under
"quick", "xbox" and "bf1943" (with each input's SHA-1). Where the Rust core now answers otherwise on purpose (a
damaged resource the Python code converted or failed on in another way, now a GrannyError saying why), the new
answer is under "now", case by case; every other case must give the recorded answer.

  python test_granny_native.py           quick: resources built here (both byte orders, every member type: inline
                                         structs and arrays, references, arrays of references, variants, variant
                                         arrays, strings by index and by pointer, transforms, 8 / 16 / 32-bit and
                                         real16 / real32 numbers, a DICE bone table renumbered), 2,500 damaged
                                         copies of them (bytes changed, cut short), to_pc_many / roots, a Granny
                                         written to (put) reading as the Python reader does
  python test_granny_native.py --game    + every Granny resource (GrannyAnimation, DeltaAnimation, GrannyModel) of
                                         your Xbox mp_common / sp_common (OPFOR's work\xbox), their PC twins and the
                                         game copy's, and every one of Battlefield 1943's bundles (the Rising Sun
                                         workspace): read both ways, the Xbox ones converted (BF1943's soldier
                                         animations also with the bone map onto BFBC2's soldier) against the
                                         recording; timings
  python test_granny_native.py --record [--game]
                                         write the "now" answers from the current code where they differ from the
                                         recording: only after a deliberate change, proven on the game's files
                                         (test_granny_port.py --game) - the recording is the only oracle

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import glob
import hashlib
import json
import math
import os
import random
import struct
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'anim-tools')
import granny  # noqa: E402
from bfbc2core import paths  # noqa: E402

KINDS = ('GrannyAnimation', 'DeltaAnimation', 'GrannyModel')
NATIVE = granny._import_native()
GOLDEN = os.path.join(HERE, 'granny_golden.json')


def use(on: bool) -> None:
    """granny's reader for this process (granny.use_native without loading the module again each time): the Rust
    core or the Python reader. to_pc always runs in the Rust core."""
    granny._native = NATIVE if on else None


def to_pc(b: bytes, bones: Any, where: str) -> bytes:
    """The conversion the recording is compared with (granny.to_pc)."""
    return granny.to_pc(b, bones, where)


def sha1(b: bytes) -> str:
    """The first 16 hex digits of b's SHA-1."""
    return hashlib.sha1(b).hexdigest()[:16]


# ------------------------------------------------------------------------------------------------ the recording

def load() -> dict[str, Any]:
    try:
        with open(GOLDEN, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def write(golden: dict[str, Any]) -> None:
    """granny_golden.json, one case per line (a change shows as the lines of the cases it changed)."""
    lines = ['{']
    keys = sorted(golden)
    for i, k in enumerate(keys):
        v, end = golden[k], (',' if i < len(keys) - 1 else '')
        if isinstance(v, dict) and v:
            rows = [f'{json.dumps(n)}: {json.dumps(v[n])}' for n in sorted(v)]
            lines += [f'{json.dumps(k)}: {{', *[r + ',' for r in rows[:-1]], *rows[-1:], '}' + end]
        else:
            lines.append(f'{json.dumps(k)}: {json.dumps(v)}{end}')
    lines.append('}')
    with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


GOLD: dict[str, Any] = {}
RECORD = [False]


class Recorded:
    """Each conversion's recorded answer ("now" over the reference's) against the current one; with --record, the
    current answers that differ from the reference's go to "now"."""

    def __init__(self, group: str):
        self.ref: dict[str, Any] = GOLD.get(group, {})
        self.now: dict[str, Any] = GOLD.setdefault('now', {})
        self.group = group
        self.n = self.same = self.changed = 0
        self.diffs: list[str] = []

    def __call__(self, name: str, got: list[str], input_sha: str | None = None) -> None:
        key = f'{self.group}/{name}'
        ref = self.ref.get(name)
        if ref is None or (input_sha is not None and ref[0] != input_sha):
            self.diffs.append(f'{key}: ' + ('not recorded' if ref is None else 'the input is not the recorded one'))
            return
        ref = ref[1:] if input_sha is not None else ref
        self.n += 1
        if RECORD[0]:
            if got != ref:
                self.now[key] = got
            else:
                self.now.pop(key, None)
            return
        want = self.now.get(key, ref)
        self.changed += key in self.now
        if got == want:
            self.same += 1
        else:
            self.diffs.append(f'{key}: to_pc {got} != recorded {want}')

    def report(self, check, what: str) -> None:
        if RECORD[0]:
            print(f'     {what}: {self.n} conversions recorded')
            return
        check(not self.diffs, f'{what}: {self.same} of {self.n} conversions as recorded ({self.changed} of them '
              'answered as "now" records, not as the Python code did)', '; '.join(self.diffs[:5]))


def converted(b: bytes, bones: Any, where: str) -> tuple[list[str], bytes | None]:
    """(to_pc's outcome as recorded: ['ok', SHA-1] or ['error', exception type, message]; the PC bytes or None)."""
    try:
        out = to_pc(b, bones, where)
    except Exception as e:                                   # noqa: BLE001 - recorded, compared
        return ['error', type(e).__name__, str(e)], None
    return ['ok', sha1(out)], out


# ------------------------------------------------------------------------------------------------ comparing

def same(a: Any, b: Any) -> bool:
    """Equal values of the same types, dict keys in the same order, floats bit for bit."""
    if type(a) is not type(b):
        return False
    if isinstance(a, float):
        return struct.pack('<d', a) == struct.pack('<d', b) or (math.isnan(a) and math.isnan(b))
    if isinstance(a, dict):
        return list(a) == list(b) and all(same(a[k], b[k]) for k in a)
    if isinstance(a, (list, tuple)):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b))
    return a == b


def outcome(fn) -> tuple[str, Any]:
    try:
        return 'ok', fn()
    except Exception as e:                                   # noqa: BLE001 - compared
        return type(e).__name__, str(e)


def both(fn) -> tuple[tuple[str, Any], tuple[str, Any]]:
    """fn() with the Rust core, then in Python."""
    use(True)
    try:
        rs = outcome(fn)
    finally:
        use(False)
    try:
        py = outcome(fn)
    finally:
        use(True)
    return rs, py


def tables(g) -> tuple:
    return (g.e, g.strings_from, g.strings, g.gr2, g.root_type, g.root_obj, g.total, g.crc_from,
            [bytes(s) for s in g.sections], list(g.section_at), list(g.reloc.items()))


def check_one(b: bytes, rec: Recorded, name: str, bones=None) -> list[str]:
    """[differences] between the Rust reader and the Python reader for one resource (the tables, root()); its
    to_pc against the recording (rec, as name)."""
    diffs = []
    rs, py = both(lambda: tables(granny.Granny(b, 'x')))
    if rs[0] != py[0] or not same(rs[1], py[1]):
        diffs.append(f'tables: {rs[0]} / {py[0]}')
    rs, py = both(lambda: granny.Granny(b, 'x').root())
    if rs[0] != py[0] or not same(rs[1], py[1]):
        diffs.append(f'root: {rs[0]} {str(rs[1])[:80]} / {py[0]} {str(py[1])[:80]}')
    rec(name, converted(b, bones, 'x')[0])
    return diffs


# ------------------------------------------------------------------------------------------------ built resources

class Builder:
    """A Granny resource of one section in byte order e ('<' or '>'): types, then objects; relocations and a
    marshalling list (every object) as Granny writes them."""

    def __init__(self, e: str):
        self.e = e
        self.sec = bytearray()
        self.reloc: list[tuple[int, int, int]] = []
        self.marsh: list[tuple[int, int, int, int]] = []
        self.strings = ['<file>']

    def s(self, name: str) -> int:
        if name not in self.strings:
            self.strings.append(name)
        return self.strings.index(name)

    def alloc(self, n: int) -> int:
        while len(self.sec) % 4:
            self.sec.append(0)
        at = len(self.sec)
        self.sec += bytes(n)
        return at

    def put(self, at: int, fmt: str, *v) -> None:
        struct.pack_into(self.e + fmt, self.sec, at, *v)

    def ptr(self, at: int, to: int) -> None:
        self.reloc.append((at, 0, to))

    def type(self, members) -> int:
        at = self.alloc(32 * (len(members) + 1))
        for i, (t, name, child, arr) in enumerate(members):
            self.put(at + 32 * i, '4I', t, self.s(name), 0, arr)
            if child is not None:
                self.ptr(at + 32 * i + 8, child)
        return at

    def resource(self, root_type: int, root_obj: int, words=(10,)) -> bytes:
        e = self.e
        size = len(self.sec)
        f16 = size - size % 4
        rel = b''.join(struct.pack(e + '3I', *r) for r in self.reloc)
        mar = b''.join(struct.pack(e + '4I', *m) for m in self.marsh)
        soff, count = 40, 1
        head = 72 + 44 * count
        sec_at = head
        roff = sec_at + size
        moff = roff + len(rel)
        total = moff + len(mar)
        g = bytearray((granny.MAGIC if e == '<' else granny.MAGIC_BE) + struct.pack(e + 'I', head) + bytes(12))
        g += struct.pack(e + '9I', 7, total, 0, soff, count, 0, root_type, 0, root_obj) + bytes(4)
        g += struct.pack(e + '11I', 0, sec_at, size, size, 4, f16, f16, roff, len(self.reloc), moff, len(self.marsh))
        g += self.sec + rel + mar
        import zlib
        struct.pack_into(e + 'I', g, 40, zlib.crc32(bytes(g[32 + soff:])) & 0xFFFFFFFF)
        out = struct.pack(e + '%dI' % len(words), *words) + struct.pack(e + 'I', len(self.strings))
        for s in self.strings:
            out += struct.pack(e + 'H', len(s)) + s.encode('latin-1')
        return bytes(out + struct.pack(e + 'I', len(g)) + g)


def built(e: str, words=(10,)) -> bytes:
    """A resource of every member type."""
    b = Builder(e)
    t_val = b.type([(10, 'Value', None, 0)])
    t_short = b.type([(16, 'S', None, 0)])
    t_byte = b.type([(12, 'B', None, 0)])
    t_item = b.type([(19, 'Count', None, 0), (8, 'Name', None, 0)])
    t_hdr = b.type([(12, 'Format', None, 0), (12, 'Degree', None, 0)])
    t_curve = b.type([(1, 'CurveDataHeader_D3Constant32f', t_hdr, 0), (16, 'Padding', None, 0),
                      (10, 'Controls', None, 3)])
    t_inline = b.type([(17, 'Short', None, 2), (11, 'Byte', None, 0), (13, 'Byte2', None, 0)])
    root = [(8, 'Name', None, 0), (19, 'Count', None, 0), (10, 'Scale', None, 0), (9, 'Pose', None, 0),
            (3, 'Values', t_val, 0), (3, 'Shorts', t_short, 0), (3, 'Items', t_item, 0), (4, 'Refs', t_item, 0),
            (5, 'Curve', None, 0), (7, 'Variants', None, 0), (1, 'Inline', t_inline, 2), (16, 'U16', None, 0),
            (15, 'I16', None, 0), (12, 'U8', None, 0), (21, 'Half', None, 0), (20, 'U32', None, 0),
            (8, 'CString', None, 0), (2, 'Item', t_item, 0), (2, 'Nothing', t_item, 0), (22, 'Empty', None, 0),
            (8, 'Missing', None, 0), (14, 'U8b', None, 1), (18, 'U16b', None, 4)]
    t_root = b.type(root)
    sizes = {1: 0, 2: 4, 3: 8, 4: 8, 5: 8, 7: 12, 8: 4, 9: 68, 10: 4, 11: 1, 12: 1, 13: 1, 14: 1, 15: 2, 16: 2,
             17: 2, 18: 2, 19: 4, 20: 4, 21: 2, 22: 4}
    inline_size = 2 * 2 + 1 + 1
    offs, o = {}, 0
    for t, name, _, arr in root:
        offs[name] = o
        o += (inline_size if t == 1 else sizes[t]) * max(arr, 1)
    obj = b.alloc(o)
    P = lambda name, fmt, *v: b.put(obj + offs[name], fmt, *v)            # noqa: E731
    P('Name', 'I', b.s('Soldier'))
    P('Count', 'i', -7)
    P('Scale', 'f', 0.1)
    P('Pose', 'I16f', 3, *[i * 0.25 - 1 for i in range(16)])
    vals = b.alloc(16)
    for i, v in enumerate((1.5, -2.25, float('inf'), 1e-40)):
        b.put(vals + 4 * i, 'f', v)
    P('Values', 'I', 4)
    b.ptr(obj + offs['Values'] + 4, vals)
    sh = b.alloc(8)
    b.put(sh, '4H', 1, 65535, 300, 7)
    b.marsh.append((4, sh, 0, t_short))
    P('Shorts', 'I', 4)
    b.ptr(obj + offs['Shorts'] + 4, sh)
    items = b.alloc(16)
    for i in range(2):
        b.put(items + 8 * i, '2I', 100 + i, b.s(f'item{i}'))
    b.marsh.append((2, items, 0, t_item))
    P('Items', 'I', 2)
    b.ptr(obj + offs['Items'] + 4, items)
    refs = b.alloc(12)
    b.ptr(refs, items + 8)
    b.ptr(refs + 8, items)                                              # the middle one: no reference
    P('Refs', 'I', 3)
    b.ptr(obj + offs['Refs'] + 4, refs)
    curve = b.alloc(16)
    b.put(curve, 'BB', 1, 2)
    b.put(curve + 2, 'H', 0xBEEF)
    b.put(curve + 4, '3f', 1.0, 2.0, 3.0)
    b.marsh.append((1, curve, 0, t_curve))
    b.ptr(obj + offs['Curve'], t_curve)
    b.ptr(obj + offs['Curve'] + 4, curve)
    var = b.alloc(4)
    b.put(var, 'H', 0x3C00)
    b.put(var + 2, 'H', 0xFFFF)
    b.marsh.append((2, var, 0, t_short))
    b.ptr(obj + offs['Variants'], t_short)
    P('Variants', 'II', 0, 2)
    b.ptr(obj + offs['Variants'] + 8, var)
    P('Inline', '2hbb2hbb', -1, 2, -3, 4, 5, -6, 7, 8)
    P('U16', 'H', 40000)
    P('I16', 'h', -2)
    P('U8', 'B', 200)
    P('Half', 'H', 0xC100)
    P('U32', 'I', 0xFFFFFFFE)
    cs = b.alloc(8)
    b.sec[cs:cs + 6] = b'\xe9t\xe9\x00ab'
    b.marsh.append((8, cs, 0, t_byte))
    b.ptr(obj + offs['CString'], cs)
    b.ptr(obj + offs['Item'], items + 8)
    P('Missing', 'I', 999)
    P('U8b', 'B', 9)
    P('U16b', '4H', 1, 2, 3, 4)
    b.marsh.append((1, obj, 0, t_root))
    return b.resource(t_root, obj, words)


def quick(check) -> None:
    anim = (10, 2, 40, 1, 0, 0x3F800000, 7, 1, 0, 0x3F000000)            # an animation's table: bones 40 and 7
    files = {'little': built('<'), 'big': built('>'), 'big, an animation table': built('>', anim)}
    rec = Recorded('quick')
    for name, b in files.items():
        d = []
        for bones in (None, {40: 3, 7: 1}, {40: 3}):
            d += check_one(b, rec, f'built {name}, bones {bones}', bones)
        check(not d, f'built resource ({name}): the same tables, tree and errors both ways', '; '.join(d))
    g = granny.Granny(files['little'])
    check(granny.backend() == 'rust' and g._raw is not None, 'the Rust core reads the built resource')
    pc = granny.to_pc(files['big'])
    check(pc == files['little'], 'to_pc of the big-endian build is the little-endian build, byte for byte')

    rng = random.Random(1943)
    n = differ = skipped = 0
    t0 = time.perf_counter()
    for k in range(2500):
        src = files[rng.choice(list(files))]
        m = bytearray(src)
        how = rng.randrange(3)
        if how == 0:
            m = m[:rng.randrange(len(m))]
        else:
            for _ in range(1 + how * rng.randrange(3)):
                m[rng.randrange(len(m))] = rng.randrange(256)
        b = bytes(m)
        if not safe(b):
            skipped += 1
            continue
        d = check_one(b, rec, f'damaged {k}', {40: 3, 7: 1} if k % 2 else None)
        n += 1
        if d:
            differ += 1
            if differ <= 5:
                print(f'  case {k}: {"; ".join(d)}')
    check(differ == 0, f'{n} damaged copies read the same both ways ({skipped} whose counts would run the Python '
          f'reader for minutes skipped; {time.perf_counter() - t0:.1f} s)')
    rec.report(check, f'{len(files)} built resources (3 bone maps each) and {n} damaged copies')

    # the batch functions: in order, failures returned or raised as to_pc raises
    items = [(files['big'], None, 'a'), files['little'], (files['big, an animation table'], {40: 3, 7: 1})]
    got = granny.to_pc_many(items, errors='return')
    ref = [outcome(lambda it=it: granny.to_pc(*((it,) if isinstance(it, bytes) else it))) for it in items]
    check([(type(r).__name__, str(r)) if isinstance(r, Exception) else r for r in got]
          == [r if r[0] != 'ok' else r[1] for r in ref], 'to_pc_many: the results of to_pc, errors returned')
    try:
        granny.to_pc_many(items)
        check(False, 'to_pc_many: an error raised by default')
    except granny.GrannyError:
        check(True, 'to_pc_many: an error raised by default')
    rs = granny.roots([files['little'], files['big']])
    use(False)
    py = [granny.Granny(b).root() for b in (files['little'], files['big'])]
    use(True)
    check(same(rs, py), 'roots: the trees of Granny(b).root()')

    # written to: Python reads from then on, and the tables are the Python reader's
    g = granny.Granny(files['little'])
    root = g.root()
    g.put(g.field(root, 'Count'), '<i', 12345)
    use(False)
    p = granny.Granny(files['little'])
    p.put(p.field(p.root(), 'Count'), '<i', 12345)
    use(True)
    check(g._raw is None and same(g.root(), p.root()) and g.to_bytes() == p.to_bytes() and g.root()['Count'] == 12345,
          'put: the tree and bytes after a write are the Python reader\'s')


def safe(b: bytes) -> bool:
    """Whether the Python code finishes quickly on b: no count in the Granny file's header or section table larger
    than the data, no more than 256 objects in a marshalling entry (a damaged count makes it loop that many times,
    past the end; the Rust core stops at the end)."""
    for e in ('<', '>'):
        k = b.find(granny.MAGIC if e == '<' else granny.MAGIC_BE)
        if k < 0:
            continue
        lim = 4 * len(b)
        try:
            if struct.unpack_from(e + 'I', b, k + 16)[0] > lim:
                return False
            soff, count = struct.unpack_from(e + '5I', b, k + 32)[3:5]
            if count > lim:
                return False
            for s in range(min(count, 64)):
                row = struct.unpack_from(e + '11I', b, k + 32 + soff + 44 * s)
                if any(x > lim for x in row):
                    return False
                for m in range(min(row[10], 64)):
                    if struct.unpack_from(e + 'I', b, k + row[9] + 16 * m)[0] > 256:     # objects walked one by one
                        return False
        except struct.error:
            pass
    # a type's array size is walked element by element too (to_pc's inline structs): none above 256
    use(False)
    try:
        g = granny.Granny(b)
        todo, seen = [g.root_type], set()
        for k in range(len(g.sections)):
            todo += [v for (sec, _), v in g.reloc.items() if sec == k]
        while todo:
            r = todo.pop()
            if r in seen or r is None:
                continue
            seen.add(r)
            try:
                ms = g.members(r)
            except Exception:                                # noqa: BLE001 - not a type
                continue
            if any(arr > 256 for _, _, _, arr in ms):
                return False
            todo += [c for _, _, c, _ in ms]
    except Exception:                                        # noqa: BLE001 - Python refuses it quickly
        pass
    # objects' counts are bounded by the Rust core's MAX_ITEMS: larger ones are refused there and skipped here
    use(True)
    try:
        granny._native.granny_root(b)
    except ValueError as er:
        if 'too many' in str(er) or 'too deep' in str(er):
            return False
    except Exception:                                        # noqa: BLE001
        pass
    return True


# ------------------------------------------------------------------------------------------------ game data

def opfor():
    try:
        return paths.find_opfor(walk=False, suite=SUITE)
    except paths.GameCopyNotFound:
        return None


class Tally:
    def __init__(self, group: str | None = None):
        self.files = 0
        self.diffs: list[str] = []
        self.rec = Recorded(group) if group else None
        self.t = {'read rust': 0.0, 'read python': 0.0, 'to_pc': 0.0}

    def one(self, name: str, b: bytes, key: str | None = None, bones=None) -> None:
        """name: where (in the messages); key: the recording's name of its conversion (None: read only)."""
        self.files += 1
        rs, py = self.timed(lambda: granny.Granny(b, name).root())
        if rs[0] != py[0] or not same(rs[1], py[1]):
            self.diffs.append(f'{name}: root {rs[0]} / {py[0]}')
        rs, py = both(lambda: tables(granny.Granny(b, name)))
        if rs[0] != py[0] or not same(rs[1], py[1]):
            self.diffs.append(f'{name}: tables')
        if key is not None and self.rec is not None:
            for m in ((None, bones) if bones is not None else (None,)):
                t = time.perf_counter()
                got, pc = converted(b, m, name)
                self.t['to_pc'] += time.perf_counter() - t
                self.rec(key + (' (bone map)' if m else ''), got, sha1(b))
                if pc is not None:
                    self.one(name + ' (PC)', pc)
                    self.files -= 1

    def timed(self, fn):
        use(True)
        t = time.perf_counter()
        rs = outcome(fn)
        self.t['read rust'] += time.perf_counter() - t
        use(False)
        t = time.perf_counter()
        py = outcome(fn)
        self.t['read python'] += time.perf_counter() - t
        use(True)
        return rs, py

    def report(self, check, what):
        times = ', '.join(f'{k} {v:.1f} s' for k, v in self.t.items())
        check(not self.diffs, f'{what}: {self.files} resources read the same both ways ({times})',
              '; '.join(self.diffs[:5]))
        if self.rec is not None:
            self.rec.report(check, what)


def game(check) -> None:
    root = opfor()
    if root is None:
        print('SKIP game part: no Project OPFOR BFBC2 game copy')
        return
    from bfbc2core import fbrb
    tally = Tally('xbox')
    for lv in ('mp_common', 'sp_common'):
        xdir = os.path.join(root, 'work', 'xbox', lv)
        man = os.path.join(xdir, 'fbrb.manifest')
        if not os.path.isfile(man):
            print(f'SKIP {lv}: no unpacked Xbox copy (work\\xbox\\{lv})')
            continue
        for line in open(man, encoding='utf-8', errors='replace'):
            f = line.rstrip('\n').split('|')
            if len(f) > 1 and f[1] in KINDS:
                p = os.path.join(xdir, *f[0].split('/'))
                if os.path.isfile(p):
                    tally.one(f'xbox {lv}/{f[0]}', open(p, 'rb').read(), f'{lv}/{f[0]}')
    tally.report(check, 'your Xbox mp_common / sp_common (each converted, the PC result read too)')

    tally = Tally()
    seen = set()
    for bundle in sorted(glob.glob(os.path.join(root, 'game', 'Dist', 'win32', '**', '*.fbrb'), recursive=True)):
        try:
            want = [e.name for e in fbrb.entries(bundle) if e.type in KINDS and e.name.lower() not in seen]
        except (ValueError, OSError):
            continue
        if not want:
            continue
        for n, b in fbrb.read(bundle, want).items():
            seen.add(n.lower())
            tally.one(f'pc {n}', b)
    tally.report(check, 'the game copy\'s PC Granny resources')

    bf1943(check, root)


def bf1943(check, root) -> None:
    xenon = paths.sibling(paths.BF1943_WORKSPACE, 'work', 'extracted', 'dist', 'xenon', env='BF1943_XENON', suite=SUITE)
    fb = paths.sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', suite=SUITE)
    bundles = sorted(glob.glob(os.path.join(xenon, '**', '*.fbrb'), recursive=True))
    if not bundles or not os.path.isfile(os.path.join(fb, 'fbrb1943.py')):
        print('SKIP Battlefield 1943: no extracted copy (Rising Sun workspace) or no BF1943 bundle reader')
        return
    paths.use_folder(fb)
    suite_paths.use('shader-tools', 'mesh-viewer')
    import fbrb1943
    import meshview
    import shaderdb
    ix = meshview.Index(shaderdb.find_opfor(root))
    common = fbrb1943.Bundle(open(os.path.join(xenon, 'levels', 'levels-common-00.fbrb'), 'rb').read())
    src = common.data('Characters/Skeletons/ske01.res')
    bones = granny.bone_map(src, ix.read('characters/skeletons/ske01.res'))
    names = {b['Name'] for b in granny.Granny(src).root()['Skeletons'][0]['Bones']}
    del common
    tally = Tally('bf1943')
    seen = set()
    for path in bundles:
        try:
            bd = fbrb1943.Bundle(open(path, 'rb').read())
        except ValueError:
            continue
        for e in bd.entries:
            if e['type'] not in KINDS or e['name'] in seen:
                continue
            seen.add(e['name'])
            b = bd.payload[e['offset']:e['offset'] + e['size']]
            soldier = None
            if e['type'] != 'GrannyModel':
                try:
                    g = granny.Granny(b)
                    tr = {t['Name'] for a in granny.animations(g) for tg in a['TrackGroups'] for t in tg['TransformTracks']}
                    soldier = bones if tr and tr <= names else None
                except granny.GrannyError:
                    pass
            tally.one(f'bf1943 {e["name"]}', b, e['name'], soldier)
        del bd
    tally.report(check, f'Battlefield 1943 ({len(bundles)} bundles; soldier animations also converted with the bone map)')


def main(argv) -> int:
    granny.require()
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    GOLD.update(load())
    RECORD[0] = '--record' in argv
    quick(check)
    if '--game' in argv:
        game(check)
    if RECORD[0] and not fails:
        GOLD['now_recorded'] = time.strftime('%Y-%m-%d')
        write(GOLD)
        print(f'wrote {GOLDEN}')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
