"""Collision of a level's blueprints: which Havok packfile each blueprint uses and the triangles in it, as NavKit
reads them (bfbc2-auto-navkit/tools/levelobjects.py: havok_index, mesh_aliases, triangles; hkpack.py).

    havok, aliases = index(bases)      # {blueprint: packfile path}, {blueprint: blueprint of its Mesh} (or the
                                       # OSError that stopped reading the converted XML)
    triangles(path)                    # (n, 3, 3) float32 local-space triangles of a packfile, or None
    triangles_many(paths)              # triangles() of many packfiles: each result an array, None or the exception

index and triangles_many run in the suite's Rust core (rust/, module bc2_native: bc2_scene::collision,
bc2_formats::havok) when it is built - one walk of each folder, the XML and packfiles read on all cores - else through
NavKit's Python functions; both give the same dictionaries (keys and values in the same order) and the same
triangles (core/tests/test_geometry_native.py: every packfile of the game copy, the index of every prepared level).
A packfile NavKit's reader refuses (not a packfile, a name with no end) gives None in both; a damaged one, whose
fields run past its end, raises ValueError in both. BFBC2_PURE_PYTHON=1 (or use_native(False)) forces Python;
backend() says which one runs.
"""
import os
import struct
import sys
from collections.abc import Sequence
from types import ModuleType
from typing import Any, cast

import numpy as np

from . import dbx as _dbx

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _navkit() -> Any:
    """NavKit's levelobjects (the Python reference)."""
    if _ROOT not in sys.path:
        sys.path.insert(0, _ROOT)
    import suite_paths
    suite_paths.use('navkit-tools')
    import levelobjects
    return levelobjects


# ------------------------------------------------------------------------------------------------ backend

def _with_collision(m: ModuleType | None) -> ModuleType | None:
    """m when it has the collision readers (a module built before they were added does not)."""
    return m if m is not None and hasattr(m, 'collision_index') else None


def _load_native() -> ModuleType | None:
    """The Rust core (found as dbx finds it) unless BFBC2_PURE_PYTHON is set."""
    if os.environ.get(_dbx.PURE_PYTHON_ENV, '') not in ('', '0'):
        return None
    return _with_collision(_dbx._import_native())


_native = _load_native()


def backend() -> str:
    """'rust' when collision is read in the Rust core, else 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever BFBC2_PURE_PYTHON says) or off for this process; returns
    backend()."""
    global _native
    _native = _with_collision(_dbx._import_native()) if on else None
    return backend()


# ------------------------------------------------------------------------------------------------ the API

def index(bases: Sequence[str]) -> tuple[dict[str, str], dict[str, str] | Exception]:
    """(levelobjects.havok_index(bases), levelobjects.mesh_aliases(bases)): every *_Havok.res under the folders by
    lower-case blueprint path (the first folder that has one wins), and the blueprints whose Mesh names another
    object's mesh, from the converted XML; the second is the exception that stopped it when an XML file could not be
    read."""
    if _native is not None:
        havok, aliases = _native.collision_index([os.fspath(b) for b in bases])
        return dict(havok), (aliases if isinstance(aliases, Exception) else dict(aliases))
    lo = _navkit()
    havok = lo.havok_index(bases)
    try:
        return havok, lo.mesh_aliases(bases)
    except Exception as e:                   # an XML file that could not be read: level.load notes it
        return havok, e


def _array(t: Any) -> np.ndarray | None:
    return None if t is None else np.frombuffer(t, '<f4').reshape(-1, 3, 3)


def _py_triangles(path: str) -> np.ndarray | None:
    try:
        return cast('np.ndarray | None', _navkit().triangles(path))
    except (struct.error, IndexError, TypeError, KeyError, ValueError) as e:
        raise ValueError(f'{path}: damaged Havok packfile ({type(e).__name__}: {e})') from e


def triangles(path: str) -> np.ndarray | None:
    """levelobjects.triangles(path): the collision triangles (local space, (n, 3, 3) float32) of a blueprint's Havok
    packfile, or None (not a packfile, refused by NavKit's reader, or no triangles). ValueError for a damaged one,
    OSError for a file that cannot be read."""
    r = triangles_many([path])[0]
    if isinstance(r, Exception):
        raise r
    return r


def triangles_many(paths: Sequence[str]) -> list[np.ndarray | None | Exception]:
    """triangles() of each path, in order: each result the array, None, or the exception (ValueError, OSError)
    returned. With the Rust core the files are read and decoded on all cores."""
    if _native is not None:
        return [r if isinstance(r, Exception) else _array(r)
                for r in _native.havok_triangles_files([os.fspath(p) for p in paths])]
    out: list[np.ndarray | None | Exception] = []
    for p in paths:
        try:
            out.append(_py_triangles(p))
        except (ValueError, OSError) as e:
            out.append(e)
    return out
