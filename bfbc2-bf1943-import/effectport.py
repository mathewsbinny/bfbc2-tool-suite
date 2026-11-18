r"""effectport.py - Battlefield 1943 effects in BFBC2: an effect, the emitters it plays, the textures they draw.

    import effectport
    files, report = effectport.port(src, ix, 'effects/weapons/misc/effect_aa_detonationblast', 'levels/wake_island_s')
    # files: {partition / resource name (BF1943's spelling): PC bytes}; report: lines

What an effect is (both games): an EffectAsset partition holding an EffectEntityData whose components are
EmitterEntityData (each naming an emitter document) and SoundEntityData (a sound). BF1943 keeps the emitter documents
per level (levels/<level>/emitters/...: the partition inside keeps the global name emitters/...), each made of Emitter.*
data instances (spawn rate, size, colour curves, ...) and naming its textures (textures/effects/...).

How it is ported:
- every partition the effect reaches is walked: emitter documents are taken from the level's copy, textures are
  converted (texture.convert), sounds and anything else BFBC2 has under the same name are kept as references;
- each partition goes through dbxport.port on BFBC2 templates (TEMPLATES: BFBC2 effect and emitter partitions that
  together hold every type, chosen so the fields BFBC2 added carry DICE's usual values - checked with dbxparity.py on
  the 70 effect partitions both games have: no enum, kind, nested or renamed field; of the 26 fields BFBC2 added, the
  ones whose values vary take their usual value (SpawnProbability 1, TriggerPostEffect false, post-effect offsets 0) -
  DEFAULTS checks the templates give exactly those);
- a BF1943 sound BFBC2 lacks is not ported here (no sound templates, no wave conversion): port() reports it
  (SOUND lines, sounds_of()) for the caller's sound porter (OPFOR wake_level.port_sounds);
- a reference that will resolve to nothing is null and reported (dbxport: no template value stands in for it);
- the report is a dbxport.Report: its records hold every partition's dbxport records (sounds_of reads them).
"""
import os
import re
import sys
from typing import cast

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'console-convert', 'core')
import dbxport  # noqa: E402
from bfbc2core import dbx  # noqa: E402

REF = re.compile(r'ref="([^"]+)/[0-9a-fA-F-]{36}"')
# BFBC2 partitions whose instances serve as templates: an effect (DICE's small generic detonation) and emitters that
# together cover the Emitter.* types (found by covering the types BF1943's effects use; first holder of a type wins)
TEMPLATES = ['fx/explostions/detonations/generic/fx_detonation_s']
# the DICE-only fields whose values vary over DICE's own effects (dbxparity.py): the value the templates must give
DEFAULTS = {('EmitterEntityData', 'SpawnProbability'): '1', ('EffectEntityData', 'TriggerPostEffect'): 'false',
            ('EffectEntityData', 'PostEffectContrastOffset'): '0', ('EffectEntityData', 'PostEffectFadeInTime'): '0',
            ('EffectEntityData', 'PostEffectFadeOutTime'): '0', ('EffectEntityData', 'PostEffectSizeScale'): '1'}


def refs(data):
    return sorted({r.lower() for r in REF.findall(dbx.to_xml(data))})


def levels_of(src):
    """BF1943's level folders (levels/<name>) that hold level copies of emitter documents, sorted."""
    return sorted({'/'.join(k.split('/')[:2]) for k in src.entries if k.startswith('levels/') and k.count('/') >= 2})


def level_copy(src, name, level, fallback=True):
    """The source key of a level copy of an emitter document (BF1943 keeps them per level, under emitters/... and under
    the fx/ folders' emitter_<size>/ folders alike: fx/vehicles/exhaust/large/emitter_l/em_exhaust_smokez_l_planestart
    is in levels/<level>/fx/...): the given level's, else (fallback) the first other level's that has one - BF1943's
    Wake lacks the tree destruction's three emitters, which Guadalcanal, Iwo Jima and Coral Sea have. None: none."""
    k = f'{level}/{name}.dbx'
    if k in src.entries:
        return k
    if fallback:
        for lv in levels_of(src):
            if lv != level and f'{lv}/{name}.dbx' in src.entries:
                return f'{lv}/{name}.dbx'
    return None


def _effect_with_emitters(data):
    """Whether a partition is an effect that draws particles (an EffectAsset with an EmitterEntityData): a projectile's
    sound and tracer in one (BF1943's bsfx_we_gen75mm_projectilefx under sound/) - ported as an effect, its sounds
    handed over; a plain sound wrapper is the sound porter's."""
    return b'Entity.EffectAsset' in data and b'Emitter.EmitterEntityData' in data


def walk(src, ix, effect, level, fallback=True):
    """[(partition name, source key, kind)] the effect reaches: kind 'port' (BF1943's, BFBC2 lacks it), 'texture',
    'bfbc2' (kept as a reference), 'sound' (BF1943's, BFBC2 lacks it: not ported here - port() lists it for the
    caller's sound porter), 'missing'. An emitter document is the level's copy (level_copy: another level's when
    the level has none and fallback is set - the source key says which)."""
    out, seen, todo = [], set(), [effect.lower()]
    while todo:
        name = todo.pop(0)
        if name in seen:
            continue
        seen.add(name)
        if name.startswith('textures/'):
            key = name + '.res'
            if name + '.dbx' in ix.where or name + '.res' in ix.where:
                out.append((name, None, 'bfbc2'))              # BFBC2's own (DICE's PC textures: often larger)
            elif key in src.entries and src.entries[key][1].get('type') == 'XenonTexture':
                out.append((name, name, 'texture'))
                if name + '.dbx' in src.entries:
                    out.append((name, name + '.dbx', 'port'))
            elif name + '.dbx' in ix.where:
                out.append((name, None, 'bfbc2'))
            else:
                out.append((name, None, 'missing'))
            continue
        if name + '.dbx' in ix.where and not name.startswith(('effects/', 'emitters/')):
            out.append((name, None, 'bfbc2'))                  # sounds and the like: BFBC2's own
            continue
        if name.startswith('sound/') and name + '.dbx' in src.entries and \
                not _effect_with_emitters(src.data(name + '.dbx')):
            # a BF1943 sound BFBC2 lacks: the caller's sound porter (its sound templates, its waves converted) -
            # aligning it on effect / emitter templates would leave its Audio.* types unaligned and its waves out
            out.append((name, name + '.dbx', 'sound'))
            continue
        key = name + '.dbx'
        if key not in src.entries:
            key = level_copy(src, name, level, fallback) or key     # BF1943 keeps emitter documents per level
        if key not in src.entries:
            out.append((name, None, 'bfbc2' if name + '.dbx' in ix.where else 'missing'))
            continue
        out.append((name, key, 'port'))
        todo += [r for r in refs(src.data(key)) if r not in seen]
    return out


def _read_many(ix, names):
    """{name: bytes} of BFBC2 partitions, read a bundle at a time (one open per bundle, not per partition)."""
    from bfbc2core import fbrb
    by: dict[str, list[tuple[str, str]]] = {}
    for n in names:
        hit = ix.where.get(n.lower())
        if hit:
            by.setdefault(hit[0], []).append((n, hit[1]))
    out = {}
    for b, items in by.items():
        want = {e.lower(): n for n, e in items}
        for i in range(0, len(items), 400):
            for e, d in fbrb.read(b, [x for _, x in items[i:i + 400]]).items():
                if e.lower() in want:
                    out[want[e.lower()]] = d
    return out


_BC2: list = [None, None]               # [the index's where, its partition names]


def bc2_partitions(ix):
    """BFBC2's partition names (the index's .dbx entries without the extension), made once per index (port() used
    to make them for every effect: 26 million string tests for Wake's 157 effects)."""
    if _BC2[0] is not ix.where:
        _BC2[:] = [ix.where, {n[:-4] for n in ix.where if n.endswith('.dbx')}]
    return _BC2[1]


# templates()' search over BFBC2's emitter documents: from EmitterIndex (round E3g) - each document's instance types
# read once (all cores) and kept on disk while the game copy's bundles are unchanged; the scan the index replaced,
# templates_reference, is the reference it is proven against (BFBC2_EFFECTPORT_REFERENCE=1 or REFERENCE = True)
REFERENCE = os.environ.get('BFBC2_EFFECTPORT_REFERENCE', '') not in ('', '0')
EMITTER_BATCH = 2000
INDEX_VERSION = 1


def cache_dir():
    """Where EmitterIndex keeps its files: BFBC2_EFFECTPORT_CACHE (a folder; 0: none), else
    %LOCALAPPDATA%\\bfbc2-tool-suite\\cache\\effectport. None: no disk cache."""
    v = os.environ.get('BFBC2_EFFECTPORT_CACHE', '')
    if v == '0':
        return None
    return v or os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'cache',
                             'effectport')


class EmitterIndex:
    """Which instance types each BFBC2 emitter document holds, for templates()' greedy search: per document (by
    name) the types (dbx.parse's instance types) and those of them its bytes name (the scan's pre-test), or 'error'
    when it does not parse; a document its batch read did not return has no row. Rows are read in templates()' own
    batches (EMITTER_BATCH documents in name order), the bundles on all cores, only as far as a search goes; a full
    index is written to cache_dir(), keyed by the game copy's bundles (path, size, time) and the documents' places,
    and read back while they are unchanged. One index per meshview.Index in a process (of_index)."""

    _of: list = [None, None]

    @classmethod
    def of_index(cls, ix):
        if cls._of[0] is not ix.where:
            cls._of[:] = [ix.where, cls(ix)]
        return cls._of[1]

    def __init__(self, ix):
        self.ix = ix
        self.cands = sorted(k for k in ix.where if '/emitters/' in k and k.endswith('.dbx'))
        self.rows = {}
        self.loaded = set()
        self.stats = {'batches_read': 0, 'from_disk': False}
        self._key = None
        self._load_disk()

    def key(self):
        """The disk cache's key: the documents' places and their bundles' sizes and times."""
        if self._key is None:
            import hashlib
            h = hashlib.sha256(f'{INDEX_VERSION}\n'.encode())
            bundles = set()
            for n in self.cands:
                b, e = self.ix.where[n]
                bundles.add(b)
                h.update(f'{n}\t{b}\t{e}\n'.encode('utf-8', 'surrogatepass'))
            for b in sorted(bundles):
                try:
                    st = os.stat(b)
                    h.update(f'{os.path.normcase(os.path.abspath(b))}\t{st.st_size}\t{st.st_mtime_ns}\n'.encode())
                except OSError:
                    h.update(f'{b}\tmissing\n'.encode())
            self._key = h.hexdigest()
        return self._key

    def _path(self):
        d = cache_dir()
        return None if d is None else os.path.join(d, f'emitters-{self.key()[:24]}.pkl')

    def _load_disk(self):
        p = self._path()
        if p is None or not os.path.isfile(p):
            return
        import pickle
        try:
            with open(p, 'rb') as fh:
                c = pickle.load(fh)
            if c.get('version') != INDEX_VERSION or c.get('key') != self.key():
                return
            rows = c['rows']
        except Exception:                                       # noqa: BLE001 - damaged or foreign: read again
            return
        from bfbc2core import caches
        caches.touch(p)                                         # used now: the pruning deletes the least recently used
        self.rows = {n: (r if r == 'error' else (frozenset(r[0]), frozenset(r[1]))) for n, r in rows.items()}
        self.loaded = set(range(0, len(self.cands), EMITTER_BATCH))
        self.stats['from_disk'] = True

    def _save_disk(self):
        p = self._path()
        if p is None:
            return
        import pickle
        try:
            from bfbc2core import atomicio
            os.makedirs(os.path.dirname(p), exist_ok=True)
            rows = {n: (r if r == 'error' else (sorted(r[0], key=str), sorted(r[1], key=str))) for n, r in
                    self.rows.items()}
            atomicio.write_bytes(p, pickle.dumps({'version': INDEX_VERSION, 'key': self.key(), 'rows': rows}))
        except OSError:
            return                                              # a cache that cannot be written is only slower
        from bfbc2core import caches
        caches.maybe_prune('effectport', os.path.dirname(p))    # BFBC2_EFFECTPORT_CACHE_MB, default 256

    def batch(self, i):
        """The rows of the batch starting at document i (read now if not yet)."""
        if i in self.loaded:
            return
        names = self.cands[i:i + EMITTER_BATCH]
        data = _read_parallel(self.ix, names)
        have = [n for n in names if n in data]
        for n, r in zip(have, _instance_types([data[n] for n in have])):
            if isinstance(r, Exception) or any(not isinstance(t, str) for t in r[0]):
                self.rows[n] = 'error'                          # the reference's own test decides
                continue
            types, present = r
            self.rows[n] = (frozenset(sys.intern(t) for t in types),
                            frozenset(sys.intern(t) for t, p in zip(types, present) if p))
        self.loaded.add(i)
        self.stats['batches_read'] += 1
        if len(self.loaded) == len(range(0, len(self.cands), EMITTER_BATCH)):
            self._save_disk()


def _instance_types(datas):
    """Per file: ([each instance's type], [whether that type's bytes occur in the file]) or the ValueError - the
    Rust core's dbx_instance_types_many (all cores, no Python tree) when built, else dbx.parse_many's trees."""
    native = getattr(dbx, '_native', None)
    if native is not None and hasattr(native, 'dbx_instance_types_many') and all(isinstance(d, bytes) for d in datas):
        return native.dbx_instance_types_many(datas)
    out: list = []
    for d, f in zip(datas, dbx.parse_many(datas)):
        if isinstance(f, Exception):
            out.append(f)
            continue
        types = [x.attr('type') for x in f.instances]
        out.append((types, [isinstance(t, str) and t.encode() in d for t in types]))
    return out


def _read_parallel(ix, names):
    """_read_many on all cores (fbrb.read_many: a bundle per task), the same bytes; a bundle that cannot be read
    raises as fbrb.read does."""
    from bfbc2core import fbrb
    by: dict[str, list[tuple[str, str]]] = {}
    for n in names:
        hit = ix.where.get(n.lower())
        if hit:
            by.setdefault(hit[0], []).append((n, hit[1]))
    items = list(by.items())
    got = fbrb.read_many([(b, [e for _, e in pairs]) for b, pairs in items])
    out = {}
    for (_, pairs), r in zip(items, got):
        if isinstance(r, Exception):
            raise r
        low = {e.lower(): d for e, d in r.items()}
        for n, e in pairs:
            if e.lower() in low:
                out[n] = low[e.lower()]
    return out


def templates(ix, types, catalogue=None):
    """dbxport.Templates from TEMPLATES plus BFBC2 emitter documents covering the types (greedy, by name order): the
    first document in name order holding a type still needed (and naming one in its bytes) joins the pool, until
    every type is covered. Decided from EmitterIndex's rows; a document whose row cannot decide (it does not parse,
    or it holds a needed type its bytes do not name) is tested on its bytes as templates_reference tests it."""
    if REFERENCE:
        return templates_reference(ix, types, catalogue)
    pool = list(TEMPLATES)
    datas = {p + '.dbx': ix.read(p + '.dbx') for p in pool}
    have = set()
    for d in datas.values():
        have |= {i.attr('type') for i in dbx.parse(d).instances}
    need = set(types) - have
    if need and any(not isinstance(t, str) for t in need):
        return templates_reference(ix, types, catalogue)
    chosen = []
    if need:
        idx = EmitterIndex.of_index(ix)
        cands = idx.cands
        for i in range(0, len(cands), EMITTER_BATCH):
            if not need:
                break
            idx.batch(i)
            for n in cands[i:i + EMITTER_BATCH]:
                row = idx.rows.get(n)
                if row is None:
                    continue
                if row == 'error' or (row[0] & need and not row[1] & need):
                    b = _read_many(ix, [n]).get(n)
                    if b is None or not any(t.encode() in b for t in need):
                        continue
                    got = {x.attr('type') for x in dbx.parse(b).instances} & need
                else:
                    got = row[0] & need
                if got:
                    pool.append(n[:-4])
                    chosen.append(n)
                    need -= got
                if not need:
                    break
    if chosen:
        got_bytes = _read_many(ix, chosen)
        for n in chosen:
            datas[n] = got_bytes[n]
    return dbxport.Templates([(p, datas[p + '.dbx']) for p in pool], catalogue), pool, need


def templates_reference(ix, types, catalogue=None):
    """templates() as it was written first (the reference): the emitter documents read a bundle at a time and parsed
    only when their bytes name a type still needed (a type's name is in the partition's string table): the scan over
    BFBC2's 18,343 emitter documents was I/O-bound (35 minutes for the material grid's effects; 17 s a call since
    the batched reads)."""
    pool = list(TEMPLATES)
    datas = {p + '.dbx': ix.read(p + '.dbx') for p in pool}
    have = set()
    for d in datas.values():
        have |= {i.attr('type') for i in dbx.parse(d).instances}
    need = set(types) - have
    if need:
        cands = sorted(k for k in ix.where if '/emitters/' in k and k.endswith('.dbx'))
        for i in range(0, len(cands), 2000):
            if not need:
                break
            batch = _read_many(ix, cands[i:i + 2000])
            for n in cands[i:i + 2000]:
                b = batch.get(n)
                if b is None or not any(t.encode() in b for t in need):
                    continue
                got = {x.attr('type') for x in dbx.parse(b).instances} & need
                if got:
                    pool.append(n[:-4])
                    datas[n] = b
                    need -= got
                if not need:
                    break
    return dbxport.Templates([(p, datas[p + '.dbx']) for p in pool], catalogue), pool, need


def check_defaults(data, report):
    f = dbx.parse(data)
    for inst in f.instances:
        t = cast(str, inst.attr('type')).split('.')[-1]
        for (tt, fld), want in DEFAULTS.items():
            if t == tt and inst.child(fld) is not None:
                got = dbx.value(cast(dbx.Element, inst.child(fld)))
                if got != want:
                    line = f'DEFAULT {t}.{fld} = {got}, DICE usually {want}'
                    if isinstance(report, dbxport.Report):
                        report.records.append(dbxport.Record(f'{t}.{fld}', 'default', old=got, new=want, text=line))
                    report.append(line)


def port_many(src, ix, effects, level, log=print, catalogue=None, schema='error'):
    """{file: bytes}, report for several effects - the templates found once for all of them (each effect's own
    search scans BFBC2's emitter documents: slow for dozens)."""
    plans = {e: walk(src, ix, e, level) for e in effects}
    types: set = set()
    seen: dict = {}                          # seen: each partition's instance types, parsed once for all the effects
    for plan in plans.values():
        for _name, key, kind in plan:
            if kind == 'port':
                types |= _types_of(src, key, seen)
    shared = templates(ix, types, catalogue)
    files, report = {}, dbxport.Report()
    for n, e in enumerate(effects):
        f, r = port(src, ix, e, level, shared=shared, plan=plans[e], catalogue=catalogue, schema=schema, _seen=seen)
        files.update(f)
        report.records += r.records                            # all records; the effect's own lines as it chose them
        report.extend(r)
        log(f'  effect {n + 1}/{len(effects)} {e.rsplit("/", 1)[-1]}: {len(f)} files')
    return files, report


# the dbxport records whose lines the effect report shows (all of them are in report.records)
SHOWN = ('substituted', 'nulled', 'relocated', 'pruned', 'guarded', 'value', 'UNMAPPED', 'left out instance', 'type',
         'untemplated', 'nested', 'renamed', 'enum', 'schema')


def _shown(name):
    def line(r: dbxport.Record) -> str | None:
        return f'  {name}: {r.line()}' if r.action in SHOWN else None
    return line


def _types_of(src, key, seen):
    """The instance types of the source partition key (seen: those parsed before, by key)."""
    if key not in seen:
        seen[key] = {i.attr('type') for i in dbx.parse(src.data(key)).instances}
    return seen[key]


def port(src, ix, effect, level, shared=None, plan=None, catalogue=None, schema='error', _seen=None):
    """({file: bytes}, dbxport.Report) of one effect. The report's records: the effect's summary ('summary'), each
    partition's dbxport records (their partition set), 'sound' (a BF1943 sound for the caller's sound porter: its
    path), 'missing', 'default'; its lines as before (the dbxport lines of SHOWN, indented under the partition).
    schema: dbxport's ('report': a shape no converter handles is kept as BF1943's and reported, 'schema' records)."""
    import texture
    plan = plan or walk(src, ix, effect, level)
    if any(kind == 'texture' for _n, _k, kind in plan):
        texture.require()                   # textures convert in the Rust core: say so before porting anything
    types = set()
    for _name, key, kind in plan:
        if kind == 'port':
            types |= _types_of(src, key, {} if _seen is None else _seen)
    T, pool, missing = shared or templates(ix, types, catalogue)
    missing = set(types) - T.types()
    report = dbxport.Report()

    def say(path, action, text, **kw):
        report.records.append(dbxport.Record(path, action, text=text, **kw))
        report.append(text)
    say(effect, 'summary', f'{effect}: {sum(k == "port" for _, _, k in plan)} partitions to port, '
        f'{sum(k == "sound" for _, _, k in plan)} sounds for the sound porter, '
        f'{sum(k == "texture" for _, _, k in plan)} textures, {sum(k == "bfbc2" for _, _, k in plan)} kept as '
        f'BFBC2\'s, {sum(k == "missing" for _, _, k in plan)} missing; templates {pool}')
    if missing:
        say(effect, 'types', 'types no BFBC2 template holds (the catalogue\'s class defaults, or kept as BF1943 has '
            f'them): {sorted(cast("set[str]", missing))}')
    for name, key, kind in plan:
        if kind == 'port' and key and key.startswith('levels/') and not key.startswith(level.lower() + '/'):
            say(name, 'level', f'  LEVEL {name}: {level} has no copy - {key.split("/")[1]}\'s (level_copy)',
                source=key[:-4])
    ours = {n for n, _, k in plan if k in ('port', 'texture', 'sound')}     # 'sound': the caller ports it
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')} if REFERENCE else bc2_partitions(ix)
    resolve = lambda q: q.lower() in ours or q.lower() in bc2 or q.lower() == 'textures/effects/emittersystem/defaultnormalmap'  # noqa: E731
    # the partitions ported here with their BF1943 guids: a reference into one is checked against what is written,
    # not against BFBC2's same-named partition (which the port replaces, and which may lack the guid)
    written = {n.lower(): dbxport.guids_of(src.data(k)) for n, k, kind in plan if kind == 'port'}
    files = {}
    for name, key, kind in plan:
        if kind == 'port':
            data, rep = dbxport.port(src.data(key), T, resolve, catalogue=catalogue, ours=written, name=name,
                                     schema=schema)
            files[name + '.dbx'] = data
            check_defaults(data, report)
            report.extend_from(rep, name, _shown(name))
        elif kind == 'texture':
            e = src.entries[name + '.res'][1]
            files[e['name']] = texture.convert(src.data(name + '.res'))
        elif kind == 'sound':
            say(name, 'sound', f'SOUND {name}: BF1943\'s, for the sound porter')
        elif kind == 'missing':
            say(name, 'missing', f'  MISSING {name}: neither BF1943 nor BFBC2 has it')
    return files, report


def sounds_of(report):
    """The BF1943 sound partitions port() / port_many() left for a sound porter (their 'sound' records; a report
    from before records - a cache - by its SOUND lines)."""
    records = getattr(report, 'records', None)
    if records is not None:
        return sorted({r.path for r in records if r.action == 'sound'})
    return sorted({r.split(' ', 2)[1][:-1] for r in report if r.startswith('SOUND ')})
