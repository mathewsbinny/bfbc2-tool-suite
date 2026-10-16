"""A BC2 level as the suite's tools see it: entities with transforms and sources, collision meshes, terrain.

    lvl = load('sp_01')                       # by level name, from Project OPFOR's game copy
    lvl.entities                              # every instance of the level partition (Entity)
    lvl.meshes                                # world-space collision triangles per placed object / group member
    lvl.terrain                               # (vertices, triangles) from NavKit's in-game samples, or None

Data comes from the level as NavKit prepares it (OPFOR's work\\levels\\<level>\\dist and pkg, and the shared
data in work\\navkit\\common\\). Run `navkit.py prepare <level>` once for a level that is not there yet.
"""
import glob
import os
import re
import sys
from dataclasses import dataclass, field
from typing import List, Optional

import numpy as np

from . import dbx

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
NAVKIT = os.path.join(SUITE, 'bfbc2-auto-navkit')
sys.path.insert(0, NAVKIT)
sys.path.insert(0, os.path.join(NAVKIT, 'tools'))

# entity categories by type (DESIGN.md open question 6)
CATEGORIES = [
    ('soldier spawn', r'\.(SoldierSpawnEntityData|AlternateSpawnEntityData)$'),
    ('vehicle spawn', r'\.VehicleSpawnEntityData$'),
    ('objective', r'\.(MissionObjectiveEntityData|ArmDisarmAwardEntityData)$'),
    ('capture point', r'\.CapturePointEntityData$'),
    ('combat area', r'\.(CombatAreaTriggerEntityData|DamageAreaTriggerEntityData)$'),
    ('ai order', r'\.(AreaOrderEntityData|PatrolOrderEntityData|InterludeOrderEntityData|FireAtAreaOrderEntityData)$'),
    ('area point', r'\.AreaGeometryEntityData$'),
    ('placed object', r'\.ReferenceObjectData$'),
]


@dataclass
class Entity:
    index: int
    guid: str
    type: str
    short_type: str
    category: str
    id: Optional[str]
    name: Optional[str]
    partition: str
    matrix: Optional[np.ndarray]          # 4 x 4, rows: right, up, forward, translation
    blueprint: Optional[str]
    element: dbx.Element
    group: Optional[str] = None           # static-group partition, for group members

    @property
    def position(self):
        return None if self.matrix is None else self.matrix[3, :3]


@dataclass
class Mesh:
    entity: int                           # index into Level.entities
    triangles: np.ndarray                 # (n, 3, 3) world space


@dataclass
class Level:
    name: str
    lv: str
    opfor: str
    entities: List[Entity] = field(default_factory=list)
    meshes: List[Mesh] = field(default_factory=list)
    terrain: Optional[tuple] = None
    missing_collision: List[str] = field(default_factory=list)
    notes: List[str] = field(default_factory=list)

    def by_guid(self, guid):
        g = guid.lower()
        return next((e for e in self.entities if e.guid.lower() == g), None)


def category_of(t):
    for name, rx in CATEGORIES:
        if re.search(rx, t or ''):
            return name
    return 'other'


def transform_of(e):
    t = e.child('Transform')
    if t is None or t.kind != dbx.NUMBERS:
        return None
    v = dbx.floats(t)
    return np.array(v, dtype=np.float64).reshape(4, 4) if len(v) == 16 else None


def blueprint_of(e):
    r = e.child('ReferencedObject')
    ref = r.attr('ref') if r is not None else None
    return ref.rsplit('/', 1)[0].lower() if ref and '/' in ref else None


def load(level, opfor=None, terrain_stride=2, log=print):
    import navkit
    import levelobjects
    root = navkit.find_opfor(opfor)
    lv = level.replace('\\', '/').split('/')[-1].lower()
    base = os.path.join(root, 'work', 'levels', lv)
    if not os.path.isdir(os.path.join(base, 'dist')):
        raise FileNotFoundError(f'{lv} is not prepared yet - run: python navkit.py prepare {lv}')
    bases = [os.path.join(base, 'pkg'), os.path.join(base, 'dist')]
    bases += [d for d in glob.glob(os.path.join(root, 'work', 'navkit', 'common', '*')) if os.path.isdir(d)]
    # the level partition (a Package overlay wins over Dist)
    part = None
    for b in bases[:2]:
        for f in glob.glob(os.path.join(b, 'levels', '*.dbx')):
            if os.path.splitext(os.path.basename(f))[0].lower() == lv:
                part = part or f
    if part is None:
        raise FileNotFoundError(f'{lv}: level partition not found under {base}')
    LV = os.path.splitext(os.path.basename(part))[0]
    lvl = Level(LV, lv, root)
    log(f'{LV}: reading {os.path.relpath(part, root)}')
    f = dbx.read(part)
    pname = 'levels/' + LV
    for el in f.instances:
        t = el.attr('type') or ''
        lvl.entities.append(Entity(len(lvl.entities), el.attr('guid') or '', t, t.rsplit('.', 1)[-1], category_of(t), el.attr('id'),
                                   (el.child('Name').text if el.child('Name') is not None and el.child('Name').kind == dbx.TEXT else None),
                                   pname, transform_of(el), blueprint_of(el), el))
    # static model groups: one entity per member placement. Each group file once: the Package copy wins over
    # Dist, as in the game (and levels\ / Levels\ are the same folder on Windows)
    group_files = {}
    for b in bases[:2]:
        for gf in glob.glob(os.path.join(b, 'levels', lv, '*_HavokAsset.dbx')):
            group_files.setdefault(os.path.basename(gf).lower(), gf)
    for gf in group_files.values():
            g = dbx.read(gf)
            for el in g.instances:
                tr, assets = el.child('Transforms'), el.child('Assets')
                if tr is None or assets is None or tr.kind != dbx.NUMBERS or not assets.children:
                    continue
                vals = np.array(dbx.floats(tr), dtype=np.float64)
                for k, item in enumerate(assets.children):
                    if 16 * (k + 1) > len(vals):
                        break
                    bp = re.sub(r'_havok$', '', (item.attr('ref') or '').rsplit('/', 1)[0].lower())
                    lvl.entities.append(Entity(len(lvl.entities), f'{el.attr("guid")}#{k}', 'StaticGroupMember', 'StaticGroupMember',
                                               'group member', None, bp.rsplit('/', 1)[-1], os.path.basename(gf)[:-4],
                                               vals[16 * k:16 * (k + 1)].reshape(4, 4), bp, item, group=os.path.basename(gf)))
    # collision
    havok = levelobjects.havok_index(bases)
    try:
        for bp, target in levelobjects.mesh_aliases(bases).items():
            if bp not in havok and target in havok:
                havok[bp] = havok[target]
    except Exception as e:                    # needs the converted XML NavKit's prepare writes
        lvl.notes.append(f'blueprint mesh aliases not read: {e}')
    cache, missing = {}, set()
    for e in lvl.entities:
        if e.blueprint is None or e.matrix is None:
            continue
        if e.blueprint not in cache:
            cache[e.blueprint] = levelobjects.triangles(havok[e.blueprint]) if e.blueprint in havok else None
        t = cache[e.blueprint]
        if t is None:
            missing.add(e.blueprint)
            continue
        m = e.matrix
        lvl.meshes.append(Mesh(e.index, (t @ m[:3, :3] + m[3, :3]).astype(np.float32)))
    lvl.missing_collision = sorted(missing)
    lvl.terrain = terrain_from_samples(root, lv, terrain_stride)
    n_tris = sum(len(m.triangles) for m in lvl.meshes)
    log(f'{LV}: {len(lvl.entities)} entities, {len(lvl.meshes)} with collision ({n_tris} triangles), '
        f'{len(missing)} blueprints without collision, terrain {"from samples" if lvl.terrain else "none"}')
    return lvl


def terrain_from_samples(root, lv, stride=2):
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
