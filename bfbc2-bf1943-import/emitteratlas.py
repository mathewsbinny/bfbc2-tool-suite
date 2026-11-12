r"""emitteratlas.py - one emitter atlas for a level whose emitters were authored against several atlases.

    import emitteratlas as ea
    result = ea.build({'fx/x/em_a.dbx': (partition_bytes, 'mp_005'), ...},
                      {'mp_005': ea.Source(alpha_res, normals_res), 'bf1943:wake_island_s': ...},
                      alpha_like=mp_005_alpha_res, normals_like=mp_005_normals_res)
    result.alpha, result.normals            # the merged atlases (PC ITexture resources)
    result.partitions                       # {name: the emitter partition re-pointed into them}
    ea.prove(result, emitters, sources)     # every emitter's image in its old and new atlas, compared

Why: BFBC2 draws every particle of a level from two per-level atlases (Levels/<level>/emittertexturealpha.res, the
8-bit format 10; emittertexturenormals.res, BC5), and each emitter document stores where its image sits in them - the
fields of the TextureInfo of its Emitter.UpdateTextureCoordsData (FORMAT below). DICE's pipeline wrote those fields
per level, against that level's atlas. A level that takes its emitters from several levels (or from BF1943, whose
level copies point into BF1943's atlases) and ships one of their atlases draws most of them from someone else's
image. This module gives such a level an atlas holding every image its emitters were authored with, and the
emitters' fields re-pointed into it.

FORMAT (measured on DICE's atlases and on the same emitters across levels whose atlases differ):
- TextureOriginU / V: the top-left of frame 0, in the alpha atlas (0-1); AnimationStepU / V: the size of one frame
  (the quad samples [origin + k * step, origin + (k + 1) * step]); frame f sits at column f % AnimationFrameColumnCount,
  row f // AnimationFrameColumnCount. NmTextureOriginU / V, NmAnimationStepU / V: the same in the normals atlas (a
  negative step - the default normal map's - runs left / up from the origin). AnimationFrameCount and
  AnimationFrameColumnCount are the flipbook's layout; nothing else in an emitter names an atlas position
  (Emitter.UpdateRotationData carries a TextureInfo1 too, always zero and null: not drawn).
- Frames drawn: every frame when EnableAnimation is true or an enabled SpawnAnimationFrameData picks the first frame;
  otherwise frame 0 only (369 of Wake's emitters say 4 or 49 frames with neither: their step covers the whole image,
  and the "frames" after it would be other images).
- DICE's alpha atlas: images in slots on an 8-texel grid, each sampled 3 or 6 texels inside its slot (the slot's own
  edge texels are the padding); every level the rounded 2 x 2 mean of the one above ((a+b+c+d+2)//4, exact on every
  texel of mp_005's and bc1_harvest_day_cq's 11 levels); 2048 x 2048 the largest of any level (55 of the 74 atlases),
  levels down to a shorter side of 2. The normals: BC5 1024 x 512 (69 of 74; nam_mp_006's 1024 x 1024), 9 levels,
  empty blocks all zero.
- Normal-map fields of origin (0, 0) and step (1, 1) (or 0, 0): no normal map - left as they are.

How the merged atlas is made (both atlases the same way; a unit is a texel of the alpha atlas, a 4 x 4 block of the
normals'):
1. Each drawn TextureInfo's rectangle (its frames) in its source atlas, widened to the 8-unit grid and by 8 units on
   each side (bilinear filtering and the levels down to K = 3 read only inside it): a cell; its core is the rectangle
   widened by 2 texels (what levels 0..D = 1 read: inside DICE's 3-texel inset, the image's own texels). Cells of one
   source stay apart even when they overlap (DICE's slots touch: joined, a whole atlas became one cell).
2. Cells are deduplicated, largest first: one whose core lies inside a kept cell of its source is drawn from it; one
   whose core's levels 0..D appear inside a kept cell of any source at an 8-unit offset is drawn from there - exactly
   (format 10), or correlating SAME (BC5: DICE's levels' normal atlases differ in 0.03 % of their blocks, inside
   images all levels share: one encoder's choices).
3. Kept cells are packed (first fit on the 8-unit grid, the first of ORDERS that fits) into the size of the `like`
   atlas - DICE's largest - each where its move keeps the largest power of two of ALIGN that still fits all (the
   levels below K then equal the source's inside the image); then the next sizes (alpha 4096 x 2048 and 4096 x 4096,
   which no DICE level has; normals 1024 x 1024, nam_mp_006's).
4. Levels 0..K are the cells' own levels copied (texels or BC5 blocks); the alpha atlas's deeper levels are the
   rounded means above (DICE's rule), the normals' deeper blocks each the block its centre's cell has there.
5. The fields are re-pointed: origin + the cell's move, steps scaled by the source / merged sizes (powers of two:
   exact in float32). Frame counts and columns stay - each flipbook moves whole. Everything else in the partition is
   kept (the element tree; dbx.write gives a fresh string table, which DbxEdit's roundtrip reproduces).

prove() is the check: every image read back through the re-pointed fields against its source's.
"""
import hashlib
import os
import struct
import sys
from dataclasses import dataclass, field
from typing import Any

import numpy as np

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'console-convert', 'model-tools', 'core')
import texture  # noqa: E402
import texwrite  # noqa: E402
from bfbc2core import dbx  # noqa: E402

L8, BC5 = 10, 20
GRID = 8            # cells on an 8-unit grid, widened by 8 units: levels 0..K copied exactly
K = 3
D = 1               # levels 0..D read only an image's own texels (DICE's inset): the same image in two atlases
ALPHA_SIZES = [(4096, 2048), (4096, 4096)]          # tried after the `like` atlas's size (not seen in DICE's data)
NORMAL_SIZES = [(1024, 1024)]                       # nam_mp_006's
UV = ('TextureOriginU', 'TextureOriginV', 'AnimationStepU', 'AnimationStepV')
NM = ('NmTextureOriginU', 'NmTextureOriginV', 'NmAnimationStepU', 'NmAnimationStepV')
CHANNELS = {'alpha': UV, 'normals': NM}


class EmitterAtlasError(Exception):
    what_to_do = 'Check the emitters\' sources and their atlases (emitteratlas.py docstring: FORMAT).'


# ------------------------------------------------------------------------------------------------ textures

@dataclass
class Tex:
    """A PC ITexture as units per level: (h, w, 1) texels of format 10, (bh, bw, 16) blocks of BC5."""
    fmt: int
    flags: int
    w: int
    h: int
    levels: list[np.ndarray]
    cache: dict[int, np.ndarray] = field(default_factory=dict, repr=False)

    @property
    def unit(self) -> int:
        return 4 if self.fmt == BC5 else 1


def read_tex(res: bytes) -> Tex:
    ver, typ, fmt, flags, w, h, depth, mips = struct.unpack_from('<8I', res, 0)
    if fmt not in (L8, BC5) or typ != 0:
        raise EmitterAtlasError(f'an emitter atlas of format {fmt}, type {typ} (8-bit 10 or BC5 20 expected)')
    out = []
    for _s, _m, lw, lh, data in texture.levels(res):
        if fmt == L8:
            out.append(texture.decode(data, lw, lh, L8)[:, :, :1].astype(np.uint8))
        else:
            bw, bh = max((lw + 3) // 4, 1), max((lh + 3) // 4, 1)
            out.append(np.frombuffer(data[:bw * bh * 16], np.uint8).reshape(bh, bw, 16).copy())
    return Tex(fmt, flags, w, h, out)


def write_tex(t: Tex) -> bytes:
    data = [texwrite.l8_bytes(lv[:, :, 0]) if t.fmt == L8 else lv.tobytes() for lv in t.levels]
    return texwrite.resource(t.fmt, t.flags, t.w, t.h, data)


def texels(t: Tex, m: int) -> np.ndarray:
    """Level m decoded: (h, w) for format 10, (h, w, 2) for BC5 (kept: a Tex is not changed once read)."""
    if m not in t.cache:
        lv = t.levels[m]
        if t.fmt == L8:
            t.cache[m] = lv[:, :, 0].astype(np.int32)
        else:
            lw, lh = max(t.w >> m, 1), max(t.h >> m, 1)
            t.cache[m] = texture.decode(lv.tobytes(), lw, lh, BC5)[:, :, :2]
    return t.cache[m]


@dataclass
class Source:
    """One authoring atlas pair (PC ITexture resources; normals None when the source has none)."""
    alpha: bytes
    normals: bytes | None = None


# ------------------------------------------------------------------------------------------------ emitters

@dataclass
class Info:
    """One TextureInfo of an emitter: its fields' elements and values (float32 as stored)."""
    instance: str
    name: str
    elements: dict[str, dbx.Element]
    values: dict[str, float]
    frames: int
    columns: int
    animated: bool
    texture: str | None
    normalmap: str | None


def _f32(e: dbx.Element) -> float:
    size, vals = dbx.numbers(e)
    if size != 4 or len(vals) != 1:
        raise EmitterAtlasError(f'{e.attr("name")}: not one 4-byte number')
    return struct.unpack('>f', vals[0])[0]


def _int(e: dbx.Element) -> int:
    """A count as the XML shows it: the frame counts are stored as integers in some partitions and as float32 in
    others (4.0 is 0x40800000: read as an integer, 1082130432 frames)."""
    return int(float(dbx.value(e)))


def infos(f: dbx.DbxFile) -> list[Info]:
    """Every TextureInfo complex of the partition's instances (UpdateTextureCoordsData's, and the inert ones)."""
    out = []
    for inst in f.instances:
        for c in inst.children or ():
            if c.kind != dbx.CONTAINER or not (c.attr('name') or '').startswith('TextureInfo'):
                continue
            el = {x.attr('name') or '': x for x in c.children or ()}
            if not all(k in el for k in UV + NM + ('AnimationFrameCount', 'AnimationFrameColumnCount')):
                raise EmitterAtlasError(f'{inst.attr("type")} {c.attr("name")}: a TextureInfo without its fields')
            out.append(Info(inst.attr('guid') or '', c.attr('name') or '', {k: el[k] for k in UV + NM},
                            {k: _f32(el[k]) for k in UV + NM}, _int(el['AnimationFrameCount']),
                            _int(el['AnimationFrameColumnCount']),
                            dbx.value(el['EnableAnimation']) == 'true' if 'EnableAnimation' in el else False,
                            el['Texture'].attr('ref') if 'Texture' in el else None,
                            el['Normalmap'].attr('ref') if 'Normalmap' in el else None))
    return out


def spawns_frames(f: dbx.DbxFile) -> bool:
    """An enabled SpawnAnimationFrameData: each particle starts on a frame of its own."""
    for inst in f.instances:
        if (inst.attr('type') or '').endswith('SpawnAnimationFrameData'):
            en = inst.child('Enable')
            if en is None or dbx.value(en) == 'true':
                return True
    return False


def grid(info: Info, spawn: bool) -> tuple[int, int]:
    """(columns, rows) of frames drawn."""
    if not (info.animated or spawn) or info.frames <= 1:
        return 1, 1
    cols = max(info.columns, 1)
    return min(cols, info.frames), -(-info.frames // cols)


def drawn(info: Info, channel: str) -> bool:
    """False for the fields of no image: origin (0, 0) with step (0, 0) or (1, 1) - the whole atlas; the alpha ones
    with a null texture (33 of Wake's: blood chunks, tracer bounces), the normal ones with a null normal map."""
    reference = info.texture if channel == 'alpha' else info.normalmap
    if reference is not None and reference.lower() in ('', 'null'):
        return False
    o_u, o_v, s_u, s_v = (info.values[k] for k in CHANNELS[channel])
    return not (o_u == 0 and o_v == 0 and (s_u, s_v) in ((0, 0), (1, 1)))


def rect(info: Info, channel: str, spawn: bool, w: int, h: int) -> tuple[float, float, float, float]:
    """The frames' rectangle in texels of a w x h atlas: (x0, y0, x1, y1)."""
    o_u, o_v, s_u, s_v = (info.values[k] for k in CHANNELS[channel])
    cols, rows = grid(info, spawn)
    xs = sorted((o_u * w, (o_u + s_u * cols) * w))
    ys = sorted((o_v * h, (o_v + s_v * rows) * h))
    return xs[0], ys[0], xs[1], ys[1]


# ------------------------------------------------------------------------------------------------ cells

@dataclass
class Cell:
    """A rectangle of a source atlas in level-0 units (multiples of GRID), and where it is drawn from in the merged one."""
    source: str
    x: int
    y: int
    w: int
    h: int
    into: 'Cell | None' = None          # deduplicated: drawn from this kept cell ...
    dx: int = 0                         # ... at this offset in it
    dy: int = 0
    at: tuple[int, int] | None = None   # kept: its place in the merged atlas
    core: tuple[int, int, int, int] = (0, 0, 0, 0)    # the units its uses read at levels 0..D (core_rect)

    def contains(self, o: 'Cell') -> bool:
        return self.x <= o.x and self.y <= o.y and o.x + o.w <= self.x + self.w and o.y + o.h <= self.y + self.h

    def place(self) -> tuple[int, int]:
        """Its level-0 unit position in the merged atlas."""
        c, dx, dy = self, 0, 0
        while c.into is not None:
            dx, dy, c = dx + c.dx, dy + c.dy, c.into
        if c.at is None:
            raise EmitterAtlasError('a cell without a place')
        return c.at[0] + dx, c.at[1] + dy


def unit_rect(r: tuple[float, float, float, float], unit: int, uw: int, uh: int) -> tuple[int, int, int, int]:
    """A texel rectangle widened to the grid and by one grid step each side, in units, clipped to the atlas."""
    x0 = max((int(np.floor(r[0] / unit)) // GRID - 1) * GRID, 0)
    y0 = max((int(np.floor(r[1] / unit)) // GRID - 1) * GRID, 0)
    x1 = min((-(-int(np.ceil(r[2] / unit)) // GRID) + 1) * GRID, uw)
    y1 = min((-(-int(np.ceil(r[3] / unit)) // GRID) + 1) * GRID, uh)
    if x1 <= x0 or y1 <= y0:
        raise EmitterAtlasError(f'a rectangle {r} outside its atlas')
    return x0, y0, x1, y1


def merge(cells: list[Cell]) -> list[Cell]:
    """One cell per distinct rectangle of a source. Overlapping cells stay apart (DICE's slots touch, so the widened
    cells of neighbours overlap: joined, a whole atlas became one cell); one inside another is drawn from it
    (dedupe)."""
    seen: dict[tuple[str, int, int, int, int], Cell] = {}
    for c in cells:
        k = (c.source, c.x, c.y, c.w, c.h)
        if k not in seen:
            seen[k] = Cell(c.source, c.x, c.y, c.w, c.h, core=c.core)
        else:
            a, b = seen[k].core, c.core
            seen[k].core = (min(a[0], b[0]), min(a[1], b[1]), max(a[2], b[2]), max(a[3], b[3]))
    return list(seen.values())


def _crop(t: Tex, m: int, x: int, y: int, w: int, h: int) -> np.ndarray:
    s = 1 << m
    return t.levels[m][y // s:(y + h) // s, x // s:(x + w) // s]


def _box(t: Tex, m: int, b: tuple[int, int, int, int]) -> np.ndarray:
    """Level m's units over the level-0 unit box b (outward)."""
    s = 1 << m
    return t.levels[m][b[1] // s:-(-b[3] // s), b[0] // s:-(-b[2] // s)]


def core_rect(r: tuple[float, float, float, float], unit: int, uw: int, uh: int) -> tuple[int, int, int, int]:
    """The units levels 0..D read for a texel rectangle: widened by 2 texels (bilinear at level 0, one texel at
    level 1). DICE samples 3 or 6 texels inside a slot, so this is the image's own texels, never a neighbour's."""
    return (max(int(np.floor((r[0] - 2) / unit)), 0), max(int(np.floor((r[1] - 2) / unit)), 0),
            min(int(np.ceil((r[2] + 2) / unit)), uw), min(int(np.ceil((r[3] + 2) / unit)), uh))


SAME = 0.995        # BC5: DICE's levels' normal atlases differ in a few blocks of one image (0.03 % of the blocks; an
                    # encoder's choice): such copies count as one image when their decoded texels correlate this well


def same_image(a: Tex, b: Tex, m: int, ba: tuple[int, int, int, int], bb: tuple[int, int, int, int]) -> bool:
    """Level m of a over the unit box ba against b over bb: equal units (format 10), or decoded texels equal or
    correlating SAME (BC5)."""
    x, y = _box(a, m, ba), _box(b, m, bb)
    if x.shape != y.shape:
        return False
    if np.array_equal(x, y):
        return True
    if a.fmt == L8:
        return False
    s, u = 1 << m, a.unit
    da = texels(a, m)[ba[1] * u // s:-(-ba[3] * u // s), ba[0] * u // s:-(-ba[2] * u // s)].astype(float).ravel()
    db = texels(b, m)[bb[1] * u // s:-(-bb[3] * u // s), bb[0] * u // s:-(-bb[2] * u // s)].astype(float).ravel()
    if da.shape != db.shape or not da.std() or not db.std():
        return False
    return float(np.corrcoef(da, db)[0, 1]) >= SAME


def dedupe(cells: list[Cell], tex: dict[str, Tex]) -> list[Cell]:
    """Kept cells (largest first); each other cell drawn from a kept one: of its own source, around its core; or of
    any source holding its core's levels 0..D exactly at a grid offset (the widened margins of one texture in two
    atlases hold different neighbours - those copies are the same image)."""
    kept: list[Cell] = []
    for c in sorted(cells, key=lambda c: (-c.w * c.h, c.source, c.y, c.x)):
        t = tex[c.source]
        q = c.core
        qw, qh = q[2] - q[0], q[3] - q[1]
        c0 = _box(t, 0, q)
        # the probe: the core's most varied 8 x 8 square (an empty one would match anywhere)
        best, py, px = -1.0, 0, 0
        for yy in range(0, max(qh - GRID, 0) + 1, 2):
            for xx in range(0, max(qw - GRID, 0) + 1, 2):
                v = float(c0[yy:yy + GRID, xx:xx + GRID].std())
                if v > best:
                    best, py, px = v, yy, xx
        probe = c0[py:py + GRID, px:px + GRID]
        for k in kept:
            if k.source == c.source and k.x <= q[0] and k.y <= q[1] and q[2] <= k.x + k.w and q[3] <= k.y + k.h:
                c.into, c.dx, c.dy = k, c.x - k.x, c.y - k.y
                break
            if k.w < qw or k.h < qh:
                continue
            kt = tex[k.source]
            k0 = _crop(kt, 0, k.x, k.y, k.w, k.h)
            hit = None
            # c's origin at (ox, oy) in k (grid steps): its core inside k
            for oy in range(-((q[1] - c.y) // GRID) * GRID, k.h - (q[3] - c.y) + 1, GRID):
                for ox in range(-((q[0] - c.x) // GRID) * GRID, k.w - (q[2] - c.x) + 1, GRID):
                    ky, kx = oy + q[1] - c.y + py, ox + q[0] - c.x + px
                    if ky < 0 or kx < 0:
                        continue
                    pk = k0[ky:ky + probe.shape[0], kx:kx + probe.shape[1]]
                    if pk.shape != probe.shape or float((pk == probe).all(-1).mean()) < (1.0 if t.fmt == L8 else 0.9):
                        continue
                    mv = (k.x + ox - c.x, k.y + oy - c.y)
                    moved = (q[0] + mv[0], q[1] + mv[1], q[2] + mv[0], q[3] + mv[1])
                    if all(same_image(kt, t, m, moved, q) for m in range(D + 1)):
                        hit = (ox, oy)
                        break
                if hit:
                    break
            if hit:
                c.into, c.dx, c.dy = k, hit[0], hit[1]
                break
        else:
            kept.append(c)
    return kept


ALIGN = (128, 64, 32, 16, GRID)
ORDERS = (lambda c: (-c.h, -c.w), lambda c: (-c.w * c.h, -c.h), lambda c: (-c.w, -c.h), lambda c: (-max(c.w, c.h), -c.h))


def pack(kept: list[Cell], uw: int, uh: int, aligns: tuple[int, ...] = (GRID,)) -> bool:
    """Places the kept cells on the GRID in a uw x uh unit atlas: first fit (top-down; left-right as a second try) in
    the first of ORDERS that fits them all, each cell where its move is a multiple of the first of `aligns` some free
    place allows; False when none does. Deterministic."""
    gw, gh = uw // GRID, uh // GRID
    if any(c.h // GRID > gh or c.w // GRID > gw for c in kept):
        return False
    for order in ORDERS:
        for by_column in (False, True):
            occ = np.zeros((gh, gw), np.int32)
            at = {}
            for c in sorted(kept, key=lambda c: (*order(c), c.source, c.y, c.x)):
                ch, cw = c.h // GRID, c.w // GRID
                s = np.zeros((gh + 1, gw + 1), np.int32)
                s[1:, 1:] = occ.cumsum(0).cumsum(1)
                win = s[ch:, cw:] - s[:-ch, cw:] - s[ch:, :-cw] + s[:-ch, :-cw]
                free = np.argwhere(win.T == 0) if by_column else np.argwhere(win == 0)
                if not len(free):
                    break
                xs, ys = (free[:, 0], free[:, 1]) if by_column else (free[:, 1], free[:, 0])
                # keep the cell's place modulo the largest power of two that still fits: the merged levels below K
                # (the rounded means) then match the source's at the image's inner texels down to that level
                for align in aligns:
                    hit = np.flatnonzero(((xs * GRID - c.x) % align == 0) & ((ys * GRID - c.y) % align == 0))
                    if len(hit):
                        break
                x, y = xs[hit[0]], ys[hit[0]]
                occ[y:y + ch, x:x + cw] = 1
                at[id(c)] = (int(x) * GRID, int(y) * GRID)
            else:
                for c in kept:
                    c.at = at[id(c)]
                return True
    return False


def compose(kept: list[Cell], tex: dict[str, Tex], fmt: int, w: int, h: int, levels: int) -> Tex:
    """The merged atlas: levels 0..K copied from the kept cells' sources, the deeper ones derived (format 10: the
    rounded means; BC5: each block from the cell its centre falls in)."""
    unit = 4 if fmt == BC5 else 1
    uw, uh = w // unit, h // unit
    depth = 1 if fmt == L8 else 16
    out: list[np.ndarray] = []
    owner = np.full((uh, uw), -1, np.int32)
    for i, c in enumerate(kept):
        if c.at is None:
            raise EmitterAtlasError('an unplaced cell')
        owner[c.at[1]:c.at[1] + c.h, c.at[0]:c.at[0] + c.w] = i
    for m in range(levels):
        s = 1 << m
        if fmt == L8:
            lw, lh = max(w >> m, 1), max(h >> m, 1)
        else:
            lw, lh = max((max(w >> m, 1) + 3) // 4, 1), max((max(h >> m, 1) + 3) // 4, 1)
        if m <= K:
            lv = np.zeros((lh, lw, depth), np.uint8)
            for c in kept:
                assert c.at is not None
                src = _crop(tex[c.source], m, c.x, c.y, c.w, c.h)
                lv[c.at[1] // s:c.at[1] // s + src.shape[0], c.at[0] // s:c.at[0] // s + src.shape[1]] = src
        elif fmt == L8:
            lv = texwrite.l8_down(out[-1][:, :, 0])[:, :, None]
        else:
            lv = np.zeros((lh, lw, depth), np.uint8)
            for by in range(lh):
                for bx in range(lw):
                    cy, cx = min(by * s + s // 2, uh - 1), min(bx * s + s // 2, uw - 1)
                    i = owner[cy, cx]
                    if i < 0:
                        continue
                    c = kept[i]
                    assert c.at is not None
                    st = tex[c.source]
                    sy, sx = (c.y + cy - c.at[1]) >> m, (c.x + cx - c.at[0]) >> m
                    sl = st.levels[m] if m < len(st.levels) else None
                    if sl is not None and sy < sl.shape[0] and sx < sl.shape[1]:
                        lv[by, bx] = sl[sy, sx]
        out.append(lv)
    return Tex(fmt, 0, w, h, out)


# ------------------------------------------------------------------------------------------------ the build

@dataclass
class Use:
    """One drawn TextureInfo channel: its source rectangle (texels), the cell drawing it, the move in texels."""
    emitter: str
    instance: str
    info: str
    channel: str
    source: str
    rect: tuple[float, float, float, float]
    cell: Cell
    move: tuple[int, int] = (0, 0)
    old: dict[str, float] = field(default_factory=dict)
    new: dict[str, float] = field(default_factory=dict)


@dataclass
class Result:
    alpha: bytes
    normals: bytes
    partitions: dict[str, bytes]
    uses: list[Use]
    cells: dict[str, list[Cell]]                 # channel -> kept cells
    sizes: dict[str, tuple[int, int]]
    report: list[str]
    left: list[str]                              # TextureInfo channels left as they are (not drawn)
    textures: dict[str, Tex] = field(default_factory=dict)      # channel -> merged


def _like(res: bytes) -> tuple[int, int, int, int]:
    _v, _t, fmt, flags, w, h, _d, mips = struct.unpack_from('<8I', res, 0)
    return fmt, flags, w, h


def build(emitters: dict[str, tuple[bytes, str]], sources: dict[str, Source], alpha_like: bytes,
          normals_like: bytes) -> Result:
    """emitters: {name: (partition bytes, source key)}; sources: {key: Source}; *_like: the atlases whose format,
    flags, size and level count the merged ones take (mp_005's for Wake)."""
    texs: dict[str, dict[str, Tex]] = {'alpha': {}, 'normals': {}}
    for k, s in sources.items():
        texs['alpha'][k] = read_tex(s.alpha)
        if s.normals is not None:
            texs['normals'][k] = read_tex(s.normals)
    parsed = {n: dbx.parse(d) for n, (d, _) in emitters.items()}
    uses: list[Use] = []
    left: list[str] = []
    raw: dict[str, dict[str, list[Cell]]] = {'alpha': {}, 'normals': {}}
    for n, f in sorted(parsed.items()):
        src = emitters[n][1]
        if src not in sources:
            raise EmitterAtlasError(f'{n}: source {src!r} has no atlas')
        spawn = spawns_frames(f)
        for info in infos(f):
            for ch in CHANNELS:
                if not drawn(info, ch):
                    left.append(f'{n} {info.instance} {info.name} {ch}')
                    continue
                t = texs[ch].get(src)
                if t is None:
                    raise EmitterAtlasError(f'{n}: source {src!r} has no {ch} atlas')
                r = rect(info, ch, spawn, t.w, t.h)
                uw, uh = t.levels[0].shape[1], t.levels[0].shape[0]
                x0, y0, x1, y1 = unit_rect(r, t.unit, uw, uh)
                c = Cell(src, x0, y0, x1 - x0, y1 - y0, core=core_rect(r, t.unit, uw, uh))
                raw[ch].setdefault(src, []).append(c)
                uses.append(Use(n, info.instance, info.name, ch, src, r, c,
                                old={k: info.values[k] for k in CHANNELS[ch]}))
    report: list[str] = []
    kept_all: dict[str, list[Cell]] = {}
    sizes: dict[str, tuple[int, int]] = {}
    merged: dict[str, Tex] = {}
    for ch, like, more in (('alpha', alpha_like, ALPHA_SIZES), ('normals', normals_like, NORMAL_SIZES)):
        fmt, flags, lw, lh = _like(like)
        cells: list[Cell] = []
        for cs in raw[ch].values():
            cells += merge(cs)
        for u in uses:                                   # each use's cell: the merged one holding it
            if u.channel == ch:
                u.cell = next(c for c in cells if (c.source, c.x, c.y, c.w, c.h) ==
                              (u.source, u.cell.x, u.cell.y, u.cell.w, u.cell.h))
        kept = dedupe(cells, texs[ch])
        unit = 4 if fmt == BC5 else 1
        for w, h in [(lw, lh)] + more:
            # with the sources' alignment kept where it fits, else on the grid alone - before a larger size
            if any(pack(kept, w // unit, h // unit, ALIGN[i:]) for i in range(len(ALIGN))):
                break
        else:
            raise EmitterAtlasError(f'{ch}: {len(kept)} cells fit in none of {[(lw, lh)] + more}')
        levels = texwrite.atlas_levels(min(w, h))
        t = compose(kept, texs[ch], fmt, w, h, levels)
        t.flags = flags
        merged[ch] = t
        kept_all[ch], sizes[ch] = kept, (w, h)
        area = sum(c.w * c.h for c in kept) * unit * unit
        report.append(f'{ch}: {len(cells)} cells from {len(raw[ch])} sources, {len(kept)} kept '
                      f'({len(cells) - len(kept)} found inside others), {area / (w * h):.0%} of {w} x {h} '
                      f'(format {fmt}, {levels} levels{"" if (w, h) == (lw, lh) else "; the like atlas: %d x %d" % (lw, lh)})')
    # re-point
    by_emitter: dict[str, list[Use]] = {}
    for u in uses:
        by_emitter.setdefault(u.emitter, []).append(u)
    partitions = {}
    for n, f in parsed.items():
        if n not in by_emitter:
            partitions[n] = emitters[n][0]
            continue
        index = {(i.instance, i.name): i for i in infos(f)}
        for u in by_emitter[n]:
            info = index[(u.instance, u.info)]
            st, mt = texs[u.channel][u.source], merged[u.channel]
            px, py = u.cell.place()
            mx, my = (px - u.cell.x) * st.unit, (py - u.cell.y) * st.unit
            u.move = (mx, my)
            ku, kv, ksu, ksv = CHANNELS[u.channel]
            new = {ku: (u.old[ku] * st.w + mx) / mt.w, kv: (u.old[kv] * st.h + my) / mt.h,
                   ksu: u.old[ksu] * st.w / mt.w, ksv: u.old[ksv] * st.h / mt.h}
            for k, v in new.items():
                b = struct.pack('>f', v)
                if struct.unpack('>f', b)[0] != v:
                    raise EmitterAtlasError(f'{n} {k}: {v!r} is not exact in float32')
                e = info.elements[k]
                e.raw = e.raw[:len(e.raw) - 4] + b
            u.new = new
        partitions[n] = dbx.write(f)
    return Result(write_tex(merged['alpha']), write_tex(merged['normals']), partitions, uses, kept_all, sizes, report,
                  left, merged)


# ------------------------------------------------------------------------------------------------ proof

def crops(t: Tex, m: int, r: tuple[float, float, float, float], move: tuple[int, int] = (0, 0),
          other: Tex | None = None, margin: int = 1) -> tuple[np.ndarray, np.ndarray] | None:
    """The rectangle r (texels, level 0) at level m - widened by `margin` texels there (1: what bilinear filtering
    reads), or with margin -1 only the texels wholly inside it - in t, and moved by `move` in other (both clipped
    alike); None when nothing is left."""
    s = 1 << m
    a = texels(t, m)
    b = texels(other, m) if other is not None else a
    mx, my = move[0] // s, move[1] // s
    if margin >= 0:
        x0, y0 = int(np.floor(r[0] / s)) - margin, int(np.floor(r[1] / s)) - margin
        x1, y1 = int(np.ceil(r[2] / s)) + margin, int(np.ceil(r[3] / s)) + margin
    else:
        x0, y0 = int(np.ceil(r[0] / s)), int(np.ceil(r[1] / s))
        x1, y1 = int(np.floor(r[2] / s)), int(np.floor(r[3] / s))
    x0, y0 = max(x0, 0, -mx), max(y0, 0, -my)
    x1 = min(x1, a.shape[1], b.shape[1] - mx)
    y1 = min(y1, a.shape[0], b.shape[0] - my)
    if x1 <= x0 or y1 <= y0:
        return None
    return a[y0:y1, x0:x1], b[y0 + my:y1 + my, x0 + mx:x1 + mx]


def correlation(a: np.ndarray, b: np.ndarray) -> float:
    if np.array_equal(a, b):
        return 1.0
    af, bf = a.astype(float).ravel(), b.astype(float).ravel()
    if not af.std() or not bf.std():
        return 0.0
    return float(np.corrcoef(af, bf)[0, 1])


PASS = 0.99         # a level 0..D image not equal texel for texel passes at this correlation (the task's bar)


def prove(result: Result, emitters: dict[str, tuple[bytes, str]], sources: dict[str, Source]) -> list[dict[str, Any]]:
    """Per drawn TextureInfo channel: its image read through the re-pointed fields (the partition read back, not the
    build's numbers) from the merged atlas, against the one read through the old fields from its source atlas.
    Levels 0..D with the texel bilinear filtering reads around it: equal (format 10: always; BC5: unless deduplicated
    onto another level's encoding of the image) or correlating PASS - this decides 'ok'. Every deeper level: the
    texels wholly inside the image, the share equal and the correlation (reported: a mip texel at the image's edge
    mixes in the neighbour, which differs by design, and DICE's atlases do the same)."""
    src_tex: dict[tuple[str, str], Tex] = {}
    merged = {'alpha': read_tex(result.alpha), 'normals': read_tex(result.normals)}
    out = []
    reread = {n: {(i.instance, i.name): i for i in infos(dbx.parse(d))} for n, d in result.partitions.items()}
    spawn = {n: spawns_frames(dbx.parse(d)) for n, (d, _) in emitters.items()}
    for u in result.uses:
        key = (u.channel, u.source)
        if key not in src_tex:
            s = sources[u.source]
            src_tex[key] = read_tex(s.alpha if u.channel == 'alpha' else s.normals)   # type: ignore[arg-type]
        st, mt = src_tex[key], merged[u.channel]
        info = reread[u.emitter][(u.instance, u.info)]
        r_new = rect(info, u.channel, spawn[u.emitter], mt.w, mt.h)
        move = (round(r_new[0] - u.rect[0]), round(r_new[1] - u.rect[1]))
        size_ok = abs((r_new[2] - r_new[0]) - (u.rect[2] - u.rect[0])) < 1e-3 and             abs((r_new[3] - r_new[1]) - (u.rect[3] - u.rect[1])) < 1e-3 and             abs(r_new[0] - u.rect[0] - move[0]) < 1e-3 and abs(r_new[1] - u.rect[1] - move[1]) < 1e-3
        near: list[dict[str, Any]] = []
        deep: list[dict[str, Any]] = []
        for m in range(min(len(st.levels), len(mt.levels))):
            c = crops(st, m, u.rect, move, mt, 1 if m <= D else -1)
            if c is None:
                continue
            a, b = c
            row = {'level': m, 'texels': int(a.shape[0] * a.shape[1]), 'equal': bool(np.array_equal(a, b)),
                   'equal_share': round(float((a == b).reshape(a.shape[0], a.shape[1], -1).all(-1).mean()), 4),
                   'correlation': round(correlation(a, b), 4)}
            (near if m <= D else deep).append(row)
        ok = size_ok and bool(near) and all(x['equal'] or x['correlation'] >= PASS for x in near)
        c0 = crops(st, 0, u.rect, move, mt)
        out.append({'emitter': u.emitter, 'instance': u.instance, 'channel': u.channel, 'source': u.source,
                    'rect': [round(v, 3) for v in u.rect], 'move': list(move), 'size_kept': size_ok,
                    'near': near, 'deep': deep, 'ok': ok,
                    'exact': ok and all(x['equal'] for x in near),
                    'sha1': hashlib.sha1(c0[0].tobytes()).hexdigest()[:12] if c0 else None})
    return out


def same_but_uv(old: bytes, new: bytes) -> list[str]:
    """The differences between two partitions' element trees other than the TextureInfo UV fields (empty: the
    re-pointed partition keeps everything else)."""
    a, b = dbx.parse(old), dbx.parse(new)
    diffs: list[str] = []

    def walk(x: dbx.Element, y: dbx.Element, path: str) -> None:
        if (x.tag, x.kind, x.attributes, x.text) != (y.tag, y.kind, y.attributes, y.text):
            diffs.append(path)
            return
        if x.raw != y.raw and (x.attr('name') not in UV + NM):
            diffs.append(path + '/' + (x.attr('name') or x.tag))
        xc, yc = x.children or [], y.children or []
        if len(xc) != len(yc):
            diffs.append(path + ' (children)')
            return
        for i, (p, q) in enumerate(zip(xc, yc)):
            walk(p, q, f'{path}/{p.attr("name") or p.attr("guid") or p.tag}[{i}]')
    if len(a.roots) != len(b.roots):
        return ['roots']
    for x, y in zip(a.roots, b.roots):
        walk(x, y, '')
    return diffs
