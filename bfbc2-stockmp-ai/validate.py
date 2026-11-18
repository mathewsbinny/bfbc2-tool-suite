"""Offline checks of a built AI layer (no game launch). Each check gives (name, ok, detail); validate() runs them all.

  1. DbxEdit check: no error, and no more warnings than the stock level has (references resolved against the built
     bundle folder and mp_common).
  2. DbxEdit roundtrip, and XML export -> import (--like) gives the same bytes.
  3. Read by the suite's other DBX reader (core/bfbc2core/dbx.py): every local reference resolves; every new entity is
     listed once in LevelData.Objects; every event connection's source and target exist; HasAI and
     PathfindingTypes set.
  4. Only the intended changes: compared with the stock level, the existing instances differ only in HasAI,
     PathfindingTypes and (stock_spawners=human) PlayerType; everything else is added.
  5. References into other partitions (template, kits, PathfindingSettings) resolve in the built bundle or mp_common.
  6. The packed template partitions and the SectorOffsets assets: present at the manifest's tail, the offsets stored as
     4-byte integers equal to the values computed from the staged path data.
  7. The AI layer against the path data: every order has a route node within ORDER_NODE m, every AI spawner stands on
     the navigation data (area, or node within SPAWN_NODE m) of the large route group, every spawner is listed by an
     order of its team and stage, and each spawner reaches its orders' objectives over the route graph (same group).
  8. Rush stages: later-stage spawners and orders start disabled; each stage's advance source is in the level.
  9. The packed bundles unpack to exactly the built files.

Round E4a: with the Rust core (stockcore.native()) checks 3, 4, 5 (its reference set and the partition index), 8 and
9 read the files in Rust (rust\\bc2-stockmp: no element tree built in Python) and word the result here; whatever
the Rust reading leaves (a file or field it would raise on) is read in Python as before. DbxEdit's read-only runs
(check, roundtrip) of every variant run at once before the checks.
"""
import json
import os
import re
import shutil
import struct
import sys
import tempfile
from collections import Counter
from typing import cast

import config as cfgmod
import dbxbin
import pipeline
import stockcore

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx as coredbx  # noqa: E402
from bfbc2core import dbxedit  # noqa: E402
from bfbc2core import fbrb as corefbrb  # noqa: E402

ORDER_NODE = 15.0
SPAWN_NODE = cfgmod.SNAP_RADIUS + 0.5
ALLOWED_CHANGED = ('HasAI', 'PlayerType')


_DBXEDIT_DONE: dict[tuple[str, ...], tuple[int, str]] = {}   # validate(): DbxEdit's read-only runs, made at once


def _dbxedit(*args):
    # in process through the Rust core (bfbc2core.dbxedit); a run validate() made beforehand (_prefetch_dbxedit) is
    # taken from there
    key = tuple(map(str, args))
    if key in _DBXEDIT_DONE:
        return _DBXEDIT_DONE.pop(key)
    r = dbxedit.run(list(key), text=True)
    return r.returncode, r.stdout + r.stderr


def _prefetch_dbxedit(runs):
    """DbxEdit's runs that only read files (check, roundtrip), run at once on all cores in process (round E4a: the
    four checks against mp_common and two round trips of a level ran one after another); their results wait in
    _DBXEDIT_DONE for _dbxedit."""
    if not runs:
        return
    keys = [tuple(map(str, r)) for r in runs]
    for k, r in zip(keys, dbxedit.run_many([list(k) for k in keys], text=True)):
        _DBXEDIT_DONE[k] = (r.returncode, r.stdout + r.stderr)


def _read(path):
    """A file's bytes (kept while validate() runs: the checks read the same partitions)."""
    if _VALIDATING and path in _DATA:
        return _DATA[path]
    with open(path, 'rb') as f:
        data = f.read()
    if _VALIDATING:
        while len(_DATA) >= 4:
            del _DATA[next(iter(_DATA))]
        _DATA[path] = data
    return data


_DATA: dict[str, bytes] = {}


def _counts(out):
    m = re.search(r'(\d+) error\(s\), (\d+) warning\(s\)', out)
    return (int(m.group(1)), int(m.group(2))) if m else (None, None)


def check_dbxedit(orig, authored, bundle_dir, orig_bundle_dir, mp_common):
    _rc, a = _dbxedit('check', authored, '--bundle', bundle_dir, '--bundle', mp_common)
    _rc, o = _dbxedit('check', orig, '--bundle', orig_bundle_dir, '--bundle', mp_common)
    ea, wa = _counts(a)
    eo, wo = _counts(o)
    ok = ea is not None and ea <= (eo or 0) and wa <= wo
    errs_a = sorted(l for l in a.splitlines() if l.startswith('ERROR'))
    errs_o = sorted(l for l in o.splitlines() if l.startswith('ERROR'))
    ok = ok and errs_a == errs_o
    return ok, f'authored {ea} error(s), {wa} warning(s); stock {eo} / {wo}'


def check_roundtrip(authored, tmp):
    rc, out = _dbxedit('roundtrip', authored)
    ok1 = '1 identical, 0 different' in out
    a = os.path.join(tmp, 'rt.dbx')
    shutil.copyfile(authored, a)
    _dbxedit('xml', a)
    rc2, out2 = _dbxedit('import', a[:-4] + '.xml', os.path.join(tmp, 'rt_back.dbx'), '--like', a)
    ok2 = rc2 == 0 and open(os.path.join(tmp, 'rt_back.dbx'), 'rb').read() == open(a, 'rb').read()
    return ok1 and ok2, f'roundtrip {"identical" if ok1 else "DIFFERENT"}; xml -> import {"identical" if ok2 else "DIFFERENT (exit " + str(rc2) + ")"}'


_PARSED: dict[bytes, tuple] = {}     # validate(): SHA-256 of a file -> its reading (the checks only read them)


def _core_instances(path):
    """(file, {upper-case guid: instance}) of a DBX file by the suite's reader; while validate() runs, a file read by
    an earlier check (the same bytes) is not parsed again."""
    import hashlib
    data = open(path, 'rb').read()
    key = hashlib.sha256(data).digest()
    if key in _PARSED:
        return _PARSED[key]
    f = coredbx.parse(data)
    got = f, {(e.attr('guid') or '').upper(): e for e in f.instances}
    if _VALIDATING:
        while len(_PARSED) >= 2:             # a variant's stock and authored level (memory: ~100 MB each)
            del _PARSED[next(iter(_PARSED))]
        _PARSED[key] = got
    return got


def _refs(e):
    for c in e.children or ():
        r = c.attr('ref')
        if r is not None and r != 'null':
            yield c, r
        yield from _refs(c)


def check_structure(orig, authored, c):
    nat = stockcore.native()
    facts = nat.stockmp_check_structure(_read(orig), _read(authored)) if nat is not None else None
    if facts is None:                          # no Rust core, or a level it leaves to the Python reading
        return _py_check_structure(orig, authored, c)
    twice, local_bad, objs_twice, n_new, unlisted, event_bad, nev, has_ai, pft = facts
    problems = []
    if twice:
        problems.append(f'{len(twice)} GUIDs used by more than one instance (e.g. {twice[0]}) - authored twice?')
    problems += [f'{g}: local reference {r} unresolved' for g, r in local_bad]
    if objs_twice:
        problems.append(f'{objs_twice} entities listed twice in Objects')
    if unlisted:
        problems.append(f'{unlisted} new instances not in Objects')
    problems += [f'event {side} {r} unresolved' for side, r in event_bad]
    if has_ai != 'true':
        problems.append('HasAI is not true')
    if [p.lower() for p in pft] != [p.lower() for p in __import__('author').PATHFINDING]:
        problems.append(f'PathfindingTypes {pft}')
    return not problems, (f'{n_new} new instances, all in Objects; {nev} event connections resolve; HasAI true; '
                          f'PathfindingTypes infantry + vehicles' if not problems else '; '.join(problems[:8]))


def _py_check_structure(orig, authored, c):
    f, inst = _core_instances(authored)
    _fo, oinst = _core_instances(orig)
    problems = []
    twice = [g for g, n in Counter((e.attr('guid') or '').upper() for e in f.instances).items() if n > 1]
    if twice:                                  # the GUID-keyed dict below would keep only the last of each
        problems.append(f'{len(twice)} GUIDs used by more than one instance (e.g. {twice[0]}) - authored twice?')
    for g, e in inst.items():
        for _c, r in _refs(e):
            if '/' not in r and r.upper() not in inst:
                problems.append(f'{g}: local reference {r} unresolved')
    lvl = next(e for e in f.instances if (e.attr('type') or '').endswith('.LevelData'))
    objs = [i.attr('ref').upper() for i in (lvl.child('Objects').children or [])]
    dup = [k for k, v in Counter(objs).items() if v > 1]
    if dup:
        problems.append(f'{len(dup)} entities listed twice in Objects')
    new = [g for g in inst if g not in oinst]
    unlisted = [g for g in new if g not in set(objs)]
    if unlisted:
        problems.append(f'{len(unlisted)} new instances not in Objects')
    ec = lvl.child('EventConnections')
    nev = 0
    for cx in ec.children or ():
        for side in ('Source', 'Target'):
            r = cx.child(side).attr('ref')
            if '/' not in r and r.upper() not in inst:
                problems.append(f'event {side} {r} unresolved')
        nev += 1
    has_ai = coredbx.value(lvl.child('HasAI'))
    pft = [i.attr('ref') for i in (lvl.child('PathfindingTypes').children or [])]
    if has_ai != 'true':
        problems.append('HasAI is not true')
    if [p.lower() for p in pft] != [p.lower() for p in __import__('author').PATHFINDING]:
        problems.append(f'PathfindingTypes {pft}')
    return not problems, (f'{len(new)} new instances, all in Objects; {nev} event connections resolve; HasAI true; '
                          f'PathfindingTypes infantry + vehicles' if not problems else '; '.join(problems[:8]))


def check_changes(orig, authored, c, tmp):
    """The existing instances: only HasAI, PathfindingTypes, PlayerType, Objects and EventConnections may differ.
    Read in the Rust core when it is built (only the differing leaves come back); several differing leaves of one
    instance are named in the Rust reading's order (the Python reading's is its set's, which varies by process)."""
    nat = stockcore.native()
    facts = nat.stockmp_check_changes(_read(orig), _read(authored)) if nat is not None else None
    if facts is None:
        return _py_check_changes(orig, authored, c, tmp)
    bad = []
    changed: Counter[str] = Counter()
    for g, removed, arrays, leaves in facts:
        if removed:
            bad.append(f'{g} removed')
            continue
        bad += [f'LevelData.{arr}: existing items changed' for arr in arrays]
        for k, x, y in leaves:
            field = re.sub(r'\[\d+\]', '', k).strip('/')
            if field in ALLOWED_CHANGED:
                changed[field] += 1
            else:
                bad.append(f'{g} {k}: {_shown(x)} -> {_shown(y)}')
    want_pt = c.get('stock_spawners') == 'human'
    if changed['PlayerType'] and not want_pt:
        bad.append('PlayerType changed although stock_spawners is not "human"')
    return not bad, (f'existing instances unchanged except {dict(changed)}' if not bad else '; '.join(bad[:8]))


def _shown(leaf):
    """check_changes' shown() of a leaf the Rust core read: (kind, value text, attributes) or None."""
    return None if leaf is None else (leaf[0], leaf[1], tuple(tuple(a) for a in leaf[2]))


def _py_check_changes(orig, authored, c, tmp):
    fo, oinst = _core_instances(orig)
    fa, ainst = _core_instances(authored)
    bad = []
    changed: Counter[str] = Counter()

    def flat(e, path=''):
        """{path: leaf element}; a leaf's value is compared as (kind, value text, attributes) - see same()."""
        out = {}
        for k, ch in enumerate(e.children or ()):
            name = ch.attr('name') or ch.tag
            p = f'{path}/{name}[{k}]'
            if ch.children is not None:
                out.update(flat(ch, p))
            else:
                out[p] = ch
        return out

    def shown(ch):
        return None if ch is None else (ch.kind, coredbx.value(ch), tuple(ch.attributes))

    def same(x, y):
        # the value text is a function of kind, text, raw bytes and attributes: leaves equal in those are equal
        # without formatting their numbers (most of the level); others are compared by their text as before
        if x is not None and y is not None and x.kind == y.kind and x.text == y.text and x.raw == y.raw \
                and x.attributes == y.attributes:
            return True
        return shown(x) == shown(y)
    for g, e in oinst.items():
        a = ainst.get(g)
        if a is None:
            bad.append(f'{g} removed')
            continue
        if (e.attr('type') or '').endswith('.LevelData'):
            fe ={k: v for k, v in flat(e).items() if not k.startswith(('/Objects[', '/EventConnections[', '/PathfindingTypes['))}
            fa_ = {k: v for k, v in flat(a).items() if not k.startswith(('/Objects[', '/EventConnections[', '/PathfindingTypes['))}
            # the old Objects / EventConnections items must be kept, in order, at the front
            for arr in ('Objects', 'EventConnections'):
                oi = [repr(x) for x in (e.child(arr).children or [])]
                ai = [repr(x) for x in (a.child(arr).children or [])][:len(oi)]
                if oi != ai:
                    bad.append(f'LevelData.{arr}: existing items changed')
        else:
            fe, fa_ = flat(e), flat(a)
        for k in set(fe) | set(fa_):
            if not same(fe.get(k), fa_.get(k)):
                field = re.sub(r'\[\d+\]', '', k).strip('/')
                if field in ALLOWED_CHANGED:
                    changed[field] += 1
                else:
                    bad.append(f'{g} {k}: {shown(fe.get(k))} -> {shown(fa_.get(k))}')
    want_pt = c.get('stock_spawners') == 'human'
    if changed['PlayerType'] and not want_pt:
        bad.append('PlayerType changed although stock_spawners is not "human"')
    return not bad, (f'existing instances unchanged except {dict(changed)}' if not bad else '; '.join(bad[:8]))


_FOLDER_INDEX: dict[str, dict[str, str]] = {}    # validate(): one folder's index, kept while it runs


def _folder_index(top):
    """lower-case partition path (no .dbx) -> file path, over one unpacked bundle folder (kept in _FOLDER_INDEX
    while validate() runs: mp_common's 15,000 files are walked once, not once per variant)."""
    if top in _FOLDER_INDEX:
        return _FOLDER_INDEX[top]
    nat = stockcore.native()
    got = nat.stockmp_folder_index(top) if nat is not None else None    # the Rust core's walk (None: left to this)
    if got is not None:
        if _VALIDATING:
            _FOLDER_INDEX[top] = dict(got)
        return dict(got)
    idx: dict[str, str] = {}
    for dp, _ds, fs in os.walk(top):
        for fn in fs:
            if fn.lower().endswith('.dbx'):
                p = os.path.join(dp, fn)
                rel = os.path.relpath(p, top)[:-4].replace('\\', '/').lower()
                idx.setdefault(rel, p)
    if _VALIDATING:
        _FOLDER_INDEX[top] = idx
    return idx


_VALIDATING = False


def _partition_index(folders):
    """lower-case partition path (no .dbx) -> folder file path, over unpacked bundle folders (the first folder's
    file where two have one)."""
    idx: dict[str, str] = {}
    for top in folders:
        for rel, p in _folder_index(top).items():
            idx.setdefault(rel, p)
    return idx


def check_external(orig, authored, folders):
    nat = stockcore.native()
    got = nat.stockmp_check_external(_read(orig), _read(authored)) if nat is not None else None
    wanted = _py_external_refs(orig, authored) if got is None else set(got)
    idx = _partition_index(folders)
    bad = []
    cache = {}
    for r in sorted(wanted):
        part, guid = r.rsplit('/', 1)
        p = idx.get(part)
        if p is None:
            bad.append(f'{r}: partition not found')
            continue
        if p not in cache:
            cache[p] = {(e.attr('guid') or '').lower() for e in coredbx.read(p).instances}
        if guid not in cache[p]:
            bad.append(f'{r}: instance not in {os.path.basename(p)}')
    return not bad, (f'{len(wanted)} new references into other partitions all resolve: ' + ', '.join(sorted(wanted))
                     if not bad else '; '.join(bad))


def _py_external_refs(orig, authored):
    """The references into other partitions the authored level adds, read in Python (check_external's set)."""
    _fo, oinst = _core_instances(orig)
    _f, inst = _core_instances(authored)
    wanted = set()
    for g, e in inst.items():
        if g in oinst and not (e.attr('type') or '').endswith('.LevelData'):
            continue
        for _c, r in _refs(e):
            if '/' in r:
                wanted.add(r.lower())
    # only the references the stock level does not already have
    old = set()
    for e in oinst.values():
        for _c, r in _refs(e):
            if '/' in r:
                old.add(r.lower())
    wanted -= old
    return wanted


def check_assets(build_dir, rep, c, lv_upper):
    bad = []
    lines = [l for l in open(os.path.join(build_dir, 'fbrb.manifest'), encoding='utf-8').read().splitlines() if l.strip()]
    added = rep.get('added_entries', [])
    tail = [l.split('|', 1)[0] for l in lines[-len(added):]] if added else []
    if [t.lower() for t in tail] != [a.lower() for a in added]:
        bad.append(f'added entries are not the manifest tail ({tail})')
    for kind, _folder in pipeline.KINDS:
        p = pipeline.find_entry(build_dir, f'Levels/{lv_upper}/Precomputed/{kind}/SectorOffsets.dbx')
        if not p:
            bad.append(f'{kind} SectorOffsets missing')
            continue
        f = coredbx.read(p)
        e = f.instances[0]
        name = coredbx.value(cast(coredbx.Element, e.child('Name')))
        if name != f'Levels/{lv_upper}/Precomputed/{kind}/SectorOffsets':
            bad.append(f'{kind}: Name {name}')
        want = rep['sector_offsets_values'][kind]
        for fld, v in (('XOffset', want[0]), ('ZOffset', want[1])):
            size, vals = coredbx.numbers(cast(coredbx.Element, e.child(fld)))
            if size != 4 or struct.unpack('>i', vals[0])[0] != v:
                bad.append(f'{kind} {fld} is not the integer {v}')
    for p in rep.get('template_pack', []):
        q = pipeline.find_entry(build_dir, p)
        if not q:
            bad.append(f'{p} not packed')
    if rep.get('template_pack'):
        t = pipeline.find_entry(build_dir, rep['template_pack'][0])
        g = c['template'].rsplit('/', 1)[1].lower()
        if t is not None and g not in {(e.attr('guid') or '').lower() for e in coredbx.read(t).instances}:   # (not packed: said above)
            bad.append('the template instance is not in the packed template partition')
    return not bad, ('SectorOffsets ' + ', '.join(f'{k} {v[0]},{v[1]}' for k, v in rep['sector_offsets_values'].items())
                     + f' as 4-byte integers; {len(rep.get("template_pack", []))} template partitions packed; added entries at the tail'
                     if not bad else '; '.join(bad))


def check_navigation(c, hpd):
    snap = cfgmod.Snapper(hpd)
    if snap.pd is None:
        return False, f'no path data at {hpd}'
    bad = []
    groups = {}
    for s in c['spawners']:
        p = tuple(s['pos'])
        q = (p[0], p[1] - cfgmod.SPAWN_LIFT, p[2])
        dist, node, size = snap.nearest(q)
        on = snap.pd.on_area(q)
        if not on and dist > SPAWN_NODE:
            bad.append(f'spawner {s["id"]}: not on the navigation data (node {dist:.1f} m)')
        if size < 0.25 * snap.biggest:
            bad.append(f'spawner {s["id"]}: on a small island ({size} nodes)')
        groups[s['id']] = snap.pd.group[snap.pd.ids[_nearest_index(snap, q)]]
    listed: Counter[str] = Counter()
    worst = 0.0
    for o in c['orders']:
        dist, node, size = snap.nearest(tuple(o['pos']))
        worst = max(worst, dist)
        if dist > ORDER_NODE:
            bad.append(f'order {o["id"]}: no route node within {ORDER_NODE} m ({dist:.1f} m)')
        og = snap.pd.group[snap.pd.ids[_nearest_index(snap, tuple(o['pos']))]]
        for x in o['soldiers']:
            listed[x] += 1
            if groups.get(x) != og:
                bad.append(f'order {o["id"]}: spawner {x} is not route-connected to the objective')
    unlisted = [s['id'] for s in c['spawners'] if not listed[s['id']]]
    if unlisted:
        bad.append(f'{len(unlisted)} AI spawners in no order: {unlisted[:6]}')
    return not bad, (f'{len(c["spawners"])} spawners on the navigation data, {len(c["orders"])} orders with a route node '
                     f'within {worst:.1f} m, every spawner route-connected to its objectives' if not bad else '; '.join(bad[:8]))


def _nearest_index(snap, p):
    import numpy as np
    P = snap.pd.pos
    d = np.hypot(P[:, 0] - p[0], P[:, 2] - p[2])
    d = np.where(np.abs(P[:, 1] - cfgmod.NODE_Y - p[1]) <= 4.0, d, 1e9)
    return int(np.argmin(d))


def check_stages(authored, c):
    stages = c.get('stages') or []
    if not stages and not any(s.get('stage') for s in c['spawners']):
        return True, 'no stages (one-stage mode)'
    nat = stockcore.native()
    if nat is not None:
        try:                        # the configuration as the Python reading reads it; anything odd is left to it
            later = []
            for s in c['spawners'] + c['orders']:
                if (s.get('stage') or 0) > 0:
                    role = 'spawner' if s in c['spawners'] else 'order'
                    later.append((s, dbxbin.new_guid('opfor:stockmp', c['lv'], role, s['id'])))
            srcs = [st['advance']['source'].upper() for st in stages]
            facts = nat.stockmp_instance_facts(_read(authored), [g for _s, g in later] + srcs)
        except Exception:                                   # the Python reading below raises its own
            facts = None
        # the Python reading where it would raise: a later-stage instance missing, or without a readable Enabled
        if facts is not None and all(f is not None and f[1] is not None for f in facts[:len(later)]):
            bad = [f'{s["id"]} (stage {s["stage"]}) starts enabled' for (s, _g), f in zip(later, facts) if f[1] != 'false']
            for st, src, f in zip(stages, srcs, facts[len(later):]):
                if f is None:
                    bad.append(f'stage {st["index"]}: source {src} missing')
                elif not (f[0] or '').endswith('MultipleTriggerEntityData'):
                    bad.append(f'stage {st["index"]}: source is a {f[0]}')
            return not bad, (f'{len(stages)} stage switches; later stages start disabled' if not bad else '; '.join(bad[:8]))
    _f, inst = _core_instances(authored)
    bad = []
    for s in c['spawners'] + c['orders']:
        if (s.get('stage') or 0) > 0:
            role = 'spawner' if s in c['spawners'] else 'order'
            g = dbxbin.new_guid('opfor:stockmp', c['lv'], role, s['id'])
            if coredbx.value(inst[g].child('Enabled')) != 'false':
                bad.append(f'{s["id"]} (stage {s["stage"]}) starts enabled')
    for st in stages:
        src = st['advance']['source'].upper()
        if src not in inst:
            bad.append(f'stage {st["index"]}: source {src} missing')
        elif not (inst[src].attr('type') or '').endswith('MultipleTriggerEntityData'):
            bad.append(f'stage {st["index"]}: source is a {inst[src].attr("type")}')
    return not bad, (f'{len(stages)} stage switches; later stages start disabled' if not bad else '; '.join(bad[:8]))


def packed_files(out_bundle):
    """What FbrbTool's unpack of the bundle writes, read in memory (bfbc2core.fbrb.read): {normcase relative path:
    bytes}, *deleted* markers left out, a later entry on the same file (Windows compares names without case) winning
    as its write would."""
    live = [e.name for e in corefbrb.entries(out_bundle) if not e.deleted]
    data = corefbrb.read(out_bundle, live)
    return {os.path.normcase(os.path.normpath(n.replace('/', os.sep).replace('\\', os.sep))): data[n] for n in live}


def check_packed(out_bundle, build_dir, tmp):
    """The packed bundle unpacks to exactly the built files (compared in memory: the same answer as FbrbTool's
    unpack into a folder, without writing and deleting every file; in the Rust core on all cores when it takes the
    bundle and folder)."""
    nat = stockcore.native()
    got = nat.stockmp_packed_check(os.fspath(out_bundle), os.fspath(build_dir)) if nat is not None else None
    if got is not None:
        n, bad = got
        return bad == 0, f'{n} files unpack identical' if not bad else f'{bad} of {n} files differ after packing'
    files = packed_files(out_bundle)
    bad = 0
    n = 0
    for dp, _ds, fs in os.walk(build_dir):
        for fn in fs:
            p = os.path.join(dp, fn)
            rel = os.path.relpath(p, build_dir)
            if rel == 'fbrb.manifest':
                continue
            n += 1
            got = files.get(os.path.normcase(rel))
            if got is None or got != open(p, 'rb').read():
                bad += 1
    return bad == 0, f'{n} files unpack identical' if not bad else f'{bad} of {n} files differ after packing'


def validate(paths, c, log=print):
    global _VALIDATING
    rep = json.load(open(os.path.join(paths.root, 'build_report.json'), encoding='utf-8'))
    results = []
    tmp = tempfile.mkdtemp(prefix='stockmp_validate_')
    mp_common = pipeline.common(paths, 'mp_common')
    so = {k: (v['x'], v['z']) for k, v in rep['sector_offsets'].items()}
    _VALIDATING = True
    try:
        try:            # every variant's read-only DbxEdit runs at once (the checks below take their results)
            pre = []
            for v, var in rep['variants'].items():
                lv_upper = var['entry'].split('/')[-1][:-4]
                orig = os.path.join(paths.root, 'build', f'{lv_upper}_{v}.original.dbx')
                auth = os.path.join(paths.root, 'build', f'{lv_upper}_{v}.authored.dbx')
                pre += [('check', auth, '--bundle', os.path.join(paths.root, 'build', v), '--bundle', mp_common),
                        ('check', orig, '--bundle', paths.src(v), '--bundle', mp_common), ('roundtrip', auth)]
            _prefetch_dbxedit(pre)
        except Exception:                   # not made ahead: each check runs its own (and fails by its own rules)
            _DBXEDIT_DONE.clear()
        for v, var in rep['variants'].items():
            lv_upper = var['entry'].split('/')[-1][:-4]
            orig = os.path.join(paths.root, 'build', f'{lv_upper}_{v}.original.dbx')
            auth = os.path.join(paths.root, 'build', f'{lv_upper}_{v}.authored.dbx')
            bdir = os.path.join(paths.root, 'build', v)
            sdir = paths.src(v)
            var = dict(var, sector_offsets_values=so, template_pack=rep.get('template_pack', []))
            for name, fn, args in (('DbxEdit check', check_dbxedit, (orig, auth, bdir, sdir, mp_common)),
                                   ('round trips', check_roundtrip, (auth, tmp)),
                                   ('structure (second reader)', check_structure, (orig, auth, c)),
                                   ('only intended changes', check_changes, (orig, auth, c, tmp)),
                                   ('references into other partitions', check_external, (orig, auth, [bdir, mp_common])),
                                   ('packed assets', check_assets, (bdir, var, c, lv_upper)),
                                   ('Rush stages', check_stages, (auth, c)),
                                   ('packed bundle', check_packed, (paths.out_bundle(v), bdir, tmp))):
                try:
                    ok, detail = fn(*args)
                except Exception as e:                       # a check that cannot run is a failed check
                    ok, detail = False, f'{type(e).__name__}: {e}'
                results.append((f'{v}: {name}', ok, detail))
                log(f'{"ok  " if ok else "FAIL"} {v}: {name}: {detail}')
        if c.get('layer', 'full') != 'minimal':
            ok, detail = check_navigation(c, paths.out_pathdata('infantry'))
            results.append(('navigation', ok, detail))
            log(f'{"ok  " if ok else "FAIL"} navigation: {detail}')
        for kind, folder in pipeline.KINDS:
            try:
                xz, _agree = pipeline.sector_offsets(paths.out_pathdata(folder))
                ok, detail = tuple(xz) == tuple(so[kind]), f'sector offsets {xz} (asset {so[kind]})'
            except Exception as e:                           # a check that cannot run is a failed check
                ok, detail = False, f'{type(e).__name__}: {e}'
            results.append((f'staged {folder} path data', ok, detail))
            log(f'{"ok  " if ok else "FAIL"} staged {folder} path data: {detail}')
    finally:
        _VALIDATING = False
        _FOLDER_INDEX.clear()
        _PARSED.clear()
        _DATA.clear()
        _DBXEDIT_DONE.clear()
        shutil.rmtree(tmp, ignore_errors=True)
    out = {'passed': all(ok for _n, ok, _d in results), 'results': [{'check': n, 'ok': ok, 'detail': d} for n, ok, d in results]}
    with open(os.path.join(paths.root, 'validation.json'), 'w', encoding='utf-8') as f:
        json.dump(out, f, indent=1)
    return out
