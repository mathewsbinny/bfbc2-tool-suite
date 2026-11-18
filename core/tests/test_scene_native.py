r"""level.load through the Rust scene (rust/bc2-scene, bc2_native.scene_load) against what the retired Python loader
(core/bfbc2core/level.py with every reader in Python; round F1a, 2026-11-18) loaded, recorded before it was deleted
(scene_golden.json): the same Level, entity by entity - guid, type, category, id, name, partition, transform (bit for
bit), blueprint, group, blueprint type, destructible, team, AI, the whole element tree -, every shape, mesh
(triangles bit for bit), the blueprints without collision, the notes, the height field, the terrain mesh and the
water; with and without geometry.

  python core\tests\test_scene_native.py [level ...]      default: sp_01 mp_005 mp_009gr ons_mp_004 (the recorded ones)

Read only: a level that is not prepared, or whose terrain unpack is missing or stale, is skipped (level.load would
prepare or unpack it); so is one whose inputs changed since the recording. Skipped (exit 0) when the game copy is not
there; needs the module.
"""
import glob
import os
import sys
import time
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
sys.path.insert(0, HERE)
import goldens  # noqa: E402
from bfbc2core import level, native, paths  # noqa: E402

DEFAULT = ['sp_01', 'mp_005', 'mp_009gr', 'ons_mp_004']
GOLDEN = os.path.join(HERE, 'scene_golden.json')


def ready(root: str, lv: str) -> str | None:
    """Why level.load would write for lv (prepare, unpack), or None when it only reads."""
    base = os.path.join(root, 'work', 'levels', lv)
    if not os.path.isdir(os.path.join(base, 'dist')):
        return 'not prepared'
    for top in (os.path.join(root, 'game', 'Dist', 'win32', 'levels'), os.path.join(root, 'game', 'Package', 'levels')):
        for d in glob.glob(os.path.join(top, '*')):
            b = os.path.join(d, 'terrain-00.fbrb')
            if os.path.basename(d).lower() == lv and os.path.isfile(b):
                st = os.stat(b)
                stamp = os.path.join(base, 'terrain', '.source.stamp')
                if not os.path.isfile(stamp) or open(stamp, encoding='ascii').read().strip() != f'{st.st_size} {st.st_mtime_ns}':
                    return 'terrain unpack missing or stale'
                return None
    return None


def inputs(root: str, lv: str) -> str:
    """The level's partition files and terrain stamp, as a digest: a level re-prepared since the recording differs."""
    base = os.path.join(root, 'work', 'levels', lv)
    parts = sorted(glob.glob(os.path.join(base, 'pkg', 'levels', '*.dbx')) + glob.glob(os.path.join(base, 'dist', 'levels', '*.dbx')))
    stamp = os.path.join(base, 'terrain', '.source.stamp')
    return goldens.rh(([(os.path.relpath(p, base), goldens.file_sha(p)) for p in parts],
                       open(stamp, encoding='ascii').read().strip() if os.path.isfile(stamp) else None), 16)


def bits(a: Any) -> Any:
    return None if a is None else (np.asarray(a).shape, np.asarray(a).dtype.str, goldens.h(np.asarray(a).tobytes(), 16))


def entity_key(e: level.Entity) -> tuple[Any, ...]:
    return (e.index, e.guid, e.type, e.short_type, e.category, e.id, e.name, e.partition, bits(e.matrix), e.blueprint,
            e.group, e.blueprint_type, e.destructible, e.team, e.ai)


def parts(a: level.Level) -> dict[str, str]:
    """The Level's pieces, each as a digest (a difference is reported by piece)."""
    ha, wa = a.heightfield, a.water
    return {
        'entities': goldens.rh([entity_key(e) for e in a.entities], 16),
        'elements': goldens.rh([goldens.rh(e.element, 16) for e in a.entities], 16),
        'shapes': goldens.rh([(s.entity, s.kind, bits(s.points), s.closed, s.height, s.field) for s in a.shapes], 16),
        'meshes': goldens.rh([(m.entity, bits(m.triangles)) for m in a.meshes], 16),
        'missing': goldens.rh(a.missing_collision, 16),
        'notes': goldens.rh([n.split(':')[0] for n in a.notes], 16),
        'heightfield': goldens.rh(None if ha is None else [bits(ha.min), bits(ha.max), ha.size, bits(ha.raw), bits(ha.heights)], 16),
        'terrain': goldens.rh(None if a.terrain is None else [bits(x) for x in a.terrain], 16),
        'water': goldens.rh(None if wa is None else [bits(wa.vertices), bits(wa.triangles), wa.shader, wa.shaders], 16),
    }


def load(lv: str, root: str, geometry: bool) -> tuple[level.Level, float]:
    t = time.perf_counter()
    lvl = level.load(lv, root, log=lambda *_: None, geometry=geometry)
    return lvl, time.perf_counter() - t


def main(argv: list[str]) -> int:
    native.require('the Rust scene', 'scene_load')
    try:
        root = paths.find_opfor(None, start=_ROOT)
    except Exception:
        print('skipped: Project OPFOR BFBC2 not found')
        return 0
    rec = goldens.load(GOLDEN)['levels']
    levels = [a.lower() for a in argv[1:]] or DEFAULT
    failed = 0
    for lv in levels:
        why = ready(root, lv) or (None if lv in rec else 'not in the recording')
        if why is None and rec[lv]['inputs'] != inputs(root, lv):
            why = 're-prepared since the recording'
        if why:
            print(f'skip {lv}: {why}')
            continue
        for geometry in (False, True):
            b, tb = load(lv, root, geometry)
            want = rec[lv]['whole' if geometry else 'entities only']
            got = parts(b)
            diff = [k for k in want if want[k] != got.get(k)]
            failed += bool(diff)
            what = 'whole load' if geometry else 'entities only'
            print(f'{"FAIL" if diff else "ok  "} {lv} {what}: {len(b.entities)} entities, {len(b.shapes)} shapes, '
                  f'{len(b.meshes)} meshes - the Python loader\'s Level; {tb:.2f} s' + (f'\n     differ: {diff}' if diff else ''))
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
