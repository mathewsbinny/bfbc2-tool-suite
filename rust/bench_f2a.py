r"""Round F2a timings (rust/BENCHMARKS.md "F2a"): each workload with the Rust core and with its Python reference, in
one process, warm (best of 3).

  python rust\bench_f2a.py

Workloads: BF1943's Wake Island and Coral Sea terrain resources converted (heightfield, material map, visual
terrain, visual water) and BFBC2's Xbox mp_005's; a console shader database's part layouts; a 1024 x 1024 picture
encoded as each kind; the M1 Garand's skinned LODs read, decoded and written; an animation's tracks shifted and
five tracks added.
"""
import os
import sys
import time
from collections.abc import Callable
from functools import partial
from typing import Any

import numpy as np

SUITE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, SUITE)
import contextlib

import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'model-tools', 'anim-tools', 'mesh-viewer', 'bf1943-import')


def best(fn: Callable[[], Any]) -> float:
    out = []
    for _ in range(3):
        t = time.perf_counter()
        fn()
        out.append(time.perf_counter() - t)
    return min(out)


def row(name: str, rust: Callable[[], Any], python: Callable[[], Any]) -> None:
    r, p = best(rust), best(python)
    print(f'| {name} | {p:.3f} s | {r:.3f} s | {p / max(r, 1e-9):.1f}x |', flush=True)


def main() -> int:
    import bf1943import
    import granny
    import mesh
    import meshview
    import shaderdb
    import skinmesh
    import terrain
    import texwrite
    src = bf1943import.Source(bf1943import.default_source())
    print('| workload | Python | Rust | |\n|---|---:|---:|---:|')
    for lv in ('wake_island_s', 'coral_sea'):
        items = [(k, src.data(k)) for k in sorted(src.entries) if k.startswith(f'terrains/{lv}/')]
        def hf(b: bytes, r: bool) -> Any:
            return (terrain.heightfield_reference if r else terrain.heightfield)(b, 'bf1943')

        def mm(b: bytes, r: bool) -> Any:
            return (terrain.material_map_reference if r else terrain.material_map)(b)

        def vt(b: bytes, r: bool) -> Any:
            return (terrain.visual_terrain_reference if r else terrain.visual_terrain)(b)

        def vw(b: bytes, r: bool) -> Any:
            return (terrain.visual_water_reference if r else terrain.visual_water)(b)
        conv: dict[str, Callable[[bytes, bool], Any]] = {'heightfield-000.res': hf, 'materialmap.res': mm,
                                                         'visual.res': vt, 'water.res': vw}
        jobs = [(f, b) for k, b in items for s, f in conv.items() if k.endswith(s)]
        def run(rf: bool, jobs: list[tuple[Callable[[bytes, bool], Any], bytes]] = jobs) -> Any:
            return [f(b, rf) for f, b in jobs]
        row(f'{lv}: {len(jobs)} terrain resources converted', partial(run, False), partial(run, True))
    dbk = next(k for k, (_, e) in sorted(src.entries.items()) if e.get('type') == 'Dx9ShaderDatabase'
               and 'wake' in k.lower())
    db = src.data(dbk)
    row(f'console_layouts of {dbk} ({len(db) >> 20} MB)', lambda: mesh.console_layouts(db), lambda: mesh.console_layouts_reference(db))
    yy, xx = np.mgrid[0:1024, 0:1024]
    img = np.dstack([(xx // 4) % 256, (yy // 4) % 256, ((xx + yy) // 8) % 256, (xx * yy) % 256]).astype(np.uint8)
    for kind, pic in (('color', img[..., :3]), ('normal', img[..., :3]), ('dxt5', img), ('color', img)):
        def enc(on: bool, kind: str = kind, pic: np.ndarray = pic) -> Any:
            texwrite.use_native(on)
            try:
                return texwrite.texture(pic, kind)
            finally:
                texwrite.use_native(True)
        row(f'texwrite.texture 1024 x 1024 {kind} ({pic.shape[2]} channels, mips)', partial(enc, True),
            partial(enc, False))
    ix = meshview.Index(shaderdb.find_opfor(None))
    name = 'Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh'
    lods = []
    for k in range(4):
        b = ix.read(f'{name}_lod{k}_data.res')
        if b is None:
            break
        m = mesh.parse_reference(b, False)
        lods.append((b, {p['name'].decode(): ix.parts[f'{name}_lod{k}/{p["name"].decode()}/standard'.lower()] for p in m['parts']}))

    def skin(on: bool) -> None:
        skinmesh.use_native(on)
        try:
            for b, lay in lods:
                s = skinmesh.SkinnedLod(b, lay)
                for p in s.parts:
                    for u in range(8):
                        s.decode(p, u)
                s.to_bytes()
                s.bounds()
        finally:
            skinmesh.use_native(True)
    row(f'SkinnedLod: {len(lods)} M1 Garand LODs read, decoded, written', lambda: skin(True), lambda: skin(False))
    anim = ix.read('animations/weapons/handheld/m1911a1/3p/standundeploy.res')
    donor = ix.read('animations/weapons/handheld/m1garand/3p/standundeploy.res')

    def shifts(on: bool) -> None:
        granny.use_native(on)
        try:
            g = granny.Granny(anim)
            for an in granny.animations(g):
                for tg in an['TrackGroups']:
                    for tr in tg['TransformTracks']:
                        cd = tr['PositionCurve']['CurveData']
                        if cd:
                            with contextlib.suppress(granny.GrannyError):
                                granny.shift_position(g, cd, [0.1, 0.2, 0.3], True)
            g.to_bytes()
        finally:
            granny.use_native(True)

    def adds(on: bool) -> None:
        granny.use_native(on)
        try:
            gd = granny.Granny(donor)
            dt = granny.animations(gd)[0]['TrackGroups'][0]['TransformTracks'][:3]
            spec = [{'name': f'zz_{t["Name"]}', 'position': ('copy', t['PositionCurve']['CurveData']),
                     'orientation': ('copy', t['OrientationCurve']['CurveData'])} for t in dt]
            granny.add_tracks(anim, spec + [{'name': 'aa', 'position': None, 'orientation': None}], donor)
        finally:
            granny.use_native(True)
    row('granny: every track of the M1911 3p undeploy shifted (ramp), to_bytes', lambda: shifts(True), lambda: shifts(False))
    row('granny.add_tracks: four tracks onto the M1911 3p undeploy', lambda: adds(True), lambda: adds(False))
    return 0


if __name__ == '__main__':
    sys.exit(main())
