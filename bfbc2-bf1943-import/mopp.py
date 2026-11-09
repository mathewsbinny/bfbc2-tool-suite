r"""mopp.py - Havok's MOPP byte code (hkpMoppCode: the bounding-volume tree of a hkpMoppBvTreeShape) decoded and
queried, to check a MOPP against the shapes it indexes: a box query, a ray cast built on it, and a completeness check.
The same byte code in Havok 4.6.1 (Battlefield 1943) and 5.5.0 (BFBC2): every opcode either version's files use
(BF1943's converted, DICE's own) is one of the set below.

Space: a point p is quantized to q = (p - info.xyz) * info.w, 24-bit integers; a command compares bytes of
(q - o) >> s, o and s starting at 0 and 16.

    0x01-0x04 k, bx, by, bz    rescale: o += b << s, s -= k (zoom into a cell, k more bits)
    0x05 / 0x06 / 0x07 / 0x08  jump by the next 1 / 2 / 3 / 4 bytes (big-endian)
    0x09 / 0x0A b              terminal offset += 1 / 2 bytes;  0x0B b4: terminal offset = 4 bytes
    0x10-0x1C hi, lo, j        split on x, y, z, y+z, y-z, x+z, x-z, x+y, x-y, x+y+z, x+y-z, x-y+z, x-y-z (sums
                               averaged, differences offset by 255 first): v <= hi -> next; v >= lo -> next + j
    0x20-0x22 sp, j            split on x / y / z at one value: v <= sp -> next; v >= sp -> next + j
    0x23-0x25 hi, lo, j1, j2   split with 16-bit jumps: v <= hi -> next + j1; v >= lo -> next + j2
    0x26-0x28 min, max         bound on x / y / z: outside -> no terminal on this path
    0x30-0x4F                  terminal (code - 0x30) + offset
    0x50-0x53 b1..b4           terminal (1 to 4 bytes) + offset

Both branches of a split may be taken (their cells overlap); the comparisons are inclusive, so a query is never
narrower than Havok's. Checked on the 88 physics files both games ship, BF1943's converted and DICE's own:
tests\test_havokport.py.

    keys = mopp.query(code, info, lo, hi)                 # terminals whose cells overlap the box lo..hi (world)
    t, key = mopp.ray_cast(code, info, tris, origin, direction, length)
    missed = mopp.complete(code, info, boxes)             # {key: (lo, hi)} whose own box does not reach the key
"""
import math
from typing import Any

START_SHIFT = 16
AXES = [(1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 1, 1), (0, 1, -1), (1, 0, 1), (1, 0, -1), (1, 1, 0), (1, -1, 0),
        (1, 1, 1), (1, 1, -1), (1, -1, 1), (1, -1, -1)]


class MoppError(Exception):
    pass


def _value(k, lo, hi):
    """The range of split k's value over the box (byte space): an axis, or a sum / difference of axes averaged."""
    w = AXES[k]
    a = sum(lo[i] * w[i] if w[i] > 0 else hi[i] * w[i] for i in range(3))
    b = sum(hi[i] * w[i] if w[i] > 0 else lo[i] * w[i] for i in range(3))
    if k < 3:
        return a, b
    terms, negs = sum(1 for x in w if x), sum(1 for x in w if x < 0)
    return (a + 255 * negs) // terms, (b + 255 * negs) // terms


def query_int(code, qmin, qmax):
    """Terminal keys reached by the quantized box qmin..qmax (3 ints each)."""
    out: set[int] = set()
    stack: list[tuple[int, tuple[int, ...], int, int]] = [(0, (0, 0, 0), START_SHIFT, 0)]
    seen = set()
    n = len(code)
    while stack:
        i, o, s, off = stack.pop()
        while i < n:
            if (i, o, s, off) in seen:
                break
            seen.add((i, o, s, off))
            c = code[i]
            if 0x30 <= c < 0x50:
                out.add(off + c - 0x30)
                break
            if 0x50 <= c <= 0x53:
                w = c - 0x4F
                out.add(off + int.from_bytes(code[i + 1:i + 1 + w], 'big'))
                break
            if c in (0x09, 0x0A):
                w = c - 0x08
                off += int.from_bytes(code[i + 1:i + 1 + w], 'big')
                i += 1 + w
                continue
            if c == 0x0B:
                off = int.from_bytes(code[i + 1:i + 5], 'big')
                i += 5
                continue
            if 0x05 <= c <= 0x08:
                w = c - 0x04
                i += 1 + w + int.from_bytes(code[i + 1:i + 1 + w], 'big')
                continue
            if 1 <= c <= 4:
                o = tuple(o[a] + (code[i + 1 + a] << s) for a in range(3))
                s -= c
                if s < 0:
                    raise MoppError(f'rescale below one unit at {i}')
                i += 4
                continue
            lo = [(qmin[a] - o[a]) >> s for a in range(3)]
            hi = [(qmax[a] - o[a]) >> s for a in range(3)]
            if 0x26 <= c <= 0x28:
                a = c - 0x26
                if hi[a] < code[i + 1] or lo[a] > code[i + 2]:
                    break
                i += 3
                continue
            if 0x10 <= c <= 0x1C:
                vlo, vhi = _value(c - 0x10, lo, hi)
                if vhi >= code[i + 2]:
                    stack.append((i + 4 + code[i + 3], o, s, off))
                if vlo <= code[i + 1]:
                    i += 4
                    continue
                break
            if 0x20 <= c <= 0x22:
                a, sp = c - 0x20, code[i + 1]
                if hi[a] >= sp:
                    stack.append((i + 3 + code[i + 2], o, s, off))
                if lo[a] <= sp:
                    i += 3
                    continue
                break
            if 0x23 <= c <= 0x25:
                a = c - 0x23
                if hi[a] >= code[i + 2]:
                    stack.append((i + 7 + int.from_bytes(code[i + 5:i + 7], 'big'), o, s, off))
                if lo[a] <= code[i + 1]:
                    stack.append((i + 7 + int.from_bytes(code[i + 3:i + 5], 'big'), o, s, off))
                break
            raise MoppError(f'opcode {c:#04x} at {i}')
    return out


def quantize(info, lo, hi):
    """World box -> the quantized box (floor / ceil)."""
    return ([math.floor((lo[a] - info[a]) * info[3]) for a in range(3)],
            [math.ceil((hi[a] - info[a]) * info[3]) for a in range(3)])


def query(code, info, lo, hi):
    """Terminal keys whose cells overlap the world box lo..hi; info: the code's (offset x, y, z, scale)."""
    return query_int(code, *quantize(info, lo, hi))


def complete(code, info, boxes, tolerance=0.001):
    """The keys of boxes ({key: (lo, hi)}) the query of their own box (widened by tolerance: a flat triangle on a
    cell boundary is otherwise a matter of rounding - 58 of DICE's 49,744 triangles at 0, none at 1 mm) does not
    reach, and the mean number of keys such a query returns (how tight the tree is)."""
    missed, total = [], 0
    for k, (lo, hi) in boxes.items():
        got = query(code, info, [x - tolerance for x in lo], [x + tolerance for x in hi])
        total += len(got)
        if k not in got:
            missed.append(k)
    return missed, total / max(len(boxes), 1)


def _ray_triangle(o, d, tri):
    """Distance along the ray to the triangle (Moeller-Trumbore, both sides), or None."""
    a, b, c = tri
    e1 = [b[i] - a[i] for i in range(3)]
    e2 = [c[i] - a[i] for i in range(3)]
    p = [d[1] * e2[2] - d[2] * e2[1], d[2] * e2[0] - d[0] * e2[2], d[0] * e2[1] - d[1] * e2[0]]
    det = sum(e1[i] * p[i] for i in range(3))
    if abs(det) < 1e-12:
        return None
    t0 = [o[i] - a[i] for i in range(3)]
    u = sum(t0[i] * p[i] for i in range(3)) / det
    if u < 0 or u > 1:
        return None
    q = [t0[1] * e1[2] - t0[2] * e1[1], t0[2] * e1[0] - t0[0] * e1[2], t0[0] * e1[1] - t0[1] * e1[0]]
    v = sum(d[i] * q[i] for i in range(3)) / det
    if v < 0 or u + v > 1:
        return None
    t = sum(e2[i] * q[i] for i in range(3)) / det
    return t if t >= 0 else None


def ray_cast(code, info, tris, origin, direction, length, step=0.25, tolerance=0.001):
    """(distance, key) of the nearest triangle hit along the ray, the candidates found by MOPP box queries over steps
    of the ray (each widened by tolerance, as Havok's queries are); tris: {key: three points}; None when nothing is
    hit within length."""
    best: Any = None
    t = 0.0
    while t < length:
        u = min(t + step, length)
        a = [origin[i] + direction[i] * t for i in range(3)]
        b = [origin[i] + direction[i] * u for i in range(3)]
        lo = [min(a[i], b[i]) - tolerance for i in range(3)]
        hi = [max(a[i], b[i]) + tolerance for i in range(3)]
        for k in sorted(query(code, info, lo, hi)):
            if k in tris:
                d = _ray_triangle(origin, direction, tris[k])
                if d is not None and d <= length and (best is None or d < best[0]):
                    best = (d, k)
        if best is not None and best[0] <= u:
            return best
        t = u
    return best


def ray_cast_brute(tris, origin, direction, length):
    """The same over every triangle."""
    best: Any = None
    for k, tri in tris.items():
        d = _ray_triangle(origin, direction, tri)
        if d is not None and d <= length and (best is None or d < best[0]):
            best = (d, k)
    return best
