"""Editing a skinned mesh's LOD (MeshData) on the PC: replace a part's triangles with new geometry, add geometry to
the depth-only part, and write the file again; plus the SkinnedMeshSet that lists the LODs. Formats: the console
converter's FORMAT.md sections 3-4 (MeshData, mesh sets); vertex layouts come from the level shader databases
(meshview.Index.parts).

The file's vertex and index buffers are kept as they are - parts not changed keep their offsets, and parts that share
a vertex window (a depth-only part drawing the main part's vertices) go on sharing it; changed parts are written
after them. A part whose window another part shares can only be replaced together with that part.

The vertex work (the parts cut out, decode, the winding, new geometry encoded, the buffers laid out, the bounds)
runs in the suite's Rust core when it is built (round F2a: bc2_convert::skinmesh; the same arrays and bytes); the
dicts, the layouts by name, the bone palettes and the geometry's tangents stay here. The Python is kept as the
reference and the fallback (the *_reference methods); BFBC2_PURE_PYTHON=1 or use_native(False) runs it only.
"""
import os
import sys
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert')
import mesh as meshdata  # noqa: E402

POS, WEIGHTS, INDICES, NORMAL, TANGENT, BINORMAL, TEX, POS_W = 1, 3, 2, 4, 5, 6, 0, 7


_NATIVE: list[Any] = []


def _native() -> Any:
    """The Rust core with skin_cut, or None: not built, or BFBC2_PURE_PYTHON set."""
    if not _NATIVE:
        m = None
        try:
            root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
            if root not in sys.path:
                sys.path.insert(0, root)
            import suite_paths
            suite_paths.use('core')
            from bfbc2core import dbx as _dbx
            if os.environ.get(_dbx.PURE_PYTHON_ENV, '') in ('', '0'):
                m = _dbx._import_native()
        except ImportError:
            m = None
        _NATIVE.append(m if m is not None and hasattr(m, 'skin_cut') else None)
    return _NATIVE[0]


def use_native(on: bool) -> bool:
    """The Rust core on (when built, whatever BFBC2_PURE_PYTHON says) or off for this process; True when it runs."""
    _NATIVE.clear()
    if not on:
        _NATIVE.append(None)
        return False
    saved = os.environ.pop('BFBC2_PURE_PYTHON', None)
    try:
        return _native() is not None
    finally:
        if saved is not None:
            os.environ['BFBC2_PURE_PYTHON'] = saved


def _rows_ok(p: Any) -> bool:
    """A part whose rows and triangles the Rust core reads as the reference does."""
    return (isinstance(p.rows, np.ndarray) and p.rows.dtype == np.uint8 and p.rows.ndim == 2
            and isinstance(p.idx, np.ndarray) and p.idx.dtype == np.int64 and p.idx.ndim == 2
            and isinstance(p.layout, (bytes, bytearray)))


class EditError(Exception):
    pass


class Part:
    def __init__(self, info, layout, rows, idx):
        self.info, self.layout, self.rows, self.idx = info, layout, rows, idx      # rows: (n, stride) uint8
        self.name = info['name'].decode('latin-1')
        self.changed = False

    @property
    def elements(self):
        return meshdata.elements(self.layout)


class SkinnedLod:
    """One MeshData resource. layouts: {part name: 73-byte layout record} for its parts."""

    def __init__(self, b, layouts, where='mesh'):
        self.where = where
        m = meshdata.parse(b, False)
        self.m = m
        vbase, ibase = m['data'], m['data'] + m['vertex_bytes']
        wide = bool(m['wide_indices'])
        if wide:
            raise EditError(f'{where}: 32-bit indices are not written by this tool')
        self.vb = b[vbase:ibase]
        self.ib = b[ibase:ibase + m['index_bytes']]
        self.parts = []
        for p in m['parts']:
            name = p['name'].decode('latin-1')
            if name not in layouts:
                raise EditError(f'{where}: no vertex layout for part {name}')
            els, stride = meshdata.elements(layouts[name])
            nat = _native() if type(b) is bytes else None
            cut = nat.skin_cut(b, ibase, vbase, (p['triangles'], p['vertices'], p['first_index'], p['vertex_offset']),
                               stride) if nat is not None else None
            if cut is not None:
                rows_, _, idx_ = cut
                self.parts.append(Part(dict(p), layouts[name], np.frombuffer(rows_, np.uint8).reshape(-1, stride).copy(),
                                       np.frombuffer(idx_, '<i8').reshape(-1, 3)))
                continue
            idx = np.frombuffer(b, '<u4' if wide else '<u2', p['triangles'] * 3, ibase + p['first_index'] * (4 if wide else 2))
            idx = idx.astype(np.int64)
            lo, hi = int(idx.min()), int(idx.max())
            rows = np.frombuffer(b, np.uint8, (hi - lo + 1) * stride, vbase + p['vertex_offset'] + lo * stride)
            self.parts.append(Part(dict(p), layouts[name], rows.reshape(-1, stride).copy(), (idx - lo).reshape(-1, 3)))
        self.winding = self._winding()

    def part(self, name):
        for p in self.parts:
            if p.name.lower() == name.lower():
                return p
        raise EditError(f'{self.where}: no part {name} (has {", ".join(q.name for q in self.parts)})')

    def _winding(self):
        """+1 when the files wind triangles counter-clockwise around their normals, else -1 (majority of the
        largest part that has normals; a depth-only part has none)."""
        nat = _native()
        if nat is not None and all(_rows_ok(q) for q in self.parts):
            got = nat.skin_winding([(q.rows.tobytes(), len(q.rows), bytes(q.layout), q.idx.tobytes()) for q in self.parts])
            if got is not None:
                return got
        return self._winding_reference()

    def _winding_reference(self):
        """The Python _winding (the reference)."""
        with_normals = [q for q in self.parts if self.decode(q, NORMAL) is not None and self.decode(q, POS) is not None]
        if not with_normals:
            return 1
        p = max(with_normals, key=lambda q: len(q.idx))
        pos, nrm = self.decode(p, POS)[:, :3], self.decode(p, NORMAL)      # positions may be 4 halves (x, y, z, 1)
        a, b, c = pos[p.idx[:, 0]], pos[p.idx[:, 1]], pos[p.idx[:, 2]]
        s = np.sum(np.cross(b - a, c - a) * (nrm[p.idx[:, 0], :3] + nrm[p.idx[:, 1], :3] + nrm[p.idx[:, 2], :3]), axis=1)
        return 1 if np.sum(s > 0) >= np.sum(s < 0) else -1

    def decode(self, p, usage):
        """The first stream-0 element of usage in part p: doubles (float / half formats) or the element's bytes; None
        when the layout has none."""
        nat = _native()
        if nat is not None and _rows_ok(p) and type(usage) is int and 0 <= usage < 256:
            got = nat.skin_decode(p.rows.tobytes(), len(p.rows), bytes(p.layout), usage)
            if got is not None:
                if got[0] == 'absent':
                    return None
                return np.frombuffer(got[2], '<f8' if got[0] == 'f' else np.uint8).reshape(len(p.rows), got[1])
        return self.decode_reference(p, usage)

    def decode_reference(self, p, usage):
        """The Python decode (the reference)."""
        els, _ = p.elements
        for f, u, off, stream in els:
            if u == usage and stream == 0:
                raw = np.ascontiguousarray(p.rows[:, off:off + meshdata.SIZE[f]])
                if f == 0x03:
                    return raw.view('<f4').reshape(len(p.rows), 3).astype(float)
                if f in (0x05, 0x06, 0x07, 0x08):
                    return raw.view('<f2').reshape(len(p.rows), -1).astype(float)
                return raw
        return None

    def _encode(self, p, geo, palette, extra_bones=()):
        """Rows for geo in part p's layout; palette: skeleton bone per blend index."""
        els, stride = p.elements
        n = len(geo.pos)
        rows = np.zeros((n, stride), np.uint8)
        where = {b: i for i, b in enumerate(palette)}
        try:
            bi = np.array([where[b] for b in geo.bone], np.uint8)
        except KeyError as e:
            raise EditError(f'{self.where}: bone {e} is not in part {p.name}\'s bones {palette}') from None
        tan, bin_ = geo.tangents()
        nat = _native()
        if nat is not None and isinstance(p.layout, (bytes, bytearray)):
            arrays = [np.asarray(a) for a in (geo.pos, geo.nrm, geo.uv, tan, bin_)]
            if all(a.dtype == np.float64 for a in arrays):
                got = nat.skin_encode(bytes(p.layout), n, *(np.ascontiguousarray(a).tobytes() for a in arrays[:3]),
                                      bi.tobytes(), *(np.ascontiguousarray(a).tobytes() for a in arrays[3:]))
                if got is not None and all(a.shape == (n, 2 if k == 2 else 3) for k, a in enumerate(arrays)):
                    return np.frombuffer(got, np.uint8).reshape(n, stride).copy()
        for f, u, off, stream in els:
            if stream != 0:
                continue
            if u == POS:
                v = geo.pos
                if f == 0x03:
                    rows[:, off:off + 12] = v.astype('<f4').view(np.uint8).reshape(n, 12)
                elif f == 0x07:
                    rows[:, off:off + 6] = v.astype('<f2').view(np.uint8).reshape(n, 6)
                elif f == 0x08:
                    rows[:, off:off + 8] = np.hstack([v, np.ones((n, 1))]).astype('<f2').view(np.uint8).reshape(n, 8)
                else:
                    raise EditError(f'position format {f:#x} is not written')
            elif u == POS_W and f == 0x05:
                rows[:, off:off + 2] = np.ones((n, 1)).astype('<f2').view(np.uint8).reshape(n, 2)
            elif u == INDICES and f == 0x0C:
                rows[:, off:off + 4] = np.repeat(bi[:, None], 4, 1)
            elif u == WEIGHTS and f == 0x0C:
                rows[:, off] = 255
            elif u in (NORMAL, TANGENT, BINORMAL) and f == 0x08:
                v = {NORMAL: geo.nrm / np.maximum(np.linalg.norm(geo.nrm, axis=1, keepdims=True), 1e-12),
                     TANGENT: tan, BINORMAL: bin_}[u]
                rows[:, off:off + 8] = np.hstack([v, np.ones((n, 1))]).astype('<f2').view(np.uint8).reshape(n, 8)
            elif u == TEX and f == 0x06:
                rows[:, off:off + 4] = geo.uv.astype('<f2').view(np.uint8).reshape(n, 4)
            else:
                raise EditError(f'{self.where}: part {p.name}: vertex element format {f:#x} usage {u} is not written')
        return rows

    def _tris(self, geo):
        t = np.asarray(geo.tris, np.int64)
        return t if self.winding > 0 else t[:, ::-1]

    def replace(self, name, geo):
        """Part `name` becomes geo (its bones: the skeleton bones geo uses, in first-use order)."""
        p = self.part(name)
        palette = list(dict.fromkeys(int(b) for b in geo.bone))
        if len(palette) > 255:
            raise EditError('too many bones for one part')
        sharing = [q.name for q in self.parts if q is not p and not q.changed
                   and q.info['vertex_offset'] == p.info['vertex_offset']]
        if sharing:
            # they draw the same vertices: replacing one would leave the other with the old shape
            raise EditError(f'{self.where}: part {p.name} shares its vertices with {", ".join(sharing)} - replace those too')
        p.rows = self._encode(p, geo, palette)
        p.idx = self._tris(geo)
        p.info['bones'] = palette
        p.changed = True
        return p

    def add_to(self, name, geo):
        """Adds geo to part `name` (e.g. the depth-only part), keeping what it has; its bones are extended."""
        p = self.part(name)
        palette = list(p.info['bones'])
        for b in dict.fromkeys(int(b) for b in geo.bone):
            if b not in palette:
                palette.append(b)
        rows = self._encode(p, geo, palette)
        p.idx = np.vstack([p.idx, self._tris(geo) + len(p.rows)])
        p.rows = np.vstack([p.rows, rows])
        p.info['bones'] = palette
        p.changed = True
        return p

    def add_part(self, name, layout, geo, like, draw_list=0, at=None):
        """A new part `name`: geo written in vertex layout `layout` (a 73-byte layout record), its header (skinning
        kind and the rest) as part `like`'s, its bones the skeleton bones geo uses; listed in the LOD's part list
        `draw_list` (0: the parts drawn - where BFBC2's scoped rifles list their lens, the M40's jntWpn_1_Glass; 3: the
        depth-only parts). Its programs are its material's: the layout must be one they are compiled for.
        at: its index among the parts (None: last) - the part lists are renumbered; a mesh set's shader list, one
        material per part, must take the new part's material at the same index."""
        if any(p.name.lower() == name.lower() for p in self.parts):
            raise EditError(f'{self.where}: there is a part {name} already')
        tmpl = self.part(like)
        palette = list(dict.fromkeys(int(b) for b in geo.bone))
        if len(palette) > 255:
            raise EditError('too many bones for one part')
        info = dict(tmpl.info)
        info['name'] = name.encode('latin-1')
        info['bones'] = palette
        # its own vertices (to_bytes places them): not the template's, which replace() would read as shared
        info['vertex_offset'] = info['first_index'] = None
        p = Part(info, layout, np.zeros((0, meshdata.elements(layout)[1]), np.uint8), np.zeros((0, 3), np.int64))
        p.rows = self._encode(p, geo, palette)
        p.idx = self._tris(geo)
        p.changed = True
        at = len(self.parts) if at is None else at
        if not 0 <= at <= len(self.parts):
            raise EditError(f'{self.where}: no part index {at}')
        self.parts.insert(at, p)
        lists = [[i + (i >= at) for i in x] for x in self.m['lists']]
        lists[draw_list].append(at)
        self.m = dict(self.m, lists=lists)
        return p

    def to_bytes(self):
        """The file: its vertex and index buffers as they were (unchanged parts point where they did), each changed
        part's vertices and indices after them."""
        nat = _native()
        if nat is not None and all(_rows_ok(p) for p in self.parts) and type(self.vb) is bytes and type(self.ib) is bytes:
            got = nat.skin_lay_out(self.vb, self.ib, [(p.rows.tobytes(), p.rows.shape[1], p.idx.tobytes()) if p.changed
                                                      else None for p in self.parts])
            if got is not None:
                vb, ib, at = got
                parts = []
                for p, a in zip(self.parts, at):
                    info = dict(p.info)
                    if a is not None:
                        info['vertex_offset'], info['first_index'], info['vertices'], info['triangles'] = a
                        info['stride'] = p.rows.shape[1]
                        info['fb'] = len(info['bones'])
                    parts.append(info)
                m = dict(self.m)
                m['parts'] = parts
                return meshdata.write(m, vb, ib)
        return self.to_bytes_reference()

    def to_bytes_reference(self):
        """The Python to_bytes (the reference)."""
        vb, ib = bytearray(self.vb), bytearray(self.ib)
        parts = []
        for p in self.parts:
            info = dict(p.info)
            if p.changed:
                stride = p.rows.shape[1]
                vb += b'\0' * (-len(vb) % stride)            # whole vertices from the buffer's start
                if len(p.rows) - 1 > 0xFFFF:
                    raise EditError(f'{self.where}: part {p.name} has more than 65536 vertices')
                info['vertex_offset'] = len(vb)
                info['first_index'] = len(ib) // 2
                info['vertices'] = len(p.rows)
                info['triangles'] = len(p.idx)
                info['stride'] = stride
                info['fb'] = len(info['bones'])
                vb += p.rows.tobytes()
                ib += p.idx.reshape(-1).astype('<u2').tobytes()
            parts.append(info)
        m = dict(self.m)
        m['parts'] = parts
        return meshdata.write(m, bytes(vb), bytes(ib))

    def bounds(self):
        """(min x, y, z, max x, y, z) of every part's positions."""
        nat = _native()
        if nat is not None and all(_rows_ok(p) for p in self.parts):
            got = nat.skin_bounds([(p.rows.tobytes(), len(p.rows), bytes(p.layout)) for p in self.parts])
            if got is not None:
                return tuple(np.float64(v) for v in got)
        return self.bounds_reference()

    def bounds_reference(self):
        """The Python bounds (the reference)."""
        pts = np.vstack([self.decode(p, POS)[:, :3] for p in self.parts])
        return tuple(pts.min(0)) + tuple(pts.max(0))


def lod_size(b):
    """The size a mesh set records for a MeshData: vertex bytes + index bytes + 12."""
    m = meshdata.parse(b, False)
    return m['vertex_bytes'] + m['index_bytes'] + 12
