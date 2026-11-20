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

In the suite's Rust core (round F2b, rust/bc2-convert/src/mopp.rs, through importcore), which it needs (without it
bfbc2core.native.NativeMissing says how to get it): query_int, query, complete, ray_cast and ray_cast_brute - the
traversal of the byte code above, the first MoppError. Where the Python would have raised anything else (an index
past the code: a cut code), or with keys or values the port does not take (NumPy scalars, keys that are not ints),
they raise a MoppError that is also a bfbc2core.native.Refused. Their Python (the *_reference functions) was retired
in round RT2 against its recorded answers (tests\f2b_havokport_replay*.json: 2,000 seeded codes, the MOPP codes of
BF1943's converted and DICE's physics files).
"""
import math
import os
import sys

if os.path.dirname(os.path.abspath(__file__)) not in sys.path:
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import importcore  # noqa: E402

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native  # noqa: E402

START_SHIFT = 16
AXES = [(1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 1, 1), (0, 1, -1), (1, 0, 1), (1, 0, -1), (1, 1, 0), (1, -1, 0),
        (1, 1, 1), (1, 1, -1), (1, -1, 1), (1, -1, -1)]


class MoppError(Exception):
    pass


def _code(code):
    return code if isinstance(code, bytes) else bytes(code)


def _exact(v):
    """A ray's numbers as the core takes them: a plain int (as tests write points) as its float - exact below 2 ** 53,
    where the ray arithmetic gives the same answer either way; anything else as it is (the core refuses NumPy
    scalars: their sums round differently)."""
    if type(v) is int and abs(v) < 2 ** 53:
        return float(v)
    if isinstance(v, list | tuple):
        return [_exact(x) for x in v]
    return v


def _tris(tris):
    return {k: [_exact(p) for p in t] for k, t in tris.items()}


def _call(name, *args):
    """The Rust core's answer (`(result,)`), or the refusal raised: a MoppError that is also a Refused."""
    if importcore.native() is not None:
        r = importcore.call(name, *args, error=MoppError)
    else:
        m = native.require('MOPP code queries (mopp.py)', 'mopp_query')
        try:
            r = getattr(m, name)(*args)
        except ValueError as e:
            raise MoppError(str(e)) from None
        except (TypeError, OverflowError):
            r = None
    if r is None:
        raise native.refused(f'{name.replace("_", " ")}: a cut code, or keys or values that are not plain ints and '
                             'floats', MoppError)
    return r


def query_int(code, qmin, qmax):
    """Terminal keys reached by the quantized box qmin..qmax (3 ints each)."""
    return set(_call('mopp_query', _code(code), tuple(qmin), tuple(qmax))[0])


def quantize(info, lo, hi):
    """World box -> the quantized box (floor / ceil)."""
    return ([math.floor((lo[a] - info[a]) * info[3]) for a in range(3)],
            [math.ceil((hi[a] - info[a]) * info[3]) for a in range(3)])


def query(code, info, lo, hi):
    """Terminal keys whose cells overlap the world box lo..hi; info: the code's (offset x, y, z, scale)."""
    return set(_call('mopp_query_box', _code(code), tuple(info), tuple(lo), tuple(hi))[0])


def complete(code, info, boxes, tolerance=0.001):
    """The keys of boxes ({key: (lo, hi)}) the query of their own box (widened by tolerance: a flat triangle on a
    cell boundary is otherwise a matter of rounding - 58 of DICE's 49,744 triangles at 0, none at 1 mm) does not
    reach, and the mean number of keys such a query returns (how tight the tree is)."""
    r = _call('mopp_complete', _code(code), tuple(info), dict(boxes), tolerance)
    return r[0][0], r[0][1]


def ray_cast(code, info, tris, origin, direction, length, step=0.25, tolerance=0.001):
    """(distance, key) of the nearest triangle hit along the ray, the candidates found by MOPP box queries over steps
    of the ray (each widened by tolerance, as Havok's queries are); tris: {key: three points}; None when nothing is
    hit within length."""
    return _call('mopp_ray_cast', _code(code), tuple(info), _tris(tris), tuple(_exact(origin)),
                 tuple(_exact(direction)), _exact(length), _exact(step), _exact(tolerance))[0]


def ray_cast_brute(tris, origin, direction, length):
    """The same over every triangle."""
    return _call('mopp_ray_cast_brute', _tris(tris), tuple(_exact(origin)), tuple(_exact(direction)),
                 _exact(length))[0]


