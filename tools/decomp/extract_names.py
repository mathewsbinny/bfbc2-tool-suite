r"""Collect every named BFBC2Game.exe address the suite and Project OPFOR BFBC2 know, for the decompiled database
(round DC0; tools/decomp/README.md).

    python tools/decomp/extract_names.py --out <private>\index\names.json [--opfor <folder>] [--bfopfor <folder>]
                                         [--private <folder>] [--old-symbols <jsonl>] [--exe <BFBC2Game.exe>]

Sources, in order of precedence (a declared name - qualified, backticked, a table cell, a constant - beats words
picked from a sentence; among equals an earlier source wins; every disagreement is listed in the output's
"conflicts"):

  1. spec      the specification: spec/**/*.md, each address with the rule id whose section cites it
  2. symbols   the crash reader's symbols_795745.txt (ADDRESS name | hint)
  3. research  Project OPFOR BFBC2's research\BFBC2_DECOMP_RESEARCH.md, in the private repository since round DOC1
               (--private, default BFBC2_DECOMP_DB or battlefield-decomp-work\bfbc2-decomp:
               <private>\opfor-research\research\BFBC2_DECOMP_RESEARCH.md)
  4. hooks     Project OPFOR BFBC2's src\dll\bc2.h constants and battlefield-opfor's `bc2_795745` hook tables
  5. ghidra    an older Ghidra project's user and imported names (ExportSymbols.java's JSON lines)

A name is taken from the text around an address: a `Class::method` or backticked identifier next to it, a table row's
first cell, a bold lead-in, or the words just before it. Words alone that say nothing ("the", "calls", "at") give no
name; the address still gets a comment citing its rule. Addresses outside the executable's image are dropped (event
hashes, offsets). The output holds facts only (addresses, names, rule ids and the sentence that cites them), and is
written outside every repo all the same, next to the private database.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
SPEC = SUITE / 'spec'
SYMBOLS = SUITE / 'bfbc2-automated-map-and-bot-test-harness' / 'crash-reader' / 'symbols_795745.txt'


def sibling(name: str) -> Path:
    """A folder beside the suite's checkout, or beside one of its parents (a round's worktree in a round folder under _rounds)."""
    for up in SUITE.parents:
        if (up / name).is_dir() and (up / name) != SUITE:
            return up / name
    return SUITE.parent / name


DEFAULT_OPFOR = Path(os.environ['OPFOR_ROOT']) if os.environ.get('OPFOR_ROOT') else sibling('project-opfor-bfbc2')
DEFAULT_BFOPFOR = sibling('battlefield-opfor')
# the private decompilation repository (bfbc2core.paths.decomp_root's order); OPFOR's research is in opfor-research
DEFAULT_PRIVATE = (Path(os.environ['BFBC2_DECOMP_DB']) if os.environ.get('BFBC2_DECOMP_DB')
                   else sibling(os.path.join('battlefield-decomp-work', 'bfbc2-decomp')))
PRECEDENCE = ('spec', 'symbols', 'research', 'hooks', 'ghidra')

# BFBC2Game.exe build 795745's image (image base 0x400000; .text to .rsrc), used when the exe is not given.
DEFAULT_RANGES = [(0x401000, 0x1D12C00)]
HEX = re.compile(r'(?<![0-9A-Za-z_])0x(?:00)?([0-9A-Fa-f]{6,8}|[0-9A-Fa-f]{2}_[0-9A-Fa-f]{4})(?![0-9A-Za-z_])')
RULE_HEAD = re.compile(r'^#{3,4}\s+([A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*-\d+):')
HEAD = re.compile(r'^#{1,6}\s')
QUALIFIED = re.compile(r'`?(~?[A-Za-z_][A-Za-z0-9_<>]*(?:::~?[A-Za-z_][A-Za-z0-9_<>]*)+)`?')
TICKED = re.compile(r'`([A-Za-z_][A-Za-z0-9_]*)`')
STOP = set(re.findall(r'\S+', '''a an the its it this that these those of to in on at by for from with and or as is are was be been
calls call called via then than which who when where while into onto per each every one two three four first second
last next same other its own their there here also only not no but if so up down out over under after before about
see also e.g eg i.e ie cf jumps jump returns return reaches reached runs run sets set reads read writes write gets get
takes take puts put uses used use does do done has have had makes make keeps keep test tests tested slot slots case
cases function functions code address addresses offset exe bfbc2game exits branch branches byte bytes
dword word value values field fields range caller callers callee callees site sites
instruction instructions near inside within between through hook hooked patch patched'''))


@dataclass
class Cand:
    source: str
    name: str | None
    where: str                 # file:line
    context: str               # the cited sentence (trimmed)
    rule: str | None = None
    quality: int = 0           # 3 qualified/backticked, 2 table/bold/declared, 1 words before, 0 none


@dataclass
class Entry:
    addr: int
    cands: list[Cand] = field(default_factory=list)


def image_ranges(exe: Path | None) -> list[tuple[int, int]]:
    if exe is None or not exe.is_file():
        return DEFAULT_RANGES
    with open(exe, 'rb') as f:
        d = f.read(4096)
    pe = struct.unpack_from('<I', d, 0x3C)[0]
    n = struct.unpack_from('<H', d, pe + 6)[0]
    opt = struct.unpack_from('<H', d, pe + 20)[0]
    base = struct.unpack_from('<I', d, pe + 24 + 28)[0]
    out = []
    for i in range(n):
        vs, va = struct.unpack_from('<II', d, pe + 24 + opt + 40 * i + 8)
        out.append((base + va, base + va + vs))
    return out


def in_image(a: int, ranges: list[tuple[int, int]]) -> bool:
    return any(s <= a < e for s, e in ranges)


def ident(text: str) -> str | None:
    """A Ghidra-safe name from free text: words joined by '_', '::' kept; None when nothing meaningful is left."""
    text = text.strip().strip('`*"\'').strip()
    if not text:
        return None
    if '::' in text and re.fullmatch(r'~?[A-Za-z_][A-Za-z0-9_<>]*(::~?[A-Za-z_][A-Za-z0-9_<>]*)+', text):
        return text
    text = re.sub(r'0x[0-9A-Fa-f_]+|\bx[0-9A-Fa-f]{2,}\b|\+\S*', ' ', text)
    words = [w for w in re.findall(r'[A-Za-z][A-Za-z0-9]*', text) if not re.fullmatch(r'x?[0-9A-Fa-f]*[0-9][0-9A-Fa-f]*', w)]
    while words and words[0].lower() in STOP:
        words.pop(0)
    while words and words[-1].lower() in STOP:
        words.pop()
    if not words or all(w.lower() in STOP for w in words) or len(words) > 6:
        return None
    name = '_'.join(words)
    return name if len(name) >= 3 else None


def name_near(line: str, m: re.Match[str]) -> tuple[str | None, int]:
    """The best name for the address matched at m in line, and its quality."""
    before, after = line[:m.start()], line[m.end():]
    # `Class::method` right after ("0x8527D0 Unlocks::applyLicenses") or right before ("`Stats::get` `0x7D6BF0`").
    ma = re.match(r'[`\s)]*' + QUALIFIED.pattern, after)
    if ma:
        return ma.group(1), 3
    mb = re.search(QUALIFIED.pattern + r'[`\s(]*$', before)
    if mb:
        return mb.group(1), 3
    mt = re.search(TICKED.pattern + r'[\s(]*`?$', before)
    if mt and ident(mt.group(1)):
        return ident(mt.group(1)), 3
    # A table row whose first cell names the thing: only a row citing this one address, and a cell naming one thing.
    if line.lstrip().startswith('|') and line.count('|') >= 3:
        cells = [c.strip() for c in line.strip().strip('|').split('|')]
        if len(cells) > 1 and cells[1].lstrip('`').startswith(m.group(0)) and len(HEX.findall(line)) == 1 \
                and ',' not in cells[0] and cells[0].count('`') <= 2:
            n = plain(ident(re.sub(r'\(.*?\)', '', cells[0])))
            if n:
                return n, 2
    # A bold lead-in right before the paragraph's first address.
    mbold = re.search(r'\*\*([^*]{2,60})\*\*([^*]{0,6})$', before)
    if mbold and not HEX.search(mbold.group(2)):
        n = plain(ident(re.sub(r'\(.*?\)', '', mbold.group(1))))
        if n:
            return n, 2
    # The words just before, back to the last delimiter.
    seg = re.split(r'[,;:(\[|.=>]|\s-\s|->', before)[-1]
    seg = re.sub(r'`[^`]*`', ' ', seg)
    seg = seg.replace('`', ' ')
    n = plain(ident(' '.join(seg.split()[-5:])))
    return (n, 1) if n else (None, 0)


def plain(n: str | None) -> str | None:
    """None for one plain lower-case word ("constructor", "body", "stores"): it names nothing."""
    if n and '_' not in n and not re.search(r'[a-z][A-Z]|^[A-Z][a-z]+[A-Z]|^[A-Z]{2,}', n):
        return None
    return n


def is_range_end(line: str, m: re.Match[str]) -> bool:
    return bool(re.search(r'(0x[0-9A-Fa-f_]+`?\s*(-|–|to)\s*`?)$', line[:m.start()]))


def add(entries: dict[int, Entry], a: int, c: Cand) -> None:
    entries.setdefault(a, Entry(a)).cands.append(c)


def scan_markdown(entries: dict[int, Entry], path: Path, rel: str, source: str, ranges: list[tuple[int, int]],
                  with_rules: bool) -> int:
    n = 0
    rule: str | None = None
    for i, line in enumerate(path.read_text(encoding='utf-8', errors='replace').splitlines(), 1):
        mr = RULE_HEAD.match(line)
        if mr:
            rule = mr.group(1)
        elif HEAD.match(line):
            rule = None
        for m in HEX.finditer(line):
            a = int(m.group(1).replace('_', ''), 16)
            if not in_image(a, ranges):
                continue
            nm, q = (None, 0) if is_range_end(line, m) else name_near(line, m)
            lo = max(0, m.start() - 110)
            ctx = line[lo:m.end() + 110].strip()
            add(entries, a, Cand(source, nm, f'{rel}:{i}', ctx, rule if with_rules else None, q))
            n += 1
    return n


def scan_symbols(entries: dict[int, Entry], path: Path, ranges: list[tuple[int, int]]) -> int:
    n = 0
    for i, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
        m = re.match(r'([0-9A-Fa-f]{8})\s+([^|]+?)\s*(?:\|\s*(.*))?$', line)
        if not m or line.startswith('#'):
            continue
        a = int(m.group(1), 16)
        if not in_image(a, ranges):
            continue
        raw = m.group(2)
        nm = raw if re.fullmatch(QUALIFIED.pattern.strip('`?'), raw) else ident(raw.replace(':', ' '))
        add(entries, a, Cand('symbols', nm, f'symbols_795745.txt:{i}', (raw + (' | ' + m.group(3) if m.group(3) else '')), None, 2))
        n += 1
    return n


CONST_CPP = re.compile(r'constexpr\s+uintptr_t\s+(k[A-Za-z0-9_]+)\s*=\s*0x([0-9A-Fa-f]{6,8})\s*;\s*(?://\s*(.*))?')
CONST_RS = re.compile(r'pub\s+const\s+([A-Z0-9_]+)\s*:\s*usize\s*=\s*0x([0-9A-Fa-f_]{6,11})\s*;')


def scan_hooks(entries: dict[int, Entry], path: Path, rel: str, ranges: list[tuple[int, int]]) -> int:
    n = 0
    lines = path.read_text(encoding='utf-8', errors='replace').splitlines()
    for i, line in enumerate(lines, 1):
        for rx in (CONST_CPP, CONST_RS):
            m = rx.search(line)
            if not m:
                continue
            a = int(m.group(2).replace('_', ''), 16)
            if not in_image(a, ranges):
                continue
            raw = m.group(1)
            nm = raw[1:] if raw.startswith('k') and raw[1:2].isupper() else raw.title().replace('_', '')
            # The comment lines just above a constant describe it.
            above: list[str] = []
            j = i - 2
            while j >= 0 and lines[j].strip().startswith('//') and len(above) < 3:
                above.insert(0, lines[j].strip().lstrip('/').strip())
                j -= 1
            ctx = ' '.join(above + ([m.group(3)] if rx is CONST_CPP and m.group(3) else []))[:300]
            add(entries, a, Cand('hooks', nm, f'{rel}:{i}', ctx or raw, None, 2))
            n += 1
    return n


def scan_old_ghidra(entries: dict[int, Entry], path: Path, ranges: list[tuple[int, int]]) -> int:
    n = 0
    for i, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
        d = json.loads(line)
        nm = str(d['name'])
        if re.search(r'RTTI|vftable|meta_ptr|^Ordinal_|^\?|^Rsrc_', nm):
            continue                       # the new analysis makes these itself (RTTI, exports)
        a = int(d['addr'])
        if in_image(a, ranges):
            add(entries, a, Cand('ghidra', nm, f'{path.name}:{i}', str(d.get('comment') or nm)[:300], None, 2))
            n += 1
    return n


def norm(n: str) -> str:
    return re.sub(r'[^a-z0-9]', '', n.lower())


def rank(c: Cand) -> tuple[int, int, int]:
    """A declared name (quality 2 - 3) from any source beats words picked from a sentence (quality 1); among
    equals the spec wins, then the PRECEDENCE order."""
    return (1 if c.quality >= 2 else 0, -PRECEDENCE.index(c.source), c.quality)


def resolve(e: Entry) -> dict[str, object]:
    named = [c for c in e.cands if c.name]
    best: Cand | None = max(named, key=rank) if named else None
    rules = sorted({c.rule for c in e.cands if c.rule})
    others = sorted({c.name for c in named if c.name and best and c.source != best.source
                     and c.quality >= 2 and norm(c.name) != norm(best.name or '')})
    return {
        'addr': e.addr,
        'name': best.name if best else None,
        'name_source': best.source if best else None,
        'name_quality': best.quality if best else 0,
        'rules': rules,
        'sources': sorted({c.source for c in e.cands}),
        'cites': [{'source': c.source, 'where': c.where, 'rule': c.rule, 'name': c.name, 'context': c.context}
                  for c in e.cands[:12]],
        'conflict': others,
    }


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--out', required=True)
    ap.add_argument('--opfor', default=str(DEFAULT_OPFOR))
    ap.add_argument('--bfopfor', default=str(DEFAULT_BFOPFOR))
    ap.add_argument('--private', default=str(DEFAULT_PRIVATE))
    ap.add_argument('--old-symbols', default='')
    ap.add_argument('--exe', default='')
    a = ap.parse_args(argv)
    opfor, bfopfor = Path(a.opfor), Path(a.bfopfor)
    exe = Path(a.exe) if a.exe else opfor / 'game' / 'BFBC2Game.exe'
    ranges = image_ranges(exe)
    entries: dict[int, Entry] = {}
    counts: dict[str, int] = {}
    counts['spec'] = sum(scan_markdown(entries, p, p.relative_to(SUITE).as_posix(), 'spec', ranges, True)
                         for p in sorted(SPEC.rglob('*.md')))
    counts['symbols'] = scan_symbols(entries, SYMBOLS, ranges) if SYMBOLS.is_file() else 0
    research = Path(a.private) / 'opfor-research' / 'research' / 'BFBC2_DECOMP_RESEARCH.md'
    counts['research'] = scan_markdown(entries, research, 'private:opfor-research/research/BFBC2_DECOMP_RESEARCH.md',
                                       'research',
                                       ranges, False) if research.is_file() else 0
    hooks = 0
    bc2h = opfor / 'src' / 'dll' / 'bc2.h'
    if bc2h.is_file():
        hooks += scan_hooks(entries, bc2h, 'opfor:src/dll/bc2.h', ranges)
    for p in sorted((bfopfor / 'crates').rglob('*.rs')) if (bfopfor / 'crates').is_dir() else []:
        txt = p.read_text(encoding='utf-8', errors='replace')
        if 'bc2_795745' in txt:
            hooks += scan_hooks(entries, p, 'battlefield-opfor:' + p.relative_to(bfopfor).as_posix(), ranges)
    counts['hooks'] = hooks
    counts['ghidra'] = scan_old_ghidra(entries, Path(a.old_symbols), ranges) if a.old_symbols else 0
    rows = [resolve(entries[k]) for k in sorted(entries)]
    conflicts = [{'addr': r['addr'], 'chosen': r['name'], 'source': r['name_source'], 'others': r['conflict']}
                 for r in rows if r['conflict']]
    out = {'about': 'Named BFBC2Game.exe (PC build 795745) addresses from the spec and Project OPFOR BFBC2; '
                    'tools/decomp/extract_names.py. Facts only; kept beside the private database.',
           'citations': counts, 'addresses': len(rows), 'named': sum(1 for r in rows if r['name']),
           'conflicts': conflicts, 'entries': rows}
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    with open(a.out, 'w', encoding='utf-8') as f:
        json.dump(out, f, indent=1, sort_keys=True)
    print(f'{len(rows)} addresses ({out["named"]} named) from {counts}; {len(conflicts)} name conflicts -> {a.out}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
