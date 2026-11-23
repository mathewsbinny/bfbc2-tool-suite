"""The specification's census: everything in DICE's game files, each item mapped to the rules that cover it
(read-only; spec/CENSUS.md, round SPG4).

    python spec/probes/census.py [--opfor <folder>] --measure   re-measure the game copy into spec/census.json
    python spec/probes/census.py --write                         CENSUS.md from census.json and the spec's rules
    python spec/probes/census.py --check                         (no game) every gated item has a rule or a
                                                                 reasoned "out of scope"; CENSUS.md current

The four categories (DICE's files only: dated before 2013, as every probe of spec/probes/ takes them; a `.opfor-orig`
backup is DICE's file of that name):

  files    every file outside the bundles in the game folder, grouped into kinds (FILE_KINDS);
  res      every bundle entry type of DICE's bundles (`**/*.fbrb`), with its live entries (RES-TYPE-1);
  dbx      every DBX instance class (TYPE-ATTR-3) of DICE's partitions, each partition name once (the first bundle
           in path order), with its instances;
  engine   every record of `BFBC2Game.exe`'s `typeinfo` section (ENG-TI-1): the engine's reflected classes, value
           types and enumerations, by module.

An item is covered by the rules whose heading, body or evidence names it as a whole word (`Class`, `Class.Field`,
`<Class>` ...; a file kind by FILE_TERMS), catalogue rules that only list names excepted (CATALOGUES), or is
OUT_OF_SCOPE with the reason.
Gated (the --check fails on "none"): every file kind, every resource type, every DBX class with at least
DBX_THRESHOLD instances, and every engine module. The census holds names and counts only (no content).
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import re
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
SPEC = SUITE / 'spec'
CENSUS_JSON = SPEC / 'census.json'
CENSUS_MD = SPEC / 'CENSUS.md'
DBX_THRESHOLD = 10
BUILD_STAMP = 0x4E55445D                     # BFBC2Game.exe build 795745

# (kind, pattern over the path relative to game/, lower case) in order; the first match names a file's kind
FILE_KINDS: list[tuple[str, str]] = [
    ('bundle (.fbrb)', r'(dist|package)/.*\.fbrb'),
    ('bundle registry: package.mft', r'dist/win32/package\.mft'),
    ('bundle registry: bundleManifest', r'dist/win32/bundlemanifest'),
    ('NavKit path data (Output/win32/ai/levels/*/*/pathdata.hpd)', r'output/win32/ai/levels/[^/]+/[^/]+/pathdata\.hpd'),
    ('movie (Output/win32/movies/**.res)', r'output/win32/movies/.*\.res'),
    ('aerial height map (Output/win32/levels/*/aerialheightmap.res)', r'output/win32/levels/[^/]+/aerialheightmap\.res'),
    ('the executable (BFBC2Game.exe)', r'bfbc2game\.exe'),
    ('database.dbmanifest', r'database\.dbmanifest'),
    ('dfe', r'dfe'),
    ('InstallationData.xml', r'installationdata\.xml'),
    ('UpdateHistory.log', r'updatehistory\.log'),
    ('dialoglogo128x128.jpg', r'dialoglogo128x128\.jpg'),
    ('binkw32.dll (RAD Bink)', r'binkw32\.dll'),
    ('DirectX redistributables (D3DCompiler_42, D3DX*_42)', r'(d3dcompiler_42|d3dx9_42|d3dx10_42|d3dx11_42)\.dll'),
    ('winui.dll', r'winui\.dll'),
    ('Support folder', r'support/.*'),
]

# '<category>:<item>' -> why the spec leaves it out (checked: the item must be in the census)
OUT_OF_SCOPE: dict[str, str] = {
    'files:DirectX redistributables (D3DCompiler_42, D3DX*_42)':
        "Microsoft's DirectX runtime (the shader compiler and D3DX helpers of the 2010 SDK), loaded by the "
        "executable as any Windows program loads them; the suite never reads or changes them",
    'files:Support folder':
        "EA's customer-service help pages, the EULA, the uninstaller, the CD-key tool and the Windows Game "
        "Explorer resource (GDFBinary.dll, GL.ini): installer and support content no game system reads",
    'files:winui.dll':
        "an MFC 8 dialog library (resource-only UI helpers: rich edit, HTML help); no file of the game folder "
        "names it (not imported by BFBC2Game.exe or binkw32.dll), so it belongs to the installer / support tools",
}


def norm_rel(game: Path, p: Path) -> str:
    r = p.relative_to(game).as_posix()
    return r[:-len('.opfor-orig')] if r.endswith('.opfor-orig') else r


def dice(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def file_kinds(game: Path) -> dict[str, dict[str, int]]:
    """Category files: DICE's files outside the bundles, by kind (count, bytes); 'unclassified: <ext>' otherwise."""
    out: dict[str, dict[str, int]] = {}
    for dp, dns, fns in os.walk(game):
        dns.sort()
        for f in sorted(fns):
            p = Path(dp) / f
            if not dice(p):
                continue
            r = norm_rel(game, p).lower()
            k = next((k for k, pat in FILE_KINDS if re.fullmatch(pat, r)), None)
            if k is None:
                k = 'unclassified: ' + (os.path.splitext(r)[1] or r)
            d = out.setdefault(k, {'files': 0, 'bytes': 0})
            d['files'] += 1
            d['bytes'] += p.stat().st_size
    return dict(sorted(out.items()))


def bundles_and_classes(game: Path) -> tuple[dict[str, dict[str, int]], dict[str, dict[str, Any]]]:
    """Categories res and dbx: entry types (live entries, distinct names) and instance classes (instances,
    partitions, member names) over DICE's bundles."""
    from bfbc2core import dbx, fbrb  # noqa: PLC0415
    bundles = [p for p in sorted(game.glob('**/*.fbrb'), key=lambda p: str(p).lower()) if dice(p)]
    res: dict[str, int] = collections.Counter()
    names: dict[str, set[str]] = collections.defaultdict(set)
    seen: set[str] = set()
    classes: dict[str, dict[str, Any]] = {}
    fields: dict[str, set[str]] = collections.defaultdict(set)
    for p in bundles:
        es = [e for e in fbrb.entries(p) if not e.deleted]
        for e in es:
            res[e.type] += 1
            names[e.type].add(e.name.lower())
        new = [e.name for e in es if e.name.lower().endswith('.dbx') and e.name.lower() not in seen]
        seen.update(n.lower() for n in new)
        for i in range(0, len(new), 400):
            chunk = new[i:i + 400]
            got = fbrb.read(p, chunk)
            for f in dbx.parse_many([got[n] for n in chunk]):
                if isinstance(f, ValueError):
                    continue
                here: set[str] = set()
                for inst in f.instances:
                    c = (inst.attr('type') or '?').split('.')[-1]
                    d = classes.setdefault(c, {'instances': 0, 'partitions': 0})
                    d['instances'] += 1
                    here.add(c)
                    fields[c].update(ch.attr('name') or '' for ch in inst.children or ())
                for c in here:
                    classes[c]['partitions'] += 1
    for c, d in classes.items():
        d['fields'] = sorted(fields[c] - {''})
    out_res = {t: {'entries': n, 'names': len(names[t])} for t, n in sorted(res.items())}
    return out_res, dict(sorted(classes.items()))


# ENG-TI-1: a typeinfo record (24 bytes, 28 for value types and enumerations) begins with the name's pointer, a word
# (size << 16 | flags) and the module record's pointer (the module record begins with a pointer to the module's name).
# ENG-TI-2: the kind is (flags >> 4) & 0x1F; the kinds of one record each are the primitives.
TI_KINDS = {0: 'void', 2: 'value type', 3: 'class', 4: 'array', 0x17: 'enumeration'}
IDENT = re.compile(r'[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*')


def engine_types(exe: Path) -> dict[str, dict[str, Any]]:
    """Category engine: the typeinfo section's records {name: {module, kind, size, va}}."""
    if str(SUITE) not in sys.path:
        sys.path.insert(0, str(SUITE))
    import suite_paths  # noqa: PLC0415
    suite_paths.use('core', 'menu-tools')
    import menucommon  # noqa: PLC0415
    img = menucommon.PEImage(os.fspath(exe))
    if img.timestamp != BUILD_STAMP:
        return {}
    _, _sva, _vs, ra, rs, _ = next(s for s in img.sections if s[0] == 'typeinfo')
    b = img.b

    def ident(va: int) -> str | None:
        if not 0x400000 <= va < 0x2000000 or img.off(va) is None:
            return None
        s = img.cstr(va, 120)
        return s if s and IDENT.fullmatch(s) else None

    out: dict[str, dict[str, Any]] = {}
    for o in range(ra, ra + rs - 12, 4):
        p, sf, m = struct.unpack_from('<III', b, o)
        n = ident(p)
        if not n:
            continue
        mo = img.off(m) if 0x400000 <= m < 0x2000000 else None
        mod = ident(struct.unpack_from('<I', b, mo)[0]) if mo is not None else None
        if not mod:
            continue
        kind = TI_KINDS.get((sf >> 4) & 0x1F, 'primitive')
        va = img.va(o)
        out.setdefault(n, {'module': mod, 'kind': kind, 'size': sf >> 16, 'va': f'0x{va or 0:X}'})
    return dict(sorted(out.items()))


def measure(game: Path) -> dict[str, Any]:
    res, classes = bundles_and_classes(game)
    return {'files': file_kinds(game), 'res': res, 'dbx': classes, 'engine': engine_types(game / 'BFBC2Game.exe')}


# ------------------------------------------------------------------------------------------------ the mapping

RULE_HEAD = re.compile(r'^#{3,4}\s+([A-Z][A-Z0-9]*-[A-Z][A-Z0-9]*-\d+):\s+(.+?)\s*$')
HEAD = re.compile(r'^#{1,6}\s')
# catalogue rules that name every item of a category without specifying it: they cover nothing here
CATALOGUES = {'RES-TYPE-1', 'RES-TYPE-2', 'RES-TYPE-3', 'GFILE-KIND-1', 'ENG-TI-3'}
# the words that name a file kind in rule text (default: the kind itself)
FILE_TERMS = {
    'bundle (.fbrb)': ['FBRB-HDR-1'], 'bundle registry: package.mft': ['package.mft'],
    'bundle registry: bundleManifest': ['bundleManifest'],
    'NavKit path data (Output/win32/ai/levels/*/*/pathdata.hpd)': ['pathdata.hpd'],
    'movie (Output/win32/movies/**.res)': ['Output/win32/movies'],
    'aerial height map (Output/win32/levels/*/aerialheightmap.res)': ['AerialHeightmap.res'],
    'the executable (BFBC2Game.exe)': ['typeinfo'], 'binkw32.dll (RAD Bink)': ['binkw32.dll'], 'dfe': ['`dfe`'],
}


def rule_texts(spec: Path = SPEC) -> dict[str, str]:
    """Every rule's heading, body and evidence as one text, by id."""
    out: dict[str, str] = {}
    for dp, dns, fns in os.walk(spec):
        dns[:] = sorted(d for d in dns if d not in ('probes', '__pycache__'))
        for f in sorted(fns):
            if not f.endswith('.md') or f in ('CENSUS.md', 'COVERAGE.md', 'README.md', 'glossary.md'):
                continue
            cur: str | None = None
            in_fence = False
            with open(os.path.join(dp, f), encoding='utf-8') as fh:
                for line in fh:
                    if line.lstrip().startswith('```'):
                        in_fence = not in_fence
                    m = None if in_fence else RULE_HEAD.match(line)
                    if m:
                        cur = m.group(1)
                        out[cur] = line
                    elif not in_fence and HEAD.match(line):
                        cur = None
                    elif cur is not None:
                        out[cur] += line
    return out


class Index:
    """Which rules name a word (one pass over the rule texts' tokens) or a phrase."""

    def __init__(self, texts: dict[str, str]) -> None:
        self.texts = {r: t for r, t in texts.items() if r not in CATALOGUES}
        self.words: dict[str, set[str]] = collections.defaultdict(set)
        tok = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')
        for rid, t in self.texts.items():
            for w in set(tok.findall(t)):
                self.words[w].add(rid)

    def word(self, w: str) -> list[str]:
        return sorted(self.words.get(w, ()))

    def phrase(self, s: str) -> list[str]:
        low = s.lower()
        if re.fullmatch(r'[A-Z]+-[A-Z]+-\d+', s):
            return [s] if s in self.texts else []
        return sorted(r for r, t in self.texts.items() if low in t.lower())


def mapping(census: dict[str, Any], idx: Index) -> dict[str, dict[str, Any]]:
    """Every item -> {'rules': [...]} | {'out': reason} | {} (none), per category; modules from their records."""
    out: dict[str, dict[str, Any]] = {'files': {}, 'res': {}, 'dbx': {}, 'engine': {}, 'modules': {}}

    def one(cat: str, item: str, found: list[str]) -> dict[str, Any]:
        key = f'{cat}:{item}'
        if key in OUT_OF_SCOPE:
            v: dict[str, Any] = {'out': OUT_OF_SCOPE[key]}
        elif found:
            v = {'rules': sorted(set(found))}
        else:
            v = {}
        out[cat][item] = v
        return v

    for k in census['files']:
        one('files', k, sorted({r for t in FILE_TERMS.get(k, [k]) for r in idx.phrase(t)}))
    for t in census['res']:
        one('res', t, idx.phrase('`' + t + '`') if not t[0].isalpha() else idx.word(t))
    for c in census['dbx']:
        one('dbx', c, idx.word(c))
    mods: dict[str, list[str]] = collections.defaultdict(list)
    for n, d in census['engine'].items():
        one('engine', n, idx.word(n.split('::')[-1]))
        mods[d['module']].append(n)
    for m, ns in sorted(mods.items()):
        one('modules', m, sorted({r for n in ns for r in out['engine'][n].get('rules', [])}))
    return out


def gated(cat: str, item: str, census: dict[str, Any]) -> bool:
    """FILE / RES / MODULE always; a DBX class with at least DBX_THRESHOLD instances; engine records never."""
    if cat in ('files', 'res', 'modules'):
        return True
    return cat == 'dbx' and census['dbx'][item]['instances'] >= DBX_THRESHOLD


def state(v: dict[str, Any]) -> str:
    return 'covered' if 'rules' in v else 'out of scope' if 'out' in v else 'none'


CATS = ('files', 'res', 'dbx', 'engine', 'modules')
CAT_NAMES = {'files': 'files outside the bundles (kinds)', 'res': 'bundle entry types', 'dbx': 'DBX instance classes',
             'engine': 'typeinfo records', 'modules': 'typeinfo modules'}


def totals(census: dict[str, Any], mp: dict[str, dict[str, Any]]) -> dict[str, dict[str, int]]:
    out: dict[str, dict[str, int]] = {}
    for cat in CATS:
        c = collections.Counter(state(v) for v in mp[cat].values())
        g = collections.Counter(state(v) for k, v in mp[cat].items() if gated(cat, k, census))
        out[cat] = {'covered': c['covered'], 'out of scope': c['out of scope'], 'none': c['none'],
                    'gated none': g['none']}
    return out


# ------------------------------------------------------------------------------------------------ CENSUS.md

def rules_text(v: dict[str, Any], limit: int = 6) -> str:
    if 'out' in v:
        return 'out of scope: ' + v['out']
    rs = v.get('rules', [])
    if not rs:
        return '**none**'
    return ', '.join(rs[:limit]) + (f' (+{len(rs) - limit})' if len(rs) > limit else '')


def render(census: dict[str, Any], mp: dict[str, dict[str, Any]]) -> str:
    t = totals(census, mp)
    before = census.get('before', {})
    L = ['# The specification census', '',
         'Generated by `python spec/probes/census.py --write` from `spec/census.json` (the measurement: '
         '`--measure` on the game copy) and the',
         "rules of `spec/**/*.md`; do not edit by hand. Everything in DICE's game files (dated before 2013), each "
         'item with',
         'the rules that name it (a rule names an item when its heading, body or evidence holds the name as a whole '
         'word;',
         'catalogue rules that only list names - ' + ', '.join(sorted(CATALOGUES)) + ' - do not count), or the '
         'reason it is out of scope.',
         f'`python spec/probes/census.py --check` (the quick tier) fails when a file kind, a resource type, a DBX '
         f'class with at least {DBX_THRESHOLD}',
         'instances or a typeinfo module has neither (GFILE-KIND-1, ENG-TI-1).', '',
         '## Totals', '',
         '| Category | Items | Covered | Out of scope | None | Gated none | Before SPG4 (covered / none) |',
         '|---|---|---|---|---|---|---|']
    for cat in CATS:
        d = t[cat]
        b = before.get(cat)
        bt = f"{b['covered']} / {b['none']}" if b else '-'
        L.append(f"| {CAT_NAMES[cat]} | {len(mp[cat]):,} | {d['covered']:,} | {d['out of scope']:,} | {d['none']:,} "
                 f"| {d['gated none']:,} | {bt} |")
    L += ['', '## 1. Files outside the bundles', '',
          "DICE's files of the game folder by kind (a `.opfor-orig` backup counts as DICE's file of that name).", '',
          '| Kind | Files | Bytes | Rules |', '|---|---|---|---|']
    for k, d in census['files'].items():
        L.append(f"| {k} | {d['files']:,} | {d['bytes']:,} | {rules_text(mp['files'][k])} |")
    L += ['', '## 2. Bundle entry types', '',
          "Every entry type of DICE's bundles (`**/*.fbrb`, live entries; names: distinct entry names).", '',
          '| Type | Entries | Names | Rules |', '|---|---|---|---|']
    for k, d in census['res'].items():
        L.append(f"| `{k}` | {d['entries']:,} | {d['names']:,} | {rules_text(mp['res'][k])} |")
    L += ['', '## 3. DBX instance classes', '',
          "Every instance class of DICE's partitions (each partition name once), by instances; partitions: how many "
          "hold one.", '',
          '| Class | Instances | Partitions | Members | Rules |', '|---|---|---|---|---|']
    for k, d in sorted(census['dbx'].items(), key=lambda kv: (-kv[1]['instances'], kv[0])):
        L.append(f"| {k} | {d['instances']:,} | {d['partitions']:,} | {len(d['fields'])} | "
                 f"{rules_text(mp['dbx'][k], 4)} |")
    L += ['', '## 4. The executable\'s typeinfo, by module', '',
          "`BFBC2Game.exe`'s `typeinfo` section (ENG-TI-1): every reflected class, value type and enumeration, "
          'by module; covered: records some rule names.', '',
          '| Module | Records | Classes | Value types | Enumerations | Other | Covered | In DICE\'s DBX |',
          '|---|---|---|---|---|---|---|---|']
    mods: dict[str, list[str]] = collections.defaultdict(list)
    for n, d in census['engine'].items():
        mods[d['module']].append(n)
    for m, ns in sorted(mods.items(), key=lambda kv: -len(kv[1])):
        k = collections.Counter(census['engine'][n]['kind'] for n in ns)
        other = len(ns) - k['class'] - k['value type'] - k['enumeration']
        cov = sum(1 for n in ns if 'rules' in mp['engine'][n])
        indbx = sum(1 for n in ns if n in census['dbx'])
        L.append(f"| {m} | {len(ns)} | {k['class']} | {k['value type']} | {k['enumeration']} | {other} | {cov} | "
                 f'{indbx} |')
    L += ['', '## 5. Not covered yet', '',
          'DBX classes below the gate and typeinfo records no rule names, by module (the next rounds\' list).', '']
    low = sorted((k for k, v in mp['dbx'].items() if state(v) == 'none'),
                 key=lambda k: (-census['dbx'][k]['instances'], k))
    L.append(f'**DBX classes** ({len(low)}): ' + (', '.join(f"{k} ({census['dbx'][k]['instances']})" for k in low)
                                                   or 'none') + '.')
    L.append('')
    for m, ns in sorted(mods.items()):
        miss = sorted(n for n in ns if state(mp['engine'][n]) == 'none')
        if miss:
            L.append(f'**{m}** ({len(miss)} of {len(ns)}): ' + ', '.join(miss) + '.')
            L.append('')
    return '\n'.join(L).rstrip() + '\n'


# ------------------------------------------------------------------------------------------------ the checks

def dump(census: dict[str, Any]) -> None:
    """spec/census.json: sorted keys, one line per item (the file stays small and diffs by item)."""
    lines = ['{']
    cats = sorted(census)
    for i, cat in enumerate(cats):
        items = census[cat]
        lines.append(f' {json.dumps(cat)}: {{')
        keys = sorted(items)
        for j, k in enumerate(keys):
            comma = ',' if j < len(keys) - 1 else ''
            lines.append(f'  {json.dumps(k)}: {json.dumps(items[k], sort_keys=True, separators=(",", ":"))}{comma}')
        lines.append(' }' + (',' if i < len(cats) - 1 else ''))
    lines.append('}')
    CENSUS_JSON.write_text('\n'.join(lines) + '\n', encoding='utf-8')


def load() -> dict[str, Any]:
    with open(CENSUS_JSON, encoding='utf-8') as f:
        return dict(json.load(f))


def problems(census: dict[str, Any], mp: dict[str, dict[str, Any]]) -> list[str]:
    out = [f'{cat}: {item} has no rule and no "out of scope" entry'
           for cat in CATS for item, v in mp[cat].items() if state(v) == 'none' and gated(cat, item, census)]
    measured = {f'{cat}:{k}' for cat in ('files', 'res', 'dbx', 'engine', 'modules') for k in mp[cat]}
    out += [f'OUT_OF_SCOPE names {k}, which the census does not hold' for k in OUT_OF_SCOPE if k not in measured]
    return out


def check() -> int:
    census = load()
    mp = mapping(census, Index(rule_texts()))
    bad = problems(census, mp)
    for p in bad:
        print('FAIL  ' + p)
    t = totals(census, mp)
    print('census: ' + '; '.join(f"{c} {d['covered']} covered / {d['out of scope']} out / {d['none']} none"
                                 for c, d in t.items()))
    if CENSUS_MD.is_file() and CENSUS_MD.read_text(encoding='utf-8') != render(census, mp):
        print('note: spec/CENSUS.md is not current (python spec/probes/census.py --write)')
    print('passed' if not bad else f'FAILED {len(bad)}')
    return 1 if bad else 0


def checks(game: Path) -> list[tuple[bool, str]]:
    """The game tier: the committed census is what the game copy holds."""
    got, want = measure(game), load()
    out = []
    for cat in ('files', 'res', 'dbx', 'engine'):
        g, w = got[cat], want[cat]
        diff = sorted(k for k in set(g) | set(w) if g.get(k) != w.get(k))
        out.append((not diff, f'census: {cat} as spec/census.json holds it ({len(w)} items; differ: {diff[:8]})'))
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    ap.add_argument('--measure', action='store_true', help='re-measure the game copy into spec/census.json')
    ap.add_argument('--before', help='with --write: record the totals against this spec folder as "before"')
    ap.add_argument('--write', action='store_true', help='write spec/CENSUS.md')
    ap.add_argument('--check', action='store_true', help='(no game) the gate: every gated item covered')
    a = ap.parse_args()
    if a.measure:
        sys.path.insert(0, str(SUITE / 'core'))
        from bfbc2core import paths  # noqa: PLC0415
        game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
        t = time.time()
        census = measure(game)
        if CENSUS_JSON.is_file():
            census['before'] = load().get('before', {})
        dump(census)
        print(f'{CENSUS_JSON} ({time.time() - t:.1f} s)')
    if a.write:
        census = load()
        if a.before:
            census['before'] = totals(census, mapping(census, Index(rule_texts(Path(a.before)))))
            dump(census)
        CENSUS_MD.write_text(render(census, mapping(census, Index(rule_texts()))), encoding='utf-8')
        print(CENSUS_MD)
    if a.check or not (a.measure or a.write):
        return check()
    return 0


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    sys.exit(main())
