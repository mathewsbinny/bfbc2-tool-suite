r"""Round OR14's parity check of the side silhouette: bc2_native.pictures_side_silhouette (bc2_convert::pictures::side)
against Project OPFOR's Python `type95_icon.silhouette` (NumPy + Pillow's ImageDraw.polygon), and
bc2_native.pictures_polygon (bc2_convert::pictures::polygon) against Pillow's polygon fill.

    python rust\parity_silhouette.py [--reference <type95_icon.py>] [--opfor <folder>] [--cache <meshview index>]
                                     [--polys N] [--meshes N] [--seed S]

--reference: the Python twin (default: the OPFOR checkout's src\tools\ww2\type95_icon.py; after OPFOR switched to the
Rust, `git show <commit before>:src/tools/ww2/type95_icon.py > file`). Only its `silhouette` and the constants it
reads are taken (by `ast`), and run with a stand-in `meshview` that hands it the parts this script loads or makes.

1. Pillow's polygon fill: N random polygons (triangles mostly, n-gons, fractional / whole / half-pixel coordinates,
   on and off the picture, negative, degenerate) drawn in batches on random 'L' pictures, Pillow vs Rust.
2. Synthetic meshes: N seeded random meshes (random, degenerate and zero-area triangles, coincident vertices, faces
   with equal depth keys, vertices on pixel centres and half pixels, skip / detail parts) at random sizes and ss.
3. The game copy's meshes (skipped without one; read only, through meshview's index): the Type 95 rowboat at the
   defaults and at other sizes / ss, every mesh onslaught\console_icons.py draws (its ICONS / SPARE tables), the
   VEHICLES list at three sizes, and --vehicles seeded vehicle meshes of the index at one size / ss each.

Every picture must be byte for byte the same. Where Python and Rust differ only because NumPy's default argsort
orders equal far-to-near keys its own way, pictures_side_ties says how many output pixels such an order could
change; the line reports it. Exits 1 on a difference, 0 otherwise.
"""
from __future__ import annotations

import argparse
import ast
import os
import sys
import time
import types
from typing import Any

import numpy as np
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
sys.path[:0] = [os.path.join(SUITE, 'core'), os.path.join(SUITE, 'bfbc2-mesh-viewer'),
                os.path.join(SUITE, 'bfbc2-shader-tools')]
from bfbc2core import native  # noqa: E402

# the vehicle meshes of part 3 besides the rowboat and console_icons' (any missing one is said and skipped)
VEHICLES = [
    'objects/vehicles/sea/pbl/pbl_mesh', 'objects/vehicles/sea/pwc/pwc_mesh',
    'objects/vehicles/vietnam/pbr_mkii/pbr_mkii_mesh', 'objects/vehicles/sea/rustyboatnew_02/rustyboatnew_02_mesh',
    'objects/vehicles/land/humvee/humvee_mesh', 'objects/vehicles/land/m1a2/m1a2_mesh',
    'objects/vehicles/land/t90/t90_mesh', 'objects/vehicles/land/quadbike/quadbike_mesh',
    'objects/vehicles/air/ah64/ah64_mesh', 'objects/vehicles/air/mi24/mi24_mesh',
    'objects/vehicles/vietnam/huey/huey_mesh', 'objects/vehicles/beach/jap_zero_fighter/zero_fighter_mesh',
]
SIZES = [((78, 36), 8), ((64, 24), 4), ((104, 40), 6), ((40, 20), 3), ((78, 36), 12), ((33, 17), 1)]


def load_reference(path: str) -> types.ModuleType:
    """type95_icon's `silhouette` and its constants, without the module's imports."""
    tree = ast.parse(open(path, encoding='utf-8').read(), path)
    keep: list[ast.stmt] = []
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name == 'silhouette':
            keep.append(node)
        elif isinstance(node, ast.Assign):
            names = [n.id for t in node.targets for n in (t.elts if isinstance(t, ast.Tuple) else [t])
                     if isinstance(n, ast.Name)]
            if set(names) & {'TOP', 'BOTTOM', 'SIZE', 'MESH'}:
                keep.append(node)
    mod = types.ModuleType('type95_icon_reference')
    mod.__file__ = path
    mod.__dict__['np'] = np
    exec(compile(ast.Module(body=keep, type_ignores=[]), path, 'exec'), mod.__dict__)
    if not hasattr(mod, 'silhouette'):
        raise SystemExit(f'{path}: no silhouette')
    return mod


class StableNumpy(types.ModuleType):
    """numpy, but argsort is kind='stable' (equal far-to-near keys in the faces' order, as the Rust draws them)."""

    def __init__(self) -> None:
        super().__init__('numpy_stable')

    def __getattr__(self, name: str) -> Any:
        return getattr(np, name)

    @staticmethod
    def argsort(a: Any, *args: Any, **kw: Any) -> Any:
        return np.argsort(a, kind='stable')


def stable_twin(ref: types.ModuleType) -> types.ModuleType:
    """The reference's silhouette with StableNumpy for np (same code, other globals)."""
    mod = types.ModuleType('type95_icon_reference_stable')
    mod.__dict__.update(ref.__dict__)
    mod.__dict__['np'] = StableNumpy()
    f = ref.__dict__['silhouette']
    mod.__dict__['silhouette'] = types.FunctionType(f.__code__, mod.__dict__, f.__name__, f.__defaults__,
                                                    f.__closure__)
    return mod


class FakeMeshview:
    """meshview's load_mesh for the reference: hands back the parts given."""

    def __init__(self) -> None:
        self.parts: list[dict[str, Any]] = []

    def load_mesh(self, _ix: Any, _name: str, _lod: int = 0) -> list[dict[str, Any]]:
        return self.parts


def rust_parts(parts: list[dict[str, Any]]) -> list[tuple[str, bytes, bytes]]:
    """What pictures_side_silhouette takes from meshview.load_mesh's parts."""
    return [(p['name'], np.ascontiguousarray(p['positions'], '<f4').tobytes(),
             np.ascontiguousarray(p['indices'], '<u4').tobytes()) for p in parts]


def run_python(ref: types.ModuleType, fake: FakeMeshview, parts: list[dict[str, Any]], size: tuple[int, int],
               ss: int, skip: tuple[str, ...], detail: tuple[str, ...]) -> tuple[np.ndarray | str, float]:
    fake.parts = parts
    t = time.perf_counter()
    try:
        with np.errstate(all='ignore'):
            out: np.ndarray | str = ref.silhouette(None, size, ss, 'mesh', skip, detail)
    except Exception as e:  # noqa: BLE001  (the error is the result compared)
        out = f'{type(e).__name__}'
    return out, time.perf_counter() - t


def run_rust(nat: Any, parts: list[dict[str, Any]], size: tuple[int, int], ss: int, skip: tuple[str, ...],
             detail: tuple[str, ...]) -> tuple[np.ndarray | str, float]:
    t = time.perf_counter()
    try:
        b = nat.pictures_side_silhouette(rust_parts(parts), size[0], size[1], ss, list(skip), list(detail))
        out: np.ndarray | str = np.frombuffer(b, np.uint8).reshape(size[1], size[0], 4)
    except ValueError as e:
        out = f'ValueError ({e})'
    return out, time.perf_counter() - t


class Tally:
    def __init__(self, name: str) -> None:
        self.name = name
        self.same = self.differ = self.both_refused = self.decided = 0
        self.tie_explained = 0
        self.tpy = self.trs = 0.0
        self.notes: list[str] = []

    def line(self) -> str:
        total = self.same + self.differ + self.both_refused + self.decided + self.tie_explained
        s = (f'{self.name}: {self.same + self.both_refused} of {total} identical ({self.both_refused} refused by both)'
             f', {self.tie_explained} differ only by the order of equal depth keys (identical to the Python with a '
             f"stable argsort), {self.differ} different")
        if self.decided:
            s += f', {self.decided} decided (Python draws, Rust refuses: a mesh of no length / non-finite)'
        return s + f'; Python {self.tpy:.2f} s, Rust {self.trs:.2f} s'


def compare(tally: Tally, nat: Any, ref: types.ModuleType, fake: FakeMeshview, label: str,
            parts: list[dict[str, Any]], size: tuple[int, int], ss: int, skip: tuple[str, ...],
            detail: tuple[str, ...], verbose: bool = False) -> None:
    py, tp = run_python(ref, fake, parts, size, ss, skip, detail)
    rs, tr = run_rust(nat, parts, size, ss, skip, detail)
    tally.tpy += tp
    tally.trs += tr
    ties = None
    if isinstance(rs, np.ndarray):
        ties = nat.pictures_side_ties(rust_parts(parts), size[0], size[1], ss, list(skip), list(detail))
    if isinstance(py, np.ndarray) and isinstance(rs, np.ndarray) and py.shape == rs.shape and (py == rs).all():
        tally.same += 1
        res = 'identical'
    elif isinstance(py, str) and isinstance(rs, str):
        tally.both_refused += 1
        res = f'refused by both ({py} / {rs})'
    elif isinstance(rs, str) and ('no length' in rs or 'not finite' in rs):
        tally.decided += 1
        res = f'decided: {rs}'
    else:
        npx = int((py != rs).any(2).sum()) if isinstance(py, np.ndarray) and isinstance(rs, np.ndarray) \
            and py.shape == rs.shape else -1
        st, _ = run_python(ref.stable, fake, parts, size, ss, skip, detail)
        if isinstance(st, np.ndarray) and isinstance(rs, np.ndarray) and st.shape == rs.shape and (st == rs).all():
            tally.tie_explained += 1
            res = f'tie order: {npx} pixels differ from the default argsort, identical to the stable one'
        else:
            tally.differ += 1
            res = f'DIFFERENT ({npx} pixels; Python {py if isinstance(py, str) else "picture"}, Rust ' \
                  f'{rs if isinstance(rs, str) else "picture"})'
            tally.notes.append(f'  {label}: {res}; ties {ties}')
    if verbose:
        print(f'  {label} {size} ss {ss}: {res}; faces with a tied key {ties[0] if ties else "-"}, shade pixels a tie '
              f'decides {ties[1] if ties else "-"}, output pixels it could change {ties[2] if ties else "-"}; '
              f'Python {tp:.3f} s, Rust {tr:.3f} s')


# ------------------------------------------------------------------------------------------------ 1. polygons

def coord(rng: np.random.Generator, n: int, lim: float) -> np.ndarray:
    kind = rng.integers(0, 5, n)
    v = rng.uniform(-0.3 * lim, 1.3 * lim, n)
    v = np.where(kind == 1, np.round(v), v)                    # whole pixels
    v = np.where(kind == 2, np.floor(v) + 0.5, v)              # half pixels
    v = np.where(kind == 3, np.round(v * 8) / 8, v)            # eighths (the silhouette's 8 x supersampling)
    return v


def polygons(nat: Any, rng: np.random.Generator, count: int) -> tuple[int, int]:
    same = total = 0
    left = count
    while left > 0:
        w, h = int(rng.integers(1, 90)), int(rng.integers(1, 90))
        img = Image.fromarray(rng.integers(0, 256, (h, w), dtype=np.uint8), 'L')
        start = np.asarray(img).tobytes()
        d = ImageDraw.Draw(img)
        batch = []
        for _ in range(min(left, int(rng.integers(1, 40)))):
            n = 3 if rng.random() < 0.75 else int(rng.integers(2, 9))
            scale = rng.choice([1.0, 1.0, 1.0, 0.1, 5.0])
            xs, ys = coord(rng, n, float(w * scale)), coord(rng, n, float(h * scale))
            if rng.random() < 0.1:                             # a repeated vertex / a flat polygon
                xs[-1], ys[-1] = xs[0], ys[0]
            if rng.random() < 0.1:
                ys[:] = ys[0]
            xy = [(float(x), float(y)) for x, y in zip(xs, ys)]
            ink = int(rng.integers(0, 256))
            d.polygon(xy, fill=ink)
            batch.append((xy, ink))
        left -= len(batch)
        got = nat.pictures_polygon(start, w, h, batch)
        total += len(batch)
        if got == np.asarray(img).tobytes():
            same += len(batch)
        else:                                                   # find the first polygon that differs
            img2 = Image.frombytes('L', (w, h), start)
            d2 = ImageDraw.Draw(img2)
            for k, (xy, ink) in enumerate(batch):
                d2.polygon(xy, fill=ink)
                if nat.pictures_polygon(start, w, h, batch[:k + 1]) != np.asarray(img2).tobytes():
                    print(f'  polygon DIFFERENT on {w} x {h}: {xy} ink {ink}')
                    same += k
                    break
    return same, total


# ------------------------------------------------------------------------------------------------ 2. synthetic

def synthetic(rng: np.random.Generator) -> tuple[list[dict[str, Any]], tuple[int, int], int, tuple[str, ...],
                                                  tuple[str, ...]]:
    w, h = int(rng.integers(3, 100)), int(rng.integers(3, 50))
    ss = int(rng.choice([1, 2, 3, 4, 5, 8, 8, 8, 12]))
    parts: list[dict[str, Any]] = []
    for p in range(int(rng.integers(1, 4))):
        nv = int(rng.integers(3, 60))
        style = rng.integers(0, 4)
        if style == 0:
            pos = rng.normal(0, 1, (nv, 3))
        elif style == 1:                                       # a grid: coincident / aligned vertices, equal keys
            pos = rng.integers(-4, 5, (nv, 3)).astype(float) / 2
        elif style == 2:                                       # a flat side (x constant): every key equal
            pos = rng.normal(0, 1, (nv, 3))
            pos[:, 0] = 0.25
        else:                                                  # big values
            pos = rng.normal(0, 1, (nv, 3)) * rng.choice([1e-3, 30.0, 1000.0])
        nt = int(rng.integers(1, 80))
        tris = rng.integers(0, nv, (nt, 3))
        if rng.random() < 0.3:                                 # degenerate: a repeated corner
            tris[: nt // 3, 2] = tris[: nt // 3, 0]
        if rng.random() < 0.2:                                 # coincident vertices
            pos[1] = pos[0]
        name = ['Hull', 'Planks', 'Engine_Metal', 'Coil_Rope'][int(rng.integers(0, 4))] + f'_{p}'
        parts.append({'name': name, 'positions': pos.astype(np.float32), 'indices': tris.astype(np.int64)})
    if rng.random() < 0.4:                                     # vertices on pixel centres / half pixels
        allp = np.concatenate([q['positions'] for q in parts]).astype(float)
        zmin, zmax, ymin = allp[:, 2].min(), allp[:, 2].max(), allp[:, 1].min()
        if zmax > zmin:
            step = (zmax - zmin) / ((w - 2) * ss) if w > 2 else 1.0
            for q in parts:
                q['positions'][:, 2] = (zmin + np.round((q['positions'][:, 2] - zmin) / step * 2) / 2 * step)
                q['positions'][:, 1] = (ymin + np.round((q['positions'][:, 1] - ymin) / step * 2) / 2 * step)
    skip = tuple(s for s in ('_Rope',) if rng.random() < 0.5) or (('Rope_0',) if rng.random() < 0.2 else ())
    detail = ('_Metal',) if rng.random() < 0.7 else ('_Metal_1', 'Hull_0')
    for q in parts:                                            # some names end exactly with the endings
        if 'Metal' in q['name'] and rng.random() < 0.5:
            q['name'] = q['name'].split('_')[0] + '_Metal'
        if 'Rope' in q['name'] and rng.random() < 0.5:
            q['name'] = 'Coil_Rope'
    return parts, (w, h), ss, skip, detail


# ------------------------------------------------------------------------------------------------ 3. game meshes

def console_table(opfor: str) -> list[tuple[str, str, tuple[str, ...], tuple[str, ...]]]:
    """(label, mesh, skip, detail) of console_icons.py's ICONS and SPARE (read by ast)."""
    path = os.path.join(opfor, 'src', 'tools', 'onslaught', 'console_icons.py')
    if not os.path.isfile(path):
        return []
    tree = ast.parse(open(path, encoding='utf-8').read(), path)
    out = []
    for node in tree.body:
        if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name):
            name = node.targets[0].id
            if name == 'ICONS':
                for rows in ast.literal_eval(node.value).values():
                    out += [(r[0], r[1], tuple(r[2]), tuple(r[3])) for r in rows]
            elif name == 'SPARE':
                d = ast.literal_eval(ast.unparse(node.value).replace('LIKE', "'PBLB'"))
                out += [(k, v[0], tuple(v[1]), tuple(v[2])) for k, v in d.items()]
    return out


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--opfor', default=os.environ.get('OPFOR_ROOT') or os.path.join(os.path.dirname(SUITE),
                                                                                   'project-opfor-bfbc2'))
    ap.add_argument('--reference')
    ap.add_argument('--game', help='the folder holding game\\ (default: --opfor; read only)')
    ap.add_argument('--cache', help="meshview's index file (default: the game folder's work\\tools_out)")
    ap.add_argument('--polys', type=int, default=5000)
    ap.add_argument('--meshes', type=int, default=300)
    ap.add_argument('--vehicles', type=int, default=40, help="the index's vehicle meshes drawn besides VEHICLES")
    ap.add_argument('--seed', type=int, default=20261127)
    ap.add_argument('-v', '--verbose', action='store_true')
    ap.add_argument('--any-module', action='store_true',
                    help="use core\\bfbc2core's module even when its fingerprint is another checkout state's")
    args = ap.parse_args(argv[1:])
    if args.any_module:                                       # meshview's index takes it too
        import importlib
        nat: Any = importlib.import_module('bfbc2core.bc2_native')
        native._state.clear()
        native._state.update(module=nat, status='ok', where='core/bfbc2core (--any-module)')
    else:
        nat = native.require('the side silhouette parity', 'pictures_side_silhouette')
    ref_path = args.reference or os.path.join(args.opfor, 'src', 'tools', 'ww2', 'type95_icon.py')
    if not os.path.isfile(ref_path):
        print(f'skipped: no Python twin ({ref_path}: give --reference or --opfor)')
        return 0
    ref = load_reference(ref_path)
    fake = FakeMeshview()
    ref.__dict__['meshview'] = fake
    ref.__dict__['stable'] = stable_twin(ref)
    rng = np.random.default_rng(args.seed)
    bad = 0

    t = time.perf_counter()
    same, total = polygons(nat, rng, args.polys)
    print(f"Pillow's polygon fill: {same} of {total} polygons identical ({time.perf_counter() - t:.1f} s)")
    bad += total - same

    syn = Tally('synthetic meshes')
    for k in range(args.meshes):
        parts, size, ss, skip, detail = synthetic(rng)
        compare(syn, nat, ref, fake, f'synthetic {k}', parts, size, ss, skip, detail, verbose=args.verbose)
    print(syn.line())
    print('\n'.join(syn.notes))
    bad += syn.differ

    game = args.game or args.opfor
    if not os.path.isdir(os.path.join(game, 'game')):
        print(f'game meshes: skipped (no game copy in {game})')
        return 1 if bad else 0
    import meshview
    ix = meshview.Index(game, cache=args.cache) if args.cache else meshview.Index(game)
    real = Tally('game meshes')

    def mesh(name: str) -> list[dict[str, Any]] | None:
        try:
            return [{'name': p['name'], 'positions': p['positions'], 'indices': p['indices']}
                    for p in meshview.load_mesh(ix, name, 0)]
        except meshview.ViewError as e:
            print(f'  {name}: not loaded ({e})')
            return None

    jobs: list[tuple[str, str, tuple[str, ...], tuple[str, ...], tuple[int, int], int]] = []
    jobs.append(('Type 95 (defaults)', ref.MESH, ('_Rope',), ('_Metal',), ref.SIZE, 8))
    for label, m, skip, detail in console_table(args.opfor):
        jobs.append((f'console_icons {label}', m, skip, detail, (78, 36), 8))
    for size, ss in SIZES:
        jobs.append(('Type 95', ref.MESH, ('_Rope',), ('_Metal',), size, ss))
    found = sorted(k[:-4] for k in ix.where if k.startswith('objects/vehicles/') and k.endswith('_mesh.res')
                   and 'cockpit' not in k and k[:-4] not in VEHICLES)
    pick = [found[int(i)] for i in sorted(rng.choice(len(found), min(args.vehicles, len(found)), replace=False))]
    for k, v in enumerate(VEHICLES + pick):
        if ix.locate(v + '.res') is None:
            print(f'  {v}: not in the game copy - skipped')
            continue
        for size, ss in (SIZES[:3] if k < len(VEHICLES) else [SIZES[k % len(SIZES)]]):
            jobs.append((v.rsplit('/', 1)[1], v, (), ('_Metal',), size, ss))
    loaded: dict[str, list[dict[str, Any]] | None] = {}
    for label, m, skip, detail, size, ss in jobs:
        if m not in loaded:
            loaded[m] = mesh(m)
        got = loaded[m]
        if got is None:
            continue
        compare(real, nat, ref, fake, label, got, size, ss, skip, detail, verbose=True)
    print(real.line())
    print('\n'.join(real.notes))
    bad += real.differ
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
