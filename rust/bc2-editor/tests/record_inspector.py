r"""Records what the Python Map Inspector lists for a level, for the map editor's outliner test (inspector_parity.rs):
every entity of level.load(lv) (geometry=True, as inspector.py loads it) in order - index, GUID, category, the
inspector's layer (inspector.layer_name), team and position (the transform's translation row) - and the inspector's
count per layer (Inspector.counts). One gzip JSON file per level in tests\data\.

  python rust\bc2-editor\tests\record_inspector.py [level ...]     default: sp_01 mp_005 mp_009gr ons_mp_004

Read only, as level.load is for a prepared level with a current terrain unpack (it would unpack otherwise). Needs the
suite's Rust core module (rust\build_native.py) and the game copy.
"""
import gzip
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
from bfbc2core import level as levelmod  # noqa: E402

DEFAULT = ['sp_01', 'mp_005', 'mp_009gr', 'ons_mp_004']


def record(lv: str) -> dict:
    lvl = levelmod.load(lv, log=lambda *_: None)
    entities = []
    counts: dict[str, int] = {}
    for e in lvl.entities:
        layer = inspector.layer_name(e)
        counts[layer] = counts.get(layer, 0) + 1
        pos = None if e.position is None else [float(v) for v in e.position]
        entities.append([e.index, e.guid, e.category, layer, e.team, pos])
    return {'level': lvl.name, 'entities': entities, 'counts': counts}


def main(argv: list[str]) -> int:
    out_dir = os.path.join(HERE, 'data')
    os.makedirs(out_dir, exist_ok=True)
    for lv in argv[1:] or DEFAULT:
        r = record(lv)
        path = os.path.join(out_dir, f'inspector_{lv}.json.gz')
        with gzip.GzipFile(path, 'wb', mtime=0) as fh:
            fh.write(json.dumps(r, separators=(',', ':')).encode('utf-8'))
        print(f'{lv}: {len(r["entities"])} entities -> {os.path.relpath(path, SUITE)}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
