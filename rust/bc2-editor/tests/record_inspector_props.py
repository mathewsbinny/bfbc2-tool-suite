r"""Records what the Python Map Inspector showed of a sample of a level's objects, for the map editor's property tree
and link view (inspect_parity.rs): per sampled entity, its property tree as the inspector's Selection panel walked
it (Inspector.tree: a container's name and child count, a reference's field and target, a value's field and
dbx.value text, nested) and its links (Entity.refs, Inspector.referrers). Only digests are kept, not the game's data:
each list's length and the SHA-1 of its compact JSON (json.dumps, separators (',', ':'), ensure_ascii=False). One
gzip JSON file per level in tests\data\. Recorded once, before the Map Inspector was retired (round GR1); kept as
the reference the editor is compared with.

  python rust\bc2-editor\tests\record_inspector_props.py [level ...]     default: sp_01 mp_005 mp_009gr ons_mp_004

The sample: of every entity type the first, middle and last entity, every 50th static group member, and the
entities with the most references and referrers (5 each). Read only, as level.load is for a prepared level. Needs
the suite's Rust core module (rust\build_native.py), the game copy, and inspector.py (before GR1's retirement).
"""
import gzip
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..', '..'))
sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, os.path.join(SUITE, 'bfbc2-map-inspector'))
import inspector  # noqa: E402
from bfbc2core import dbx  # noqa: E402
from bfbc2core import level as levelmod  # noqa: E402

DEFAULT = ['sp_01', 'mp_005', 'mp_009gr', 'ons_mp_004']


def tree(ins, el) -> list:
    """Inspector.tree's rows: [name, 'c', count, children] for a container, [name, 'r', ref, target index or None]
    for a reference, [name, 'v', value] for a value."""
    rows = []
    for c in el.children or ():
        name = c.attr('name') or c.tag
        ref = c.attr('ref')
        if c.children is not None:
            rows.append([name, 'c', len(c.children), tree(ins, c)])
        elif ref is not None:
            t = ins.guids.get(ref.lower())
            rows.append([name, 'r', ref, None if t is None else t.index])
        else:
            rows.append([name, 'v', dbx.value(c)])
    return rows


def fields(rows: list) -> int:
    """How many fields a tree's rows hold, nested ones included."""
    return sum(1 + (fields(r[3]) if r[1] == 'c' else 0) for r in rows)


def digest(rows: list, count: int) -> list:
    """[count, SHA-1 of the rows' compact JSON]."""
    text = json.dumps(rows, separators=(',', ':'), ensure_ascii=False)
    return [count, hashlib.sha1(text.encode('utf-8')).hexdigest()]


def sample(lvl, ins) -> list[int]:
    picked: dict[int, None] = {}
    by_type: dict[str, list[int]] = {}
    for e in lvl.entities:
        by_type.setdefault(e.short_type, []).append(e.index)
    for ks in by_type.values():
        for k in (ks[0], ks[len(ks) // 2], ks[-1]):
            picked[k] = None
    members = [e.index for e in lvl.entities if e.group]
    for k in members[::50]:
        picked[k] = None
    most_refs = sorted(lvl.entities, key=lambda e: -len(e.refs()))[:5]
    most_referred = sorted(lvl.entities, key=lambda e: -len(ins.referrers.get(e.index, ())))[:5]
    for e in most_refs + most_referred:
        picked[e.index] = None
    return sorted(picked)


def record(lv: str) -> dict:
    lvl = levelmod.load(lv, log=lambda *_: None)
    ins = inspector.Inspector(lvl)
    out = []
    for k in sample(lvl, ins):
        e = lvl.entities[k]
        rows = tree(ins, e.element)
        refs = [list(r) for r in e.refs()]
        referrers = [[f, t.index] for f, t in ins.referrers.get(k, [])]
        out.append({'index': k, 'guid': e.guid, 'type': e.type, 'tree': digest(rows, fields(rows)),
                    'refs': digest(refs, len(refs)), 'referrers': digest(referrers, len(referrers))})
    return {'level': lvl.name, 'entities': len(lvl.entities), 'sample': out}


def main(argv: list[str]) -> int:
    out_dir = os.path.join(HERE, 'data')
    os.makedirs(out_dir, exist_ok=True)
    for lv in argv[1:] or DEFAULT:
        r = record(lv)
        path = os.path.join(out_dir, f'inspector_props_{lv}.json.gz')
        with gzip.GzipFile(path, 'wb', mtime=0) as fh:
            fh.write(json.dumps(r, separators=(',', ':')).encode('utf-8'))
        size = os.path.getsize(path)
        print(f'{lv}: {len(r["sample"])} of {r["entities"]} entities sampled -> {os.path.relpath(path, SUITE)} '
              f'({size:,} bytes)')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
