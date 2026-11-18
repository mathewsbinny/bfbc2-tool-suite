"""The harness's and the crash reader's Rust core (round E4f, rust\\bc2-harness): opfor.log sessions, the crash log
window, the exe handle (exeref's and the crash reader's searches and disassembly with capstone's text), the
reachability pair bits and the ground cell index, run by the suite's module bc2_native (`python rust\\build_native.py`).

native() is the module while it has the harness functions, else None. Every caller keeps its Python as the reference
and the fallback: a Rust function returns None (a method raises NotImplementedError) where the port refuses - a
Unicode digit on a log line the reference reads, an instruction outside the capstone parity proof (every byte offset
of BFBC2Game.exe's code was decoded both ways), an image pefile maps by rules not reproduced - and the caller then
runs the Python, so the result is always the reference's. BFBC2_PURE_PYTHON=1 or BFBC2_HARNESS_PYTHON=1 (or
use_native(False)) runs the Python everywhere.
"""
import os
import sys
from types import ModuleType

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbx as _coredbx  # noqa: E402

ENV = 'BFBC2_HARNESS_PYTHON'
ATTR = 'ExeHandle'                 # the newest name: an older module lacks it


def _load() -> ModuleType | None:
    if any(os.environ.get(k, '') not in ('', '0') for k in (ENV, _coredbx.PURE_PYTHON_ENV)):
        return None
    m = _coredbx._import_native()
    return m if m is not None and hasattr(m, ATTR) else None


_native = _load()


def native() -> ModuleType | None:
    """bc2_native with the harness functions, or None (Python only)."""
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
