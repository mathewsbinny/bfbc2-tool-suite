r"""Xbox 360 -> PC conversion of mesh data (MeshData resources: a mesh LOD's parts, vertices and indices). Format:
FORMAT.md section 3.

A MeshData file names its parts, their counts and where their vertices and indices are; how a vertex is laid out
is not in it but in the level's shader database (section 7 vertex layouts, named per mesh part in section 8.3).
Converting a mesh therefore takes, for each part, the console layout and the PC layout to write: each element is
converted from the console's encoding to the PC's in order, by the elements' formats.

    import mesh
    m = mesh.parse(console_bytes, big_endian=True)
    pc_bytes = mesh.convert(console_bytes, [(console_layout, pc_layout), ...])     # one pair per part

convert's vertex loop (each element of each vertex by element()'s rule) runs in the suite's Rust core
(bc2_native.mesh_vertices, round E3g), which it needs: without the module (or with one built from other sources) it
raises bfbc2core.native.NativeMissing saying how to get it; BFBC2_PURE_PYTHON does not change that. Vertices it does
not convert (a format element() refuses, an element past the PC vertex, a read past the data) raise ConvertError
saying which. The Python loop that was its reference was retired on 2026-11-18, after the port had given the same
bytes on every BF1943 mesh port and Onslaught console mesh and passed the full test tier; its results are recorded in
bfbc2-bf1943-import\tests\ports_golden.json (test_ports_native.py). element() stays: the per-value rule (OPFOR's
wake_audit checks meshes with it).

The header's parse and write (and meshset.py's) run in the Rust core too (round F0: bc2_native.mesh_parse /
mesh_write, meshset_parse / meshset_write, the same dicts and bytes), and so do console_layouts, decode and relayout
(round F2a: bc2_convert::meshconv: the same dicts, arrays and bytes, the same ConvertError messages; with an edit
callback, relayout's blocks and columns come from the core and the callback and the rows stay here). Their Python
(parse_reference, write_reference, console_layouts_reference, decode_reference, relayout_reference) was retired in
round RT2 against its recorded answers (bfbc2-console-convert\\tests\\f0_meshdata_replay*.json,
f2a_console_replay*.json): where it raised another error (a read past the data: a file cut short or damaged) or a
value is one the core does not take, a ConvertError that is also a bfbc2core.native.Refused says so.
"""
import os
import struct
import sys
from typing import Any

import numpy as np

# vertex element formats (layout records): size in bytes and how a console value becomes a PC one
SIZE = {0x02: 8, 0x03: 12, 0x04: 16, 0x05: 2, 0x06: 4, 0x07: 6, 0x08: 8, 0x0C: 4, 0x0D: 4, 0x2B: 4}
SWAP = {0x02: 4, 0x03: 4, 0x04: 4, 0x05: 2, 0x06: 2, 0x07: 2, 0x08: 2, 0x0C: 1}   # same format: values byte-swapped by width
PC_FORMAT = {0x2B: 0x08, 0x0D: 0x08}                                    # console-only packed formats -> 4 halves


class ConvertError(Exception):
    pass


def require():
    """The Rust core, which converts the vertices (bfbc2core.native.require): NativeMissing, saying how to get it,
    when it is not built or downloaded or was built from other sources."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('Mesh conversion (bfbc2-console-convert/mesh.py)', 'mesh_vertices')


def _native() -> Any:
    """The Rust core with the header readers (bfbc2core.native.require): NativeMissing, saying how to get it."""
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('Mesh conversion (bfbc2-console-convert/mesh.py)', 'mesh_parse')


def _refused(message: str) -> Exception:
    from bfbc2core import native
    return native.refused(message, ConvertError)


def _bytes(b: Any) -> Any:
    return bytes(b) if isinstance(b, bytearray | memoryview) else b


def parse(b, big_endian):
    """The header: {h0, h1, parts: [{name, triangles, vertices, first_index, vertex_offset, stride, k, fa, fb,
    bones}], lists: 4 lists of part indices, vertex_bytes, index_bytes, wide_indices, data: offset of the vertices}.
    In the Rust core (round F0); a file it does not read (cut short or damaged) raises a Refused ConvertError."""
    b = _bytes(b)
    got = _native().mesh_parse(b, bool(big_endian)) if type(b) is bytes else None
    if got is None:
        raise _refused(f'mesh data: {len(b) if type(b) is bytes else type(b).__name__} bytes the header reader does '
                       'not read (cut short or damaged)')
    return got


def write(m, vertices, indices):
    """PC (little-endian) bytes of header m with the vertex and index bytes: in the Rust core when it is built
    (round F0); a header with values it does not write raises a Refused ConvertError."""
    got = _native().mesh_write(m, _bytes(vertices), _bytes(indices))
    if got is None:
        raise _refused('mesh data: a header (or vertex / index bytes) with values the writer does not take')
    return got


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


def console_layouts(db):
    """{mesh part name (lower case, '<mesh>_lod<N>/<part>/<technique>'): layout record body} of a console shader
    database (Shaders/Single.res: BFBC2's Xbox levels, Battlefield 1943's). Part names are stored as name, NUL, the
    16-byte guid of its layout; a run of them is walked name by name, so a guid byte that happens to be printable is
    not taken for the next name's first letter. Each guid's layout record is found where the guid is followed by a
    plausible record (1 to 16 elements). In the Rust core (round F2a)."""
    db = _bytes(db)
    if type(db) is not bytes:
        raise TypeError(f'console_layouts: a shader database\'s bytes, not {type(db).__name__}')
    return _native().mesh_console_layouts(db)


TEXCOORD_PAIRS = (0x06, 0x02)          # two-component texture coordinates: half2, float2
TEXCOORD_FLOATS = (0x06, 0x02, 0x07, 0x08, 0x03, 0x04)


def texcoord_element(els):
    """Index into els of the element holding a part's texture coordinates (stream 0, usage 0), or None. Usage 0 is
    every texture-coordinate-like element, and the first is not always the texture's: BFBC2's soldiers start with a
    half4 (a grey value, its w 1) at offset 16 and keep the coordinates in the half2 at 40; trees start with a half4
    of zeros before the bark's half2. So: the first two-component one (half2 / float2); failing that, the first other
    float one (a material-mask mesh's half4, its x / y the coordinates) - never a byte one (its colour). Found over
    every layout of the game's shader databases (FORMAT.md section 3)."""
    zero = [(i, f) for i, (f, usage, _, stream) in enumerate(els) if usage == 0 and stream == 0]
    for i, f in zero:
        if f in TEXCOORD_PAIRS:
            return i
    for i, f in zero:
        if f in TEXCOORD_FLOATS:
            return i
    return None


USAGES = {0: 'uvs', 1: 'positions', 2: 'blend_indices', 3: 'weights', 4: 'normals', 5: 'tangents', 6: 'binormals'}
_PC_DTYPE = {0x02: ('<f4', 2), 0x03: ('<f4', 3), 0x04: ('<f4', 4), 0x05: ('<f2', 1), 0x06: ('<f2', 2), 0x07: ('<f2', 3),
             0x08: ('<f2', 4), 0x0C: ('u1', 4)}


def _bytes_like(*vs):
    return all(isinstance(v, (bytes, bytearray)) for v in vs)


def decode(b, layout, part, big_endian=True):
    """One part of a MeshData file as arrays: {positions, normals, tangents, binormals, uvs (the texture coordinates:
    texcoord_element), weights (0..1),
    blend_indices (into the part's bone palette), bones (skeleton indices: the palette applied), indices (m, 3; into
    these arrays)} - each element present in the layout record body (console: the source game's shader database; PC:
    BFBC2's). Only the vertices the part's triangles use (its window) are read; stream 0. In the Rust core (round
    F2a); a file or layout it does not read raises a Refused ConvertError."""
    got = None
    if _bytes_like(b, layout) and type(part) in (int, str):
        got = _native().mesh_decode(bytes(b), bytes(layout), part, bool(big_endian))
    if got is None:
        raise _refused(f'mesh data: part {part!r}: a file or layout the decoder does not read (cut short or damaged)')
    if not got[0]:
        raise ConvertError(got[1])
    return {k: np.frombuffer(data, dt).reshape(rows, cols) for k, dt, rows, cols, data in got[1]}


def pair_by_kind(ec, ep, leftovers=False):
    """Console element index for each PC element when the two layouts hold the same elements in another order:
    the k-th PC element of a kind (format, usage, stream) takes the k-th console element of that kind. With
    leftovers, console elements beyond the PC layout's (the last of a kind) are left out."""
    pools: dict[tuple[int, int, int], list[int]] = {}
    for i, (f, u, _, s) in enumerate(ec):
        pools.setdefault((PC_FORMAT.get(f, f), u, s), []).append(i)
    out = []
    for f, u, _, s in ep:
        pool = pools.get((f, u, s))
        if not pool:
            return None
        out.append(pool.pop(0))
    return out if leftovers or not any(pools.values()) else None


_pair_by_kind = pair_by_kind  # the earlier private name, kept for callers that still use it


# A part's primitive type (its record's k): 3 a triangle list, 5 a triangle strip (triangles + 2 indices; consecutive
# triangles alternate winding; repeated indices join strips with degenerate triangles; no restart index). Battlefield
# 1943's trees draw their bark and some depth-only parts as strips (81 parts of its meshes); BFBC2's PC meshes are
# lists only - DICE's PC palms and acacias have k = 3 throughout.
LIST, STRIP = 3, 5


def index_count(p):
    """How many indices a part's triangles take."""
    return p['triangles'] + 2 if p['k'] == STRIP else p['triangles'] * 3


def strip_to_list(s):
    """A triangle strip's indices -> a triangle list's (degenerate triangles left out, each triangle's winding the
    strip's: triangle i is (s[i], s[i+1], s[i+2]) for even i, (s[i+1], s[i], s[i+2]) for odd i)."""
    out: list[tuple[int, int, int]] = []
    for i in range(len(s) - 2):
        a, b_, c = s[i], s[i + 1], s[i + 2]
        if a in (b_, c) or b_ == c:
            continue
        out += (a, b_, c) if i % 2 == 0 else (b_, a, c)
    return out


def _not_converted(name, pairs, sp):
    """Why the Rust core does not convert a part's vertices: an element conversion element() does not make, an
    element past the PC vertex, else a read past the data."""
    for (fc, _, _, _), (fp, _, op, _) in pairs:
        if fc not in SIZE or fp not in SIZE or not (fc == fp and fc in SWAP or (fc, fp) in ((0x2B, 0x08), (0x0D, 0x08))):
            return f'vertex element format {fc:#04x} -> {fp:#04x} is not converted'
        if op + SIZE[fp] > sp:
            return f'part {name!r}: a {SIZE[fp]}-byte element at {op} of its {sp}-byte PC vertex reaches past the vertex'
    return f'part {name!r}: its vertices reach past the end of the mesh data'


def convert(b, layouts, drop=None, reorder=False, bones=None):
    """Console MeshData -> PC MeshData. layouts: per part (console layout record body, PC layout record body);
    drop: per part, indices of console elements the PC layout does not have (left out, in order); otherwise the
    two layouts must have the same elements in the same order (formats per PC_FORMAT), or, with reorder, the same
    elements in another order (paired by kind, in order: _pair_by_kind); reorder='drop' also leaves out console
    elements the PC layout has no place for (the last ones of their kind). bones ({old index: new index})
    renumbers the parts' bone palettes - a mesh from another game's skeleton order (Battlefield 1943's soldier); the
    vertices index the palette, so they stay as they are."""
    native = require()
    b = bytes(b)
    m = parse(b, True)
    if len(layouts) != len(m['parts']):
        raise ConvertError(f'{len(m["parts"])} parts, {len(layouts)} layout pairs')
    if bones is not None:
        for p in m['parts']:
            missing = [x for x in p['bones'] if x not in bones]
            if missing:
                raise ConvertError(f'part {p["name"]!r}: bones {missing} have no counterpart in the target skeleton')
            p['bones'] = [bones[x] for x in p['bones']]
    vb = bytearray()
    idx_size = 4 if m['wide_indices'] else 2
    offsets = sorted({q['vertex_offset'] for q in m['parts']} | {m['vertex_bytes']})     # console offsets
    idx_all = struct.unpack_from('>%d%s' % (m['index_bytes'] // idx_size, 'I' if idx_size == 4 else 'H'), b,
                                 m['data'] + m['vertex_bytes'])
    plans = []                           # per part: (element pairs, console stride, PC stride, colour, key)
    for n, (p, (lc, lp)) in enumerate(zip(m['parts'], layouts)):
        ec, sc = elements(lc)
        ep, sp = elements(lp)
        keep: list[int] | None                       # None: the layouts do not pair up (raised below)
        keep = [i for i in range(len(ec)) if not (drop and i in drop[n])]
        if reorder:
            order = pair_by_kind([ec[i] for i in keep], ep, leftovers=reorder == 'drop')
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
    layouts_at: dict[int, set[tuple[Any, ...]]] = {}
    for p, plan in zip(m['parts'], plans):
        layouts_at.setdefault(p['vertex_offset'], set()).add(plan[4][1:])

    def emit(name, pairs, sc, sp, colour, src, first, count):
        """Converted vertices first .. first + count - 1 of the console region at src, appended (the Rust core);
        their start."""
        start = len(vb)
        got = native.mesh_vertices(b, src, sc, sp, first, count,
                                   [(fc, oc, fp, op) for (fc, _, oc, _), (fp, _, op, _) in pairs], colour)
        if got is None:
            raise ConvertError(_not_converted(name, pairs, sp))
        vb.extend(got)
        return start

    # parts sharing a console region and their PC layout share one converted region, to the next offset as the
    # console has it (the parts' indices unchanged); when one console region serves parts given different PC layouts
    # (a vertex has one layout), each part gets the window of vertices it uses (its lowest index and its vertex
    # count) in its own layout - or the copy of a larger window in the same layout - and a vertex offset that puts
    # its first index on the window (offset + index x stride), so indices stay as they are and depth-only parts that
    # draw several parts' triangles still find theirs
    shared: dict[tuple[Any, ...], int]
    windows: dict[tuple[Any, ...], list[tuple[int, int, int]]]
    shared, windows = {}, {}
    new_offset: list[Any] = [None] * len(m['parts'])          # each part's PC vertex offset, filled in below
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
        used = idx_all[p['first_index']:p['first_index'] + index_count(p)]
        lo = min(used) if used else 0
        if used and max(used) >= lo + p['vertices'] or lo + p['vertices'] > region:
            raise ConvertError(f'part {p["name"]!r}: its indices leave the window of its {p["vertices"]} vertices')
        if len(layouts_at[p['vertex_offset']]) == 1:
            if key not in shared:
                shared[key] = emit(p['name'], pairs, sc, sp, colour, src, 0, region)
            new_offset[n] = shared[key]
            continue
        hit = next(((clo, start) for clo, chi, start in windows.get(key, []) if clo <= lo and lo + p['vertices'] <= chi),
                   None)
        if hit is None:
            hit = (lo, emit(p['name'], pairs, sc, sp, colour, src, lo, p['vertices']))
            windows.setdefault(key, []).append((lo, lo + p['vertices'], hit[1]))
        new_offset[n] = hit[1] - hit[0] * sp
        if new_offset[n] < 0:
            raise ConvertError(f'part {p["name"]!r}: its window would start before the vertex data')
    for p, (_, _, sp, _, _), off in zip(m['parts'], plans, new_offset):
        p.update(stride=sp, vertex_offset=off)
    # the index buffer as a whole (parts' ranges may overlap: a depth-only part can draw several parts' triangles),
    # byte-swapped and, when the console's are 32-bit, narrowed to 16 bits; first_index counts indices on both
    count = m['index_bytes'] // idx_size
    ind = list(struct.unpack_from('>%d%s' % (count, 'I' if idx_size == 4 else 'H'), b, m['data'] + m['vertex_bytes']))
    # strips become lists, appended after the console's indices (every other part keeps its first index; the strip's
    # own indices stay unused where they were)
    for p in m['parts']:
        if p['k'] == STRIP:
            tri = strip_to_list(ind[p['first_index']:p['first_index'] + index_count(p)])
            p.update(first_index=len(ind), triangles=len(tri) // 3, k=LIST)
            ind += tri
    if max(ind, default=0) > 0xFFFF:
        raise ConvertError(f'index {max(ind)}: the PC mesh data has 16-bit indices only')
    ib = struct.pack('<%dH' % len(ind), *ind)
    m['wide_indices'] = 0
    return write(m, bytes(vb), bytes(ib))


def relayout(b, old, new, edit=None):
    """A PC MeshData's vertices re-laid into other PC vertex layouts (each part: old[i] -> new[i], layout record
    bodies). A PC element takes the old element of its kind (usage, stream; the k-th of a kind the k-th), its value
    carried across formats of the same family (half / float, as many components as both have, the rest 0); an element
    the old layout lacks is 0. edit(parts, cols) may then change the values: parts the header dicts of the parts
    drawing from one block of vertices (their palettes may be extended), cols {new element index: array (n, k)} -
    float32 for float / half formats, uint8 for UBYTE4. Index data is kept; vertex offsets and strides follow. A kind
    is (usage, stream, component count) first: a half4 texture coordinate pairs with a half4, a half2 with a half2,
    whatever order the two layouts list them in; a new element no old one of its count matches then takes the next
    unpaired old element of its usage and stream (a half4 position into a float3: x, y, z; a half2 into a float4:
    the rest 0).

    A block is the vertices parts draw (from each part's vertex offset to its highest index), overlapping ones
    merged; every part of a block must have the same old and the same new layout. Returns the PC bytes. In the Rust
    core (round F2a: with edit, the blocks and their columns from the core, the edit and the rows here); a file or
    layouts it does not read raise a Refused ConvertError."""
    b = _bytes(b)
    got = None
    if (type(b) is bytes and isinstance(old, (list, tuple)) and isinstance(new, (list, tuple))
            and _bytes_like(*old, *new)):
        o, n = [bytes(x) for x in old], [bytes(x) for x in new]
        got = _native().mesh_relayout(b, o, n) if edit is None else _native().mesh_relayout_blocks(b, o, n)
    if got is None:
        raise _refused('mesh data: a file or layouts the relayout does not read (cut short or damaged)')
    if not got[0]:
        raise ConvertError(got[1])
    if edit is None:
        return got[1]
    blocks, tail = got[1]
    return _relayout_edited(b, new, edit, blocks, tail)


def _relayout_edited(b, new, edit, blocks, tail):
    """relayout's steps after the core's blocks: each block's edit and rows (the reference's code), then the
    header."""
    m = parse(b, False)
    vb = bytearray()
    new_off = {}
    for start, members, n, so, sn, raw in blocks:
        cols = {k: np.frombuffer(data, dt).reshape(n, comps) for k, (dt, comps, data) in enumerate(raw)}
        edit([m['parts'][i] for i in members], cols)
        en, _ = elements(new[members[0]])
        rows = np.zeros((n, sn), np.uint8)
        for k, (f, _u, off, _s) in enumerate(en):
            dt, _comps = _PC_DTYPE[f]
            rows[:, off:off + SIZE[f]] = np.frombuffer(np.ascontiguousarray(cols[k].astype(dt)).tobytes(), np.uint8).reshape(n, -1)
        base = len(vb)
        vb.extend(rows.tobytes())
        for i in members:
            new_off[i] = base + (m['parts'][i]['vertex_offset'] - start) // so * sn
            m['parts'][i]['stride'] = sn
    if tail is not None:
        raise ConvertError(tail)
    return _relayout_write(b, m, new, vb, new_off)


def _relayout_write(b, m, new, vb, new_off):
    """relayout's last step: offsets and strides of the parts, the header written (32-bit indices kept)."""
    for i, p in enumerate(m['parts']):
        if i in new_off:
            p['vertex_offset'] = new_off[i]
        else:                                                    # a part with no triangles: no vertices
            p['vertex_offset'], p['stride'] = len(vb), elements(new[i])[1]
    ib = b[m['data'] + m['vertex_bytes']:m['data'] + m['vertex_bytes'] + m['index_bytes']]
    res = bytearray(write(m, bytes(vb), ib))
    if m['wide_indices']:                                        # write() states 16-bit indices
        struct.pack_into('<I', res, len(res) - len(vb) - len(ib) - 4, 1)
    return bytes(res)


