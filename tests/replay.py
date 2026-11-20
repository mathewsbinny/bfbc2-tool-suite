r"""Round RT2: a retired Python reference's answers, recorded while it was there and replayed after it was deleted.

The tests of rounds F0, F2a, F2b, E4b and E4e compared each Rust port with its Python reference live
(outcome(fn, *args) == outcome(fn_reference, *args)). Before a reference is deleted its answers are recorded; after,
the same test compares the port with the recording. The test changes in two lines: Replay(...) names the references
(module or class, attribute) and finish() ends the run.

  python <test> --record            the references run (they must still be there) and every answer is recorded in
                                    <the test's folder>\<name>_replay.json (with --game: <name>_replay_game.json;
                                    files of more than 900 kB are split into _0, _1, ...)
  python <test>                     where a named reference is missing (retired), a stub replays it: called through
                                    outcome() it gives a Recorded that equals the port's outcome when that is the
                                    recorded answer

An answer is keyed by the reference's name and its arguments (content-addressed: bytes, arrays and objects by value,
a path that names a file by the file's bytes, a path under the temporary folder by its last part), so the order of
the calls does not matter and a game input that changed since the recording is counted as not recorded (compared to
nothing) instead of failing. Answers are digests of the outcome ('ok', canon(result)) or ('error', type, message),
with the arguments' paths and the temporary folder taken out of the text.

Refused inputs (RT1's marked digest). While recording, a reference called by the port itself (its fallback, not by
the test: the caller's module is not a test's) marks that input as refused by the Rust core. After the retirement the
port raises bfbc2core.native.Refused there, and the replay of a refused input equals exactly that error, whatever the
reference had said; the count of refused inputs is printed.
"""
import base64
import contextlib
import hashlib
import json
import os
import re
import struct
import sys
import tempfile
from collections.abc import Callable, Iterator
from typing import Any

RECORD = '--record' in sys.argv
GAME = '--game' in sys.argv
TEST_MODULES = ('__main__', 'replay', 'f0_parity')
TEMP_ROOTS = sorted({os.path.normcase(os.path.abspath(p)) for p in
                     (tempfile.gettempdir(), os.environ.get('BFBC2_TEST_SCRATCH') or tempfile.gettempdir())},
                    key=len, reverse=True)


def _is_test_module(name: str) -> bool:
    base = name.rsplit('.', 1)[-1]
    return name in TEST_MODULES or base.startswith('test_') or base.endswith('_parity')


def _under_temp(p: str) -> bool:
    q = os.path.normcase(p)
    return any(q.startswith(r + os.sep) for r in TEMP_ROOTS)


def _looks_like_path(s: str) -> bool:
    return len(s) < 1024 and os.path.isabs(s) and ('\\' in s or '/' in s)


def keycanon(v: Any, depth: int = 0) -> Any:
    """An argument by value: bytes / arrays hashed, a path to a file by its bytes, a temporary path by its last
    part, objects by their attributes."""
    if depth > 60:
        return ('deep',)
    t = type(v).__name__
    if isinstance(v, bool) or v is None or isinstance(v, int):
        return (t, v)
    if isinstance(v, float):
        return (t, struct.pack('<d', v).hex())
    if isinstance(v, str):
        if _looks_like_path(v):
            if os.path.isfile(v):
                with open(v, 'rb') as f:
                    return ('file', hashlib.sha1(f.read()).hexdigest())
            if os.path.isdir(v):
                return ('dir', _dir_key(v))
            if _under_temp(v):
                return ('tmp', os.path.basename(v))
        return (t, v)
    if isinstance(v, bytes | bytearray | memoryview):
        b = bytes(v)
        return (t, len(b), hashlib.sha1(b).hexdigest())
    if hasattr(v, 'dtype') and hasattr(v, 'shape') and hasattr(v, 'tobytes'):
        import numpy as np
        return ('ndarray', str(v.dtype), tuple(v.shape), hashlib.sha1(np.ascontiguousarray(v).tobytes()).hexdigest())
    if isinstance(v, list | tuple):
        return (t, tuple(keycanon(x, depth + 1) for x in v))
    if isinstance(v, dict):
        return (t, tuple((keycanon(k, depth + 1), keycanon(x, depth + 1)) for k, x in v.items()))
    if isinstance(v, set | frozenset):
        return (t, tuple(sorted(repr(keycanon(x, depth + 1)) for x in v)))
    if callable(v) and hasattr(v, '__qualname__'):
        return ('fn', v.__qualname__)
    if hasattr(v, '__dict__'):
        return (t, tuple((k, keycanon(x, depth + 1)) for k, x in vars(v).items()))
    if hasattr(v, '__slots__'):
        return (t, tuple((k, keycanon(getattr(v, k, None), depth + 1)) for k in v.__slots__))
    return (t, repr(v))


def _dir_key(d: str) -> str:
    h = hashlib.sha1()
    for top, dirs, files in os.walk(d):
        dirs.sort()
        for f in sorted(files):
            p = os.path.join(top, f)
            h.update(os.path.relpath(p, d).replace('\\', '/').encode())
            with open(p, 'rb') as fh:
                h.update(hashlib.sha1(fh.read()).digest())
    return h.hexdigest()


def _paths(args: Any, out: set[str], depth: int = 0) -> None:
    if depth > 6:
        return
    if isinstance(args, str):
        if _looks_like_path(args):
            out.add(args)
    elif isinstance(args, list | tuple):
        for a in args:
            _paths(a, out, depth + 1)


_TEMP_RE = re.compile('|'.join(
    re.escape(r).replace(r'\\', r'(?:\\\\|\\|/)') + r'(?:\\\\|\\|/)[^\\/\'"\s,)]+' for r in TEMP_ROOTS), re.I)


def normal(text: str, paths: set[str]) -> str:
    """text with the arguments' paths (as written and as a repr writes them) and the temporary folder taken out."""
    for i, p in enumerate(sorted(paths, key=len, reverse=True)):
        for form in (p, p.replace('\\', '\\\\'), p.replace('\\', '/')):
            text = text.replace(form, f'<arg{i}>')
    return _TEMP_RE.sub('<tmp>', text)


def _strings(o: Any, paths: set[str], depth: int = 0) -> Any:
    """o with every string in it normalized (before repr doubles its backslashes again)."""
    if isinstance(o, str):
        return normal(o, paths)
    if depth < 300 and isinstance(o, tuple | list):
        return type(o)(_strings(x, paths, depth + 1) for x in o)
    if depth < 300 and isinstance(o, set | frozenset):          # in an order that does not depend on hashing
        return (type(o).__name__, sorted(repr(_strings(x, paths, depth + 1)) for x in o))
    return o


HEX = 12                                         # a key's and an answer's digest: 48 bits each


def digest(o: Any, paths: set[str]) -> str:
    return hashlib.sha1(repr(_strings(o, paths)).encode()).hexdigest()[:HEX]


# The recording's files: {"format": 2, "<reference>": ["<entries>", ...]}, each entry 17 characters: the key's and the
# answer's digests (6 bytes each) in URL-safe base64, then '.' (answered), '!' (refused by the Rust core, the reference
# raised) or '+' (refused, the reference answered); strings of 100 entries. Format 1 (a dict of hex digests) is read.
ENTRY = 17


def _encode(calls: dict[str, dict[str, str]]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for fn, d in sorted(calls.items()):
        es = [base64.urlsafe_b64encode(bytes.fromhex(k[:HEX] + v[:HEX])).decode() + (v[HEX:] or '.')
              for k, v in sorted(d.items())]
        out[fn] = [''.join(es[i:i + 100]) for i in range(0, len(es), 100)]
    return out


def _decode(doc: dict[str, Any]) -> dict[str, dict[str, str]]:
    out: dict[str, dict[str, str]] = {}
    for fn, v in doc.items():
        if fn == 'format':
            continue
        d = out.setdefault(fn, {})
        if isinstance(v, dict):                                   # format 1
            for k, a in v.items():
                d[k[:HEX]] = a[:HEX] + a[16:]
            continue
        for s in v:
            for i in range(0, len(s), ENTRY):
                raw = base64.urlsafe_b64decode(s[i:i + 16]).hex()
                d[raw[:HEX]] = raw[HEX:] + ('' if s[i + 16] == '.' else s[i + 16])
    return out


def _files(stem: str) -> list[str]:
    return [p for p in [stem + '.json'] + [f'{stem}_{i}.json' for i in range(100)] if os.path.isfile(p)]


def load(stem: str) -> dict[str, dict[str, str]]:
    """A recording (all its shards): {reference: {key: answer + mark}}."""
    out: dict[str, dict[str, str]] = {}
    for p in _files(stem):
        with open(p, encoding='utf-8') as f:
            for fn, d in _decode(json.load(f)).items():
                out.setdefault(fn, {}).update(d)
    return out


def save(stem: str, calls: dict[str, dict[str, str]]) -> list[str]:
    """Writes a recording in shards of at most about 900 kB; the files written."""
    for p in _files(stem):
        os.remove(p)
    enc = _encode(calls)
    shards: list[dict[str, Any]] = [{'format': 2}]
    size = 0
    for fn, strings in enc.items():
        for s in strings:
            if size > 900_000:
                shards.append({'format': 2})
                size = 0
            shards[-1].setdefault(fn, []).append(s)
            size += len(s) + 4
    out = []
    for i, sh in enumerate(shards):
        p = stem + '.json' if len(shards) == 1 else f'{stem}_{i}.json'
        with open(p, 'w', encoding='utf-8', newline='\n') as f:
            json.dump(sh, f, indent=0, sort_keys=True)
            f.write('\n')
        out.append(p)
    return out


class Recorded:
    """A replayed reference's outcome: equal to an outcome tuple when that digests to the recorded answer; for an
    input the Rust core refuses, equal to exactly a Refused error; for an input not recorded, equal to anything
    (counted)."""
    __slots__ = ('_d', '_paths', '_refused', 'key', 'replay')

    def __init__(self, replay: 'Replay', key: str, d: str | None, refused: bool, paths: set[str]) -> None:
        self.replay, self.key, self._d, self._refused, self._paths = replay, key, d, refused, paths

    def __eq__(self, other: object) -> bool:
        if self._d is None:
            return True
        # a refused input: the Refused error, or (the port now reads it) the reference's own answer
        ok = digest(other, self._paths) == self._d or (
            self._refused and isinstance(other, tuple) and len(other) >= 2 and other[0] == 'error'
            and other[1] == 'Refused')
        if not ok:
            self.replay.wrong.append(self.key)
        return ok

    def __ne__(self, other: object) -> bool:
        return not self.__eq__(other)

    __hash__ = None  # type: ignore[assignment]

    def __getitem__(self, i: int) -> Any:          # r[0] == 'ok' style checks: what the recording says
        if i == 0:
            return 'ok' if self._d is None or not self._refused else 'error'
        raise IndexError('a replayed answer has only its kind (r[0])')

    def __repr__(self) -> str:
        what = 'not recorded' if self._d is None else 'refused by the Rust core' if self._refused else self._d
        return f'<recorded {self.key}: {what}>'


class Replay:
    """The references of one test: recorded (--record, while they are there) or replayed (once retired)."""

    def __init__(self, name: str, folder: str, refs: list[tuple[Any, str]],
                 canon: Callable[[Any], Any] | None = None) -> None:
        if canon is None:
            from f0_parity import canon as canon0
            canon = canon0
        self.name, self.canon = name, canon
        self.base = os.path.join(folder, f'{name}_replay{"_game" if GAME else ""}.json')
        self.calls: dict[str, dict[str, str]] = {}
        self.wrong: list[str] = []
        self.replayed = self.refused = self.refused_ok = self.missing = self.recorded = self.live_pairs = 0
        self.python_side = False
        self.folder = folder
        if not RECORD:
            for part in (False, True):
                for fn, d in load(os.path.join(folder, f'{name}_replay{"_game" if part else ""}')).items():
                    self.calls.setdefault(fn, {}).update(d)
        self.retired: list[str] = []
        for owner, attr in refs:
            fn_name = f'{getattr(owner, "__name__", type(owner).__name__)}.{attr}'
            real = owner.__dict__.get(attr) if isinstance(owner, type) else getattr(owner, attr, None)
            if RECORD:
                if real is None:
                    raise SystemExit(f'--record needs the reference {fn_name} (it was retired)')
                setattr(owner, attr, self._recorder(fn_name, real, isinstance(owner, type)))
            elif real is None:
                self.retired.append(fn_name)
                setattr(owner, attr, self._stub(fn_name, isinstance(owner, type)))

    @staticmethod
    def live(owner: Any, attr: str) -> bool:
        """The reference is there and runs (not a replay)."""
        fn = owner.__dict__.get(attr) if isinstance(owner, type) else getattr(owner, attr, None)
        fn = getattr(fn, '__func__', fn)
        return fn is not None and not getattr(fn, '__replay__', False)

    def key(self, args: tuple[Any, ...]) -> str:
        return hashlib.sha1(repr(keycanon(args)).encode()).hexdigest()[:HEX]

    def _recorder(self, fn_name: str, real: Any, method: bool) -> Any:
        replay = self
        fn = real.__func__ if isinstance(real, staticmethod | classmethod) else real

        def record(*args: Any) -> Any:
            caller = sys._getframe(1).f_globals.get('__name__', '')
            k = replay.key(args)
            paths: set[str] = set()
            _paths(args, paths)
            try:
                result = fn(*args)
                o: tuple[Any, ...] = ('ok', replay.canon(result))
            except RecursionError:
                raise
            except Exception as ex:          # noqa: BLE001 - recorded, then raised again
                result, o = ex, ('error', type(ex).__name__, str(ex))
            d = digest(o, paths)
            calls = replay.calls.setdefault(fn_name, {})
            internal = not _is_test_module(caller) and not replay.python_side
            mark = ('+' if o[0] == 'ok' else '!') if internal else ''
            old = calls.get(k)
            if old is None or (mark and old[-1] not in '!+'):
                calls[k] = d + (mark or (old[-1] if old and old[-1] in '!+' else ''))
            replay.recorded += 1
            if isinstance(result, Exception) and o[0] == 'error':
                raise result
            return result
        record.__replay_real__ = fn  # type: ignore[attr-defined]
        if isinstance(real, staticmethod):
            return staticmethod(record)
        return record

    @contextlib.contextmanager
    def python(self) -> Iterator[None]:
        """While recording: the test runs the Python side on purpose (use_native(False)); a reference called by the
        port then is not a refused input."""
        old, self.python_side = self.python_side, True
        try:
            yield
        finally:
            self.python_side = old

    def pair(self, name: str, args: tuple[Any, ...], got: Any, ref: Callable[[], Any] | None,
             refused: bool = False) -> bool:
        """A comparison of the port's answer `got` with the Python side's, ref() (any value canon-able alike: an
        outcome, a tuple of states). While the Python side is there (ref not None) its answer is recorded (--record)
        and compared live; once it is retired (ref None) `got` is compared with the recording under name and the
        arguments' key. refused: the Rust core refuses this input (marked; `got` may then be a Refused error)."""
        k = self.key(args)
        paths: set[str] = set()
        _paths(args, paths)
        calls = self.calls.setdefault(name, {})
        if ref is not None:
            r = ref()
            if RECORD:
                d = digest(r, paths)
                mark = ('+' if not (isinstance(r, tuple) and r[:1] == ('error',)) else '!') if refused else ''
                old = calls.get(k)
                if old is None or (mark and old[-1] not in '!+'):
                    calls[k] = d + (mark or (old[-1] if old and old[-1] in '!+' else ''))
                self.recorded += 1
            self.live_pairs += 1
            return bool(got == r)
        if name not in self.retired:
            self.retired.append(name)
        want = calls.get(k)
        if want is None:
            self.missing += 1
            return True
        self.replayed += 1
        self.refused += want[-1] in '!+'
        self.refused_ok += want[-1] == '+'
        return Recorded(self, f'{name}/{k}', want.rstrip('!+'), want[-1] in '!+', paths) == got

    def _stub(self, fn_name: str, method: bool) -> Any:
        replay = self

        def stub(*args: Any) -> Recorded:
            k = replay.key(args)
            paths: set[str] = set()
            _paths(args, paths)
            d = replay.calls.get(fn_name, {}).get(k)
            if d is None:
                replay.missing += 1
                return Recorded(replay, f'{fn_name}/{k}', None, False, paths)
            refused = d[-1] in '!+'
            replay.replayed += 1
            replay.refused += refused
            replay.refused_ok += d[-1] == '+'
            return Recorded(replay, f'{fn_name}/{k}', d.rstrip('!+'), refused, paths)
        stub.__replay__ = True  # type: ignore[attr-defined]
        return stub

    def text(self) -> str:
        if RECORD:
            n = sum(len(v) for v in self.calls.values())
            nr = sum(v[-1] in '!+' for d in self.calls.values() for v in d.values())
            nok = sum(v[-1] == '+' for d in self.calls.values() for v in d.values())
            per = ', '.join(f'{fn} {sum(v[-1] == "+" for v in d.values())}' for fn, d in sorted(self.calls.items())
                            if any(v[-1] == '+' for v in d.values()))
            return (f'replay {self.name}: {self.recorded:,} answers recorded ({n:,} distinct; {nr:,} inputs refused '
                    f'by the Rust core, {nok:,} of them answered by the reference' + (f': {per}' if per else '') + ')')
        if not self.retired:
            return f'replay {self.name}: the references are all there (compared live)'
        return (f'replay {self.name}: {self.replayed:,} answers of the retired references replayed '
                f'({self.refused:,} refused by the Rust core, as recorded: {self.refused_ok:,} of them the reference '
                f'answered), {self.missing:,} not recorded '
                f'(inputs changed since), {len(self.wrong)} wrong')

    def finish(self) -> bool:
        """Writes the recording (--record); prints the summary; False when a replayed answer was wrong."""
        if RECORD:
            calls = self.calls
            if GAME:                         # the quick answers the game run gave again stay in the quick file only
                quick = load(os.path.join(self.folder, f'{self.name}_replay'))
                calls = {fn: kept for fn, d in calls.items()
                         if (kept := {k: v for k, v in d.items() if quick.get(fn, {}).get(k) != v})}
            save(self.base[:-5], calls)
        print(self.text(), flush=True)
        if self.wrong:
            print('  wrong: ' + ', '.join(self.wrong[:8]), flush=True)
        return not self.wrong


def replayed(fn: Any) -> bool:
    """fn is a retired reference's stub (outcome() gives its Recorded instead of calling it)."""
    return bool(getattr(fn, '__replay__', False))
