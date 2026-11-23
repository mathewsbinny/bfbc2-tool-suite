"""A BC2 level as the suite's tools see it: entities with transforms and sources, collision meshes, terrain.

    lvl = load('sp_01')                       # by level name, from Project OPFOR BFBC2's game copy
    lvl.entities                              # every instance of the level partition (Entity)
    lvl.meshes                                # world-space collision triangles per placed object / group member
    lvl.terrain                               # (vertices, triangles): the level's height field (terrain.py),
                                              # or NavKit's in-game samples when it cannot be read, or None
    lvl.heightfield                           # terrain.Heightfield, for exact heights (height(x, z))
    lvl.shapes                                # outlines: areas (capture, combat, order areas) and routes (Shape)
    lvl.water                                 # water.Water: surface triangles and each one's shader (water.py)

Data comes from the level as NavKit prepares it (OPFOR's work\\levels\\<level>\\dist and pkg, and the shared
data in work\\navkit\\common\\); a level that is not prepared yet is prepared on first use (offline, no game).

The level is read by the suite's Rust core (rust/, module bc2_native: bc2_scene::scene): the entities, blueprints,
teams and outlines, the collision index and each blueprint's triangles, the terrain and the water at once on all
cores; this module finds the files, prepares and unpacks (tools and writes), and keeps its own element trees of the
level partition and the static groups for Entity.element. The Python loader was retired on 2026-11-18 (round F1a,
rust/BENCHMARKS.md "Retired Python"); the Levels it loaded (SP_01, MP_005, MP_009GR, ONS_MP_004, with and without
geometry) are kept as recorded digests (core/tests/scene_golden.json, compared by test_scene_native.py). Without the
module load raises bfbc2core.native.NativeMissing saying how to get it.
"""
import glob
import os
import sys
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass, field
from types import ModuleType
from typing import TYPE_CHECKING, Any, cast

import numpy as np

from . import atomicio, collision, dbx, paths, terrain, water
from .water import Water

if TYPE_CHECKING:
    from .terrain import Heightfield
    from .water import Water

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
NAVKIT = os.path.join(SUITE, 'bfbc2-auto-navkit')
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('navkit-tools', 'navkit')


@dataclass
class Entity:
    index: int
    guid: str
    type: str
    short_type: str
    category: str
    id: str | None
    name: str | None
    partition: str
    matrix: np.ndarray | None             # 4 x 4, rows: right, up, forward, translation
    blueprint: str | None
    element: dbx.Element
    group: str | None = None              # static-group partition, for group members
    blueprint_type: str | None = None     # placed objects: the type of the blueprint's object (StaticModelEntityData ...)
    destructible: bool = False            # placed objects: the blueprint has health states / breakable parts
    team: int | None = None               # 1, 2, 0 (neutral): spawns, capture points (initial owner), triggers
    ai: bool | None = None                # soldier spawns: True for AI soldiers, False for human players

    @property
    def position(self) -> np.ndarray | None:
        return None if self.matrix is None else self.matrix[3, :3]

    def refs(self) -> list[tuple[str, str]]:
        """Every reference in the property tree: (field path, ref), local GUIDs and partition/GUID paths."""
        out: list[tuple[str, str]] = []

        def walk(el: dbx.Element, path: str) -> None:
            for c in el.children or ():
                name = c.attr('name') or c.tag
                r = c.attr('ref')
                if r is not None and r != 'null':
                    out.append((path + name, r))
                if c.children is not None:
                    walk(c, path + name + '/')
        walk(self.element, '')
        return out


@dataclass
class Mesh:
    entity: int                           # index into Level.entities
    triangles: np.ndarray                 # (n, 3, 3) world space


@dataclass
class Shape:
    entity: int                           # the entity whose field it is (CapturePoint, CombatArea, an order ...)
    kind: str                             # 'area', 'outer area' or 'route'
    points: np.ndarray                    # (n, 3) world positions, in chain order
    closed: bool                          # areas are rings; routes are open
    height: float                         # areas: prism height above the points (0: flat)
    field: str


@dataclass
class Level:
    name: str
    lv: str
    opfor: str
    entities: list[Entity] = field(default_factory=list)
    meshes: list[Mesh] = field(default_factory=list)
    terrain: tuple[np.ndarray, np.ndarray] | None = None
    heightfield: 'Heightfield | None' = None  # terrain.Heightfield, when the level's terrain bundle was read
    water: 'Water | None' = None          # water.Water (may have no triangles), from the same bundle
    shapes: list[Shape] = field(default_factory=list)
    missing_collision: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def by_guid(self, guid: str) -> Entity | None:
        g = guid.lower()
        return next((e for e in self.entities if e.guid.lower() == g), None)


def _rust() -> ModuleType:
    """The Rust core, or NativeMissing saying how to get it."""
    from . import native
    return native.require('the level loader (bfbc2core.level)', 'scene_load')


def load(level: str, opfor: str | None = None, terrain_stride: int = 2, log: Callable[[str], Any] = print,
         geometry: bool = True) -> Level:
    """geometry=False: the level partition's entities, teams and shapes only - no static-group members, collision,
    terrain or water (a second or two; the test harness's reachability and level checks)."""
    _rust()                                              # before anything is prepared or unpacked
    import navkit
    root = paths.find_opfor(opfor, start=SUITE)          # NavKit's search and message (GameCopyNotFound)
    lv = level.replace('\\', '/').split('/')[-1].lower()
    base = os.path.join(root, 'work', 'levels', lv)
    if not os.path.isdir(os.path.join(base, 'dist')):           # first use: NavKit's offline prepare step
        log(f'{lv}: not prepared yet - unpacking it (navkit.py prepare {lv}, once)')
        pr = navkit.Project(root)
        navkit.prepare(pr, pr.level(lv), log=log)
    bases = [os.path.join(base, 'pkg'), os.path.join(base, 'dist')]
    bases += [d for d in glob.glob(os.path.join(root, 'work', 'navkit', 'common', '*')) if os.path.isdir(d)]
    # the level partition (a Package overlay wins over Dist)
    part = None
    for b in bases[:2]:
        for pf in glob.glob(os.path.join(b, 'levels', '*.dbx')):
            if os.path.splitext(os.path.basename(pf))[0].lower() == lv:
                part = part or pf
    if part is None:
        raise FileNotFoundError(f'{lv}: level partition not found under {base}')
    LV = os.path.splitext(os.path.basename(part))[0]
    lvl = Level(LV, lv, root)
    log(f'{LV}: reading {os.path.relpath(part, root)}')
    # static model groups: one entity per member placement. Each group file once: the Package copy wins over
    # Dist, as in the game (and levels\ / Levels\ are the same folder on Windows)
    group_files: dict[str, str] = {}
    for b in (bases[:2] if geometry else ()):
        for gf in glob.glob(os.path.join(b, 'levels', lv, '*_HavokAsset.dbx')):
            group_files.setdefault(os.path.basename(gf).lower(), gf)
    # the partition and the group files in one batch (all cores); errors raise as dbx.read's would
    f, *groups = _strict(read_many([part, *group_files.values()]))
    return _load_scene(lvl, part, list(group_files.values()), bases, f, groups, geometry, terrain_stride, log)


def _load_scene(lvl: Level, part: str, group_paths: list[str], bases: list[str], f: dbx.DbxFile,
                groups: list[dbx.DbxFile], geometry: bool, terrain_stride: int, log: Callable[[str], Any]) -> Level:
    """load()'s Level from the Rust scene, its elements taken from this module's trees (f, groups)."""
    root, lv, LV = lvl.opfor, lvl.lv, lvl.name
    name = terrain_name(f.instances)
    hpath: str | None = None
    terrain_note: str | None = None
    water_files: tuple[str | None, str | None] = (None, None)
    if geometry:
        try:
            hpath = _terrain_file(root, lv, log, name)          # unpacks the terrain bundle when it has to
        except Exception as e:                # no terrain bundle, FbrbTool missing, unexpected layout
            terrain_note = f'terrain height field not read: {e}'
        water_files = water.find_files(os.path.join(root, 'work', 'levels', lv, 'terrain'), name or lv)
    s = _rust().scene_load(LV, part, group_paths, bases, geometry, hpath, terrain_stride, *water_files,
                                             alias_cache=collision.cache_folder())   # the mesh-alias cache
    collision.prune_cache()
    instances = [f.instances] + [g.instances for g in groups]
    for (index, guid, t, short, category, eid, ename, pname, matrix, bp, group, bp_type, destructible, team, ai,
         (file, inst, item)) in s['entities']:
        el = instances[file][inst]
        if item >= 0:                          # a static group's member: the item of its Assets
            el = cast('list[dbx.Element]', cast(dbx.Element, el.child('Assets')).children)[item]
        ent = Entity(index, guid, t, short, category, eid, ename, pname,
                     None if matrix is None else np.array(matrix, dtype=np.float64).reshape(4, 4), bp, el, group=group)
        ent.blueprint_type, ent.destructible, ent.team, ent.ai = bp_type, destructible, team, ai
        lvl.entities.append(ent)
    lvl.shapes = [Shape(entity, kind, np.array(points), closed, height, fld)
                  for entity, kind, points, closed, height, fld in s['shapes']]
    if not geometry:
        log(f'{LV}: {len(lvl.entities)} entities (no geometry)')
        return lvl
    notes = [n for n in s['notes'] if n.startswith('blueprint mesh aliases')]
    tris, meshes, missing = s['collision']
    arrays = {bp: None if t is None else np.frombuffer(t, '<f4').reshape(-1, 3, 3) for bp, t in tris.items()}
    for index, bp in meshes:
        m = cast(np.ndarray, lvl.entities[index].matrix)
        lvl.meshes.append(Mesh(index, (arrays[bp] @ m[:3, :3] + m[3, :3]).astype(np.float32)))
    lvl.missing_collision = list(missing)
    notes += [terrain_note] if terrain_note else [n for n in s['notes'] if n.startswith('terrain')]
    notes += [n for n in s['notes'] if n.startswith('water')]
    lvl.notes = notes
    if s['terrain'] is not None:
        lo, hi, size, raw, heights, verts, triangles = s['terrain']
        lvl.heightfield = terrain.Heightfield(np.array(lo, dtype=np.float64), np.array(hi, dtype=np.float64), size,
                                              np.frombuffer(raw, '<u2').reshape(size, size),
                                              np.frombuffer(heights, np.float32).reshape(size, size))
        lvl.terrain, source = (np.frombuffer(verts, np.float32).reshape(-1, 3),
                               np.frombuffer(triangles, np.uint32).reshape(-1, 3)), 'height field'
    else:
        lvl.terrain = terrain_from_samples(root, lv, terrain_stride)
        source = 'samples' if lvl.terrain else 'none'
    if s['water'] is not None:
        v, t, shader, shaders = s['water']
        lvl.water = Water(np.frombuffer(v, np.float32).reshape(-1, 3), np.frombuffer(t, np.uint32).reshape(-1, 3),
                          shader, shaders)
    n_tris = sum(len(m.triangles) for m in lvl.meshes)
    log(f'{LV}: {len(lvl.entities)} entities, {len(lvl.meshes)} with collision ({n_tris} triangles), '
        f'{len(lvl.missing_collision)} blueprints without collision, terrain from {source}')
    return lvl


def read_many(files: Sequence[str]) -> list[dbx.DbxFile | Exception]:
    """dbx.read of each file, in order, through dbx.parse_many: on all cores, and with the cycle
    collector paused while the trees are built (a level keeps thousands of them: rust/BENCHMARKS.md). A file that
    cannot be read or parsed gives its exception (OSError, ValueError, dbx.DeletedEntry) in its place."""
    out: list[dbx.DbxFile | Exception | None] = [None] * len(files)
    todo: list[int] = []
    data: list[bytes] = []
    for i, p in enumerate(files):
        try:
            with open(p, 'rb') as fh:
                data.append(fh.read())
            todo.append(i)
        except OSError as e:
            out[i] = e
    for i, r in zip(todo, dbx.parse_many(data)):
        out[i] = r
    return cast('list[dbx.DbxFile | Exception]', out)


def _strict(results: list[dbx.DbxFile | Exception]) -> list[dbx.DbxFile]:
    """read_many's results, the first error raised (as reading the files one by one with dbx.read would)."""
    for r in results:
        if isinstance(r, Exception):
            raise r
    return cast('list[dbx.DbxFile]', results)


def terrain_name(entities: Iterable[dbx.Element]) -> str | None:
    """The terrain's name from the level's TerrainEntityData.TerrainAsset (terrains/mp_009/mp_009 for MP_009GR:
    a mode variant shares its base map's terrain), or None.

    The entity's transform is NOT applied to the heights: on 11 of the 13 campaign levels it moves the
    terrain by 1-59 m, yet the stored heights unmoved match the game's terrain to within centimetres."""
    for el in entities:
        if (el.attr('type') or '').endswith('.TerrainEntityData'):
            a = el.child('TerrainAsset')
            parts = ((a.attr('ref') or '') if a is not None else '').split('/')
            return parts[1].lower() if len(parts) >= 3 else None
    return None


def water_of(root: str, lv: str, log: Callable[[str], Any] = print) -> 'water.Water | None':
    """The level's water (water.Water, or None), its terrain bundle unpacked once (as heightfield_of does)."""
    part = None
    for b in ('pkg', 'dist'):
        for f in glob.glob(os.path.join(root, 'work', 'levels', lv, b, 'levels', '*.dbx')):
            if os.path.splitext(os.path.basename(f))[0].lower() == lv:
                part = part or f
    name = (terrain_name(dbx.read(part).instances) if part else None) or lv
    heightfield_of(root, lv, log, name)                  # unpacks the terrain bundle when it is not there yet
    return water.read(os.path.join(root, 'work', 'levels', lv, 'terrain'), name)


def heightfield_of(root: str, lv: str, log: Callable[[str], Any] = print,
                   name: str | None = None) -> terrain.Heightfield | None:
    """The level's terrain height field, unpacking its terrain-00.fbrb into work\\levels\\<lv>\\terrain once.
    name: from terrain_name(); read from the level partition if not given."""
    path = _terrain_file(root, lv, log, name)
    return terrain.read(path) if path else None


def heightfield_path(root: str, lv: str, log: Callable[[str], Any] = print) -> str | None:
    """The level's .Heightfield-000.res (heightfield_of's file, the terrain unpacked once), or None: NavKit's build
    reads it for DICE's terrain rule of D lines and F cells (KYG-DF-1)."""
    return _terrain_file(root, lv, log)


def _terrain_file(root: str, lv: str, log: Callable[[str], Any] = print, name: str | None = None) -> str | None:
    """heightfield_of's file: the level's .Heightfield-000.res, its terrain bundle unpacked when the unpack is missing,
    incomplete or from another bundle; None when there is none."""
    if name is None:
        part = None
        for b in ('pkg', 'dist'):
            for f in glob.glob(os.path.join(root, 'work', 'levels', lv, b, 'levels', '*.dbx')):
                if os.path.splitext(os.path.basename(f))[0].lower() == lv:
                    part = part or f
        name = terrain_name(dbx.read(part).instances) if part else None
    name = name or lv
    dest = os.path.join(root, 'work', 'levels', lv, 'terrain')
    bundle = None
    # Dist first; a level whose terrain bundle is only in Package (ONS_MP_005GR) takes that one
    for top in (os.path.join(root, 'game', 'Dist', 'win32', 'levels'), os.path.join(root, 'game', 'Package', 'levels')):
        for d in glob.glob(os.path.join(top, '*')):
            if bundle is None and os.path.basename(d).lower() == lv and os.path.isfile(os.path.join(d, 'terrain-00.fbrb')):
                bundle = os.path.join(d, 'terrain-00.fbrb')
    path = terrain.find(dest, name) if os.path.isdir(dest) else None
    if bundle is None:
        return path                           # no game copy: the unpack as it is, if there is one
    # the unpack is reused only while it is complete and from this bundle (suite deep scan 2026-11-15, C4: any unpack
    # was reused - a patched or re-imported terrain kept the old height field, and an unpack cut short was taken as
    # whole): its stamp, the bundle's size and modification time, is written after the unpack finishes
    stamp = os.path.join(dest, '.source.stamp')
    st = os.stat(bundle)
    want = f'{st.st_size} {st.st_mtime_ns}'

    def current() -> bool:
        if path is None or not os.path.isfile(stamp):
            return False
        with open(stamp, encoding='ascii') as fh:
            return fh.read().strip() == want

    if not current():
        import shutil
        # one process unpacks at a time; one that waited takes the unpack the other just finished (two tools or tests
        # side by side removed each other's folder mid-read; run_tests.py -j, 2026-11-17)
        with atomicio.locked(os.path.join(root, 'work', 'levels', lv, '.terrain.lock')):
            path = terrain.find(dest, name) if os.path.isdir(dest) else None
            if not current():
                log(f'{lv}: unpacking {os.path.relpath(bundle, root)}')
                from . import fbrbtool
                if os.path.isdir(dest):
                    shutil.rmtree(dest)
                fbrbtool.run(['unpack', bundle, dest], check=True)       # in process (the Rust core)
                atomicio.write_text(stamp, want)
                path = terrain.find(dest, name)
    return path


def terrain_from_samples(root: str, lv: str, stride: int = 2) -> tuple[np.ndarray, np.ndarray] | None:
    """A height-field mesh from NavKit's in-game samples: per column the highest surface that hit terrain,
    every <stride>-th sample. Returns (vertices (n, 3), triangles (m, 3) indices) or None."""
    import navsample
    import navwalk
    path = None
    for name in (f'{lv}_navkit.bin', f'{lv}_full.bin'):
        p = os.path.join(root, 'work', 'navsample', name)
        if os.path.isfile(p):
            path = p
            break
    if path is None:
        return None
    s, _ = navsample.load(path)
    terrain = (s.hit[:, 0] == navwalk.TERRAIN_SHAPE_VTABLE) if s.hit is not None and len(s.hit) else np.ones(len(s.y), bool)
    cols = s.nx * s.nz
    column = np.repeat(np.arange(cols), np.diff(s.start[:cols + 1]))        # column of every surface
    top = np.full(cols, -np.inf, dtype=np.float32)
    np.maximum.at(top, column[terrain], s.y[terrain])                         # highest terrain surface per column
    full = np.where(np.isfinite(top), top, np.nan).reshape(s.nz, s.nx)
    ii = np.arange(0, s.nx, stride)
    jj = np.arange(0, s.nz, stride)
    h = full[np.ix_(jj, ii)]
    xs = s.min_x + ii * s.pitch
    zs = s.min_z + jj * s.pitch
    X, Z = np.meshgrid(xs, zs)
    verts = np.stack([X.ravel(), h.ravel(), Z.ravel()], axis=1).astype(np.float32)
    nz, nx = h.shape
    idx = np.arange(nz * nx).reshape(nz, nx)
    a, b, c, d = idx[:-1, :-1].ravel(), idx[:-1, 1:].ravel(), idx[1:, :-1].ravel(), idx[1:, 1:].ravel()
    ok = ~(np.isnan(h[:-1, :-1]) | np.isnan(h[:-1, 1:]) | np.isnan(h[1:, :-1]) | np.isnan(h[1:, 1:])).ravel()
    # no triangles across cliffs or between stacked surfaces (a step of more than 4 m between neighbours)
    hv = h.ravel()
    steep = (np.abs(hv[a] - hv[b]) > 4) | (np.abs(hv[a] - hv[c]) > 4) | (np.abs(hv[d] - hv[b]) > 4) | (np.abs(hv[d] - hv[c]) > 4)
    ok &= ~steep
    tris = np.concatenate([np.stack([a, c, b], 1)[ok], np.stack([b, c, d], 1)[ok]]).astype(np.uint32)
    verts[np.isnan(verts[:, 1]), 1] = 0
    return verts, tris
