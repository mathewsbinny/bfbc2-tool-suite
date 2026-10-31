r"""Xbox 360 -> PC conversion of mesh data (MeshData resources: a mesh LOD's parts, vertices and indices). Format:
FORMAT.md section 3.

A MeshData file names its parts, their counts and where their vertices and indices are; how a vertex is laid out
is not in it but in the level's shader database (section 7 vertex layouts, named per mesh part in section 8.3).
Converting a mesh therefore takes, for each part, the console layout and the PC layout to write: each element is
converted from the console's encoding to the PC's in order, by the elements' formats.

    import mesh
    m = mesh.parse(console_bytes, big_endian=True)
    pc_bytes = mesh.convert(console_bytes, [(console_layout, pc_layout), ...])     # one pair per part
"""
import struct

import numpy as np

# vertex element formats (layout records): size in bytes and how a console value becomes a PC one
SIZE = {0x03: 12, 0x04: 16, 0x05: 2, 0x06: 4, 0x07: 6, 0x08: 8, 0x0C: 4, 0x0D: 4, 0x2B: 4}
SWAP = {0x03: 4, 0x04: 4, 0x05: 2, 0x06: 2, 0x07: 2, 0x08: 2, 0x0C: 1}   # same format: values byte-swapped by width
PC_FORMAT = {0x2B: 0x08, 0x0D: 0x08}                                    # console-only packed formats -> 4 halves


class ConvertError(Exception):
    pass


class _Reader:
    def __init__(self, b, be):
        self.b, self.p, self.e = b, 0, '>' if be else '<'

    def num(self, fmt):
        v = struct.unpack_from(self.e + fmt, self.b, self.p)
        self.p += struct.calcsize(fmt)
        return v if len(v) > 1 else v[0]

    def u8(self):
        self.p += 1
        return self.b[self.p - 1]

    def cstr(self):
        e = self.b.index(b'\0', self.p)
        s = self.b[self.p:e]
        self.p = e + 1
        return s


def parse(b, big_endian):
    """The header: {h0, h1, parts: [{name, triangles, vertices, first_index, vertex_offset, stride, k, fa, fb,
    bones}], lists: 4 lists of part indices, vertex_bytes, index_bytes, wide_indices, data: offset of the vertices}."""
    r = _Reader(b, big_endian)
    m = {'h0': r.num('I'), 'h1': bytes([r.u8(), r.u8()]), 'parts': []}          # h1: two bytes, not a number
    for _ in range(r.u8()):
        p = {'name': r.cstr()}
        p['triangles'], p['vertices'], p['first_index'], p['vertex_offset'] = r.num('4I')
        p['stride'], p['k'], p['fa'], p['fb'] = r.u8(), r.u8(), r.u8(), r.u8()
        p['bones'] = [r.num('H') for _ in range(r.num('H'))]
        m['parts'].append(p)
    m['lists'] = []
    for _ in range(4):
        n = r.num('I')
        m['lists'].append(list(b[r.p:r.p + n]))
        r.p += n
    m['vertex_bytes'], m['index_bytes'], m['wide_indices'] = r.num('3I')
    m['data'] = r.p
    if m['data'] + m['vertex_bytes'] + m['index_bytes'] != len(b):
        raise ConvertError(f'mesh data: header, vertices and indices make {m["data"] + m["vertex_bytes"] + m["index_bytes"]}'
                           f' bytes, the file has {len(b)}')
    return m


def write(m, vertices, indices):
    """PC (little-endian) bytes of header m with the vertex and index bytes."""
    out = bytearray(struct.pack('<I', m['h0']) + m['h1'] + bytes([len(m['parts'])]))
    for p in m['parts']:
        out += p['name'] + b'\0'
        out += struct.pack('<4I4B', p['triangles'], p['vertices'], p['first_index'], p['vertex_offset'], p['stride'],
                           p['k'], p['fa'], p['fb'])
        out += struct.pack('<H%dH' % len(p['bones']), len(p['bones']), *p['bones'])
    for l in m['lists']:
        out += struct.pack('<I', len(l)) + bytes(l)
    out += struct.pack('<3I', len(vertices), len(indices), 0)
    return bytes(out) + vertices + indices


def elements(layout):
    """[(format, usage, offset, stream)] of a 0x49-byte layout record body; and stream 0's stride."""
    return [tuple(layout[4 * i:4 * i + 4]) for i in range(layout[64])], layout[69]


def _hend3n(v):
    x, y, z = v & 0x7FF, (v >> 11) & 0x7FF, (v >> 22) & 0x3FF
    x = x - 2048 if x >= 1024 else x
    y = y - 2048 if y >= 1024 else y
    z = z - 1024 if z >= 512 else z
    return max(x / 1023, -1.0), max(y / 1023, -1.0), max(z / 511, -1.0)


def element(fc, fp, raw, colour=False):
    """One console element value -> the PC one (format fc -> fp)."""
    if fc == fp and fc in SWAP:
        w = SWAP[fc]
        if fc == 0x0C and colour:                       # a colour-like UBYTE4 of a material-mask mesh
            return raw[::-1]
        return raw if w == 1 else b''.join(raw[i:i + w][::-1] for i in range(0, len(raw), w))
    if fc == 0x2B and fp == 0x08:                       # signed normalized 11:11:10 -> 4 halves (x, y, z, 1)
        return np.array(_hend3n(struct.unpack('>I', raw)[0]) + (1.0,), '<f2').tobytes()
    if fc == 0x0D and fp == 0x08:                       # 4 unsigned normalized bytes (w z y x) -> 4 halves
        return np.array([raw[3] / 255, raw[2] / 255, raw[1] / 255, raw[0] / 255], '<f2').tobytes()
    raise ConvertError(f'vertex element format {fc:#04x} -> {fp:#04x} is not converted')


def _pair_by_kind(ec, ep, leftovers=False):
    """Console element index for each PC element when the two layouts hold the same elements in another order:
    the k-th PC element of a kind (format, usage, stream) takes the k-th console element of that kind. With
    leftovers, console elements beyond the PC layout's (the last of a kind) are left out."""
    pools = {}
    for i, (f, u, _, s) in enumerate(ec):
        pools.setdefault((PC_FORMAT.get(f, f), u, s), []).append(i)
    out = []
    for f, u, _, s in ep:
        pool = pools.get((f, u, s))
        if not pool:
            return None
        out.append(pool.pop(0))
    return out if leftovers or not any(pools.values()) else None


def convert(b, layouts, drop=None, reorder=False):
    """Console MeshData -> PC MeshData. layouts: per part (console layout record body, PC layout record body);
    drop: per part, indices of console elements the PC layout does not have (left out, in order); otherwise the
    two layouts must have the same elements in the same order (formats per PC_FORMAT), or, with reorder, the same
    elements in another order (paired by kind, in order: _pair_by_kind); reorder='drop' also leaves out console
    elements the PC layout has no place for (the last ones of their kind)."""
    m = parse(b, True)
    if len(layouts) != len(m['parts']):
        raise ConvertError(f'{len(m["parts"])} parts, {len(layouts)} layout pairs')
    vb = bytearray()
    idx_size = 4 if m['wide_indices'] else 2
    offsets = sorted({q['vertex_offset'] for q in m['parts']} | {m['vertex_bytes']})     # console offsets
    idx_all = struct.unpack_from('>%d%s' % (m['index_bytes'] // idx_size, 'I' if idx_size == 4 else 'H'), b,
                                 m['data'] + m['vertex_bytes'])
    plans = []                           # per part: (element pairs, console stride, PC stride, colour, key)
    for n, (p, (lc, lp)) in enumerate(zip(m['parts'], layouts)):
        ec, sc = elements(lc)
        ep, sp = elements(lp)
        keep = [i for i in range(len(ec)) if not (drop and i in drop[n])]
        if reorder:
            order = _pair_by_kind([ec[i] for i in keep], ep, leftovers=reorder == 'drop')
            keep = None if order is None else [keep[j] for j in order]
            if keep is None:
                raise ConvertError(f'part {p["name"]!r}: the console and PC layouts do not hold the same elements')
        if len(keep) != len(ep):
            raise ConvertError(f'part {p["name"]!r}: {len(keep)} console elements for {len(ep)} PC elements')
        pairs = []
        for i, e in zip(keep, ep):
            fc, fp = ec[i][0], e[0]
            if PC_FORMAT.get(fc, fc) != fp or ec[i][1] != e[1] or ec[i][3] != e[3]:
                raise ConvertError(f'part {p["name"]!r}: element {i} {ec[i]} does not become {e}')
            pairs.append((ec[i], e))
        if p['stride'] != sc:
            raise ConvertError(f'part {p["name"]!r}: stride {p["stride"]}, its console layout says {sc}')
        key = (p['vertex_offset'], bytes(lp), tuple(keep))
        plans.append((pairs, sc, sp, any(e[0] == 0x0D for e in ec), key))
    layouts_at = {}
    for p, plan in zip(m['parts'], plans):
        layouts_at.setdefault(p['vertex_offset'], set()).add(plan[4][1:])

    def emit(pairs, sc, sp, colour, src, first, count):
        """Converted vertices first .. first + count - 1 of the console region at src, appended; their start."""
        start = len(vb)
        for v in range(first, first + count):
            row = bytearray(sp)
            base = src + v * sc
            for (fc, _, oc, _), (fp, _, op, _) in pairs:
                row[op:op + SIZE[fp]] = element(fc, fp, b[base + oc:base + oc + SIZE[fc]], colour)
            vb.extend(row)
        return start

    # parts sharing a console region and their PC layout share one converted region, to the next offset as the
    # console has it (the parts' indices unchanged); when one console region serves parts given different PC layouts
    # (a vertex has one layout), each part gets the window of vertices it uses (its lowest index and its vertex
    # count) in its own layout - or the copy of a larger window in the same layout - and a vertex offset that puts
    # its first index on the window (offset + index x stride), so indices stay as they are and depth-only parts that
    # draw several parts' triangles still find theirs
    shared, windows = {}, {}
    new_offset = [None] * len(m['parts'])
    single = [i for i, p in enumerate(m['parts']) if len(layouts_at[p['vertex_offset']]) == 1]
    order = single + sorted((i for i in range(len(m['parts'])) if i not in single),     # the console's order;
                            key=lambda i: -m['parts'][i]['vertices'])                   # windows: largest first
    for n in order:
        p = m['parts'][n]
        pairs, sc, sp, colour, key = plans[n]
        src = m['data'] + p['vertex_offset']
        nxt = min((o for o in offsets if o > p['vertex_offset']), default=p['vertex_offset'])   # an empty last part
        if (nxt - p['vertex_offset']) % sc:
            raise ConvertError(f'part {p["name"]!r}: {nxt - p["vertex_offset"]} vertex bytes, not a whole number of '
                               f'{sc}-byte vertices')
        region = (nxt - p['vertex_offset']) // sc
        used = idx_all[p['first_index']:p['first_index'] + p['triangles'] * 3]
        lo = min(used) if used else 0
        if used and max(used) >= lo + p['vertices'] or lo + p['vertices'] > region:
            raise ConvertError(f'part {p["name"]!r}: its indices leave the window of its {p["vertices"]} vertices')
        if len(layouts_at[p['vertex_offset']]) == 1:
            if key not in shared:
                shared[key] = emit(pairs, sc, sp, colour, src, 0, region)
            new_offset[n] = shared[key]
            continue
        hit = next(((clo, start) for clo, chi, start in windows.get(key, []) if clo <= lo and lo + p['vertices'] <= chi),
                   None)
        if hit is None:
            hit = (lo, emit(pairs, sc, sp, colour, src, lo, p['vertices']))
            windows.setdefault(key, []).append((lo, lo + p['vertices'], hit[1]))
        new_offset[n] = hit[1] - hit[0] * sp
        if new_offset[n] < 0:
            raise ConvertError(f'part {p["name"]!r}: its window would start before the vertex data')
    for p, (_, _, sp, _, _), off in zip(m['parts'], plans, new_offset):
        p.update(stride=sp, vertex_offset=off)
    # the index buffer as a whole (parts' ranges may overlap: a depth-only part can draw several parts' triangles),
    # byte-swapped and, when the console's are 32-bit, narrowed to 16 bits; first_index counts indices on both
    count = m['index_bytes'] // idx_size
    ind = struct.unpack_from('>%d%s' % (count, 'I' if idx_size == 4 else 'H'), b, m['data'] + m['vertex_bytes'])
    if max(ind, default=0) > 0xFFFF:
        raise ConvertError(f'index {max(ind)}: the PC mesh data has 16-bit indices only')
    ib = struct.pack('<%dH' % count, *ind)
    m['wide_indices'] = 0
    return write(m, bytes(vb), bytes(ib))
