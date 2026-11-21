r"""The specification's traceability index: checks spec/**/*.md against spec/rules.json and the evidence they cite.

  python spec\trace.py --check            structure: every rule ID in the markdown is in rules.json and the other
                                          way round (title, file, status and evidence the same), each rule's status
                                          backed by its evidence, every cited test and code path present
  python spec\trace.py --lint [FILE ...]  the markdown alone (no rules.json): rule headings, status, evidence and
                                          cited paths, for the files given or every file
  python spec\trace.py --emit             writes rules.json from the markdown (after editing rules)
  python spec\trace.py --coverage         rules per status and per area, rules without a test, tests that cite no rule
  python spec\trace.py --json             the --coverage numbers as JSON

A rule is a heading `### <AREA>-<SECTION>-<n>: <title>` (level 3 or 4) in a spec markdown file. Its body runs to
the next heading and holds one status line, `Status: **verified**` (or implemented, observed, inferred; optionally
followed by ` · Variants: PC, Xbox 360, BF1943`), and its evidence anywhere in the body as bracketed items:

  [test: path::name]        a test that checks the rule: a Python `def` / `class` or a Rust `fn` in that file
  [code: path:line]         where a reader or writer implements it (path:line or path:first-last)
  [data: text]              what was observed on real files, with counts
  [exe: text]               a BFBC2Game.exe address or function
  [inferred: text]          the reasoning
  [doc: path:line]          a document that records a finding (supporting only; counts toward no status)

Paths are relative to the suite's root; `opfor:` before a path names Project OPFOR BFBC2's checkout (its sibling
folder), checked only when that checkout is there. A status needs its evidence: verified a test, implemented a code
citation, observed a data or exe item, inferred an inferred item.

A test cites a rule when rules.json names it as evidence, or when a comment `# spec: ID[, ID ...]` (Python) or
`// spec: ID[, ID ...]` (Rust) sits on its definition line or the lines directly above it; --check also checks that
every such comment names an existing rule.

Every rule ID the prose names must be defined. IDs of the areas in EXTERNAL belong to another index instead (L1: the
map editor's placement study, bfbc2-map-editor/placement/l1_rules.json, IDs such as L1-CQ-001w) and must be in it.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import Counter
from dataclasses import asdict, dataclass, field

SPEC = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(SPEC)
OPFOR = os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2')
RULES_JSON = os.path.join(SPEC, 'rules.json')

STATUSES = ('verified', 'implemented', 'observed', 'inferred')
KINDS = ('test', 'code', 'data', 'exe', 'inferred', 'doc')
NEEDS = {'verified': ('test',), 'implemented': ('code',), 'observed': ('data', 'exe'), 'inferred': ('inferred',)}

ID = r'[A-Z][A-Z0-9]*-[A-Z][A-Z0-9]*-\d+'
HEADING = re.compile(r'^(#{1,6})\s+(.*?)\s*$')
RULE_HEADING = re.compile(rf'^#{{3,4}}\s+({ID}):\s+(.+?)\s*$')
# anything that looks meant as a rule: a valid ID at a wrong level, or an ID-like token of the wrong shape
# (`CSUM-1:`, `FBRB-HDR:`) before a colon
LOOSE_ID = re.compile(rf'^#{{1,6}}\s+(?:({ID})\b|[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+\s*:)')
STATUS = re.compile(r'^Status:\s+\*\*(\w+)\*\*(?:\s+·\s+Variants:\s+(.+?))?\s*$')
EVIDENCE = re.compile(r'\[(' + '|'.join(KINDS) + r'):\s*([^\]]+?)\s*\]')
CODE_REF = re.compile(r'^(.+?):(\d+)(?:-(\d+))?$')
SPEC_COMMENT = re.compile(rf'(?:#|//)\s*spec:\s*({ID}(?:\s*,\s*{ID})*)')
PY_DEF = re.compile(r'^\s*(?:async\s+)?(?:def|class)\s+(\w+)')
RS_FN = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)')
TEST_DIRS_SKIP = {'target', '__pycache__', '.git', 'node_modules'}


@dataclass
class Rule:
    id: str
    title: str
    file: str
    line: int
    status: str
    variants: str = ''
    evidence: list[dict[str, str]] = field(default_factory=list)

    def index(self) -> dict[str, object]:
        d = asdict(self)
        del d['line']
        if not d['variants']:
            del d['variants']
        return d


def md_files() -> list[str]:
    out: list[str] = []
    for dp, dns, fns in os.walk(SPEC):
        dns[:] = sorted(d for d in dns if d not in TEST_DIRS_SKIP)
        out.extend(os.path.join(dp, f) for f in sorted(fns) if f.endswith('.md'))
    return out


def rel(p: str, base: str = SUITE) -> str:
    return os.path.relpath(p, base).replace(os.sep, '/')


def parse(path: str, problems: list[str]) -> list[Rule]:
    """The rules of one markdown file; problems gets what is malformed."""
    rules: list[Rule] = []
    cur: Rule | None = None
    where = rel(path)
    in_fence = False
    with open(path, encoding='utf-8') as f:
        lines = f.read().splitlines()
    for n, line in enumerate(lines, 1):
        if line.lstrip().startswith('```'):
            in_fence = not in_fence
            continue
        if in_fence:
            if cur is not None:
                for kind, text in EVIDENCE.findall(line):
                    cur.evidence.append({'kind': kind, 'ref': text})
            continue
        if HEADING.match(line):
            m = RULE_HEADING.match(line)
            if m:
                cur = Rule(m.group(1), m.group(2), where, n, '')
                rules.append(cur)
                continue
            if LOOSE_ID.match(line):
                problems.append(f'{where}:{n}: a rule heading must be level 3 or 4 and read "<ID>: <title>"')
            cur = None
            continue
        if cur is None:
            continue
        s = STATUS.match(line)
        if s:
            if cur.status:
                problems.append(f'{where}:{n}: {cur.id} has a second status line')
            cur.status = s.group(1)
            cur.variants = s.group(2) or ''
            continue
        for kind, text in EVIDENCE.findall(line):
            cur.evidence.append({'kind': kind, 'ref': text})
    for r in rules:
        if not r.status:
            problems.append(f'{where}:{r.line}: {r.id} has no "Status: **...**" line')
        elif r.status not in STATUSES:
            problems.append(f'{where}:{r.line}: {r.id}: status "{r.status}" is not one of {", ".join(STATUSES)}')
        elif not any(e['kind'] in NEEDS[r.status] for e in r.evidence):
            problems.append(f'{where}:{r.line}: {r.id} is {r.status} but cites no '
                            + ' or '.join(NEEDS[r.status]) + ' evidence')
        if not r.evidence:
            problems.append(f'{where}:{r.line}: {r.id} cites no evidence')
    return rules


def resolve(path: str) -> tuple[str, bool]:
    """(absolute path, whether its absence is a problem): `opfor:` paths only matter with OPFOR's checkout there."""
    if path.startswith('opfor:'):
        return os.path.join(OPFOR, path[len('opfor:'):]), os.path.isdir(OPFOR)
    return os.path.join(SUITE, path), True


_lines_cache: dict[str, list[str]] = {}


def file_lines(p: str) -> list[str]:
    if p not in _lines_cache:
        with open(p, encoding='utf-8', errors='replace') as f:
            _lines_cache[p] = f.read().splitlines()
    return _lines_cache[p]


def defined_names(p: str) -> set[str]:
    pat = RS_FN if p.endswith('.rs') else PY_DEF
    out = set()
    for line in file_lines(p):
        m = pat.match(line)
        if m:
            out.add(m.group(1))
    return out


def check_evidence(r: Rule, problems: list[str]) -> None:
    for e in r.evidence:
        kind, ref = e['kind'], e['ref']
        if kind == 'test':
            path, _, name = ref.partition('::')
            full, matters = resolve(path)
            if not os.path.isfile(full):
                if matters:
                    problems.append(f'{r.file}:{r.line}: {r.id}: test file {path} does not exist')
                continue
            if name:
                last = name.split('::')[-1]
                if last not in defined_names(full):
                    problems.append(f'{r.file}:{r.line}: {r.id}: {path} defines no {last}')
        elif kind in ('code', 'doc'):
            m = CODE_REF.match(ref)
            path = m.group(1) if m else ref
            full, matters = resolve(path)
            if not os.path.isfile(full):
                if matters:
                    problems.append(f'{r.file}:{r.line}: {r.id}: {kind} file {path} does not exist')
                continue
            if kind == 'code' and not m:
                problems.append(f'{r.file}:{r.line}: {r.id}: code citation "{ref}" has no line (path:line)')
            if m:
                end = int(m.group(3) or m.group(2))
                if end > len(file_lines(full)) or int(m.group(2)) < 1:
                    problems.append(f'{r.file}:{r.line}: {r.id}: {path} has no line {end}')


def collect(problems: list[str], files: list[str] | None = None) -> list[Rule]:
    rules: list[Rule] = []
    for p in files or md_files():
        rules.extend(parse(p, problems))
    seen: dict[str, Rule] = {}
    for r in rules:
        if r.id in seen:
            problems.append(f'{r.file}:{r.line}: {r.id} is already defined at {seen[r.id].file}:{seen[r.id].line}')
        seen[r.id] = r
        check_evidence(r, problems)
    if files is None:
        check_mentions(seen, problems)
    return rules


MENTION = re.compile(rf'\b({ID}[a-z]?)\b')

# Rule IDs of other indexes the specification may name: area -> the JSON list of {"id": ...} that defines them.
# L1: the map editor's placement study (DICE's measured layouts), whose IDs may end in a letter (L1-CQ-001w).
EXTERNAL = {'L1': 'bfbc2-map-editor/placement/l1_rules.json'}
_external_cache: dict[str, set[str]] = {}


def external_ids(area: str) -> set[str]:
    if area not in _external_cache:
        with open(os.path.join(SUITE, EXTERNAL[area]), encoding='utf-8') as f:
            _external_cache[area] = {str(d['id']) for d in json.load(f)}
    return _external_cache[area]


def check_mentions(ids: dict[str, Rule], problems: list[str]) -> None:
    """Every rule ID the prose names (outside code blocks) is defined somewhere: no dangling cross-reference.
    An ID of an EXTERNAL area must be in that area's index instead."""
    for p in md_files():
        in_fence = False
        for n, line in enumerate(file_lines(p), 1):
            if line.lstrip().startswith('```'):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            for m in MENTION.finditer(line):
                name = m.group(1)
                area = name.split('-')[0]
                if area in EXTERNAL:
                    if name not in external_ids(area):
                        problems.append(f'{rel(p)}:{n}: {name} is not in {EXTERNAL[area]}')
                elif name not in ids:
                    problems.append(f'{rel(p)}:{n}: {name} is named but defined nowhere')


# ------------------------------------------------------------------------------------------------ tests

@dataclass
class Test:
    path: str
    name: str
    line: int
    cites: list[str]


def test_files() -> list[str]:
    """Every test source of the suite: Python test_*.py files in tests folders, Rust files with #[test]."""
    out = []
    for dp, dns, fns in os.walk(SUITE):
        dns[:] = sorted(d for d in dns if d not in TEST_DIRS_SKIP and not d.startswith('.'))
        r = rel(dp)
        for f in sorted(fns):
            p = os.path.join(dp, f)
            py_test = f.endswith('.py') and f.startswith('test_') and '/tests' in '/' + r
            if py_test or (f.endswith('.rs') and r.startswith('rust/')):
                out.append(p)
    return out


def tests_in(p: str) -> list[Test]:
    lines = file_lines(p)
    out = []
    is_rs = p.endswith('.rs')
    for i, line in enumerate(lines):
        if is_rs:
            if line.strip() != '#[test]':
                continue
            j = i + 1
            while j < len(lines) and not RS_FN.match(lines[j]):
                j += 1
            if j == len(lines):
                continue
            m = RS_FN.match(lines[j])
            name_line = j
        else:
            m = re.match(r'^(?:async\s+)?def\s+(test_\w+)', line)
            name_line = i
        if not m:
            continue
        cites: list[str] = []
        k = name_line
        while k >= 0:
            c = SPEC_COMMENT.search(lines[k])
            if c:
                cites.extend(x.strip() for x in c.group(1).split(','))
            if k < name_line and not lines[k].strip().startswith(('#', '//')):
                break
            k -= 1
        out.append(Test(rel(p), m.group(1), name_line + 1, cites))
    return out


def coverage(rules: list[Rule], problems: list[str] | None = None) -> dict[str, object]:
    ids = {r.id for r in rules}
    cited: set[tuple[str, str]] = set()
    for r in rules:
        for e in r.evidence:
            if e['kind'] == 'test':
                path, _, name = e['ref'].partition('::')
                cited.add((path, name.split('::')[-1]))
    tests = [t for p in test_files() for t in tests_in(p)]
    if problems is not None:
        for t in tests:
            for c in t.cites:
                if c not in ids:
                    problems.append(f'{t.path}:{t.line}: {t.name}: "spec: {c}" names no rule')
    uncited = [t for t in tests if (t.path, t.name) not in cited and not t.cites]
    by_status = Counter(r.status for r in rules)
    by_area = Counter(r.id.split('-')[0] for r in rules)
    return {
        'rules': len(rules),
        'by_status': {s: by_status.get(s, 0) for s in STATUSES},
        'by_area': dict(sorted(by_area.items())),
        'rules_without_test': sorted(r.id for r in rules if not any(e['kind'] == 'test' for e in r.evidence)),
        'tests': len(tests),
        'tests_citing_a_rule': len(tests) - len(uncited),
        'tests_citing_no_rule': len(uncited),
    }


# ------------------------------------------------------------------------------------------------ commands

def load_index() -> list[dict[str, object]]:
    with open(RULES_JSON, encoding='utf-8') as f:
        data = json.load(f)
    return list(data['rules'])


def check_index(rules: list[Rule], problems: list[str]) -> None:
    if not os.path.isfile(RULES_JSON):
        problems.append('spec/rules.json is missing: python spec/trace.py --emit')
        return
    index = {str(d['id']): d for d in load_index()}
    md = {r.id: r.index() for r in rules}
    for i in sorted(md.keys() - index.keys()):
        problems.append(f'{i} is in {md[i]["file"]} but not in rules.json (python spec/trace.py --emit)')
    for i in sorted(index.keys() - md.keys()):
        problems.append(f'{i} is in rules.json but in no markdown file')
    for i in sorted(md.keys() & index.keys()):
        if md[i] != index[i]:
            diff = [k for k in md[i].keys() | index[i].keys() if md[i].get(k) != index[i].get(k)]
            problems.append(f'{i}: rules.json differs from the markdown in {", ".join(sorted(diff))} '
                            '(python spec/trace.py --emit)')


def emit(rules: list[Rule]) -> None:
    data = {'about': 'The BFBC2 tool suite specification\'s rule index, written by spec/trace.py --emit from '
                     'spec/**/*.md; checked by spec/trace.py --check.',
            'rules': [r.index() for r in sorted(rules, key=lambda r: (r.file, r.line))]}
    tmp = RULES_JSON + '.tmp'
    with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(data, f, indent=1, ensure_ascii=False)
        f.write('\n')
    os.replace(tmp, RULES_JSON)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument('--check', action='store_true')
    g.add_argument('--lint', action='store_true')
    g.add_argument('--emit', action='store_true')
    g.add_argument('--coverage', action='store_true')
    g.add_argument('--json', action='store_true')
    ap.add_argument('files', nargs='*')
    a = ap.parse_args(argv)
    problems: list[str] = []
    files = [os.path.abspath(f) for f in a.files] if a.lint and a.files else None
    rules = collect(problems, files)
    if a.emit:
        if problems:
            print('\n'.join(problems))
            print(f'FAIL {len(problems)} problem(s): rules.json not written')
            return 1
        emit(rules)
        print(f'ok   rules.json: {len(rules)} rules')
        return 0
    if a.check:
        check_index(rules, problems)
        coverage(rules, problems)
    if a.lint or a.check:
        if problems:
            print('\n'.join(problems))
            print(f'FAIL {len(problems)} problem(s) in the specification')
            return 1
        c = Counter(r.status for r in rules)
        print(f'ok   {len(rules)} rules in {len({r.file for r in rules})} files: '
              + ', '.join(f'{c.get(s, 0)} {s}' for s in STATUSES))
        return 0
    cov = coverage(rules)
    if a.json:
        print(json.dumps(cov, indent=1))
        return 0
    bs = cov['by_status']
    assert isinstance(bs, dict)
    print(f'{cov["rules"]} rules: ' + ', '.join(f'{v} {k}' for k, v in bs.items()))
    ba = cov['by_area']
    assert isinstance(ba, dict)
    print('by area: ' + ', '.join(f'{k} {v}' for k, v in ba.items()))
    nt = cov['rules_without_test']
    assert isinstance(nt, list)
    print(f'rules without a test: {len(nt)}')
    print(f'tests: {cov["tests"]}, citing a rule {cov["tests_citing_a_rule"]}, citing none '
          f'{cov["tests_citing_no_rule"]}')
    if problems:
        print('\n'.join(problems))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
