r"""portparity.py - how close our Battlefield 1943 conversions come to DICE's own. BFBC2 ships PC versions of many
BF1943 assets (the campaign prologue's Zero and Corsair, the Japanese island's props and trees, weapons, soldiers):
DICE converted the same models. Each is converted again here with our tools and compared with DICE's, element by
element - a measure of the tools and a list of what they still do differently.

    python portparity.py [name part ...] [-o report.txt]      (default: every model both games have)

Meshes (meshport.Porter.port, BF1943 -> PC MeshData, per LOD and part):
  identical       the bytes are DICE's
  precision       the same model, every element within the formats' precision (positions equal, normal frames
                  within PRECISION_DEG - DICE built its PC normals from the source art, not from the Xbox's packed
                  11:11:10 ones, so about 0.1 degree is the console data's own rounding, not a converter fault)
  differs         same vertex and triangle counts and index buffer, some element beyond that (named, with its
                  largest difference)
  other-model     other vertex / triangle counts: DICE's file is another revision of the model (no element
                  comparison possible)
The mesh set (meshset): LOD count, composite part names and transforms.
Textures (texture.convert): format, size, mip count; the picture compared at the smaller size (PSNR, dB). DICE's PC
textures are often larger: made from the source art, not from the Xbox's.

Findings this tool produced are kept in the OPFOR project's research\WW2_VISUAL_PARITY.md (tool parity).
"""
import argparse
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in ('core', 'bfbc2-console-convert', 'bfbc2-anim-tools', 'bfbc2-mesh-viewer', 'bfbc2-model-tools'):
    sys.path.insert(0, os.path.join(SUITE, p))
sys.path.insert(0, HERE)
import mesh  # noqa: E402
import meshset  # noqa: E402
import texture  # noqa: E402

PRECISION_DEG = 0.25        # normal frame: the 11:11:10 console packing's rounding (DICE's are from the source art)
PRECISION_POS = 1e-4        # metres
PRECISION_UV = 1e-3
# vertex element format -> numpy dtype of its components (PC formats)
DTYPE = {0x02: ('<f4', 3), 0x03: ('<f4', 3), 0x04: ('<f4', 4), 0x05: ('<f2', 1), 0x06: ('<f2', 2), 0x07: ('<f2', 3),
         0x08: ('<f2', 4), 0x0C: ('u1', 4)}
POSITION, BLEND_INDICES, TEXCOORD = 1, 2, 0
NORMAL_FRAME = (4, 5, 6)


def _element(raw, off, fmt):
    dt, n = DTYPE[fmt]
    size = np.dtype(dt).itemsize * n
    return raw[:, off:off + size].copy().view(dt).reshape(len(raw), n).astype(np.float64)


def compare_part(ours, dice, po, pd, layout):
    """One part's verdict and per-element notes (ours / dice: MeshData bytes; po / pd: their part records)."""
    if (po['vertices'], po['triangles'], po['stride']) != (pd['vertices'], pd['triangles'], pd['stride']):
        return 'other-model', [f"{po['vertices']} / {po['triangles']} verts / tris, DICE {pd['vertices']} / "
                               f"{pd['triangles']}"]
    n, st = po['vertices'], po['stride']
    a = np.frombuffer(ours[0], np.uint8, n * st, ours[1] + po['vertex_offset']).reshape(n, st)
    b = np.frombuffer(dice[0], np.uint8, n * st, dice[1] + pd['vertex_offset']).reshape(n, st)
    if np.array_equal(a, b):
        return 'identical', []
    notes, beyond = [], []
    for fmt, usage, off, stream in mesh.elements(layout)[0]:
        if stream != 0 or fmt not in DTYPE or np.array_equal(a[:, off:off + 1], b[:, off:off + 1]) and \
                np.array_equal(_element(a, off, fmt), _element(b, off, fmt)):
            continue
        x, y = _element(a, off, fmt), _element(b, off, fmt)
        if usage in NORMAL_FRAME:
            u, v = x[:, :3], y[:, :3]
            cos = (u * v).sum(1) / np.maximum(np.linalg.norm(u, axis=1) * np.linalg.norm(v, axis=1), 1e-12)
            worst = float(np.degrees(np.arccos(np.clip(cos.min(), -1, 1))))
            text = f'usage {usage} @{off}: up to {worst:.3f} deg'
            (beyond if worst > PRECISION_DEG else notes).append(text)
        elif usage == BLEND_INDICES:
            k = int((x != y).any(1).sum())
            beyond.append(f'part / bone indices @{off}: {k} vertices differ')
        else:
            d = np.abs(x - y)
            worst = float(d.max())
            tol = PRECISION_POS if usage == POSITION else PRECISION_UV if usage == TEXCOORD else PRECISION_UV
            text = f'usage {usage} @{off}: up to {worst:.4g}'
            if usage == POSITION and worst > tol:
                shift = (x[:, :3] - y[:, :3]).mean(0)
                if np.abs((x[:, :3] - y[:, :3]) - shift).max() < 1e-3:
                    text += f' (one offset for every vertex: {np.round(shift, 3).tolist()} - the part rebased)'
            (beyond if worst > tol else notes).append(text)
    return ('differs' if beyond else 'precision'), beyond + notes


def compare_mesh(port, ix, name):
    """{lod: {part: (verdict, notes)}}, mesh set notes, texture rows for one model both games have."""
    r = port.port(name)
    low = name.lower()
    out = {'mesh': name, 'kind': r['kind'], 'lods': [], 'meshset': [], 'textures': []}
    dice_ms = ix.read(low + '.res')
    if r['meshset'] == dice_ms:
        out['meshset'].append('identical')
    else:
        try:
            a, b = meshset.parse(r['meshset'], r['kind'], False), meshset.parse(dice_ms, r['kind'], False)
            if len(a['lods']) != len(b['lods']):
                out['meshset'].append(f"{len(a['lods'])} LODs, DICE {len(b['lods'])}")
            ca, cb = a['lods'][0].get('parts') or [], b['lods'][0].get('parts') or []
            if [c[0] for c in ca] != [c[0] for c in cb]:
                out['meshset'].append('composite part names / order differ')
            else:
                worst = max((float(np.abs(np.array(x[3], float) - np.array(y[3], float)).max()) for x, y in zip(ca, cb)),
                            default=0.0)
                if worst > 1e-4:
                    out['meshset'].append(f'composite part transforms differ by up to {worst:.4g}')
            if not out['meshset']:
                out['meshset'].append('same LODs and parts (other bytes: sizes, flags)')
        except Exception as e:                                      # noqa: BLE001 - reported, the rest goes on
            out['meshset'].append(f'not compared ({e})')
    for lod, ours in enumerate(r['lods']):
        dice = ix.read(f'{low}_lod{lod}_data.res')
        row = {'lod': lod, 'parts': {}}
        out['lods'].append(row)
        if dice is None:
            row['note'] = 'DICE has no such LOD'
            continue
        if ours == dice:
            row['identical'] = True
            continue
        mo, md = mesh.parse(ours, False), mesh.parse(dice, False)
        names_d = {p['name']: p for p in md['parts']}
        # DICE renamed some parts in its versions (the soldiers' 'lod0_..._Body'): by name, else by place when both
        # have as many parts
        by_place = len(mo['parts']) == len(md['parts'])
        for i, po in enumerate(mo['parts']):
            pd = names_d.get(po['name']) or (md['parts'][i] if by_place else None)
            pn = po['name'].decode('latin-1')
            if pd is None:
                row['parts'][pn] = ('missing', [f'DICE has no part of this name ({len(md["parts"])} parts, ours '
                                                f'{len(mo["parts"])})'])
                continue
            if pd['name'] != po['name']:
                pn += f' (DICE: {pd["name"].decode("latin-1")})'
            row['parts'][pn] = compare_part((ours, mo['data']), (dice, md['data']), po, pd, r['pc_layouts'][lod][i])
        io, idd = ours[mo['data'] + mo['vertex_bytes']:], dice[md['data'] + md['vertex_bytes']:]
        row['index_buffer'] = 'same' if io == idd else 'differs'
    for t, tb in sorted(r['textures'].items()):
        out['textures'].append(compare_texture(t, tb, ix.read(t.lower())))
    return out


def _level0(pc):
    import struct
    fmt = struct.unpack_from('<8I', pc, 0)[2]
    for s, m, w, h, data in texture.levels(pc):
        if m == 0 and s == 0:
            img = texture.decode(data, w, h, fmt).astype(np.float64)
            return img[..., :3] if img.ndim == 3 else img
    return None


def compare_texture(name, ours, dice):
    import struct
    if dice is None:
        return {'texture': name, 'verdict': 'DICE has none'}
    ho, hd = struct.unpack_from('<8I', ours, 0), struct.unpack_from('<8I', dice, 0)
    row = {'texture': name, 'ours': f'{ho[4]}x{ho[5]} fmt {ho[2]} mips {ho[7]}',
           'dice': f'{hd[4]}x{hd[5]} fmt {hd[2]} mips {hd[7]}'}
    if ours == dice:
        row['verdict'] = 'identical'
        return row
    try:
        a, b = _level0(ours), _level0(dice)
        while a.shape[0] > b.shape[0] and a.shape[1] > b.shape[1]:
            a = (a[0::2, 0::2] + a[1::2, 0::2] + a[0::2, 1::2] + a[1::2, 1::2]) / 4
        while b.shape[0] > a.shape[0] and b.shape[1] > a.shape[1]:
            b = (b[0::2, 0::2] + b[1::2, 0::2] + b[0::2, 1::2] + b[1::2, 1::2]) / 4
        if a.shape != b.shape:
            row['verdict'] = 'other shape'
            return row
        mse = float(((a - b) ** 2).mean())
        row['psnr'] = round(10 * np.log10(255 ** 2 / mse), 1) if mse else float('inf')
        row['verdict'] = ('DICE larger' if hd[4] > ho[4] else 'DICE smaller' if hd[4] < ho[4] else 'same size') + \
            (', same picture' if row['psnr'] >= 35 else ', other picture' if row['psnr'] < 20 else ', close')
    except Exception as e:                                          # noqa: BLE001
        row['verdict'] = f'not compared ({e})'
    return row


def shared_meshes(src, ix):
    bf = {k[:-len('_lod0_data.res')] for k in src.entries if k.endswith('_lod0_data.res')}
    bc = {k[:-len('_lod0_data.res')] for k in ix.where if k.endswith('_lod0_data.res')}
    return {k: k for k in sorted(bf & bc) if k + '.res' in src.entries}


def summary(rows):
    import collections
    parts = collections.Counter()
    for r in rows:
        for L in r.get('lods', []):
            if L.get('identical'):
                parts['identical (whole LOD)'] += 1
            for v, _ in L.get('parts', {}).values():
                parts[v] += 1
    tex = collections.Counter(t['verdict'] for r in rows for t in r.get('textures', []))
    return parts, tex


def main(argv=None):
    import bf1943import
    import meshport
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools'))
    import meshview
    import shaderdb
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('names', nargs='*')
    ap.add_argument('-o', default='portparity.txt')
    a = ap.parse_args(argv)
    src = bf1943import.Source(bf1943import.default_source())
    ix = meshview.Index(shaderdb.find_opfor(None))
    port = meshport.Porter(src, ix)
    names = shared_meshes(src, ix)
    todo = [n for k, n in sorted(names.items()) if not a.names or any(x.lower() in k for x in a.names)]
    rows = []
    for n in todo:
        try:
            rows.append(compare_mesh(port, ix, n))
        except Exception as e:                                      # noqa: BLE001 - one model's failure is a row
            rows.append({'mesh': n, 'error': f'{type(e).__name__}: {e}'})
        print(f'  {n}', flush=True)
    json.dump(rows, open(os.path.splitext(a.o)[0] + '.json', 'w'), indent=1)
    parts, tex = summary(rows)
    with open(a.o, 'w') as f:
        f.write(f'{len(rows)} models both games have; parts: {dict(parts)}; textures: {dict(tex)}\n\n')
        for r in rows:
            f.write(f"{r['mesh']}: {r.get('error') or '; '.join(r['meshset'])}\n")
            for L in r.get('lods', []):
                if L.get('identical'):
                    f.write(f"  lod{L['lod']}: identical\n")
                    continue
                if L.get('note'):
                    f.write(f"  lod{L['lod']}: {L['note']}\n")
                for pn, (v, notes) in L.get('parts', {}).items():
                    f.write(f"  lod{L['lod']} {pn}: {v}" + (f" - {'; '.join(notes)}" if notes else '') + '\n')
                if 'index_buffer' in L:
                    f.write(f"  lod{L['lod']} index buffer: {L['index_buffer']}\n")
            for t in r.get('textures', []):
                f.write(f"  texture {t['texture']}: {t['verdict']} (ours {t.get('ours')}, DICE {t.get('dice')}, "
                        f"PSNR {t.get('psnr')})\n")
    print(f'{len(rows)} models; parts {dict(parts)}; textures {dict(tex)} -> {a.o}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
