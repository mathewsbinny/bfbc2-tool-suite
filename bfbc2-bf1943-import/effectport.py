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
- a reference that will resolve to nothing is reported (dbxport substitutes the template's value).
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in ('core', 'bfbc2-console-convert'):
    sys.path.insert(0, os.path.join(SUITE, p))
sys.path.insert(0, HERE)
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


def walk(src, ix, effect, level):
    """[(partition name, source key, kind)] the effect reaches: kind 'port' (BF1943's, BFBC2 lacks it), 'texture',
    'bfbc2' (kept as a reference), 'sound' (BF1943's, BFBC2 lacks it: not ported here - port() lists it for the
    caller's sound porter), 'missing'."""
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
        if name.startswith('sound/') and name + '.dbx' in src.entries:
            # a BF1943 sound BFBC2 lacks: the caller's sound porter (its sound templates, its waves converted) -
            # aligning it on effect / emitter templates would leave its Audio.* types unaligned and its waves out
            out.append((name, name + '.dbx', 'sound'))
            continue
        key = name + '.dbx'
        if key not in src.entries and name.startswith('emitters/'):
            key = f'{level}/{name}.dbx'                        # BF1943 keeps emitter documents per level
        if key not in src.entries:
            out.append((name, None, 'bfbc2' if name + '.dbx' in ix.where else 'missing'))
            continue
        out.append((name, key, 'port'))
        todo += [r for r in refs(src.data(key)) if r not in seen]
    return out


def templates(ix, types):
    """dbxport.Templates from TEMPLATES plus BFBC2 emitter documents covering the types (greedy, by index order)."""
    pool = list(TEMPLATES)
    have = set()
    for p in pool:
        have |= {i.attr('type') for i in dbx.parse(ix.read(p + '.dbx')).instances}
    need = set(types) - have
    if need:
        for n in sorted(k for k in ix.where if '/emitters/' in k and k.endswith('.dbx')):
            b = ix.read(n)
            if b is None:
                continue
            got = {i.attr('type') for i in dbx.parse(b).instances} & need
            if got:
                pool.append(n[:-4])
                need -= got
            if not need:
                break
    return dbxport.Templates([(p, ix.read(p + '.dbx')) for p in pool]), pool, need


def check_defaults(data, report):
    f = dbx.parse(data)
    for inst in f.instances:
        t = inst.attr('type').split('.')[-1]
        for (tt, fld), want in DEFAULTS.items():
            if t == tt and inst.child(fld) is not None:
                got = dbx.value(inst.child(fld))
                if got != want:
                    report.append(f'DEFAULT {t}.{fld} = {got}, DICE usually {want}')


def port_many(src, ix, effects, level, log=print):
    """{file: bytes}, report for several effects - the templates found once for all of them (each effect's own
    search scans BFBC2's emitter documents: slow for dozens)."""
    plans = {e: walk(src, ix, e, level) for e in effects}
    types = set()
    for plan in plans.values():
        for name, key, kind in plan:
            if kind == 'port':
                types |= {i.attr('type') for i in dbx.parse(src.data(key)).instances}
    shared = templates(ix, types)
    files, report = {}, []
    for n, e in enumerate(effects):
        f, r = port(src, ix, e, level, shared=shared, plan=plans[e])
        files.update(f)
        report += r
        log(f'  effect {n + 1}/{len(effects)} {e.rsplit("/", 1)[-1]}: {len(f)} files')
    return files, report


def port(src, ix, effect, level, shared=None, plan=None):
    import texture
    plan = plan or walk(src, ix, effect, level)
    types = set()
    for name, key, kind in plan:
        if kind == 'port':
            types |= {i.attr('type') for i in dbx.parse(src.data(key)).instances}
    T, pool, missing = shared or templates(ix, types)
    missing = set(types) - set(T.by_type)
    report = [f'{effect}: {sum(k == "port" for _, _, k in plan)} partitions to port, '
              f'{sum(k == "sound" for _, _, k in plan)} sounds for the sound porter, '
              f'{sum(k == "texture" for _, _, k in plan)} textures, {sum(k == "bfbc2" for _, _, k in plan)} kept as '
              f'BFBC2\'s, {sum(k == "missing" for _, _, k in plan)} missing; templates {pool}']
    if missing:
        report.append(f'types no BFBC2 template holds (kept as BF1943 has them): {sorted(missing)}')
    ours = {n for n, _, k in plan if k in ('port', 'texture', 'sound')}     # 'sound': the caller ports it
    bc2 = {n[:-4] for n in ix.where if n.endswith('.dbx')}
    resolve = lambda q: q.lower() in ours or q.lower() in bc2 or q.lower() == 'textures/effects/emittersystem/defaultnormalmap'  # noqa: E731
    files = {}
    for name, key, kind in plan:
        if kind == 'port':
            data, rep = dbxport.port(src.data(key), T, resolve)
            files[name + '.dbx'] = data
            check_defaults(data, report)
            report += [f'  {name}: {r}' for r in rep if r.startswith(('substituted', 'value', 'UNMAPPED', 'left out instance', 'type ', 'nested', 'renamed'))]
        elif kind == 'texture':
            e = src.entries[name + '.res'][1]
            files[e['name']] = texture.convert(src.data(name + '.res'))
        elif kind == 'sound':
            report.append(f'SOUND {name}: BF1943\'s, for the sound porter')
        elif kind == 'missing':
            report.append(f'  MISSING {name}: neither BF1943 nor BFBC2 has it')
    return files, report


def sounds_of(report):
    """The BF1943 sound partitions port() / port_many() left for a sound porter (their SOUND report lines)."""
    return sorted({r.split(' ', 2)[1][:-1] for r in report if r.startswith('SOUND ')})
