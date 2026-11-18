"""The BF1943 importer's Rust core (round F2b, rust/bc2-convert: mopp, havokport, emitteratlas, dbxcatalogue,
meshport's searches), run by the suite's module bc2_native (`python rust\\build_native.py`).

native() is the module while it has the importer's functions, else None. Every caller keeps its Python as the
reference and the fallback: a Rust function returns `(result,)` when it answered and None where the port refuses (the
reference would raise an exception other than its own, or meets a case the port does not reproduce); it raises
ValueError with the reference's own message for the reference's own error (MoppError, PortError, MaterialError,
EmitterAtlasError), which the caller raises as that class. Arguments the port cannot take (TypeError,
OverflowError) are refusals too. BFBC2_PURE_PYTHON=1 or BFBC2_IMPORTER_PYTHON=1 (or use_native(False)) runs the
Python everywhere.

    r = importcore.call('mopp_query', code, qmin, qmax, error=MoppError)
    if r is not None:
        return set(r[0])
"""
import os
import sys
from types import ModuleType
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx as _coredbx  # noqa: E402

ENV = 'BFBC2_IMPORTER_PYTHON'
ATTR = 'MeshLayouts'                 # the newest name: an older module lacks it


def _load() -> ModuleType | None:
    if any(os.environ.get(k, '') not in ('', '0') for k in (ENV, _coredbx.PURE_PYTHON_ENV)):
        return None
    m = _coredbx._import_native()
    return m if m is not None and hasattr(m, ATTR) else None


_native = _load()


def native() -> ModuleType | None:
    """bc2_native with the importer's functions, or None (Python only)."""
    return _native


def backend() -> str:
    """'rust' or 'python'."""
    return 'rust' if _native is not None else 'python'


def use_native(on: bool) -> str:
    """The Rust core on (when it is built, whatever the environment says) or off for this process; returns
    backend()."""
    global _native
    m = _coredbx._import_native() if on else None
    _native = m if m is not None and hasattr(m, ATTR) else None
    return backend()


def call(name: str, *args: Any, error: type[Exception] = ValueError) -> tuple[Any, ...] | None:
    """The Rust function's `(result,)`, or None (no core, a refusal, or arguments it does not take); its
    ValueError raised as `error` with the same message."""
    if _native is None:
        return None
    try:
        r: tuple[Any, ...] | None = getattr(_native, name)(*args)
    except ValueError as e:
        raise error(str(e)) from None
    except (TypeError, OverflowError):
        return None
    return r
