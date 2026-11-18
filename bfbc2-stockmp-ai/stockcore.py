"""The stock-MP tool's Rust core (round E4a, rust\\bc2-stockmp): the level model, the authoring and the heavy parts of
the offline checks, run by the suite's module bc2_native (`python rust\\build_native.py`).

native() is the module while it has the stockmp_* functions, else None. Every caller keeps its Python as the
reference and the fallback: a Rust function returns None where the port refuses (an input or a path through it the
reference answers itself - an error it raises, a value it formats), and the caller then runs the Python, so the
result is always the reference's. BFBC2_PURE_PYTHON=1 or BFBC2_STOCKMP_PYTHON=1 (or use_native(False)) runs the
Python everywhere.
"""
import os
import sys
from types import ModuleType

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx as _coredbx  # noqa: E402

ENV = 'BFBC2_STOCKMP_PYTHON'
ATTR = 'stockmp_author'           # the newest function: an older module lacks it


def _load() -> ModuleType | None:
    if any(os.environ.get(k, '') not in ('', '0') for k in (ENV, _coredbx.PURE_PYTHON_ENV)):
        return None
    m = _coredbx._import_native()
    return m if m is not None and hasattr(m, ATTR) else None


_native = _load()


def native() -> ModuleType | None:
    """bc2_native with the stock-MP functions, or None (Python only)."""
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
