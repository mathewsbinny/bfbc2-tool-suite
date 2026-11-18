"""Round F2a's parity helpers (beside f0_parity.py's canon / outcome / Checks): a tally of both-ways comparisons and
the game data the F2a tests read - entries of given types from bundles (one bundle at a time) and from Battlefield
1943's source.
"""
import glob
import random
from collections.abc import Callable, Iterator
from typing import Any

from f0_parity import outcome


class Tally:
    """Counts of both-ways comparisons; fallback: answers the Rust core left to the reference."""

    def __init__(self) -> None:
        self.same = self.ndiff = self.errors = 0
        self.differ: list[str] = []

    def add(self, ok: bool, label: str, error: bool) -> None:
        self.errors += error
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)

    def text(self) -> str:
        return f'{self.same} identical ({self.errors} of them the same error)'


def both(t: Tally, label: str, fn: Callable[..., Any], ref: Callable[..., Any], *args: Any,
         canon: Callable[[Any], Any] | None = None) -> Any:
    """fn(*args) against ref(*args): the same result (by canon when given) or the same error; ref's result or None."""
    def run(f: Callable[..., Any]) -> tuple[Any, ...]:
        return outcome(lambda: canon(f(*args)) if canon else f(*args))
    a, r = run(fn), run(ref)
    t.add(a == r, f'{label}: {str(a)[:100]} / {str(r)[:100]}', r[0] == 'error')
    if r[0] != 'ok':
        return None
    try:
        return ref(*args)
    except Exception:                                            # noqa: BLE001
        return None


def damaged(b: bytes, rng: random.Random) -> bytes:
    """A copy cut short and / or with a few bytes changed."""
    x = bytearray(b)
    k = rng.randrange(3)
    if k in (1, 2) and x:
        for _ in range(rng.randint(1, 3)):
            x[rng.randrange(len(x))] = rng.choice([0, 255, rng.randrange(256)])
    if k in (0, 2) and x:
        del x[rng.randrange(len(x)):]
    return bytes(x)


def bundle_entries(pattern: str, types: set[str]) -> Iterator[tuple[str, str, bytes]]:
    """(name lower case, type, bytes) of every live entry of the types in the bundles matching pattern, the first
    bundle of a name only; one bundle read at a time."""
    from bfbc2core import fbrb
    seen: set[str] = set()
    for p in sorted(glob.glob(pattern, recursive=True)):
        try:
            entries = fbrb.entries(p)
        except Exception:                                        # noqa: BLE001 - not a bundle this reader takes
            continue
        want = [(e.name, e.type) for e in entries if e.type in types and not e.deleted and e.name.lower() not in seen]
        if not want:
            continue
        data = fbrb.read(p, [n for n, _ in want])
        for n, ty in want:
            seen.add(n.lower())
            yield n.lower(), ty, data[n]


def bf1943_entries(types: set[str], limit: int | None = None) -> Iterator[tuple[str, str, bytes]]:
    """(name, type, bytes) of Battlefield 1943's entries of the types (every limit-th for a sample); nothing when its
    source is missing."""
    try:
        import suite_paths
        suite_paths.use('bf1943-import')
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'skip Battlefield 1943 ({e})')
        return
    ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') in types)
    if limit:
        ks = ks[::max(len(ks) // limit, 1)][:limit]
    for k in ks:
        yield k, str(src.entries[k][1].get('type')), src.data(k)
