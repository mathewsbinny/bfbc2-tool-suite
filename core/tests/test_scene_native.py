r"""The Rust scene (rust/bc2-scene, through bc2_native.scene_load) against the Python loader (core/bfbc2core/level.py
with every reader in Python): the same Level, entity by entity - guid, type, category, id, name, partition, transform
(bit for bit), blueprint, group, blueprint type, destructible, team, AI, the whole element tree -, every shape, mesh
(triangles bit for bit), the blueprints without collision, the notes, the height field, the terrain mesh and the
water; with and without geometry.

  python core\tests\test_scene_native.py [level ...]      default: sp_01 mp_005 mp_009gr ons_mp_004

Read only: a level that is not prepared, or whose terrain unpack is missing or stale, is skipped (level.load would
prepare or unpack it). Skipped (exit 0) when the module is not built or the game copy is not there.
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
from bfbc2core import collision, dbx, fbrb, level, paths, terrain, water  # noqa: E402

DEFAULT = ['sp_01', 'mp_005', 'mp_009gr', 'ons_mp_004']
READERS = (dbx, fbrb, collision, terrain, water)


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


def bits(a: Any) -> Any:
    return None if a is None else (np.asarray(a).shape, np.asarray(a).dtype.str, np.asarray(a).tobytes())


def entity_key(e: level.Entity) -> tuple[Any, ...]:
    return (e.index, e.guid, e.type, e.short_type, e.category, e.id, e.name, e.partition, bits(e.matrix), e.blueprint,
            e.group, e.blueprint_type, e.destructible, e.team, e.ai)


def compare(a: level.Level, b: level.Level) -> list[str]:
    """The differences between two loads of a level (empty when they are the same)."""
    out: list[str] = []
    if len(a.entities) != len(b.entities):
        out.append(f'{len(a.entities)} -> {len(b.entities)} entities')
    for x, y in zip(a.entities, b.entities):
        if entity_key(x) != entity_key(y):
            out.append(f'entity {x.index} {x.guid}: {entity_key(x)[:8]} -> {entity_key(y)[:8]}')
        elif x.element != y.element:
            out.append(f'entity {x.index} {x.guid}: element differs')
        if len(out) > 5:
            return out
    sa = [(s.entity, s.kind, bits(s.points), s.closed, s.height, s.field) for s in a.shapes]
    sb = [(s.entity, s.kind, bits(s.points), s.closed, s.height, s.field) for s in b.shapes]
    if sa != sb:
        out.append(f'shapes differ ({len(sa)} / {len(sb)})')
    if [(m.entity, bits(m.triangles)) for m in a.meshes] != [(m.entity, bits(m.triangles)) for m in b.meshes]:
        out.append(f'meshes differ ({len(a.meshes)} / {len(b.meshes)})')
    if a.missing_collision != b.missing_collision:
        out.append('blueprints without collision differ')
    if len(a.notes) != len(b.notes) or [n.split(':')[0] for n in a.notes] != [n.split(':')[0] for n in b.notes]:
        out.append(f'notes differ: {a.notes} / {b.notes}')
    ha, hb = a.heightfield, b.heightfield
    if (ha is None) != (hb is None) or (ha is not None and hb is not None and (
            [bits(ha.min), bits(ha.max), ha.size, bits(ha.raw), bits(ha.heights)]
            != [bits(hb.min), bits(hb.max), hb.size, bits(hb.raw), bits(hb.heights)])):
        out.append('height field differs')
    ta = None if a.terrain is None else [bits(x) for x in a.terrain]
    tb = None if b.terrain is None else [bits(x) for x in b.terrain]
    if ta != tb:
        out.append('terrain mesh differs')
    wa, wb = a.water, b.water
    if (wa is None) != (wb is None) or (wa is not None and wb is not None and (
            [bits(wa.vertices), bits(wa.triangles), wa.shader, wa.shaders]
            != [bits(wb.vertices), bits(wb.triangles), wb.shader, wb.shaders])):
        out.append('water differs')
    return out


def load(lv: str, root: str, native: bool, geometry: bool) -> tuple[level.Level, float]:
    level.use_native(native)
    for m in READERS:
        m.use_native(native)
    t = time.perf_counter()
    lvl = level.load(lv, root, log=lambda *_: None, geometry=geometry)
    return lvl, time.perf_counter() - t


def main(argv: list[str]) -> int:
    if level._with_scene(dbx._import_native()) is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    try:
        root = paths.find_opfor(None, start=_ROOT)
    except Exception:
        print('skipped: Project OPFOR BFBC2 not found')
        return 0
    levels = [a.lower() for a in argv[1:]] or DEFAULT
    failed = 0
    for lv in levels:
        why = ready(root, lv)
        if why:
            print(f'skip {lv}: {why} (level.load would write; load it once first)')
            continue
        for geometry in (False, True):
            a, ta = load(lv, root, False, geometry)
            b, tb = load(lv, root, True, geometry)
            diff = compare(a, b)
            failed += bool(diff)
            what = 'whole load' if geometry else 'entities only'
            print(f'{"FAIL" if diff else "ok  "} {lv} {what}: {len(b.entities)} entities, {len(b.shapes)} shapes, '
                  f'{len(b.meshes)} meshes; Python {ta:.2f} s, Rust {tb:.2f} s' + ''.join(f'\n     {d}' for d in diff))
    level.use_native(True)
    for m in READERS:
        m.use_native(True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
