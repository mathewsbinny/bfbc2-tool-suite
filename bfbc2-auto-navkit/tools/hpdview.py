"""hpdview.py - draw one sector of a pathdata.hpd top-down (x right, z up) to a PNG.

Usage: python hpdview.py <pathdata.hpd> <sector index | 0xID> <out.png>

Colours: grey = tile grid, black = E lines, blue = D lines, green = B portals (to areas in the same
sector), red = C portals (sector border), orange = graph nodes, light orange = graph edges.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

import hpdtool

u32 = lambda m, o: struct.unpack_from('<I', m, o)[0]
f32 = lambda m, o: struct.unpack_from('<f', m, o)[0]


def segs_from_lists(m, o, count_off, table_off):
    """D/E lists: index table of count+1 offsets (relative to the object), data = 3-D points."""
    cnt = u32(m, o + count_off)
    out = []
    if not cnt:
        return out
    tab = o + u32(m, o + table_off)
    idx = [u32(m, tab + j * 4) for j in range(cnt + 1)]
    for j in range(cnt):
        a, b = o + idx[j], o + idx[j + 1]
        pts = [struct.unpack_from('<3f', m, q) for q in range(a, b - 11, 12)]
        out.append(pts)
    return out


def main():
    path, which, out = sys.argv[1], sys.argv[2], sys.argv[3]
    f = hpdtool.parse(open(path, 'rb').read())
    if which.startswith('0x'):
        s = next(x for x in f.sectors if x.id == int(which, 16))
    else:
        s = f.sectors[int(which)]
    m = s.mesh
    _, tiles, objects, _, _ = hpdtool.mesh_layout(m)
    fig, ax = plt.subplots(figsize=(12, 12))
    x0, x1, z0, z1 = (f32(m, 0x28 + i * 4) for i in range(4))
    nx, nz = u32(m, 0x38), u32(m, 0x3C)
    for i in range(nx + 1):
        x = x0 + (x1 - x0) * i / nx
        ax.plot([x, x], [z0, z1], color='0.85', lw=0.8)
    for i in range(nz + 1):
        z = z0 + (z1 - z0) * i / nz
        ax.plot([x0, x1], [z, z], color='0.85', lw=0.8)
    for o in objects:
        for pts in segs_from_lists(m, o, 0x40, 0x44):
            ax.plot([p[0] for p in pts], [p[2] for p in pts], color='black', lw=0.6)
        for pts in segs_from_lists(m, o, 0x38, 0x3C):
            ax.plot([p[0] for p in pts], [p[2] for p in pts], color='tab:blue', lw=0.6)
        nb = u32(m, o + 0x28)
        pos = o + 0x50
        for k in range(nb):
            r = pos + k * 0x20
            ax.plot([f32(m, r), f32(m, r + 12)], [f32(m, r + 8), f32(m, r + 20)], color='tab:green', lw=2)
        pos += nb * 0x20
        for k in range(u32(m, o + 0x30)):
            r = pos + k * 0x2C
            ax.plot([f32(m, r), f32(m, r + 12)], [f32(m, r + 8), f32(m, r + 20)], color='tab:red', lw=2)
    for e in s.edges:
        a, b = s.nodes[e.src].pos, s.nodes[e.dst].pos
        ax.plot([a[0], b[0]], [a[2], b[2]], color='moccasin', lw=0.8, zorder=1)
    ax.scatter([n.pos[0] for n in s.nodes], [n.pos[2] for n in s.nodes], s=6, color='tab:orange', zorder=3)
    ax.set_aspect('equal')
    ax.set_title(f'sector {s.id:#06x}: {len(objects)} areas, {len(s.nodes)} nodes, {len(s.edges)} edges')
    fig.savefig(out, dpi=110, bbox_inches='tight')
    print(f'wrote {out}')


if __name__ == '__main__':
    main()
