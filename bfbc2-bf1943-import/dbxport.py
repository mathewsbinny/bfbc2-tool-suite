r"""dbxport.py - Battlefield 1943's DBX partitions in BFBC2's schema.

The binary format is the same in both games (strings, varints, big-endian numbers: the suite's `bfbc2core.dbx`
reads both); what differs is the schema: a few fields per type (BFBC2's SkinnedMeshAsset has MemoryReportGroup,
HighPriorityOccluderMesh, RadiosityMesh, RadiosityAlbedoTexture that BF1943's lacks, for instance). A partition is
ported by aligning each instance with a BFBC2 instance of the same type (a template, from BFBC2's own partitions):
fields both have keep BF1943's value, in BFBC2's order (structs aligned the same way, array items against BFBC2's
first item); fields only BFBC2 has take the template's value; fields only BF1943 has are left out. Every addition,
omission and type BFBC2 has no template for is reported. The result is written with `bfbc2core.dbx.write`.

    t = dbxport.Templates([<unpacked BFBC2 bundle folders>])
    data, report = dbxport.port(bf1943_dbx_bytes, t)
"""
import copy
import glob
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), 'core'))
from bfbc2core import dbx  # noqa: E402


class Templates:
    """BFBC2 instances by type: the first of each type found in the given partitions (an instance of a type always
    carries all its fields, so one is enough). Folders of unpacked bundles, .dbx files, or (name, bytes) pairs."""

    def __init__(self, sources):
        self.by_type = {}
        self.where = {}
        for s in sources:
            items = []
            if isinstance(s, tuple):
                items = [s]
            elif os.path.isdir(s):
                items = [(p, None) for p in glob.glob(os.path.join(s, '**', '*.dbx'), recursive=True)]
            elif os.path.isfile(s):
                items = [(s, None)]
            for name, data in items:
                try:
                    f = dbx.parse(data if data is not None else open(name, 'rb').read())
                except (ValueError, IndexError):
                    continue
                for inst in f.instances:
                    t = inst.attr('type')
                    if t and t not in self.by_type:
                        self.by_type[t] = inst
                        self.where[t] = name

    def __contains__(self, t):
        return t in self.by_type


# BFBC2 array field: the BF1943 single-reference field it replaced (a weapon state's meshes)
ONE_TO_ARRAY = {'Meshes1p': 'Mesh1p', 'Meshes3p': 'Mesh3p'}
# BFBC2 field: the BF1943 field it was renamed from, aligned as the same field (a physics asset's part table: BF1943's
# {Name, MaterialName} items, BFBC2's with HashName, MaterialIndex, Aabb, Translation - all 0 in DICE's files)
RENAMED = {'PartInfoDatas': 'PartMaterialDatas'}


# Field values BF1943 writes otherwise than BFBC2. The vehicle seat hints (HintLine1 / 2 + HintAction1 / 2: the text
# and the input action whose key the HUD names beside it): BF1943 spells the input-action enum EIA... / EIUndefined,
# BFBC2 Eia... / EiaUndefined - the engine resolves enums by name, so BF1943's spellings named no key; BF1943's hint
# texts VH_SWITCH (BFBC2 has it, without a PC text) and VH_BOAT_THROTTLE (BFBC2 has none) become the texts BFBC2's
# own vehicles show (VEHICLE_HINT_CHANGE_POSITION, whose PC text WIN32_... names the key; VEHICLE_HINT_ACCELERATE);
# BF1943's countermeasure action (the Sherman's coax line) has no BFBC2 action - undefined, as BFBC2's tanks have it.
HINT_ACTIONS = {'EIUndefined': 'EiaUndefined', 'EIAFireCountermeasure': 'EiaUndefined'}
HINT_TEXTS = {'VH_SWITCH': 'VEHICLE_HINT_CHANGE_POSITION', 'VH_BOAT_THROTTLE': 'VEHICLE_HINT_ACCELERATE'}


def hint_value(field, value):
    """The BFBC2 value of a BF1943 seat-hint field's value (unchanged when it needs none)."""
    v = value.strip()
    if field in ('HintAction1', 'HintAction2'):
        if v in HINT_ACTIONS:
            return HINT_ACTIONS[v]
        if v.startswith('EIA'):
            return 'Eia' + v[3:]
    elif field in ('HintLine1', 'HintLine2') and v in HINT_TEXTS:
        return HINT_TEXTS[v]
    return value


def _values(e, report):
    """BF1943's values of the fields above, as BFBC2 writes them (in place)."""
    n = e.attr('name')
    if n in ('HintAction1', 'HintAction2', 'HintLine1', 'HintLine2') and e.text is not None:
        new = hint_value(n, e.text)
        if new != e.text:
            report.append(f'value {n}: {e.text.strip()} -> {new}')
            e.text = new
    for c in e.children or ():
        _values(c, report)


def _named(children):
    out = {}
    for c in children or ():
        n = c.attr('name')
        if n is not None and n not in out:
            out[n] = c
    return out


def _lost(e, resolve):
    """The reference e itself holds (its 'ref', or its items' for an array of references), if it will not resolve:
    resolve(target) gets '<partition>' for a reference into another partition, '#<guid>' for one inside this one."""
    refs = [e.attr('ref')] if e.attr('ref') else []
    if e.tag == 'array':
        refs += [c.attr('ref') for c in (e.children or ()) if c.attr('ref') and not c.children]
    out = []
    for r in refs:
        if r == 'null':
            continue
        target = r.rsplit('/', 1)[0] if '/' in r else '#' + r.lower()
        if not resolve(target):
            out.append(target)
    return out


def _pruned(e, resolve, path, report):
    """An array without the items that would not resolve: a reference item, or a struct item one of whose own
    reference fields would not (an event connection to a left-out entity)."""
    keep = []
    for c in e.children or ():
        if c.kind == dbx.CONTAINER and c.tag != 'array':
            gone = [x for f in (c.children or ()) if f.attr('ref') for x in _lost(f, resolve)]
        else:
            gone = [r for r in [c.attr('ref')] if r and r != 'null' and
                    not resolve(r.rsplit('/', 1)[0] if '/' in r else '#' + r.lower())]
        if gone:
            continue
        keep.append(c)
    n = len(e.children or ()) - len(keep)
    if n:
        report.append(f'pruned {path}: {n} item(s) referring to what will not exist')
    return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, keep)


def _local(r):
    return bool(r) and r != 'null' and '/' not in r


def _without_local_refs(e, path, report):
    """A field taken from the template without the template's own instances: a reference to one (a guid with no
    partition) would name an instance this partition does not have - array items naming one are left out, a single
    reference is nulled (BF1943's Wake ambient sounds have no IndoorDampings: mp_005's three, all dangling)."""
    if _local(e.attr('ref')):
        e.attributes = [(k, 'null' if k == 'ref' else v) for k, v in e.attributes]
        report.append(f'nulled {path} (the template\'s own instance)')
    if e.children:
        keep = []
        for c in e.children:
            if e.tag == 'array' and not c.children and _local(c.attr('ref')):
                continue
            keep.append(_without_local_refs(c, path, report))
        if len(keep) != len(e.children):
            report.append(f'emptied {path}: {len(e.children) - len(keep)} item(s) naming the template\'s instances')
        e.children = keep
    return e


def _align(e, t, path, report, resolve=None, prune=False):
    """e (BF1943) aligned with t (BFBC2): a new element. resolve(target) says whether a reference will resolve (see
    _lost); a field holding one that will not takes the template's value instead (structs and arrays of structs are
    aligned field by field, so only the field holding it is replaced); an array of references - and with prune, any
    array - loses the items that would not resolve (_pruned)."""
    # an array of references always loses just the items that will not resolve: the template's array would name the
    # template's own instances, which this partition does not have (BF1943's Willys: the VehicleComponentData's
    # components - one, the car horn, left out - became the donor's 23 guids, all dangling: a null component at load)
    if resolve is not None and e.tag == 'array' and (prune or any(
            c.attr('ref') and not c.children for c in (e.children or ()))):
        e = _pruned(e, resolve, path, report)
    if resolve is not None and e.attr('name') is not None:
        lost = _lost(e, resolve)
        if lost:
            report.append(f'substituted {path} (refers to {lost[0]}{" and more" if len(lost) > 1 else ""}): the '
                          f'template\'s value')
            return copy.deepcopy(t)
    if e.kind != dbx.CONTAINER or t.kind != dbx.CONTAINER:
        return e
    if e.tag == 'array':
        items = [c for c in (e.children or ())]
        proto = next((c for c in (t.children or ()) if c.kind == dbx.CONTAINER), None)
        new = []
        for i, c in enumerate(items):
            new.append(_align(c, proto, f'{path}[{i}]', report, resolve, prune) if proto is not None and c.kind == dbx.CONTAINER
                       else c)
        return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, new)
    mine = _named(e.children)
    theirs = _named(t.children)
    new = []
    for c in t.children or ():
        n = c.attr('name')
        if n is None:
            continue
        single = ONE_TO_ARRAY.get(n)
        if n not in mine and single in mine and c.tag == 'array' and c.children:
            # BF1943's single reference -> BFBC2's array of references (Mesh1p -> Meshes1p): one item, BF1943's
            item = copy.deepcopy(c.children[0])
            item.attributes = [(k, mine[single].attr('ref') if k == 'ref' else v) for k, v in item.attributes]
            arr = dbx.Element(c.tag, c.kind, list(c.attributes), c.text, c.raw, [item])
            new.append(_align(arr, c, f'{path}.{n}', report, resolve, prune))
            report.append(f'converted {path}.{single} -> {n} (one item)')
            continue
        old = RENAMED.get(n)
        if n not in mine and old in mine:
            el = copy.deepcopy(mine[old])
            el.attributes = [(k, n if k == 'name' else v) for k, v in el.attributes]
            new.append(_align(el, c, f'{path}.{n}', report, resolve, prune))
            report.append(f'renamed {path}.{old} -> {n}')
            continue
        if n in mine:
            new.append(_align(mine[n], c, f'{path}.{n}', report, resolve, prune))
        else:
            new.append(_without_local_refs(copy.deepcopy(c), f'{path}.{n}', report))
            report.append(f'added {path}.{n} (BFBC2\'s value)')
    converted = {ONE_TO_ARRAY[n] for n in theirs if n in ONE_TO_ARRAY} | {RENAMED[n] for n in theirs if n in RENAMED}
    for n in mine:
        if n not in theirs and n not in converted:
            report.append(f'left out {path}.{n} (BF1943 only)')
    # unnamed children BFBC2's template has none of (rare): kept as they are
    new += [c for c in (e.children or ()) if c.attr('name') is None and e.tag != 'array']
    return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, new)


# BF1943 types BFBC2's executable has no class for (2026-10-31): dropped by port(drop_types=NOT_IN_BFBC2)
# a placement needs its object: port(required=REQUIRED) leaves out one whose object no partition will supply
REQUIRED = {'Entity.ReferenceObjectData': 'ReferencedObject'}
NOT_IN_BFBC2 = {'GameSharedResources.AirRaidEffectsData', 'GameSharedResources.TeamVehicleProjectileEntityData',
                'GameSharedResources.CarHornComponentData', 'GameSharedResources.ScorePartComponentData'}


def port(data, templates, resolve=None, drop_untemplated=False, drop_types=(), prune=False, required=None):
    """(BFBC2 .dbx bytes, report lines) for a BF1943 partition. resolve(partition): whether a referenced partition
    will exist (None: references kept). drop_untemplated: instances of a type the templates do not have (a BFBC2
    donor partition does not carry, e.g. BF1943's embedded PersistentWeapon) are left out, and references to them
    take the template's value; otherwise they are kept as BF1943 has them. drop_types: types left out whatever the
    templates (NOT_IN_BFBC2: the classes BFBC2 does not have), references to them handled the same way. prune (a
    level): arrays lose the items that would not resolve - an event connection to a left-out entity, an object list's
    entry - instead of taking the template's array. required {type: field}: an instance whose reference in that field
    would not resolve is left out too (a placement of an object no partition will supply: REQUIRED), and what refers
    to it pruned in turn."""
    f = dbx.parse(data)
    report = []
    part = f.partition
    dropped = set()
    for inst in part.children or ():
        if inst.tag != 'instance':
            continue
        t = inst.attr('type')
        if t in drop_types:
            dropped.add((inst.attr('guid') or '').lower())
            report.append(f'left out instance {t} ({inst.attr("guid")}): BFBC2 has no such class')
        elif drop_untemplated and t not in templates:
            dropped.add((inst.attr('guid') or '').lower())
            report.append(f'left out instance {t} ({inst.attr("guid")}): the donor has none')
    for inst in part.children or ():
        field = (required or {}).get(inst.attr('type'))
        f_ = _field(inst, field) if field else None
        r = f_.attr('ref') if f_ is not None else None
        if r and r != 'null' and '/' in r and resolve is not None and not resolve(r.rsplit('/', 1)[0]):
            dropped.add((inst.attr('guid') or '').lower())
            report.append(f'left out instance {inst.attr("type")} ({inst.attr("guid")}): its {field} {r.rsplit("/", 1)[0]}'
                          f' will not exist')
    res = None
    if resolve is not None or dropped:
        def res(target):
            if target.startswith('#'):
                return target[1:] not in dropped
            return resolve is None or resolve(target)
    new = []
    for inst in part.children or ():
        if inst.tag != 'instance':
            new.append(inst)
            continue
        t = inst.attr('type')
        if (inst.attr('guid') or '').lower() in dropped:
            continue
        if t not in templates:
            report.append(f'type {t} ({inst.attr("guid")}): no BFBC2 template - kept as BF1943 has it')
            new.append(inst)
            continue
        new.append(_align(inst, templates.by_type[t], t, report, res, prune))
    _part_link_names(new, report)
    for e in new:
        _values(e, report)
    out = dbx.DbxFile(f.strings, [dbx.Element(part.tag, part.kind, list(part.attributes), part.text, part.raw, new)]
                      + f.roots[1:])
    return dbx.write(out), report


def _field(e, name):
    return next((c for c in e.children or () if c.attr('name') == name), None)


def _part_link_names(instances, report):
    """A part link's PartComponentName1 / 2 (BFBC2 only; a template's value after aligning): the names of the
    components its indices point at, in its object's Components list - as DICE's files hold them (house_01_10x8,
    barrack, house_12_14x8: 112 of 112 links)."""
    by = {(i.attr('guid') or '').lower(): i for i in instances if i.tag == 'instance'}
    for i in by.values():
        links, comps = _field(i, 'PartLinks'), _field(i, 'Components')
        if links is None or comps is None:
            continue
        names = []
        for c in comps.children or ():
            target = by.get((c.attr('ref') or '').lower())
            n = _field(target, 'Name') if target is not None else None
            names.append(n.text if n is not None else None)
        fixed = 0
        for item in links.children or ():
            link = by.get((item.attr('ref') or '').lower())
            if link is None:
                continue
            for k in ('1', '2'):
                idx, name = _field(link, 'PartComponentIndex' + k), _field(link, 'PartComponentName' + k)
                if idx is None or name is None:
                    continue
                j = int(dbx.value(idx))
                want = names[j] if 0 <= j < len(names) and names[j] is not None else ''
                if name.text != want:
                    name.text = want
                    fixed += 1
        if fixed:
            report.append(f'derived {fixed} part link names from their indices')
