r"""meshview.py - look at the game's models: a picture of a mesh's parts with their colour textures. To look around
a mesh (orbit, levels of detail, parts, destruction states, textures by channel), use the map editor's asset preview.

  python meshview.py <mesh>... [--out <folder>] [--lod N] [--size W] [--view side|top|front] [--opfor <folder>]
  python meshview.py find <text> [--opfor <folder>]          mesh names containing <text>

<mesh> is a mesh asset name as the game's files give it, e.g. Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh
(any case; the .dbx / _lod0_data.res endings may be given). Several meshes are drawn one above the other.
Default output: <OPFOR>\work\tools_out\meshview\<last name part>_<view>.png (--png is accepted: the picture is
always drawn). The picture contains game content: keep it in your work folder, do not share it.

How a mesh is read (the console converter's FORMAT.md sections 3-4): <mesh>_lod<N>_data.res (MeshData) names its parts,
their vertex and index ranges; each part's vertex layout is in the shader database of a level that draws the mesh
(section 8.3: '<mesh>_lod<N>/<part>/standard' -> layout); the mesh blueprint (<mesh>.dbx) gives each surface name its
shader, and the database entry '<mesh>/<shader>' names the shader's textures (the colour map is drawn). Positions are
the parts' rest positions (skinned meshes: the pose they were modelled in). Nothing in the game copy is written.
"""
# Round PR1b-2-rest (audit AU5 ARC-4): the index, load_mesh, mesh_surfaces, stack_models and render are the Rust
# port's (bc2_native: MeshViewIndex, meshview_stack, meshview_render; rust/bc2-convert/src/meshview), this module the
# NumPy arrays and dicts its callers take. The index's cache is work\tools_out\meshview_index.bin (bc2_gamefs'
# format; the pickle meshview_index.pkl retired). page() went in round OR4: Project OPFOR BFBC2's WW2 previews write
# their pages with the Rust's (bc2_native.meshview_page, rust/bc2-convert/src/meshview/page.rs).
import collections
import os
import re
import sys
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'shader-tools')
import shaderdb  # noqa: E402
from bfbc2core import fbrb  # noqa: E402

CACHE_NAME = 'meshview_index.bin'


class ViewError(Exception):
    pass


def _native() -> Any:
    from bfbc2core import native
    return native.require("the mesh viewer (meshview.py: the index, load_mesh, render)", 'meshview_open')


def _say(text: str) -> None:
    print(text, file=sys.stderr)


# ------------------------------------------------------------------------------------------------ game copy index

def _bundles(root):
    """Stock bundles - levels, async (weapons, characters ...) and the rest - Package before Dist (OPFOR's own
    builds and Onslaught levels left out, and any level an OPFOR install step put there: its folder holds an
    opfor-*-installed.json record, e.g. Wake Island's)."""
    out = []
    for w in ('Package', os.path.join('Dist', 'win32')):
        top = os.path.join(root, 'game', w)
        for dp, dn, fn in os.walk(top):
            dn.sort()
            rel = os.path.relpath(dp, top).replace(os.sep, '/').lower()
            if re.match(r'levels/(ons_|opfor_)', rel):
                continue
            if rel.startswith('levels/') and any(re.fullmatch(r'opfor-.+-installed\.json', f.lower()) for f in fn):
                dn[:] = []                                   # its vo\ sub-folder too
                continue
            for f in sorted(fn):
                if f.lower().endswith('.fbrb'):
                    out.append((rel, os.path.join(dp, f)))
    return out


class Partitions:
    """The .dbx partitions of the bundles read last, each bundle's read in one pass (Index.read). A bundle is one
    gzip stream, so one entry costs decompressing the bundle up to it (about 1 s in mp_common's 588 MB); all 8,446
    partitions of mp_common are 36 MB and come in one pass of 2.4 s. A converter that reads hundreds of partitions
    one by one (the WW2 sound porter's templates: half of a WW2 level build, 2026-11-16) reads each bundle once.
    Bundles are kept by their path, size and modification time, the least recently used dropped beyond budget
    bytes; the bytes are fbrb.read's, so the results are the same as reading one at a time."""

    def __init__(self, budget: int = 768 << 20) -> None:
        self.budget = budget
        self.size = 0
        self.bundles: collections.OrderedDict[tuple[str, int, int], dict[str, bytes]] = collections.OrderedDict()

    def get(self, path: str, name: str, names) -> bytes | None:
        """Entry name of bundle path from the cache, the bundle's partitions (names(path): the entry names to read)
        read on a miss; None when the bundle cannot be read that way (the caller reads the entry alone)."""
        try:
            st = os.stat(path)
        except OSError:
            return None
        key = (os.path.normcase(os.path.abspath(path)), st.st_size, st.st_mtime_ns)
        got = self.bundles.get(key)
        if got is None:
            wanted = names(path)
            try:
                got = fbrb.read(path, wanted)
            except (KeyError, ValueError, OSError):
                return None
            self.bundles[key] = got
            self.size += sum(map(len, got.values()))
            while self.size > self.budget and len(self.bundles) > 1:
                _, old = self.bundles.popitem(last=False)
                self.size -= sum(map(len, old.values()))
        else:
            self.bundles.move_to_end(key)
        return got.get(name)

    def peek(self, path: str, name: str) -> bytes | None:
        """Entry name of bundle path when the cache holds the bundle (unchanged on disk), else None - never reads."""
        try:
            st = os.stat(path)
        except OSError:
            return None
        got = self.bundles.get((os.path.normcase(os.path.abspath(path)), st.st_size, st.st_mtime_ns))
        return got.get(name) if got is not None else None


PARTITIONS = Partitions()


class Index:
    """Which bundle holds each entry (first stock bundle), and every level's Dx9 shader database facts: part name ->
    vertex layout record, entry name -> texture names - the Rust port's (bc2_native.meshview_open). Cached in
    work\\tools_out\\meshview_index.bin, rebuilt when a bundle changes; an index with an unreadable bundle or database
    left out is said and not cached. where / parts / entries are the dicts, made when first asked."""

    def __init__(self, root, cache=None):
        """cache: the index file (default work\\tools_out\\meshview_index.bin in the OPFOR folder; renderqa.py keeps
        its own, so that it writes nothing into the project it checks)."""
        self.root = root
        path = cache or os.path.join(root, 'work', 'tools_out', CACHE_NAME)
        # one process builds a missing index at a time (under the cache's lock); one that waited reads what the other
        # wrote (tools or tests side by side: each built it for minutes, 2026-11-17)
        self._ix = _native().meshview_open([b for _lv, b in _bundles(root)], path, _say)
        if self._ix.databases:
            fbrb._budget()                                  # as after a read of the bundles' databases

    def _tables(self) -> None:
        self.__dict__['where'], self.__dict__['parts'], self.__dict__['entries'] = self._ix.tables()

    def __getattr__(self, name: str) -> Any:
        """where (entry name lower case -> (bundle, entry name)), parts (part name -> vertex layout record after its
        GUID) and entries (shader database entry name -> texture names), made from the index on first use."""
        if name in ('where', 'parts', 'entries') and '_ix' in self.__dict__:
            self._tables()
            return self.__dict__[name]
        raise AttributeError(name)

    def locate(self, name: str) -> tuple[str, str] | None:
        """(bundle, entry name as the bundle holds it) of an entry (any case), or None."""
        if 'where' in self.__dict__:
            return self.__dict__['where'].get(name.lower())
        return self._ix.locate(name)

    def read(self, name):
        hit = self.locate(name)
        if not hit:
            return None
        if hit[1].lower().endswith('.dbx'):
            b = PARTITIONS.get(hit[0], hit[1], self._partitions_of)
            if b is not None:
                return b
        return fbrb.read(hit[0], [hit[1]])[hit[1]]

    def read_many(self, names, whole_bundles=True):
        """{name: bytes or None} of read() for many names (None: a name the index lacks, as read() gives): each
        bundle read once for all of its names - a bundle is one gzip stream, so read() of each name decompresses it
        from the start again (round E3g: test_havokport's 991 physics files, 0.36 s each one at a time, 369 of its 381
        s; round E3i: the Granny port test) - the bundles on all cores with the Rust core; partitions (.dbx) through
        their own cache, as read() - or, with whole_bundles=False, read with the rest (only the partitions named, the
        bundles on all cores; those the partition cache holds from it: round E3r, the fair-port test read 42 bundles'
        partitions one bundle at a time, 11 s). A bundle that cannot be read raises as read() does."""
        out: dict[str, bytes | None] = {}
        by: dict[str, list[tuple[str, str]]] = {}
        for n in names:
            hit = self.locate(n)
            if hit and hit[1].lower().endswith('.dbx') and not whole_bundles:
                cached = PARTITIONS.peek(hit[0], hit[1])
                if cached is not None:
                    out[n] = cached
                    continue
            if not hit or (hit[1].lower().endswith('.dbx') and whole_bundles):
                out[n] = self.read(n)                       # partitions: their own cache
            else:
                by.setdefault(hit[0], []).append((n, hit[1]))
        bundles = list(by)
        for bundle, got in zip(bundles, fbrb.read_many([(b, [e for _, e in by[b]]) for b in bundles])):
            if isinstance(got, Exception):
                raise got
            for n, e in by[bundle]:
                out[n] = got[e]
        return {n: out[n] for n in names}

    def _partitions_of(self, bundle):
        """The .dbx entry names the index finds in bundle (Partitions reads them in one pass)."""
        return self._ix.partitions_of(bundle)


def _native_index(ix) -> Any:
    """The Rust index behind a meshview.Index (load_mesh and mesh_surfaces need it)."""
    n = getattr(ix, '_ix', None) if isinstance(ix, Index) else None
    if n is None:
        raise TypeError(f'a meshview.Index is needed, not {type(ix).__name__}')
    return n


# ------------------------------------------------------------------------------------------------ reading a mesh

def mesh_surfaces(ix, name):
    """{surface name (lower): shader asset path} from the mesh blueprint; without one, the database's entries for the
    mesh, by the shader's last name."""
    return dict(_native_index(ix).mesh_surfaces(name))


_surfaces = mesh_surfaces  # the earlier private name, kept for callers that still use it


def _array(b: bytes, dtype: Any, cols: int) -> np.ndarray:
    """A writable (n, cols) array of the bytes (the callers change parts in place)."""
    return np.frombuffer(b, dtype).reshape(-1, cols).copy()


def load_mesh(ix, name, lod=0):
    """[{name, surface, positions (n,3), normals, uvs, indices (m,3), texture name, image}] of the mesh's LOD
    (positions, normals and uvs float32, indices int64, the image (h, w, 3) uint8). Depth-only copies (_zonly) are
    left out; a part without a vertex layout in any level's shader database, or without positions, is skipped and
    said."""
    try:
        parts, notes = _native_index(ix).load_mesh(name, lod)
    except ValueError as e:
        raise ViewError(str(e)) from None
    for n in notes:
        print(n, file=sys.stderr)
    out = []
    for pname, surf, (pos, n), nrm, uv, idx, tname, img in parts:
        out.append({'name': pname, 'surface': surf, 'positions': _array(pos, '<f4', 3),
                    'normals': _array(nrm, '<f4', 3) if nrm is not None else None,
                    'uvs': _array(uv, '<f4', 2) if uv is not None else None,
                    'indices': np.frombuffer(idx, '<u4').astype(np.int64).reshape(-1, 3),
                    'texture': tname,
                    'image': np.frombuffer(img[2], np.uint8).reshape(img[1], img[0], 3).copy() if img is not None
                    else None})
        if n != len(out[-1]['positions']):
            raise ViewError(f'{name}: {pname}: {n} vertices read, {len(out[-1]["positions"])} given')
    return out


# ------------------------------------------------------------------------------------------------ output

def _f32(a: Any) -> bytes:
    return np.ascontiguousarray(a, dtype='<f4').tobytes()


def stack_models(models):
    """The models one above the other (y), each centred on its own bounds: [(model name, parts, offset)] (offset a
    float32 (3,) array; positions drawn as float32)."""
    models = list(models)
    offs = _native().meshview_stack([b''.join(_f32(np.asarray(p['positions'])[:, :3]) for p in parts)
                                     for _, parts in models])
    return [(name, parts, np.array(off, np.float32)) for (name, parts), off in zip(models, offs)]


_layout = stack_models  # the earlier private name, kept for callers that still use it


def _render_part(p: dict) -> tuple:
    """A part for bc2_native.meshview_render: float32 positions, whether it has normals, float32 uvs, uint32
    indices, the colour image as (w, h, rgb bytes)."""
    pos = np.asarray(p['positions'])[:, :3]
    idx = np.asarray(p['indices']).reshape(-1, 3)
    if len(idx) and (idx.min() < 0 or idx.max() >= len(pos)):
        raise IndexError(f'{p.get("name", "a part")}: an index past its {len(pos)} vertices')
    uv, tex = p.get('uvs'), p.get('image')
    image = None
    if tex is not None:
        tex = np.asarray(tex)
        if tex.dtype != np.uint8:
            tex = np.clip(tex, 0, 255).astype(np.uint8)
        tex = np.ascontiguousarray(tex[..., :3])
        image = (tex.shape[1], tex.shape[0], tex.tobytes())
    return (_f32(pos), p.get('normals') is not None, _f32(np.asarray(uv)[:, :2]) if uv is not None else None,
            np.ascontiguousarray(idx, dtype='<u4').tobytes(), image)


def render(laid, size=1200, view='side'):
    """A z-buffered picture (RGB array) of the laid-out models: textured, lit by the triangles' facing (parts with
    normals). side: the models' long horizontal axis across, up up; top: from above; front: along the long axis.
    Drawn by the Rust port (bc2_convert::meshview::render) in float32 positions, as NumPy drew float32 parts."""
    if view not in ('side', 'top', 'front'):
        raise KeyError(view)
    jobs = [([float(v) for v in np.asarray(off, np.float32)[:3]], [_render_part(p) for p in parts])
            for _, parts, off in laid]
    w, h, rgb = _native().meshview_render(jobs, int(size), view)
    return np.frombuffer(rgb, np.uint8).reshape(h, w, 3).copy()


# ------------------------------------------------------------------------------------------------ command line

def _clean(name):
    name = name.replace('\\', '/')
    return re.sub(r'(?i)(_lod\d+_data)?\.(res|dbx)$', '', name)


def main(argv):
    args, opts, flags = [], {}, set()
    it = iter(argv[1:])
    for a in it:
        if a in ('--png',):
            flags.add(a)
        elif a.startswith('--'):
            opts[a] = next(it, None)
        else:
            args.append(a)
    if not args:
        print(__doc__)
        return 1
    if '--level' in opts:                 # once named the level whose shader database to read; all are indexed now
        print('note: --level is not needed (every level\'s shader database is indexed); ignored', file=sys.stderr)
    root = shaderdb.find_opfor(opts.get('--opfor'))
    ix = Index(root)
    if args[0] == 'find':
        text = ' '.join(args[1:]).lower()
        hits = sorted({re.sub(r'_lod\d+_data\.res$', '', n) for n in ix.where if n.endswith('_data.res') and text in n})
        print('\n'.join(ix.where[h + '_lod0_data.res'][1][:-len('_lod0_data.res')] if h + '_lod0_data.res' in ix.where
                        else h for h in hits) or f'no mesh name contains "{text}"')
        return 0
    lod = int(opts.get('--lod') or 0)
    models = []
    for a in args:
        name = _clean(a)
        hit = ix.locate(f'{name}_lod{lod}_data.res')
        name = hit[1][:-len(f'_lod{lod}_data.res')] if hit else name
        parts = load_mesh(ix, name, lod)
        if not parts:
            raise ViewError(f'{name}: no part could be read')
        models.append((name, parts))
        for p in parts:
            print(f'{name.split("/")[-1]}: {p["name"]:45s} {len(p["indices"]):6d} tris  '
                  f'{(p["texture"] or "(no colour texture)").split("/")[-1]}')
    laid = stack_models(models)
    out = opts.get('--out') or os.path.join(root, 'work', 'tools_out', 'meshview')
    os.makedirs(out, exist_ok=True)
    stem = '+'.join(n.split('/')[-1] for n, _ in models)[:120]
    # round GR3: the picture only (the page for a browser went: the map editor's asset preview shows a mesh)
    from PIL import Image
    png = os.path.join(out, f'{stem}_{opts.get("--view") or "side"}.png')
    Image.fromarray(render(laid, int(opts.get('--size') or 1200), opts.get('--view') or 'side')).save(png)
    print(f'picture: {png}')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except (ViewError, shaderdb.ShaderDbError) as e:
        print(f'error: {e}', file=sys.stderr)
        sys.exit(1)
