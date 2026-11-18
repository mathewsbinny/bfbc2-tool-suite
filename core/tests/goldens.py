r"""Recorded reference data: what the suite's retired Python readers and writers answered, kept as digests (round F1a,
2026-11-18; rust/BENCHMARKS.md, "Retired Python"). The tests compare the Rust core's answers with them.

    h(data, n)              the first n hex digits of data's SHA-1 (a str is taken as UTF-8, lone surrogates kept)
    rh(value, n)            h(repr(value)): a digest of a plain value (tuples, lists, str, bytes, ints, floats, None)
    file_sha(path)          a file's SHA-1, read in 1 MiB pieces
    load(path) / save(path, data)
                            a recording (JSON); save writes one key per line under 'cases' / 'files' maps

A recording's 'cases' lists are in the order the test makes its cases (seeded random generators, fixed inputs); its
'files' maps are keyed by a file's SHA-1 (16 digits) or path, so a changed file is reported, not compared.
"""
import hashlib
import json
import os
from typing import Any


def h(data: bytes | str, n: int = 12) -> str:
    if isinstance(data, str):
        data = data.encode('utf-8', 'surrogatepass')
    return hashlib.sha1(data).hexdigest()[:n]


def rh(value: Any, n: int = 12) -> str:
    return h(repr(value), n)


def file_sha(path: str) -> str:
    s = hashlib.sha1()
    with open(path, 'rb') as f:
        while chunk := f.read(1 << 20):
            s.update(chunk)
    return s.hexdigest()


def load(path: str) -> dict[str, Any]:
    with open(path, encoding='utf-8') as f:
        d = json.load(f)
    assert isinstance(d, dict)
    return d


def save(path: str, data: dict[str, Any]) -> None:
    """JSON with each top-level entry on its own lines and long lists / maps one item per line (diffs stay small)."""
    def dump(v: Any, indent: str) -> str:
        if isinstance(v, dict) and v and (len(v) > 8 or any(isinstance(x, (dict, list)) for x in v.values())):
            inner = indent + ' '
            return '{\n' + ',\n'.join(f'{inner}{json.dumps(k)}: {dump(x, inner)}' for k, x in v.items()) + '\n' + indent + '}'
        if isinstance(v, list) and len(v) > 16:
            rows = [json.dumps(v[i:i + 8], ensure_ascii=False) for i in range(0, len(v), 8)]
            inner = indent + ' '
            return '[\n' + ',\n'.join(inner + r[1:-1] for r in rows) + '\n' + indent + ']'
        return json.dumps(v, ensure_ascii=False)
    tmp = path + '.tmp'
    with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
        f.write(dump(data, '') + '\n')
    os.replace(tmp, path)
