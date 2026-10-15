"""NavKit: are a level's AI order positions on the walkable areas / route network of a pathdata.hpd?

Collects AreaOrder / PatrolOrder / InterludeOrder / FireAtArea positions and the patrol waypoints
(AreaGeometryEntityData chains referenced by PatrolOrders) from the converted level partition, then for
each pathdata.hpd given reports per point: inside an area (horizontal, with height tolerance) and the
route-network group of the nearest node.

Usage: ordercheck.py <level.xml> <a.hpd> [<b.hpd> ...]
"""
import math
import os
import re
import sys
from collections import Counter, defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hpdtool  # noqa: E402
import kymesh  # noqa: E402

Y_OFFSET = 0.95


def order_points(xml_path):
    x = open(xml_path, encoding='utf-8', errors='replace').read()
    inst = {}
    for m in re.finditer(r'<instance guid="([^"]*)" type="([^"]*)">(.*?)</instance>', x, re.S):
        inst[m.group(1).lower()] = (m.group(2), m.group(3))

    def pos(body):
        t = re.search(r'<complex name="Transform">([^<]*)</complex>', body)
        if not t:
            return None
        v = [float(a) for a in t.group(1).split('/')]
        return (v[12], v[13], v[14]) if any(abs(c) > 1e-3 for c in v[12:15]) else None

    pts = []
    for guid, (typ, body) in inst.items():
        short = typ.split('.')[-1]
        if short in ('AreaOrderEntityData', 'PatrolOrderEntityData', 'InterludeOrderEntityData',
                     'FireAtAreaOrderEntityData'):
            name = (re.search(r'<field name="Name">([^<]*)</field>', body) or [None, ''])[1] if re.search(r'<field name="Name">([^<]*)</field>', body) else ''
            p = pos(body)
            if p:
                pts.append((short.replace('EntityData', ''), name, p))
            if short == 'PatrolOrderEntityData':
                for ref in re.findall(r'ref="([0-9a-f-]{36})"', body):
                    seen = set()
                    g = ref.lower()
                    while g in inst and g not in seen and inst[g][0].endswith('AreaGeometryEntityData'):
                        seen.add(g)
                        q = pos(inst[g][1])
                        if q:
                            pts.append(('PatrolWaypoint', name, q))
                        nxt = re.search(r'<field ref="([0-9a-f-]{36})" name="Next"', inst[g][1])
                        g = nxt.group(1).lower() if nxt else ''
    # unique
    out, seen = [], set()
    for k, n, p in pts:
        key = (k, round(p[0], 1), round(p[2], 1))
        if key not in seen:
            seen.add(key)
            out.append((k, n, p))
    return out


class PathData:
    def __init__(self, path):
        f = hpdtool.parse(open(path, 'rb').read())
        self.areas = []      # (minx, maxx, minz, maxz, miny, maxy, segs)
        for s in f.sectors:
            m = kymesh.parse_mesh(s.mesh)
            for a in m.areas:
                segs = [(l[k], l[k + 1]) for l in a.outline for k in range(len(l) - 1)]
                segs += [(b.p0, b.p1) for b in a.portals] + [(c.p0, c.p1) for c in a.border_portals]
                if not segs:
                    continue
                xs = [p[0] for sg in segs for p in sg]
                zs = [p[2] for sg in segs for p in sg]
                ys = [p[1] for sg in segs for p in sg]
                self.areas.append((min(xs), max(xs), min(zs), max(zs), min(ys) - Y_OFFSET, max(ys) - Y_OFFSET, segs))
        # graph groups (shared border node ids join sectors)
        parent = {}

        def find(a):
            while parent.setdefault(a, a) != a:
                parent[a] = parent[parent[a]]
                a = parent[a]
            return a
        self.nodes = []
        for s in f.sectors:
            for e in s.edges:
                parent[find(s.nodes[e.src].id)] = find(s.nodes[e.dst].id)
            for n in s.nodes:
                self.nodes.append((n.id, n.pos))
        self.group = {nid: find(nid) for nid, _ in self.nodes}
        sizes = Counter(self.group.values())
        self.group_size = sizes
        self.pos = np.array([p for _, p in self.nodes])
        self.ids = [nid for nid, _ in self.nodes]

    def on_area(self, p, ytol=2.5):
        x, y, z = p
        for x0, x1, z0, z1, y0, y1, segs in self.areas:
            if not (x0 <= x <= x1 and z0 <= z <= z1 and y0 - ytol <= y <= y1 + ytol):
                continue
            inside = False
            for (ax, _, az), (bx, _, bz) in segs:
                if az == bz:
                    continue
                if min(az, bz) <= z < max(az, bz) and x < ax + (z - az) * (bx - ax) / (bz - az):
                    inside = not inside
            if inside:
                return True
        return False

    def nearest_group(self, p, radius=15.0):
        d = np.hypot(self.pos[:, 0] - p[0], self.pos[:, 2] - p[2])
        d = np.where(np.abs(self.pos[:, 1] - Y_OFFSET - p[1]) <= 4.0, d, 1e9)
        k = int(np.argmin(d))
        if d[k] > radius:
            return None, None
        g = self.group[self.ids[k]]
        return g, self.group_size[g]


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 1
    pts = order_points(argv[1])
    print(f'{len(pts)} order / waypoint positions: {dict(Counter(k for k, _, _ in pts))}')
    results = {}
    for path in argv[2:]:
        pdata = PathData(path)
        biggest = max(pdata.group_size.values())
        res = []
        for k, n, p in pts:
            on = pdata.on_area(p)
            g, size = pdata.nearest_group(p)
            res.append((on, size))
        results[path] = res
        on = sum(1 for o, _ in res if o)
        near = sum(1 for _, s in res if s)
        big = sum(1 for _, s in res if s and s >= 0.25 * biggest)
        print(f'{os.path.basename(os.path.dirname(os.path.dirname(path))) or path}: {os.path.basename(path)}  on an area: {on}/{len(pts)}  '
              f'node within 15 m: {near}  in a large route group (>= 25% of largest): {big}')
    if len(results) == 2:
        (pa, ra), (pb, rb) = list(results.items())
        diff = [(pts[i], ra[i], rb[i]) for i in range(len(pts)) if ra[i][0] != rb[i][0] or bool(ra[i][1]) != bool(rb[i][1])]
        print(f'\npoints that differ ({len(diff)}): first = {os.path.basename(pa)}, second = {os.path.basename(pb)}')
        for (k, n, p), a, b in diff[:60]:
            print(f'  {k:22s} {n[:34]:34s} ({p[0]:8.1f} {p[1]:6.1f} {p[2]:8.1f})  on-area {a[0]!s:5} / {b[0]!s:5}   group size {a[1]} / {b[1]}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
