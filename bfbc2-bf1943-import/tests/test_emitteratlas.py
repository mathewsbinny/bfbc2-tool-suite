r"""emitteratlas.py, the merged emitter atlas (quick tier: synthetic atlases and emitter partitions, no game data).

- the textures: an 8-bit atlas written with DICE's level rule and row padding reads back to the same bytes (the
  suite's texture.decode included); a BC5 one block for block;
- the frames drawn: every frame when animated or a SpawnAnimationFrameData picks one, else frame 0; the fields of
  no image (origin 0, step 0 or 1) are left alone;
- packing: the cells land on the grid without overlap, keep their sources' alignment where it fits, and a set too
  big for the atlas is refused;
- the build: three emitters from two source atlases (one image in both, at other places) - the image found once,
  every emitter's frames read through its new fields equal its source's (levels 0..K: texels exactly), the
  partitions' element trees unchanged but for the texture fields, the new values exact in float32;
- the round trip: the merged atlases and partitions read back (prove) as built.

  python test_emitteratlas.py
"""
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('bf1943-import', 'console-convert', 'model-tools', 'core')
import emitteratlas as ea  # noqa: E402
import texture  # noqa: E402
import texwrite  # noqa: E402
from bfbc2core import dbx  # noqa: E402

BOOL = 6
S = 256             # the synthetic alpha atlases' side; normals: N x N
N = 256


def l8_atlas(img):
    """An 8-bit atlas as DICE's: every level the rounded means, down to a side of 2."""
    lv = [img]
    while len(lv) < texwrite.atlas_levels(min(img.shape)):
        lv.append(texwrite.l8_down(lv[-1]))
    return texwrite.resource(ea.L8, 0, img.shape[1], img.shape[0], [texwrite.l8_bytes(x) for x in lv])


def bc5_atlas(rng, side):
    levels, w = [], side
    for _ in range(texwrite.atlas_levels(side)):
        bw = max(w // 4, 1)
        levels.append(rng.integers(0, 256, (bw * bw * 16,), dtype=np.uint8).tobytes())
        w //= 2
    return texwrite.resource(ea.BC5, 0, side, side, levels)


def field(name, v):
    return dbx.Element('field', dbx.NUMBERS, [('name', name)], raw=b'\x01\x04' + struct.pack('>f', v))


def flag(name, v):
    return dbx.Element('field', BOOL, [('name', name)], raw=bytes([1, int(v)]))


def ref(name, r):
    return dbx.Element('field', dbx.TEXT, [('ref', r), ('name', name)])


def emitter(guid, uv, nm, frames=1, cols=1, animated=False, spawn=False):
    """An emitter partition: an UpdateTextureCoordsData with one TextureInfo1 (and a SpawnAnimationFrameData)."""
    info = dbx.Element('complex', dbx.CONTAINER, [('name', 'TextureInfo1')], children=[
        ref('Texture', 'textures/x/a/00000000-0000-0000-0000-000000000001'), ref('Normalmap', 'null'),
        field('AnimationFrameCount', float(frames)), field('AnimationFrameColumnCount', float(cols))] +
        [field(k, v) for k, v in zip(ea.UV + ea.NM, uv + nm)] + [flag('EnableAnimation', animated)])
    inst = [dbx.Element('instance', dbx.CONTAINER, [('guid', guid + '-0000-0000-0000-000000000001'),
                                                    ('type', 'Emitter.UpdateTextureCoordsData')],
                        children=[flag('Enable', True), info, field('TextureBlendValue', 1.0)])]
    if spawn:
        inst.append(dbx.Element('instance', dbx.CONTAINER, [('guid', guid + '-0000-0000-0000-000000000002'),
                                                            ('type', 'Emitter.SpawnAnimationFrameData')],
                                children=[flag('Enable', True)]))
    part = dbx.Element('partition', dbx.CONTAINER, [('guid', guid + '-0000-0000-0000-000000000000'),
                                                    ('primaryInstance', inst[0].attributes[0][1]), ('exportMode', 'All')],
                       children=inst)
    strings = ['']
    return dbx.write(dbx.DbxFile(strings, [part]))


def main(argv):
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    rng = np.random.default_rng(1943)

    # ---------------------------------------------------------------- textures
    img = rng.integers(0, 256, (S, S), dtype=np.uint8)
    res = l8_atlas(img)
    t = ea.read_tex(res)
    check(ea.write_tex(t) == res, 'an 8-bit atlas reads and writes back byte for byte (rows of the 2 x 2 level padded)')
    lv = texture.levels(res)
    check(len(lv) == 8 and len(lv[-1][4]) == 8 and lv[-1][2] == 2, 'DICE\'s level layout: down to 2 x 2, stored 8 bytes')
    check(np.array_equal(texture.decode(lv[-1][4], 2, 2, ea.L8)[..., 0], ea.texels(t, 7)),
          'texture.decode reads format 10 (padded rows)')
    check(np.array_equal(texwrite.l8_down(np.array([[1, 2], [3, 4]], np.uint8)), [[3]]), 'l8_down: (a+b+c+d+2)//4')
    nres = bc5_atlas(rng, N)
    check(ea.write_tex(ea.read_tex(nres)) == nres, 'a BC5 atlas reads and writes back block for block')

    # ---------------------------------------------------------------- frames, fields of no image
    f = dbx.parse(emitter('11111111', (0.5, 0.5, 0.125, 0.125), (0.0, 0.0, 1.0, 1.0), frames=4, cols=2))
    i = ea.infos(f)[0]
    check(ea.grid(i, False) == (1, 1), 'not animated, no spawn frame: frame 0 only')
    check(ea.grid(i, True) == (2, 2), 'a spawn frame: the 2 x 2 flipbook')
    check(ea.drawn(i, 'alpha') and not ea.drawn(i, 'normals'), 'normal fields 0, 0 / 1, 1: no image, left alone')
    f2 = dbx.parse(emitter('11111112', (0.5, 0.5, 0.125, 0.125), (0.0, 0.0, 0.0, 0.0), frames=4, cols=2, animated=True))
    check(ea.grid(ea.infos(f2)[0], False) == (2, 2), 'animated: every frame')

    # ---------------------------------------------------------------- packing
    cells = [ea.Cell('a', 8, 16, 64, 32), ea.Cell('b', 0, 0, 32, 32), ea.Cell('c', 40, 8, 48, 48), ea.Cell('d', 128, 128, 16, 8)]
    check(ea.pack(cells, S, S, ea.ALIGN), 'four cells pack into a 256 x 256 atlas')
    occ = np.zeros((S, S), int)
    for c in cells:
        assert c.at is not None
        occ[c.at[1]:c.at[1] + c.h, c.at[0]:c.at[0] + c.w] += 1
    check(occ.max() == 1, 'no two cells overlap')
    check(all(c.at is not None and c.at[0] % ea.GRID == 0 and c.at[1] % ea.GRID == 0 for c in cells), 'cells on the grid')
    check(all(c.at is not None and (c.at[0] - c.x) % 16 == 0 and (c.at[1] - c.y) % 16 == 0 for c in cells),
          'each cell moved by a multiple of 16 at least (its source\'s alignment kept)')
    check(not ea.pack([ea.Cell('a', 0, 0, 200, 200), ea.Cell('b', 0, 0, 200, 200)], S, S), 'cells too big: refused')

    # ---------------------------------------------------------------- the build
    a = np.zeros((S, S), np.uint8)
    b = np.zeros((S, S), np.uint8)
    pic = rng.integers(1, 256, (32, 32), dtype=np.uint8)            # one image in both atlases
    flip = rng.integers(1, 256, (64, 64), dtype=np.uint8)           # a 2 x 2 flipbook in b
    a[64:96, 32:64] = pic
    b[160:192, 96:128] = pic
    b[0:64, 128:192] = flip
    a[128:160, 128:160] = 255 - pic
    nm = bc5_atlas(rng, N)
    sources = {'a': ea.Source(l8_atlas(a), nm), 'b': ea.Source(l8_atlas(b), bc5_atlas(rng, N))}
    # sampled 3 texels inside the slots, as DICE does
    em = {'fx/a1.dbx': (emitter('aaaaaaa1', ((32 + 3) / S, (64 + 3) / S, 26 / S, 26 / S),
                                ((16 + 3) / N, (16 + 3) / N, 10 / N, 10 / N)), 'a'),
          'fx/b1.dbx': (emitter('bbbbbbb1', ((96 + 3) / S, (160 + 3) / S, 26 / S, 26 / S),
                                (0.0, 0.0, 1.0, 1.0)), 'b'),
          'fx/b2.dbx': (emitter('bbbbbbb2', ((128 + 3) / S, 3 / S, 29 / S, 29 / S), (3 / N, 3 / N, 13 / N, 13 / N),
                                frames=4, cols=2, animated=True), 'b'),
          'fx/a2.dbx': (emitter('aaaaaaa2', ((128 + 3) / S, (128 + 3) / S, 26 / S, 26 / S), (0.0, 0.0, 0.0, 0.0)), 'a')}
    like_a = l8_atlas(np.zeros((S, S), np.uint8))
    like_n = bc5_atlas(rng, N)
    r = ea.build(em, sources, like_a, like_n)
    check(len(r.cells['alpha']) == 3, 'the image both atlases hold is kept once', str(r.report))
    check(r.sizes['alpha'] == (S, S) and r.sizes['normals'] == (N, N), 'the like atlases\' sizes', str(r.sizes))
    hdr = struct.unpack_from('<8I', r.alpha, 0)
    check(hdr[2:8] == (ea.L8, 0, S, S, 0, 8), 'the merged alpha atlas: format 10, flags 0, the like size, 8 levels', str(hdr))
    proof = ea.prove(r, em, sources)
    check(len(proof) == 6, 'six drawn images (4 alpha, 2 normals)', str(len(proof)))
    check(all(p['ok'] and p['exact'] for p in proof), 'every image read through its new fields equals its source\'s',
          str([p for p in proof if not p['exact']]))
    check(all(all(x['equal'] for x in p['deep'] if x['level'] <= ea.K) for p in proof if p['channel'] == 'alpha'),
          'levels 2..K: equal inside the image')
    check(all(not ea.same_but_uv(em[n][0], d) for n, d in r.partitions.items()), 'element trees unchanged but the fields')
    # the frame layout kept: b2's steps in texels unchanged, its flipbook moved whole
    old = ea.infos(dbx.parse(em['fx/b2.dbx'][0]))[0]
    new = ea.infos(dbx.parse(r.partitions['fx/b2.dbx']))[0]
    check(new.frames == 4 and new.columns == 2 and new.values['AnimationStepU'] == old.values['AnimationStepU'],
          'a flipbook keeps its frame count, columns and step')
    left = ea.infos(dbx.parse(r.partitions['fx/b1.dbx']))[0]
    check(all(left.values[k] == v for k, v in zip(ea.NM, (0.0, 0.0, 1.0, 1.0))), 'fields of no image left as they were')
    for n, d in r.partitions.items():
        for x in ea.infos(dbx.parse(d)):
            for k, v in x.values.items():
                px = v * (S if k in ea.UV else N)
                if k in ('TextureOriginU', 'TextureOriginV') and abs(px - round(px * 2) / 2) > 1e-6:
                    check(False, f'{n} {k}: origin off the half-texel grid', str(px))
    # a source image moved by whole texels: the merged texels at the new origin are the source's at the old one
    a1o, a1n = ea.infos(dbx.parse(em['fx/a1.dbx'][0]))[0], ea.infos(dbx.parse(r.partitions['fx/a1.dbx']))[0]
    mt = ea.read_tex(r.alpha)
    x0, y0 = round(a1n.values['TextureOriginU'] * S), round(a1n.values['TextureOriginV'] * S)
    sx, sy = round(a1o.values['TextureOriginU'] * S), round(a1o.values['TextureOriginV'] * S)
    check(np.array_equal(ea.texels(mt, 0)[y0:y0 + 26, x0:x0 + 26], a[sy:sy + 26, sx:sx + 26]),
          'the round trip: the merged atlas read back holds a1\'s image at its new fields')
    try:
        ea.build({'fx/x.dbx': (em['fx/a1.dbx'][0], 'nowhere')}, sources, like_a, like_n)
        check(False, 'an emitter of an unknown source is refused')
    except ea.EmitterAtlasError:
        check(True, 'an emitter of an unknown source is refused')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
