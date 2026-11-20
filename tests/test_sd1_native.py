r"""Round SD1: the shader database reader's last Python in the Rust core - ShaderDb.parse's dispatch (the byte order
from the first bytes, the Dx9 then Dx10 layout, the messages), pick_database's choice of a bundle's database and
dx_from_name - against the Python they came from (kept as *_reference) and against the reference's answers recorded
in tests\sd1_golden_*.json, recorded from a frozen copy of the round's starting commit (102165d), whose functions
were the Python's dispatch over the E1c / E4b reader.

  parse   ShaderDb.parse(data, dx, where, big_endian): every database read with its kind, with none, with the other
          layout, and damaged copies (cut, bytes changed) with none and its kind
  pick    pick_database(bundle, dx, entry) on every bundle that holds a database: each Dx, no entry, each database's
          name, its folder, other case, a name that is not there
  load    load(spec) and load_levels(levels, (9, 10, 11)): the game copy's levels, bundles and files as the suite and
          OPFOR give them (level:<name>[:<entry>], <bundle>[:<entry>], a .res file), the errors of each
  names   dx_from_name on every database's name and path, and made-up names

A database's answer is its kind (dx, byte order, version, flag, key size, label) and a digest of every section's
items with their types; an error its type and message. Counted: how many inputs the Rust answered and how many it
handed to the Python (a real input handed back fails the test; built and damaged ones may be, each then the
reference's answer).

  python tests\test_sd1_native.py                    quick: built databases and bundles, damaged copies
  python tests\test_sd1_native.py --game             + every shader database of the game copy, OPFOR's Xbox
                                                      levels, Battlefield 1943's and OPFOR's work\ww2 / work\levels
                                                      outputs (read only)
  python tests\test_sd1_native.py --game --record    the goldens from the reference's answers
  --live                                              also run every area in a process with BFBC2_PURE_PYTHON=1:
                                                      both ways' answers must be the same
  --only=parse,pick,...                               some areas only
"""
import contextlib
import gc
import hashlib
import json
import os
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from collections import Counter
from collections.abc import Callable, Iterator
from concurrent.futures import ProcessPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in (SUITE, HERE, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'shader-tools')
import shaderdb as sd  # noqa: E402
import test_shaderdb as ts  # noqa: E402
import test_shaderdb_native as tsn  # noqa: E402
from bfbc2core import fbrb  # noqa: E402
from f0_parity import Checks, stdout_utf8  # noqa: E402

GAME = '--game' in sys.argv
RECORD = '--record' in sys.argv
LIVE = '--live' in sys.argv
EMIT = next((a.split('=', 1)[1] for a in sys.argv if a.startswith('--emit=')), '')
DUMP = next((a.split('=', 1)[1] for a in sys.argv if a.startswith('--dump=')), '')   # each reference answer, to compare runs
ONLY = next((a.split('=', 1)[1] for a in sys.argv if a.startswith('--only=')), '')
GOLDEN = os.path.join(HERE, 'sd1_golden_{}.json')
SCRATCH = os.environ.get('BFBC2_TEST_SCRATCH') or tempfile.gettempdir()
WORKERS = max(1, min(3, (os.cpu_count() or 2) - 1))
# the folder whose siblings hold Battlefield 1943's extracted copy and bundle tool (a frozen copy elsewhere: the
# checkout it was made from)
SIBLINGS_OF = os.environ.get('BFBC2_SD1_SIBLINGS_OF') or SUITE
SECTIONS = ('params', 'bindings', 'regs', 'vs', 'ps', 'programs', 'keys', 'layouts', 'entries', 'fragments',
            'parts', 'groups')


def wanted(area: str) -> bool:
    return not ONLY or area in ONLY.split(',')


# ------------------------------------------------------------------------------------------------ the answers

def _update(h: Any, v: Any) -> None:
    """One section item into the digest, its types with it."""
    t = type(v).__name__
    if isinstance(v, bytes | bytearray | memoryview):
        b = bytes(v)
        h.update(f'{t}:{len(b)}:'.encode() + b)
    elif isinstance(v, str):
        b = v.encode('utf-8', 'surrogatepass')
        h.update(f'{t}:{len(b)}:'.encode() + b)
    elif isinstance(v, int):
        h.update(f'{t}:{v};'.encode())
    elif isinstance(v, list | tuple):
        h.update(f'{t}({len(v)}:'.encode())
        if t == 'list' and v and all(type(x) is int for x in v):
            h.update(struct.pack(f'<{len(v)}q', *v))
        else:
            for x in v:
                _update(h, x)
        h.update(b')')
    elif isinstance(v, sd.Shader):
        h.update(b'Shader(')
        for k in ('guid', 'code', 'param', 'binding', 'regs', 'tail'):
            _update(h, getattr(v, k))
        h.update(b')')
    else:
        h.update(f'{t}:{v!r};'.encode())


def summary(db: Any) -> tuple[Any, ...]:
    """A database as it is compared: its kind and a digest of each section (items, order and types)."""
    if not isinstance(db, sd.ShaderDb):
        return ('not a ShaderDb', type(db).__name__)
    secs = []
    for s in SECTIONS:
        lst = getattr(db, s)
        h = hashlib.sha1()
        _update(h, lst)
        secs.append((s, type(lst).__name__, len(lst), h.hexdigest()[:20]))
    return (db.dx, db.big_endian, db.e, db.version, db.flag, db.key_size, db.label, tuple(secs))


def outcome(fn: Callable[..., Any], *args: Any, **kw: Any) -> tuple[Any, ...]:
    try:
        return ('ok', fn(*args, **kw))
    except RecursionError:
        raise
    except Exception as ex:          # noqa: BLE001 - the error itself is what is compared
        return ('error', type(ex).__name__, str(ex))


@contextlib.contextmanager
def python_only() -> Iterator[None]:
    """shaderdb's Rust off for the reference (the frozen copy has no *_reference: its use_native)."""
    sd.use_native(False)
    try:
        yield
    finally:
        sd.use_native(True)


def ref_of(name: str) -> Callable[..., Any]:
    """The module's <name>_reference, or <name> with the Rust off (the frozen copy)."""
    fn = getattr(sd, name + '_reference', None)
    if fn is not None:
        return fn
    real = getattr(sd, name)

    def ref(*a: Any, **k: Any) -> Any:
        with python_only():
            return real(*a, **k)
    return ref


def parse_ref(*a: Any, **k: Any) -> Any:
    fn = getattr(sd.ShaderDb, 'parse_reference', None)
    if fn is not None:
        return fn(*a, **k)
    with python_only():
        return sd.ShaderDb.parse(*a, **k)


def answered(fn_name: str, *a: Any, **k: Any) -> bool | None:
    """Did the Rust answer (a value or the error it raises), or hand to the Python? None: no probe (the frozen
    copy)."""
    if fn_name == 'parse':
        probe = getattr(sd.ShaderDb, '_parse_native', None)
    else:
        probe = getattr(sd, f'_{fn_name}_native', None)
    if probe is None or sd._native is None:
        return None
    try:
        return probe(*a, **k) is not None
    except Exception:                # noqa: BLE001 - an error the port raised (or the bundle read before it): answered
        return True


class Golden:
    """The reference's answers per group of inputs (a source file, or a quick set): (stamp, digest of every answer
    in order, count), and the digest of this process's own answers (for --live)."""

    def __init__(self, area: str) -> None:
        self.area = area
        self.path = GOLDEN.format(area)
        self.old: dict[str, list[Any]] = {}
        if os.path.isfile(self.path):
            with open(self.path, encoding='utf-8') as f:
                self.old = json.load(f)
        self.new: dict[str, list[Any]] = {}
        self.mine: dict[str, str] = {}
        self.cur: dict[str, Any] = {}
        self.same = self.differ = self.changed = self.missing = 0
        self.bad: list[str] = []
        self.strip: list[str] = []

    def add(self, group: str, stamp: Any, ref: Any, got: Any) -> None:
        h = self.cur.setdefault(group, [stamp, hashlib.sha1(), hashlib.sha1(), 0])
        r, a = repr(ref), repr(got)
        for s in self.strip:
            r, a = r.replace(s, '<tmp>'), a.replace(s, '<tmp>')
        if DUMP:
            with open(DUMP, 'a', encoding='utf-8', errors='surrogatepass') as f:
                f.write(f'{self.area} | {group} | {r}' + chr(10))
        h[1].update(r.encode('utf-8', 'surrogatepass'))
        h[2].update(a.encode('utf-8', 'surrogatepass'))
        h[3] += 1

    def close(self) -> None:
        for group, (stamp, ref, got, n) in self.cur.items():
            self.new[group] = [stamp, ref.hexdigest()[:20], n]
            self.mine[group] = got.hexdigest()[:20]
            want = self.old.get(group)
            if want is None:
                self.missing += 1
            elif want[0] != stamp:
                self.changed += 1
            elif want[1] == got.hexdigest()[:20] and want[2] == n:
                self.same += 1
            else:
                self.differ += 1
                if len(self.bad) < 5:
                    self.bad.append(group)
        self.cur.clear()

    def save(self) -> None:
        if not RECORD:
            return
        items = self.new if GAME else {k: v for k, v in self.old.items() if not k.startswith('quick:')} | self.new
        with open(self.path, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(dict(sorted(items.items())), f, indent=0, sort_keys=True)
            f.write('\n')

    def text(self) -> str:
        if RECORD:
            return f'{len(self.new)} groups recorded'
        return (f'goldens: {self.same} groups as recorded, {self.differ} different, {self.changed} changed since '
                f'recorded, {self.missing} not recorded')


class Tally:
    """One area's counts: same / different, answered in Rust / handed to the reference."""

    def __init__(self, golden: Golden) -> None:
        self.g = golden
        self.same = self.ndiff = self.rust = self.handed = self.handed_ok = 0
        self.real_handed: list[str] = []
        self.n_real_handed = 0
        self.differ: list[str] = []

    def add(self, label: str, group: str, stamp: Any, got: Any, ref: Any, rust: bool | None, real: bool) -> None:
        if rust is not None:
            self.rust += rust
            self.handed += not rust
            self.handed_ok += (not rust) and ref[0] == 'ok'
            if real and not rust:
                self.n_real_handed += 1
                if len(self.real_handed) < 10:
                    self.real_handed.append(label)
        if got == ref:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 8:
                self.differ.append(f'{label}: {str(got)[:160]} / {str(ref)[:160]}')
        self.g.add(group, stamp, ref, got)

    def both(self, label: str, group: str, stamp: Any, fn: Callable[..., Any], ref: Callable[..., Any],
             probe: str | None, *args: Any, real: bool = False, **kw: Any) -> None:
        got = outcome(fn, *args, **kw)
        r = outcome(ref, *args, **kw)
        rust = answered(probe, *args, **kw) if probe else None
        self.add(label, group, stamp, got, r, rust, real)

    def report(self, c: Checks, what: str, real: bool) -> None:
        self.g.close()
        self.g.save()
        ok = not self.ndiff and (not real or self.n_real_handed == 0) and (RECORD or not self.g.differ)
        c.check(ok, f'{what}: {self.same:,} identical, {self.ndiff} different; {self.rust:,} answered in Rust, '
                    f'{self.handed:,} handed to the reference ({self.handed_ok:,} of them answered by it); '
                    f'{self.g.text()}',
                '; '.join(self.differ + [f'handed back (real): {self.n_real_handed}: {self.real_handed}'] +
                          self.g.bad))
        EMITTED[self.g.area] = self.g.mine


EMITTED: dict[str, dict[str, str]] = {}


def damaged(b: bytes, rng: random.Random) -> bytes:
    x = bytearray(b)
    k = rng.randrange(3)
    if k in (1, 2) and x:
        for _ in range(rng.randint(1, 4)):
            x[rng.randrange(len(x))] = rng.randrange(256)
    if k in (0, 2) and x:
        del x[rng.randrange(len(x)):]
    return bytes(x)


def escaped(text: str) -> list[str]:
    """text as it shows in the answers' repr: twice escaped (a message quoting a path), once, as it is."""
    out = [text]
    for _ in range(2):
        out.append(repr(out[-1])[1:-1])
    return out[::-1]


def opfor_root() -> str | None:
    try:
        return sd.find_opfor(None)
    except Exception as ex:          # noqa: BLE001 - no OPFOR: built inputs only
        print(f'skip the game files ({ex})')
        return None


def rel(p: str, root: str | None) -> str:
    return os.path.relpath(p, root).replace('\\', '/') if root else p


# ------------------------------------------------------------------------------------------------ parse

def parse_sum(*a: Any, **k: Any) -> Any:
    return summary(sd.ShaderDb.parse(*a, **k))


def parse_ref_sum(*a: Any, **k: Any) -> Any:
    return summary(parse_ref(*a, **k))


def parse_variants(data: bytes, dx: int | None, big: bool) -> list[tuple[Any, ...]]:
    """(dx, big_endian) the database is read with: its kind, none, the other layout; an Xbox one also as PC."""
    own = 10 if dx == 11 else dx
    if big:
        return [(None, None), (9, None), (None, False)]
    other = 9 if own in (10, 11) else 10
    return [(None, None), (own, None), (other, None)]


def parse_item(item: tuple[str, str, str, int | None], tool: str, root: str) -> list[tuple[Any, ...]]:
    """(label, group, stamp, got, ref, rust) for one database of the corpus: read as given, then damaged copies."""
    if item[0] == 'bf1943':
        from bfbc2core import paths
        paths.use_folder(tool)
    label = rel(item[1], root) + (f':{item[2]}' if item[2] else '')
    try:
        data = tsn._load(item)
    except Exception as e:           # noqa: BLE001
        return [(label, label, 0, ('not loaded', str(e)), ('not loaded', str(e)), None)]
    out = []
    big = sd.ShaderDb.is_big_endian(data)
    where = label
    for dx, be in parse_variants(data, item[3], big):
        args = (data, dx, where, be)
        got = outcome(parse_sum, *args)
        ref = outcome(parse_ref_sum, *args)
        out.append((f'{label} dx={dx} big={be}', label, len(data), got, ref, answered('parse', *args)))
        gc.collect()
    rng = random.Random(f'{label}:{len(data)}')
    own = 10 if item[3] == 11 else item[3]
    for k in range(6):
        bad = damaged(data, rng)
        for dx in (None, own):
            args = (bad, dx, where, None)
            got = outcome(parse_sum, *args)
            ref = outcome(parse_ref_sum, *args)
            out.append((f'{label} damaged {k} dx={dx}', label, len(data), got, ref, answered('parse', *args)))
    return out


def built_parse_inputs() -> list[tuple[str, Any, Any, Any, Any]]:
    """(label, data, dx, where, big_endian) of the quick tier: built databases of every kind, the dispatcher's
    arguments of every kind, damaged copies."""
    out: list[tuple[str, Any, Any, Any, Any]] = []
    rng = random.Random(1)
    for label, data, dx in tsn.built():
        for d in (None, 0, 9, 10, 11, 12, dx):
            for be in (None, True, False):
                out.append((f'{label} dx={d} big={be}', data, d, 'f', be))
        for where in ('w\xe9ird', 'emoji \U0001f600', 'sur\udc80', ''):
            out.append((f'{label} where={where!r}', data, dx, where, None))
        out.append((f'{label} bytearray', bytearray(data), dx, 'f', None))
        out.append((f'{label} memoryview', memoryview(data), dx, 'f', None))
        out.append((f'{label} dx=True', data, True, 'f', None))
        out.append((f'{label} dx=9.0', data, 9.0, 'f', None))
        out.append((f'{label} dx=-1', data, -1, 'f', None))
        out.append((f'{label} dx=2**40', data, 2 ** 40, 'f', None))
        out.append((f'{label} big=1', data, dx, 'f', 1))
        for i, bad in enumerate(tsn.mutations(data, rng, 40)):
            out.append((f'{label} mutation {i}', bad, dx, 'f', None))
            if i % 3 == 0:
                out.append((f'{label} mutation {i} dx=None', bad, None, 'f', None))
    for n in range(0, 12):
        out.append((f'short {n}', bytes(n), None, 'f', None))
        out.append((f'short {n} ones', b'\x01' * n, 9, 'f', None))
    out.append(('big-endian looking, short', b'\0\0\0\x78\0', None, 'f', None))
    out.append(('empty sections, big', struct.pack('>IB', 120, 0) + bytes(48), None, 'f', None))
    out.append(('empty sections, big as PC', struct.pack('>IB', 120, 0) + bytes(48), None, 'f', False))
    out.append(('empty sections, PC as big', struct.pack('<IB', 120, 0) + bytes(48), 9, 'f', True))
    return out


def area_parse(c: Checks, root: str | None) -> None:
    g = Golden('parse')
    t = Tally(g)
    for label, data, dx, where, be in built_parse_inputs():
        t.both(label, 'quick:built', 0, parse_sum, parse_ref_sum, 'parse', data, dx, where, be)
    t.report(c, 'parse, built and damaged databases, every kind of argument', False)
    if not (GAME and root):
        return
    g = Golden('parse_game')
    t = Tally(g)
    items, tool, skipped = tsn.corpus(root, SIBLINGS_OF)
    if skipped:
        print(f'skipped: {skipped}')
    kinds: Counter[str] = Counter()
    t0 = time.perf_counter()
    with ProcessPoolExecutor(WORKERS) as pool:
        for item, rows in zip(items, pool.map(parse_item, items, [tool] * len(items), [root] * len(items),
                                              chunksize=1)):
            kinds[item[0]] += 1
            for label, group, stamp, got, ref, rust in rows:
                # a real database read as itself must be answered in Rust; damaged copies may be handed back
                t.add(label, group, stamp, got, ref, rust, 'damaged' not in label and ref[0] == 'ok')
    t.report(c, f'parse, {len(items)} databases ({", ".join(f"{n} {k}" for k, n in kinds.items())}) read three '
                f'ways and 6 damaged copies each two ways ({time.perf_counter() - t0:.0f} s)', True)


# ------------------------------------------------------------------------------------------------ pick

def built_bundles(folder: str) -> list[str]:
    """Bundles holding databases in every arrangement pick_database chooses among."""
    db = {dx: ts.build(dx, [('Objects/A/A_Mesh/Objects/A/Shaders/Metal', 1)], dx).to_bytes() for dx in (9, 10, 11)}
    layouts = {
        'mp_one': [('levels/mp_one/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9]),
                   ('levels/mp_one/shaders/dx10_single.res', 'Dx10ShaderDatabase', db[10]),
                   ('levels/mp_one/shaders/dx11_single.res', 'Dx11ShaderDatabase', db[11])],
        'mp_two': [('levels/mp_two/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9]),
                   ('levels/other/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9]),
                   ('levels/mp_two_b/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9])],
        'MP_Case': [('Levels/MP_CASE/Shaders/Dx9_Single.res', 'DX9SHADERDATABASE', db[9]),
                    ('Levels/Else/Shaders/Dx9_Single.res', 'dx9shaderdatabase', db[9])],
        'mp_none': [('levels/mp_none/thing.dbx', 'DBX', b'x'), ('levels/mp_none/shaders/dx9_single.res', 'Other', b'')],
        'mp_twin': [('levels/a/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9]),
                    ('levels/a/shaders/dx9_single.res/b', 'Dx9ShaderDatabase', db[9])],
        'caf\xe9': [('levels/caf/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9]),
                    ('levels/x/shaders/dx9_single.res', 'Dx9ShaderDatabase', db[9])],
    }
    out = []
    for lv, items in layouts.items():
        d = os.path.join(folder, lv)
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, 'level-00.fbrb')
        with open(p, 'wb') as f:
            f.write(fbrb.build(items))
        out.append(p)
    return out


def entries_for(path: str) -> list[Any]:
    names = sorted({n for n, _ in sd.bundle_databases(path)})
    out: list[Any] = [None, '', 'nope', 'levels', 'levels/']
    for n in names:
        folder = n.rsplit('/', 2)[0]
        out += [n, n.upper(), folder, folder + '/', folder.upper() + '//', n + '/']
    return out


def pick_sum(*a: Any) -> Any:
    return sd.pick_database(*a)


def area_pick(c: Checks, root: str | None) -> None:
    g = Golden('pick')
    t = Tally(g)
    ref = ref_of('pick_database')
    folder = tempfile.mkdtemp(prefix='sd1_pick_', dir=SCRATCH)
    g.strip += escaped(folder)
    try:
        for p in built_bundles(folder):
            lv = os.path.basename(os.path.dirname(p))
            for dx in (9, 10, 11, 12):
                for e in entries_for(p) + ['LEVELS/CAF\xc9', 'İ', 'levels/\U0001f600']:
                    t.both(f'{lv} dx={dx} entry={e!r}', f'quick:{lv}', 0, pick_sum, ref, 'pick', p, dx, e)
        missing = os.path.join(folder, 'missing', 'level-00.fbrb')
        t.both('missing bundle', 'quick:missing', 0, pick_sum, ref, 'pick', missing, 9, None)
    finally:
        shutil.rmtree(folder, ignore_errors=True)
    t.report(c, 'pick_database, built bundles', False)
    if not (GAME and root):
        return
    g = Golden('pick_game')
    t = Tally(g)
    items, _, _ = tsn.corpus(root, SIBLINGS_OF)
    bundles = sorted({p for k, p, _, _ in items if k == 'bundle'})
    t0 = time.perf_counter()
    n = 0
    for p in bundles:
        label = rel(p, root)
        for dx in (9, 10, 11):
            for e in entries_for(p):
                t.both(f'{label} dx={dx} entry={e!r}', label, os.path.getsize(p), pick_sum, ref, 'pick', p, dx, e,
                       real=True)
                n += 1
    t.report(c, f'pick_database, {len(bundles)} bundles, {n:,} choices ({time.perf_counter() - t0:.0f} s)', True)


# ------------------------------------------------------------------------------------------------ load

def load_sum(spec: str, dx: Any = None, opfor: Any = None) -> Any:
    db, data = sd.load(spec, dx, opfor)
    return summary(db), len(data), hashlib.sha1(data).hexdigest()[:16]


def levels_sum(levels: list[str], dxs: Any, opfor: Any = None) -> Any:
    out = []
    for lv, dbs in sd.load_levels(levels, dxs, opfor):
        out.append((lv, tuple((dx, summary(v) if isinstance(v, sd.ShaderDb) else ('error', type(v).__name__, str(v)))
                              for dx, v in dbs.items())))
        gc.collect()
    return out


def levels_ref(levels: list[str], dxs: Any, opfor: Any = None) -> Any:
    with python_only():
        return levels_sum(levels, dxs, opfor)


def load_ref(spec: str, dx: Any = None, opfor: Any = None) -> Any:
    with python_only():
        return load_sum(spec, dx, opfor)


INNER: Counter[str] = Counter()


def counted(fn: Callable[..., Any]) -> Callable[..., Any]:
    """fn with load's inner calls counted: each ShaderDb.parse and pick_database the Rust answered or handed back
    (nothing counted in the frozen copy: it has no such functions)."""
    def run(*a: Any, **k: Any) -> Any:
        saved = []
        for owner, name, what in ((sd.ShaderDb, '_parse_native', 'parse'), (sd, '_pick_native', 'pick')):
            real = owner.__dict__.get(name)
            if real is None:
                continue
            inner = real.__func__ if isinstance(real, classmethod) else real

            def probe(*pa: Any, _inner: Any = inner, _what: str = what, **pk: Any) -> Any:
                try:
                    got = _inner(*pa, **pk)
                except Exception:
                    INNER[f'{_what} rust'] += 1
                    raise
                INNER[f'{_what} {"rust" if got is not None else "handed"}'] += 1
                return got
            setattr(owner, name, classmethod(probe) if isinstance(real, classmethod) else probe)
            saved.append((owner, name, real))
        try:
            return fn(*a, **k)
        finally:
            for owner, name, real in saved:
                setattr(owner, name, real)
    return run


def inner_text() -> str:
    out = ', '.join(f'{k} {v:,}' for k, v in sorted(INNER.items())) or 'no inner calls counted'
    INNER.clear()
    return f'inner calls: {out}'


def area_load(c: Checks, root: str | None) -> None:
    g = Golden('load')
    t = Tally(g)
    folder = tempfile.mkdtemp(prefix='sd1_load_', dir=SCRATCH)
    g.strip += escaped(folder)
    try:
        bundles = built_bundles(folder)
        files: list[str] = []
        for _label, data, dx in tsn.built():
            for name in (f'Dx{dx}_Single.res' if dx else 'Single.res', 'plain.res', 'DX9_named.res'):
                p = os.path.join(folder, f'{len(files)}_{name}')
                with open(p, 'wb') as f:
                    f.write(data)
                files.append(p)
        specs: list[tuple[str, Any]] = []
        for p in files:
            specs += [(p, None), (p, 9), (p, 10), (p, 11)]
        for p in bundles:
            for e in ('', ':', ':levels/mp_one', ':levels/mp_one/shaders/dx10_single.res', ':nope'):
                for dx in (None, 9, 10, 11):
                    specs.append((p + e, dx))
        specs += [(os.path.join(folder, 'missing.res'), None), (os.path.join(folder, 'missing.fbrb'), None),
                  (os.path.join(folder, 'missing.fbrb:x'), None), (folder + os.sep + 'x.FBRB', 9)]
        for spec, dx in specs:
            t.both(f'load {spec} dx={dx}', 'quick:load', 0, counted(load_sum), load_ref, None, spec, dx)
    finally:
        shutil.rmtree(folder, ignore_errors=True)
    t.report(c, f'load, built files and bundles ({inner_text()})', False)
    if not (GAME and root):
        return
    g = Golden('load_game')
    t = Tally(g)
    t0 = time.perf_counter()
    levels = sorted(sd.level_bundles(root))
    for i in range(0, len(levels), 8):
        chunk = levels[i:i + 8] + (['no_such_level'] if i == 0 else [])
        t.both(f'load_levels {chunk[0]}..', f'levels:{chunk[0]}', len(chunk), counted(levels_sum), levels_ref, None, chunk,
               (9, 10, 11), real=True)
    for lv in levels[::6]:
        for dx in (None, 9, 10, 11):
            t.both(f'load level:{lv} dx={dx}', f'level:{lv}', 0, counted(load_sum), load_ref, None, f'level:{lv}', dx,
                   real=True)
        t.both(f'load level:{lv}:levels/{lv}', f'level:{lv}', 0, counted(load_sum), load_ref, None,
               f'LEVEL:{lv.upper()}:levels/{lv}', 10, real=True)
    t.both('load level:nope', 'level:nope', 0, counted(load_sum), load_ref, None, 'level:nope', None)
    items, _, _ = tsn.corpus(root, SIBLINGS_OF)
    xbox = sorted({p for k, p, _, _ in items if k == 'bundle' and os.sep + 'work' + os.sep in p})[::4]
    for p in xbox:
        t.both(f'load {rel(p, root)}', rel(p, root), os.path.getsize(p), counted(load_sum), load_ref, None, p, None,
               real=True)
    work = [p for k, p, _, _ in items if k == 'file'][::10]
    for p in work:
        t.both(f'load {rel(p, root)}', rel(p, root), os.path.getsize(p), counted(load_sum), load_ref, None, p, None,
               real=True)
    t.report(c, f'load / load_levels, {len(levels)} levels x Dx9 / 10 / 11, level / bundle / file specs ({inner_text()}) '
                f'({time.perf_counter() - t0:.0f} s)', True)


# ------------------------------------------------------------------------------------------------ names

def area_names(c: Checks, root: str | None) -> None:
    g = Golden('names')
    t = Tally(g)
    ref = ref_of('dx_from_name')
    names = ['', 'dx9', 'DX10', 'dX11', 'dx1', 'dx12', 'Dx9_Single.res', 'a/b/dx10/x.res', 'a\\dx11\\b.res',
             'x_dx110.res', 'ddxx9', 'dx99dx10', 'dxİ', 'caf\xe9_dx9', 'sur\udc80dx10', 'C:dx9', 'dx9/',
             'levels/mp_005/shaders/dx11_single.res', '\U0001f600Dx10']
    for n in names:
        t.both(f'{n!r}', 'quick:names', 0, sd.dx_from_name, ref, 'dx_from_name', n)
    t.report(c, 'dx_from_name, made-up names', False)
    if not (GAME and root):
        return
    g = Golden('names_game')
    t = Tally(g)
    items, _, _ = tsn.corpus(root, SIBLINGS_OF)
    seen = sorted({n for _, p, e, _ in items for n in (p, e) if n})
    for n in seen:
        t.both(rel(n, root), 'game:names', len(seen), sd.dx_from_name, ref, 'dx_from_name', n, real=True)
    t.report(c, f'dx_from_name, {len(seen):,} database names and paths', True)


# ------------------------------------------------------------------------------------------------ main

def live(c: Checks) -> None:
    """The same areas in a process with BFBC2_PURE_PYTHON=1: its answers must be this process's."""
    out = os.path.join(SCRATCH, f'sd1_live_{os.getpid()}.json')
    args = [a for a in sys.argv[1:] if a not in ('--live', '--record') and not a.startswith('--emit=')]
    env = dict(os.environ, BFBC2_PURE_PYTHON='1')
    t0 = time.perf_counter()
    r = subprocess.run([sys.executable, os.path.abspath(__file__), *args, f'--emit={out}'], env=env,
                       capture_output=True, text=True, encoding='utf-8', errors='replace')
    try:
        with open(out, encoding='utf-8') as f:
            theirs = json.load(f)
    except OSError:
        c.check(False, '--live: the pure-Python run', r.stdout[-2000:] + r.stderr[-2000:])
        return
    finally:
        with contextlib.suppress(OSError):
            os.remove(out)
    groups = same = 0
    bad: list[str] = []
    for area, mine in EMITTED.items():
        for grp, d in mine.items():
            groups += 1
            if theirs.get(area, {}).get(grp) == d:
                same += 1
            elif len(bad) < 5:
                bad.append(f'{area}:{grp}')
    c.check(groups > 0 and same == groups and sorted(theirs) == sorted(EMITTED),
            f'--live: BFBC2_PURE_PYTHON=1 against the Rust, {same} of {groups} groups the same '
            f'({time.perf_counter() - t0:.0f} s for the Python run)', '; '.join(bad))


def main() -> int:
    stdout_utf8()
    c = Checks()
    print(f'shaderdb backend: {sd.backend()}', flush=True)
    root = opfor_root() if GAME else None
    for name, fn in (('names', area_names), ('pick', area_pick), ('load', area_load), ('parse', area_parse)):
        if wanted(name):
            t0 = time.perf_counter()
            fn(c, root)
            print(f'-- {name}: {time.perf_counter() - t0:.1f} s', flush=True)
            gc.collect()
    if EMIT:
        with open(EMIT, 'w', encoding='utf-8') as f:
            json.dump(EMITTED, f)
    if LIVE:
        live(c)
    return c.done()


if __name__ == '__main__':
    sys.exit(main())
