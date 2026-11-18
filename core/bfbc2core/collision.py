"""Collision of a level's blueprints: which Havok packfile each blueprint uses and the triangles in it, as NavKit
reads them (bfbc2-auto-navkit/tools/levelobjects.py: havok_index, mesh_aliases, triangles).

    havok, aliases = index(bases)      # {blueprint: packfile path}, {blueprint: blueprint of its Mesh} (or the
                                       # OSError that stopped reading the converted XML)
    havok = havok_index(bases)         # the packfiles alone (NavKit's tools that use no aliases)
    blueprint_triangles(bps, havok)    # {blueprint: triangles or None}, the packfiles read at once
    triangles(path)                    # (n, 3, 3) float32 local-space triangles of a packfile, or None
    triangles_many(paths)              # triangles() of many packfiles: each result an array, None or the exception

All of it runs in the suite's Rust core (rust/, module bc2_native: bc2_scene::collision, bc2_formats::havok): one walk
of each folder, the partitions and packfiles read on all cores, each blueprint's Mesh from its binary .dbx (the
converted XML only where the binary cannot answer). A packfile NavKit's reader refuses (not a packfile, a name with no
end) gives None; a damaged one, whose fields run past its end, raises ValueError. The Python path (NavKit's
levelobjects functions over hkpack.py) was retired on 2026-11-18 (round F1a, rust/BENCHMARKS.md "Retired Python");
its answers - every packfile of the game copy, the index of every prepared level - are kept as recorded digests
(core/tests/geometry_golden.json, compared by test_geometry_native.py). Without the module these functions raise
bfbc2core.native.NativeMissing saying how to get it.

The index keeps each partition's Mesh reference in a cache (cache_folder(): bfbc2-tool-suite/cache/collision under
%LOCALAPPDATA%, or BFBC2_COLLISION_CACHE; one file per data folder), reused while the XML file and its .dbx keep the
size and modification time they had when it was read - an unchanged level is indexed without opening its partitions.
use_cache(None) (or BFBC2_COLLISION_CACHE=0) turns it off; the index is the same either way. It is kept under
BFBC2_COLLISION_CACHE_MB (default 256), the least recently used files deleted first (prune_cache, bfbc2core.caches).
"""
import os
from collections.abc import Iterable, Mapping, Sequence
from types import ModuleType
from typing import Any

import numpy as np

_native: ModuleType | None = None


def _rust() -> ModuleType:
    """The Rust core, or NativeMissing saying how to get it."""
    global _native
    if _native is None:
        from . import native
        _native = native.require('the collision index and Havok reader (bfbc2core.collision)', 'collision_index')
    return _native


CACHE_ENV = 'BFBC2_COLLISION_CACHE'      # the cache folder; '0' turns the cache off


def _default_cache() -> str | None:
    v = os.environ.get(CACHE_ENV, '')
    if v == '0':
        return None
    return v or os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'cache',
                             'collision')


_cache: str | None = _default_cache()


def cache_folder() -> str | None:
    """The folder of the index's mesh-alias cache, or None when it is off."""
    return _cache


def use_cache(folder: str | None) -> str | None:
    """The mesh-alias cache in folder (None: off) for this process; returns cache_folder()."""
    global _cache
    _cache = folder
    return _cache


def prune_cache() -> None:
    """Keeps the mesh-alias cache under BFBC2_COLLISION_CACHE_MB (default 256), the least recently used folders'
    files deleted first (bfbc2core.caches; at most every 15 minutes; the index marks a file it reuses)."""
    if _cache:
        from . import caches
        caches.maybe_prune('collision', _cache)


# ------------------------------------------------------------------------------------------------ the API

def index(bases: Sequence[str]) -> tuple[dict[str, str], dict[str, str] | Exception]:
    """(havok_index(bases), the mesh aliases): every *_Havok.res under the folders by lower-case blueprint path (the
    first folder that has one wins; <name>_Havok_Havok.res also under <name>), and the blueprints whose Mesh names
    another object's mesh (lower-case blueprint -> the mesh's blueprint, without _mesh), from the binary partitions
    and the converted XML; the second is the exception that stopped it when an XML file could not be read."""
    havok, aliases = _rust().collision_index([os.fspath(b) for b in bases], _cache)
    prune_cache()
    return dict(havok), (aliases if isinstance(aliases, Exception) else dict(aliases))


def havok_index(bases: Sequence[str]) -> dict[str, str]:
    """Every *_Havok.res under the folders by lower-case blueprint path (the first folder that has one wins), without
    the mesh aliases."""
    return dict(_rust().havok_index([os.fspath(b) for b in bases]))


def _array(t: Any) -> np.ndarray | None:
    return None if t is None else np.frombuffer(t, '<f4').reshape(-1, 3, 3)


def triangles(path: str) -> np.ndarray | None:
    """The collision triangles (local space, (n, 3, 3) float32) of a blueprint's Havok packfile - its storage
    extended mesh subparts' triangles and each convex shape's bounding box -, or None (not a packfile, refused by
    NavKit's reader, or no triangles). ValueError for a damaged one, OSError for a file that cannot be read."""
    r = triangles_many([path])[0]
    if isinstance(r, Exception):
        raise r
    return r


def blueprint_triangles(blueprints: Iterable[str], havok: Mapping[str, str]) -> dict[str, np.ndarray | None]:
    """{blueprint: triangles(havok[blueprint])} for each blueprint (None for one havok has no packfile for), the
    packfiles read at once (triangles_many), each once. A packfile that cannot be read raises its ValueError /
    OSError, as triangles() does."""
    out: dict[str, np.ndarray | None] = dict.fromkeys(blueprints)
    wanted = [bp for bp in out if bp in havok]
    for bp, t in zip(wanted, triangles_many([havok[bp] for bp in wanted])):
        if isinstance(t, Exception):
            raise t
        out[bp] = t
    return out


def triangles_many(paths: Sequence[str]) -> list[np.ndarray | None | Exception]:
    """triangles() of each path, in order, read and decoded on all cores: each result the array, None, or the
    exception (ValueError, OSError) returned."""
    return [r if isinstance(r, Exception) else _array(r)
            for r in _rust().havok_triangles_files([os.fspath(p) for p in paths])]
