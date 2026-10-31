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


def _named(children):
    out = {}
    for c in children or ():
        n = c.attr('name')
        if n is not None and n not in out:
            out[n] = c
    return out


def _align(e, t, path, report):
    """e (BF1943) aligned with t (BFBC2): a new element."""
    if e.kind != dbx.CONTAINER or t.kind != dbx.CONTAINER:
        return e
    if e.tag == 'array':
        items = [c for c in (e.children or ())]
        proto = next((c for c in (t.children or ()) if c.kind == dbx.CONTAINER), None)
        new = []
        for i, c in enumerate(items):
            new.append(_align(c, proto, f'{path}[{i}]', report) if proto is not None and c.kind == dbx.CONTAINER
                       else c)
        return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, new)
    mine = _named(e.children)
    theirs = _named(t.children)
    new = []
    for c in t.children or ():
        n = c.attr('name')
        if n is None:
            continue
        if n in mine:
            new.append(_align(mine[n], c, f'{path}.{n}', report))
        else:
            new.append(copy.deepcopy(c))
            report.append(f'added {path}.{n} (BFBC2\'s value)')
    for n in mine:
        if n not in theirs:
            report.append(f'left out {path}.{n} (BF1943 only)')
    # unnamed children BFBC2's template has none of (rare): kept as they are
    new += [c for c in (e.children or ()) if c.attr('name') is None and e.tag != 'array']
    return dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, new)


def port(data, templates):
    """(BFBC2 .dbx bytes, report lines) for a BF1943 partition."""
    f = dbx.parse(data)
    report = []
    part = f.partition
    new = []
    for inst in part.children or ():
        if inst.tag != 'instance':
            new.append(inst)
            continue
        t = inst.attr('type')
        if t not in templates:
            report.append(f'type {t} ({inst.attr("guid")}): no BFBC2 template - kept as BF1943 has it')
            new.append(inst)
            continue
        new.append(_align(inst, templates.by_type[t], t, report))
    out = dbx.DbxFile(f.strings, [dbx.Element(part.tag, part.kind, list(part.attributes), part.text, part.raw, new)]
                      + f.roots[1:])
    return dbx.write(out), report
