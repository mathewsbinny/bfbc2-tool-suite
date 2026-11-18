"""Round F0's parity helpers, shared by the format tests that hold a Rust port against its Python reference
(test_pathdata_native.py, test_bc2loc_native.py, test_console_readers_native.py, ...).

- canon(value): a value as a nested tuple that two results are compared by: every type by name, dataclasses and
  objects with their attributes in order, dicts with their keys in order, floats by their bits (so -0.0 and 0.0 differ
  and a NaN equals only the same NaN), bytes as they are.
- outcome(fn, *args): ('ok', canon(result)) or ('error', exception type name, message): what "the same result or the
  same error" compares.
- Checks: counts passed / failed checks and prints the failures.
"""
import dataclasses
import struct
import sys
import time
from collections.abc import Callable
from typing import Any


def canon(v: Any, depth: int = 0) -> Any:
    """v as a comparable nested tuple (see the module's docstring)."""
    if depth > 200:
        raise RecursionError('canon: nested too deep')
    t = type(v).__name__
    if isinstance(v, bool) or v is None or isinstance(v, int | str):
        return (t, v)
    if isinstance(v, float):
        return (t, struct.pack('<d', v))
    if isinstance(v, bytes | bytearray):
        return (t, bytes(v))
    if isinstance(v, list | tuple):
        return (t, tuple(canon(x, depth + 1) for x in v))
    if isinstance(v, dict):
        return (t, tuple((canon(k, depth + 1), canon(x, depth + 1)) for k, x in v.items()))
    if isinstance(v, set | frozenset):
        return (t, tuple(sorted(canon(x, depth + 1) for x in v)))
    if dataclasses.is_dataclass(v) or hasattr(v, '__dict__'):
        return (t, tuple((k, canon(x, depth + 1)) for k, x in vars(v).items()))
    return (t, repr(v))


def outcome(fn: Callable[..., Any], *args: Any) -> tuple[Any, ...]:
    """('ok', canon(fn(*args))) or ('error', type name, message)."""
    try:
        return ('ok', canon(fn(*args)))
    except RecursionError:
        raise
    except Exception as ex:          # noqa: BLE001 - the error itself is what is compared
        return ('error', type(ex).__name__, str(ex))


class Checks:
    """Named checks: a failure is printed with its detail; done() prints the summary and gives the exit code."""

    def __init__(self) -> None:
        self.passed = 0
        self.failed = 0
        self.started = time.perf_counter()

    def check(self, ok: bool, what: str, detail: str = '') -> bool:
        if ok:
            self.passed += 1
            print(f'ok    {what}', flush=True)
        else:
            self.failed += 1
            print(f'FAIL  {what}' + (f': {detail}' if detail else ''), flush=True)
        return ok

    def done(self) -> int:
        secs = time.perf_counter() - self.started
        print(f'{self.passed} passed, {self.failed} failed ({secs:.1f} s)', flush=True)
        return 1 if self.failed else 0


def timed(fn: Callable[[], Any]) -> tuple[Any, float]:
    """fn()'s result and the seconds it took."""
    t = time.perf_counter()
    r = fn()
    return r, time.perf_counter() - t


def stdout_utf8() -> None:
    """Print any path on any console."""
    reconfigure = getattr(sys.stdout, 'reconfigure', None)
    if reconfigure is not None:
        reconfigure(errors='replace')
