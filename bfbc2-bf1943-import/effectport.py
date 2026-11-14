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


def templates(ix, types, catalogue=None):
    """dbxport.Templates from TEMPLATES plus BFBC2 emitter documents covering the types (greedy, by name order).
    The emitter documents are read a bundle at a time and parsed only when their bytes name a type still needed
    (a type's name is in the partition's string table): the scan over BFBC2's 18,343 emitter documents was I/O-bound
    (35 minutes for the material grid's effects)."""
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
    types = set()
    for plan in plans.values():
        for _name, key, kind in plan:
            if kind == 'port':
                types |= {i.attr('type') for i in dbx.parse(src.data(key)).instances}
    shared = templates(ix, types, catalogue)
    files, report = {}, dbxport.Report()
    for n, e in enumerate(effects):
        f, r = port(src, ix, e, level, shared=shared, plan=plans[e], catalogue=catalogue, schema=schema)
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


def port(src, ix, effect, level, shared=None, plan=None, catalogue=None, schema='error'):
    """({file: bytes}, dbxport.Report) of one effect. The report's records: the effect's summary ('summary'), each
    partition's dbxport records (their partition set), 'sound' (a BF1943 sound for the caller's sound porter: its
    path), 'missing', 'default'; its lines as before (the dbxport lines of SHOWN, indented under the partition).
    schema: dbxport's ('report': a shape no converter handles is kept as BF1943's and reported, 'schema' records)."""
    import texture
    plan = plan or walk(src, ix, effect, level)
    types = set()
    for _name, key, kind in plan:
        if kind == 'port':
            types |= {i.attr('type') for i in dbx.parse(src.data(key)).instances}
    T, pool, missing = shared or templates(ix, types, catalogue)
    missing = set(types) - set(T.candidates)
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
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
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
