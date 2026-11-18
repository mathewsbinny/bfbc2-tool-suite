"""The Rust core for NavKit's tools around the build (round E4e: navvehicle, navdomains, levelobjects, the parity
measures, ...): each ported piece keeps its Python as the reference and the fallback this round (rust/BENCHMARKS.md
"E4e"), so the core is optional here - not built, BFBC2_PURE_PYTHON=1 (every Python reader), BFBC2_NAVTOOLS_PYTHON=1
(these pieces only) or use_native(False): the Python runs.

  core(name)       the module when it has that function and the core is in use, else None
  use_native(on)   switch the E4e pieces to the core (when built, whatever BFBC2_PURE_PYTHON says) or to Python;
                   returns 'rust' or 'python'
"""
import os
import sys
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

PURE_PYTHON_ENV = 'BFBC2_PURE_PYTHON'
NAVTOOLS_PYTHON_ENV = 'BFBC2_NAVTOOLS_PYTHON'
_forced: bool | None = None          # use_native's choice (None: the environment decides)


def _module() -> Any:
    try:
        suite_paths.use('core')
        from bfbc2core import dbx as _dbx
    except ImportError:
        return None
    return _dbx._import_native()


def core(name: str) -> Any:
    """The Rust module if `name` is one of its functions and the E4e pieces run in Rust, else None."""
    if _forced is False:
        return None
    if _forced is None and any(os.environ.get(e, '') not in ('', '0') for e in (PURE_PYTHON_ENV, NAVTOOLS_PYTHON_ENV)):
        return None
    m = _module()
    return m if m is not None and hasattr(m, name) else None


def use_native(on: bool) -> str:
    """The E4e pieces in the Rust core (when built) or in Python; 'rust' or 'python'."""
    global _forced
    _forced = bool(on)
    return 'rust' if on and _module() is not None else 'python'
