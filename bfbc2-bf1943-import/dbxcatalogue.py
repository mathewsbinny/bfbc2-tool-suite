r"""dbxcatalogue.py - what BFBC2's own partitions say about its data schema: the ground truth dbxport checks a port
against.

One pass over every BFBC2 partition of the game copy (meshview.Index: the first stock bundle holding each name) gives:

  classes      every instance type BFBC2's partitions hold (with the executable's type names: whether BFBC2 has a
               class at all - dbxport's drop_untemplated drops only classes it lacks)
  order        each type's field names, in DICE's order
  shapes       per (type, field path) the shapes DICE writes: 'complex-container', 'array-container', 'array-text'
               (an empty array: DICE writes all of them as TEXT), 'ref', 'field-text', 'num4x1', 'num4x4', 'k6' (a
               bool), ... - a path is '.Field', '.Struct.Field', '.Array[]' for an array's items
  texts        per TEXT field path the values DICE uses (enum names: an enum field's valid names)
  numeric      per 4-byte number path whether DICE's values read as whole numbers or as floats (how a bool is
               written into it: no float bits into an integer)
  protos       per array path an item DICE writes (the item prototype of an array a template has empty)
  guids        guid -> the partition(s) holding an instance of it (references into partitions DICE moved: the
               guid is still there, under another name - BF1943's beach/defaulthaloaimer is common/defaulthaloaimer)
  primary      partition -> its primaryInstance
  defaults     per type a DICE instance of it (partition, element): the class default dbxport aligns on when no
               template of the caller's pool holds the type (the last step of its ranked template choice); others:
               a second one from another partition (a fair test leaves a partition's own instances out)
  exe_names    every identifier in BFBC2Game.exe's strings (class and enum names: an enum value BFBC2 can read)

Built once and cached beside meshview's index (work\tools_out\dbx_catalogue.pkl), rebuilt when a bundle or the
executable changes.

    cat = dbxcatalogue.load(ix)                     # meshview.Index
    cat.has_class('GameSharedResources.CarHornComponentData')       # False
    cat.where('46021992-...')                       # ('objects/soldiers/common/defaulthaloaimer',)
"""
import collections
import hashlib
import os
import pickle
import re
import sys
import time
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx  # noqa: E402

CACHE_VERSION = 3
PER_TYPE = 200                      # instances of a type walked for shapes / texts (all of them for the guid index)
TEXTS_CAP = 400                     # distinct text values kept per path


def shape(e: dbx.Element) -> str:
    """An element's shape as the catalogue records it."""
    if e.kind == dbx.CONTAINER:
        return f'{e.tag}-container'
    if e.attr('ref') is not None:
        return 'ref'
    if e.kind == dbx.TEXT:
        return f'{e.tag}-text'
    if e.kind == dbx.NUMBERS:
        size, vals = dbx.numbers(e)
        return f'num{size}x{len(vals)}'
    return f'k{e.kind}'


def looks_whole(raw: bytes) -> bool | None:
    """Whether a nonzero 4-byte value reads as a whole number rather than a float's bits (None: 0, either)."""
    v = int.from_bytes(raw, 'big', signed=True)
    if v == 0:
        return None
    return (v >> 24) in (0, -1)                 # a float's exponent puts bits in the top byte (1.0: 0x3F800000)


def exe_identifiers(path: str) -> frozenset[str]:
    """The identifiers among the executable's NUL-terminated strings (class, field and enum names)."""
    d = open(path, 'rb').read()
    return frozenset(m.group(1).decode() for m in re.finditer(rb'\x00([A-Za-z_][A-Za-z0-9_]{1,80})(?=\x00)', d))


class Catalogue:
    """See the module docstring. Also built by hand (tests): Catalogue() and fill the tables."""

    def __init__(self) -> None:
        self.classes: set[str] = set()
        self.order: dict[str, list[str]] = {}
        self.shapes: dict[tuple[str, str], collections.Counter[str]] = collections.defaultdict(collections.Counter)
        self.texts: dict[tuple[str, str], collections.Counter[str]] = collections.defaultdict(collections.Counter)
        self.numeric: dict[tuple[str, str], collections.Counter[str]] = collections.defaultdict(collections.Counter)
        self.protos: dict[tuple[str, str], dbx.Element] = {}
        self.guids: dict[str, Any] = {}             # guid -> partition, or a tuple of partitions
        self.primary: dict[str, str] = {}
        self.defaults: dict[str, tuple[str, dbx.Element]] = {}
        self.others: dict[str, tuple[str, dbx.Element]] = {}     # a second one, from another partition
        self.exe_names: frozenset[str] = frozenset()
        self.partitions = 0
        self.problems: list[str] = []               # bundles / partitions that could not be read: an incomplete one

    # ---- questions
    def has_class(self, t: str) -> bool:
        """Whether BFBC2 has the class: an instance of it in its partitions, or its name in the executable."""
        return t in self.classes or (bool(self.exe_names) and t.rsplit('.', 1)[-1] in self.exe_names)

    def where(self, guid: str) -> tuple[str, ...]:
        """The BFBC2 partitions holding an instance of this guid."""
        w = self.guids.get(guid.lower())
        return () if w is None else (w,) if isinstance(w, str) else tuple(w)

    def exists(self, partition: str, guid: str) -> bool:
        return partition.lower() in self.where(guid)

    def has_partition(self, partition: str) -> bool:
        return partition.lower() in self.primary

    def enum_names(self, t: str, path: str) -> set[str] | None:
        """The names DICE uses in an enum field (a TEXT field whose every value is an identifier of the executable),
        None when the field is not one."""
        vals = self.texts.get((t, path))
        if not vals or not self.exe_names:
            return None
        names = [v for v in vals if v]
        if not names or not all(re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*', v) and v in self.exe_names for v in names):
            return None
        if names[0] in ('true', 'false'):
            return None
        return set(names)

    def valid_enum(self, value: str) -> bool:
        return not self.exe_names or value in self.exe_names

    def is_whole(self, t: str, path: str) -> bool | None:
        c = self.numeric.get((t, path))
        if not c:
            return None
        if c.get('whole') and not c.get('float'):
            return True
        if c.get('float') and not c.get('whole'):
            return False
        return None

    # ---- building
    def add_partition(self, name: str, f: dbx.DbxFile, seen: collections.Counter[str]) -> None:
        self.partitions += 1
        name = name.lower()
        prim = f.partition.attr('primaryInstance')
        if prim:
            self.primary[name] = prim.lower()
        for inst in f.instances:
            g = (inst.attr('guid') or '').lower()
            w = self.guids.get(g)
            if w is None:
                self.guids[g] = name
            elif isinstance(w, str):
                if w != name:
                    self.guids[g] = (w, name)
            elif name not in w:
                self.guids[g] = w + (name,)
            t = inst.attr('type') or ''
            self.classes.add(t)
            if seen[t] >= PER_TYPE:
                continue
            seen[t] += 1
            if t not in self.order:
                self.order[t] = [c.attr('name') or '' for c in inst.children or ()]
                self.defaults[t] = (name, inst)
            elif t not in self.others and self.defaults[t][0] != name:
                self.others[t] = (name, inst)
            self._walk(inst, t, '')

    def _walk(self, e: dbx.Element, t: str, path: str) -> None:
        for c in e.children or ():
            n = c.attr('name')
            p = f'{path}.{n}' if n is not None else f'{path}[]'
            s = shape(c)
            self.shapes[(t, p)][s] += 1
            if s in ('field-text', 'complex-text') and len(self.texts[(t, p)]) < TEXTS_CAP:
                self.texts[(t, p)][c.text] += 1
            elif s.startswith('num4x'):
                for v in dbx.numbers(c)[1]:
                    w = looks_whole(v)
                    if w is not None:
                        self.numeric[(t, p)]['whole' if w else 'float'] += 1
            if c.tag == 'array' and c.kind == dbx.CONTAINER and c.children and (t, p + '[]') not in self.protos:
                self.protos[(t, p + '[]')] = c.children[0]
            if c.kind == dbx.CONTAINER:
                self._walk(c, t, p)

    def freeze(self) -> None:
        self.shapes = dict(self.shapes)
        self.texts = dict(self.texts)
        self.numeric = dict(self.numeric)


def _signature(ix: Any, exe: str) -> str:
    bundles = sorted({b for b, _ in ix.where.values()})
    h = hashlib.sha1()
    for b in bundles + [exe]:
        try:
            h.update(f'{b}|{os.path.getsize(b)}|{int(os.path.getmtime(b))}\n'.encode())
        except OSError:
            h.update(f'{b}|missing\n'.encode())
    return h.hexdigest()


def build(ix: Any, exe: str | None, log: Any = None) -> Catalogue:
    """A Catalogue over every BFBC2 partition the index names (stock bundles; FbrbTool's build overlays left out)."""
    from bfbc2core import fbrb
    cat = Catalogue()
    if exe and os.path.isfile(exe):
        cat.exe_names = exe_identifiers(exe)
    names: dict[str, list[str]] = collections.defaultdict(list)
    for k, (b, n) in ix.where.items():
        if k.endswith('.dbx') and 'build_overlay' not in b.lower():
            names[b].append(n)
    seen: collections.Counter[str] = collections.Counter()
    t0 = time.time()
    for i, (b, ns) in enumerate(sorted(names.items())):
        for j in range(0, len(ns), 400):
            try:
                got = fbrb.read(b, ns[j:j + 400])
            except Exception as ex:                             # noqa: BLE001 - a locked or damaged bundle: recorded
                cat.problems.append(f'{b}: {len(ns[j:j + 400])} partitions not read ({type(ex).__name__}: {ex})')
                continue
            for n, d in sorted(got.items()):
                try:
                    f = dbx.parse(d)
                except dbx.DeletedEntry:                        # a *deleted* marker: nothing to read
                    continue
                except Exception as ex:                         # noqa: BLE001 - a broken file: recorded
                    cat.problems.append(f'{b}: {n} not parsed ({type(ex).__name__}: {ex})')
                    continue
                cat.add_partition(n[:-4] if n.lower().endswith('.dbx') else n, f, seen)
        if log and i % 50 == 0:
            log(f'  catalogue: {i + 1}/{len(names)} bundles, {cat.partitions} partitions, {time.time() - t0:.0f} s')
    cat.freeze()
    return cat


def load(ix: Any, exe: str | None = None, log: Any = None) -> Catalogue:
    """The cached Catalogue of the index's game copy (built on first use: a few minutes)."""
    root = getattr(ix, 'root', None)
    if exe is None and root:
        exe = os.path.join(root, 'game', 'BFBC2Game.exe')
    sig = _signature(ix, exe or '')
    path = os.path.join(root, 'work', 'tools_out', 'dbx_catalogue.pkl') if root else None
    if path:
        try:
            with open(path, 'rb') as fh:
                c = pickle.load(fh)
            if c['version'] == CACHE_VERSION and c['sig'] == sig:
                cat: Catalogue = c['catalogue']
                return cat
        except Exception:                                       # noqa: BLE001 - missing, old or corrupt: rebuilt
            pass
    print('cataloguing BFBC2\'s partitions (once; cached) ...', file=sys.stderr)
    say = log or (lambda m: print(m, file=sys.stderr))
    cat = build(ix, exe, say)
    if cat.problems:
        # an incomplete catalogue (a bundle locked by the running game, say) is used but not cached: a cache keyed
        # on the bundles' size and time would keep it after the cause is gone
        say(f'catalogue: {len(cat.problems)} read problem(s) - this catalogue is incomplete and NOT cached:')
        for p in cat.problems[:20]:
            say(f'  {p}')
        if len(cat.problems) > 20:
            say(f'  ... {len(cat.problems) - 20} more')
        return cat
    if path:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = path + '.tmp'
        with open(tmp, 'wb') as fh:
            pickle.dump({'version': CACHE_VERSION, 'sig': sig, 'catalogue': cat}, fh, protocol=pickle.HIGHEST_PROTOCOL)
        os.replace(tmp, path)
    return cat


if __name__ == '__main__':
    suite_paths.use('mesh-viewer', 'shader-tools')
    import dbxcatalogue  # the cache pickles dbxcatalogue.Catalogue, not __main__'s
    import meshview
    import shaderdb
    t = time.time()
    c = dbxcatalogue.load(meshview.Index(shaderdb.find_opfor(None)))
    print(f'{c.partitions} partitions, {len(c.classes)} classes, {len(c.guids)} guids, {len(c.shapes)} paths, '
          f'{len(c.exe_names)} executable names ({time.time() - t:.0f} s)')
