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


def _align(e, t, path, report, resolve=None):
    """e (BF1943) aligned with t (BFBC2): a new element. resolve(target) says whether a reference will resolve (see
    _lost); a field - or an array of references - holding one that will not takes the template's value instead
    (structs and arrays of structs are aligned field by field, so only the field holding it is replaced)."""
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
            new.append(_align(c, proto, f'{path}[{i}]', report, resolve) if proto is not None and c.kind == dbx.CONTAINER
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
            new.append(_align(arr, c, f'{path}.{n}', report, resolve))
            report.append(f'converted {path}.{single} -> {n} (one item)')
            continue
        if n in mine:
            new.append(_align(mine[n], c, f'{path}.{n}', report, resolve))
        else:
            new.append(copy.deepcopy(c))
            report.append(f'added {path}.{n} (BFBC2\'s value)')
    converted = {ONE_TO_ARRAY[n] for n in theirs if n in ONE_TO_ARRAY}
    for n in mine:
        if n not in theirs and n not in converted:
            report.append(f'left out {path}.{n} (BF1943 only)')
    # unnamed children BFBC2's template has none of (rare): kept as they are
    new += [c for c in (e.children or ()) if c.attr('name') is None and e.tag != 'array']
    return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, new)


# BF1943 types BFBC2's executable has no class for (2026-10-31): dropped by port(drop_types=NOT_IN_BFBC2)
NOT_IN_BFBC2 = {'GameSharedResources.AirRaidEffectsData', 'GameSharedResources.TeamVehicleProjectileEntityData',
                'GameSharedResources.CarHornComponentData', 'GameSharedResources.ScorePartComponentData'}


def port(data, templates, resolve=None, drop_untemplated=False, drop_types=()):
    """(BFBC2 .dbx bytes, report lines) for a BF1943 partition. resolve(partition): whether a referenced partition
    will exist (None: references kept). drop_untemplated: instances of a type the templates do not have (a BFBC2
    donor partition does not carry, e.g. BF1943's embedded PersistentWeapon) are left out, and references to them
    take the template's value; otherwise they are kept as BF1943 has them. drop_types: types left out whatever the
    templates (NOT_IN_BFBC2: the classes BFBC2 does not have), references to them handled the same way."""
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
        new.append(_align(inst, templates.by_type[t], t, report, res))
    out = dbx.DbxFile(f.strings, [dbx.Element(part.tag, part.kind, list(part.attributes), part.text, part.raw, new)]
                      + f.roots[1:])
    return dbx.write(out), report
