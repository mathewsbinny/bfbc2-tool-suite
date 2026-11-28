r"""The repository's own rules, checked before anything is pushed (no game data needed):

- no game file types and no large files anywhere in the history (EA / DICE content is never published);
- no co-author or generated-by lines in commit messages (the author is the only author);
- no build output or caches tracked (.exe, .pyc, __pycache__);
- no application icon tracked from design/icons/local (a personal override), nor one in design/icons without the
  README.md recording its source;
- a README in every tool folder, a LICENSE at the root;
- every Python file compiles;
- every tests/*.rs of a Rust crate with `autotests = false` is a `mod` of its test binary's main.rs (round RT3: a
  file left off that list silently never runs);
- every tracked test_*.py is run by a tier of run_tests.py, or named in UNTIERED_ALLOWED with the reason (round TS1,
  audit AU3 TS-8);
- no runner entry named by a round code (O4:, F2a, E3o ...) beyond the 60 named so before round TS1's rule (audit AU3 TS-14),
  keyed by the entries' stable ids (round PR1a), every id well formed, unique and free of round codes;
- no Rust test file, test data or module (tests/ and src/) named by a round code beyond those grandfathered by path
  (round PR1a, audit AU5 NAM-1);
- every .py file has its row in rust/python_status.py's table (the inventory PYTHON_STATUS.md), and no row names a
  file that is gone.

  python tests\test_repo.py
"""
import os
import py_compile
import re
import subprocess
import sys
import tempfile

SUITE = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
GAME_TYPES = re.compile(r'\.(dbx|res|fbrb|hpd|sb|raw|bik|fsb|fev|dds|tga|hkx)$', re.I)
MAX_BLOB = 1_000_000
WARN_BLOB = MAX_BLOB * 8 // 10                                         # warned of, not failed (round SPLIT1)
LARGE_ALLOWED = {'bfbc2-dbx-tools/tests/fingerprints_795745.txt'}       # SHA-1 hashes only
# single historical blobs, by object id: rust/BENCHMARKS.md's version of 2026-11-23 (1,000,626 bytes, pushed before the
# file was split into BENCHMARKS.md + BENCHMARKS_ARCHIVE_1.md); the path itself stays under the limit
LARGE_ALLOWED_BLOBS = {'40dc6621808d1435705e899a98f42f046ed6548c'}
ATTRIBUTION = re.compile(r'co-authored-by|generated with \[?claude|noreply@anthropic', re.I)
TRACKED_NEVER = re.compile(r'(\.exe|\.pyc|\.dll|\.pdb)$|(^|/)__pycache__/', re.I)


def git(*args: str) -> str:
    return subprocess.run(['git', '-C', SUITE, *args], capture_output=True, text=True, encoding='utf-8',
                          errors='replace', check=True).stdout


MOD_LINE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;', re.M)
PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;')


def unregistered_test_files(rust: str) -> list[str]:
    """Round RT3: a crate with `autotests = false` (round QW1: bc2-editor, bc2-levelwrite, bc2-render) builds only
    the [[test]] targets it lists, each a tests/main.rs whose `mod <file>;` lines are its test files; a tests/*.rs left
    off that list never compiles nor runs, and nothing says so (SV1's simplified_view.rs went unrun this way). Each
    such file, as '<crate>/tests/<file>.rs: not a module of tests/main.rs'."""
    problems = []
    for crate in sorted(os.listdir(rust)):
        toml = os.path.join(rust, crate, 'Cargo.toml')
        tests = os.path.join(rust, crate, 'tests')
        if not os.path.isfile(toml) or not os.path.isdir(tests):
            continue
        with open(toml, encoding='utf-8') as f:
            manifest = f.read()
        if not re.search(r'^\s*autotests\s*=\s*false\b', manifest, re.M):
            continue
        roots = [os.path.normpath(os.path.join(rust, crate, p))
                 for p in re.findall(r'^\s*path\s*=\s*"([^"]+)"', manifest.split('[[test]]', 1)[1], re.M)] \
            if '[[test]]' in manifest else []
        covered = set(roots)
        for root in roots:
            if not os.path.isfile(root):
                problems.append(f'{crate}: [[test]] path {os.path.relpath(root, rust)} is missing')
                continue
            with open(root, encoding='utf-8') as f:
                text = f.read()
            here = os.path.dirname(root)
            covered |= {os.path.normpath(os.path.join(here, p)) for p in PATH_ATTR.findall(text)}
            covered |= {os.path.normpath(os.path.join(here, f'{name}.rs'))
                        for name in MOD_LINE.findall(PATH_ATTR.sub('', text))}
        for name in sorted(os.listdir(tests)):
            full = os.path.normpath(os.path.join(tests, name))
            if name.endswith('.rs') and os.path.isfile(full) and full not in covered:
                problems.append(f'{crate}/tests/{name}: not a module of '
                                f'{", ".join(os.path.relpath(r, os.path.join(rust, crate)) for r in roots) or "any [[test]] target"}'
                                f' (autotests = false: it never runs; add "mod {name[:-3]};")')
    return problems


def registration_self_check() -> list[str]:
    """unregistered_test_files on a stand-in crate: it must name the file left off main.rs, and only it."""
    with tempfile.TemporaryDirectory(prefix='repo_mods_') as tmp:
        crate = os.path.join(tmp, 'c')
        os.makedirs(os.path.join(crate, 'tests', 'data'))
        with open(os.path.join(crate, 'Cargo.toml'), 'w', encoding='utf-8') as f:
            f.write('[package]\nname = "c"\nautotests = false\n\n[[test]]\nname = "c"\npath = "tests/main.rs"\n')
        files = {'main.rs': '//! mod commented;\n#[path = "../examples/x.rs"]\nmod helper;\nmod a;\n#[cfg(test)]\npub mod b;\n'
                            '#[path = "c_renamed.rs"]\nmod c;\n',
                 'a.rs': '', 'b.rs': '', 'c_renamed.rs': '', 'forgotten.rs': '', 'commented.rs': ''}
        for name, text in files.items():
            with open(os.path.join(crate, 'tests', name), 'w', encoding='utf-8') as f:
                f.write(text)
        got = [p.split(':')[0] for p in unregistered_test_files(tmp)]
    want = ['c/tests/commented.rs', 'c/tests/forgotten.rs']
    return [] if got == want else [f'the check on a stand-in crate named {got}, not {want}']


# test files no tier of run_tests.py runs, on purpose (round TS1, audit AU3 TS-8): path -> why
UNTIERED_ALLOWED = {
    'tests/test_editor.py': "by hand only since round QW1: its tests are the Rust entries' (tests/test_rust.py)",
}


def untiered_test_files() -> list[str]:
    """Every tracked test_*.py that no entry of run_tests.py's tiers runs and UNTIERED_ALLOWED does not name (audit
    AU3 TS-8: three such files, two cited by the specification as a rule's evidence, ran in no tier)."""
    sys.path.insert(0, SUITE)
    import run_tests
    named = set()
    for _id, _name, _tier, args in run_tests.tests():
        argv = args(os.path.join(SUITE, 'no such folder')) if callable(args) else args
        named |= {os.path.normcase(os.path.abspath(a)) for a in argv if isinstance(a, str) and a.endswith('.py')}
    out = []
    for t in git('ls-files').splitlines():
        base = t.rsplit('/', 1)[-1]
        if (base.startswith('test_') and base.endswith('.py') and t not in UNTIERED_ALLOWED
                and os.path.normcase(os.path.abspath(os.path.join(SUITE, t))) not in named):
            out.append(f'{t}: in no tier of run_tests.py (add it, or to UNTIERED_ALLOWED with the reason)')
    return out


# Round codes in names (audit AU3 TS-14 / TS-13): a runner entry is named by what it tests, not the round that wrote
# it (O4:, F2a, E3o ...). Since round PR1a the entries are keyed by stable id (run_tests.tests(): (id, name, tier,
# argv)); the 60 entries whose display names carried a round code before round TS1's rule landed are grandfathered
# by id until PR1c rewords them; a new one fails. An id itself never carries a round code.
ROUND = re.compile(r'(?<![A-Za-z0-9_])[A-Z]{1,2}[0-9]{1,2}[a-z]?(?![A-Za-z0-9_])')
NOT_ROUND = {'BC2', 'WW2', 'L8'}            # the game, the war, a texture format
ROUND_CODED_ENTRIES = frozenset({
    'aim-pathmesh-audio-native', 'aim-pathmesh-audio-native:game', 'apt-decode-graft-native',
    'apt-decode-graft-native:game', 'apt-model-check-refs-native', 'apt-model-check-refs-native:game', 'bc2loc-native',
    'bc2loc-native:game', 'cli-caches-stockmp-graft-meshview-cull', 'cli-caches-stockmp-graft-meshview-cull:live',
    'cli-menu-crash-navkit-convert', 'cli-menu-crash-navkit-convert:live', 'cli-navkit-prepare-harness-texqa',
    'cli-navkit-prepare-harness-texqa:live', 'cli-prepare-level:game', 'cli-readers-shaders-import-stockmp',
    'cli-readers-shaders-import-stockmp:live', 'console-convert-native', 'console-convert-native:game',
    'console-readers-native', 'console-readers-native:game', 'culltree-rows-cache', 'culltree-rows-cache:game',
    'culltree-strings-disasm-native', 'culltree-strings-disasm-native:game', 'dbxcatalogue-native',
    'dbxcatalogue-native:game', 'emitteratlas-native', 'emitteratlas-native:game', 'gameledger', 'granny-write-native',
    'granny-write-native:game', 'havok-classes-native', 'havok-classes-native:game', 'havokport-native',
    'havokport-native:game', 'meshdata-native', 'meshdata-native:game', 'model-tools-native', 'model-tools-native:game',
    'navbuild-native', 'navmesh-cells-routes-native', 'navmesh-cells-routes-native:game', 'navtools-native',
    'navtools-native:game', 'navtune-native', 'pathdata-native', 'pathdata-native:game', 'pe-native', 'pe-native:game',
    'ports-native', 'ports-native:game', 'shaderdb-open-native', 'shaderdb-open-native:game', 'texture-helpers-native',
    'texture-helpers-native:game', 'wave-native', 'wave-native:game', 'xmlinst-native', 'xmlinst-native:game',
})
# a stable id: lower-case words joined by '-', an optional ':variant'; the lower-case form of the round pattern
ENTRY_ID = re.compile(r'[a-z0-9][a-z0-9-]*(:[a-z0-9][a-z0-9-]*)?')
ROUND_LOWER = re.compile(r'(?<![a-z0-9])[a-z]{1,2}[0-9]{1,2}[a-z]?(?![a-z0-9])')
NOT_ROUND_LOWER = {'bc2', 'ww2', 'l8', 'dx9', 'x86', 'd3d', 'sp01', 'x360'}      # the game, the war, formats, a level


def lower_codes(name: str) -> list[str]:
    """The round codes in a lower-case name (an id, a file or module name): e3o, gr4, f2a, v1_rules ...; a name that
    is only a format version (the project files v1.mapedit.json ... v6) is none."""
    low = name.lower()
    if re.fullmatch(r'v[0-9]+', low):
        return []
    return [c for c in ROUND_LOWER.findall(low.replace('_', '-')) if c not in NOT_ROUND_LOWER]


def round_coded_entries() -> list[str]:
    """Runner entries named with a round code that were not so named before round TS1; ids that are malformed,
    repeated or round-coded."""
    sys.path.insert(0, SUITE)
    import run_tests
    out, seen = [], set()
    for sid, name, _tier, _args in run_tests.tests():
        if not ENTRY_ID.fullmatch(sid) or sid in seen or lower_codes(sid):
            out.append(f'{sid}: an id is unique, lower case words joined by "-" with an optional ":variant", and '
                       'names what the entry tests, never a round')
        seen.add(sid)
        codes = [c for c in ROUND.findall(name) if c not in NOT_ROUND]
        if codes and sid not in ROUND_CODED_ENTRIES:
            out.append(f'{name[:80]} [{sid}]: named by round {", ".join(codes)}; name it by what it tests')
    stale = sorted(ROUND_CODED_ENTRIES - seen)
    out += [f'{s}: in ROUND_CODED_ENTRIES but no entry has that id (take it out)' for s in stale]
    return out


# Round PR1a (audit AU5 NAM-1): the same rule for the Rust side - test files (tests/*.rs), `mod` names (tests/ and
# src/) and test data (tests/data/...). Those named by round before the rule are grandfathered by path until PR1c
# renames them; the seven added after round TS1 and the round-named production modules were renamed in PR1a.
ROUND_CODED_RUST = frozenset({
    'rust/bc2-editor/tests/df1.rs', 'rust/bc2-editor/tests/e2b.rs', 'rust/bc2-editor/tests/ex1.rs',
    'rust/bc2-editor/tests/g2.rs', 'rust/bc2-editor/tests/g3.rs', 'rust/bc2-editor/tests/g4.rs',
    'rust/bc2-editor/tests/game_view_r5d.rs', 'rust/bc2-editor/tests/gr1.rs', 'rust/bc2-editor/tests/gr2.rs',
    'rust/bc2-editor/tests/vw2.rs', 'rust/bc2-editor/tests/xf1.rs', 'rust/bc2-levelwrite/tests/culltool_ct1.rs',
    'rust/bc2-levelwrite/tests/df1.rs', 'rust/bc2-levelwrite/tests/export_e3b.rs',
    'rust/bc2-levelwrite/tests/export_e3c.rs', 'rust/bc2-levelwrite/tests/export_e3d.rs',
    'rust/bc2-levelwrite/tests/export_ex1.rs', 'rust/bc2-levelwrite/tests/export_xf1.rs',
    'rust/bc2-levelwrite/tests/gameplay_xf1.rs', 'rust/bc2-levelwrite/tests/vw2.rs', 'rust/bc2-render/tests/r5d.rs',
})


def round_coded_rust() -> list[str]:
    """Tracked Rust test files, test data and modules (a `mod x;` under src/ or tests/, which names its file) named
    by a round code, other than ROUND_CODED_RUST's."""
    out = []
    tracked = git('ls-files', 'rust').splitlines()
    for t in tracked:
        parts = t.split('/')
        if len(parts) < 3 or not t.startswith('rust/bc2-') or t in ROUND_CODED_RUST:
            continue
        in_tests = parts[2] == 'tests'
        rust_src = parts[2] in ('src', 'tests') and t.endswith('.rs')
        if not (in_tests or rust_src):
            continue
        stem = parts[-1].split('.')[0]
        codes = lower_codes(stem)
        if codes:
            out.append(f'{t}: named by round {", ".join(codes)}; name it by what it holds')
    for t in tracked:
        if t.startswith('rust/bc2-') and t.endswith('.rs') and t.split('/')[2] in ('src', 'tests'):
            with open(os.path.join(SUITE, t), encoding='utf-8', errors='replace') as f:
                text = f.read()
            here = t.rsplit('/', 1)[0]
            for name in MOD_LINE.findall(PATH_ATTR.sub('', text)):
                if lower_codes(name) and f'{here}/{name}.rs' not in ROUND_CODED_RUST:
                    out.append(f'{t}: mod {name} is named by round {", ".join(lower_codes(name))}')
    stale = sorted(p for p in ROUND_CODED_RUST if p not in tracked)
    out += [f'{p}: in ROUND_CODED_RUST but not tracked (take it out)' for p in stale]
    return out


ROUND_PHRASE = re.compile(r'\b[Rr]ound [A-Z]{1,2}[0-9]{1,2}[a-z]?\b')


def round_codes_shown_to_users() -> list[str]:
    """Round PR1a (audit AU5 NAM-2): what a user reads - bfbc2.exe's usage and the crates' descriptions - names
    things by what they do, not by the round that wrote them (rounds are history: CHANGELOG.md, BENCHMARKS.md)."""
    out = []
    main_rs = os.path.join(SUITE, 'rust', 'bc2-cli', 'src', 'main.rs')
    with open(main_rs, encoding='utf-8') as f:
        text = f.read()
    usage = text[text.index('const USAGE'):text.index('";', text.index('const USAGE'))]
    out += [f'bfbc2.exe usage: "{m}"' for m in ROUND_PHRASE.findall(usage)]
    for crate in sorted(os.listdir(os.path.join(SUITE, 'rust'))):
        toml = os.path.join(SUITE, 'rust', crate, 'Cargo.toml')
        if os.path.isfile(toml):
            with open(toml, encoding='utf-8') as f:
                desc = re.search(r'^description\s*=\s*"(.*)"', f.read(), re.M)
            found = ROUND_PHRASE.search(desc.group(1)) if desc else None
            if found:
                out.append(f'{crate}/Cargo.toml description: "{found.group(0)}"')
    return out


# Determinism (round PR1a, PR0 item 2, queued post-Rust item 4): a JSON file or text the suite writes has its keys in
# a fixed order. New json.dump / json.dumps calls in the suite's (non-test) Python pass sort_keys=True (or use
# bfbc2core.atomicio.dumps_stable). The calls below were written before the rule: their key order is the order the
# code builds each dict in (insertion order, the same on every run and under every PYTHONHASHSEED: the two-seed
# check in rust/BENCHMARKS.md "PR1a"), and the Rust ports and OPFOR read or compare their bytes, so sorting them would
# change outputs; they keep their order until their Python twin retires (round PY1). A file may not gain one.
JSON_ORDER_KEPT = {
    'bfbc2-auto-navkit/navkit.py': 3, 'bfbc2-auto-navkit/tools/ladders.py': 1,
    'bfbc2-auto-navkit/tools/levelobjects.py': 5, 'bfbc2-auto-navkit/tools/navboundary.py': 1,
    'bfbc2-auto-navkit/tools/navcompare.py': 1, 'bfbc2-auto-navkit/tools/navdiff.py': 1,
    'bfbc2-auto-navkit/tools/navdomains.py': 1, 'bfbc2-auto-navkit/tools/navparity.py': 1,
    'bfbc2-auto-navkit/tools/navregress.py': 2, 'bfbc2-auto-navkit/tools/navsample_offline.py': 5,
    'bfbc2-auto-navkit/tools/navvehicle.py': 4, 'bfbc2-auto-navkit/tools/navwater.py': 1,
    'bfbc2-auto-navkit/tools/navwater_mesh.py': 3, 'bfbc2-automated-map-and-bot-test-harness/harness.py': 1,
    'bfbc2-automated-map-and-bot-test-harness/testharness/levelinfo.py': 2,
    'bfbc2-automated-map-and-bot-test-harness/testharness/runs.py': 1, 'bfbc2-bf1943-import/bf1943import.py': 1,
    'bfbc2-console-convert/texqa.py': 4, 'bfbc2-navigation-studio/overlay.py': 1,
    'bfbc2-shader-tools/renderqa.py': 1, 'bfbc2-shader-tools/xdiff.py': 1, 'bfbc2-stockmp-ai/config.py': 2,
    'bfbc2-stockmp-ai/pipeline.py': 1, 'bfbc2-stockmp-ai/validate.py': 1, 'core/bfbc2core/atomicio.py': 1,
    'core/bfbc2core/gameledger.py': 1, 'core/bfbc2core/navdata.py': 1, 'spec/trace.py': 1,
}


def unsorted_json_writes() -> list[str]:
    """json.dump / json.dumps calls without sort_keys in the suite's non-test Python beyond JSON_ORDER_KEPT's."""
    import ast
    import collections
    found: collections.Counter = collections.Counter()
    for t in git('ls-files', '*.py').splitlines():
        parts = t.split('/')
        if 'tests' in parts or parts[-1].startswith('test_'):
            continue
        try:
            with open(os.path.join(SUITE, t), encoding='utf-8-sig') as f:
                tree = ast.parse(f.read())
        except (OSError, SyntaxError):
            continue                                # the compile rule reports a file that does not parse
        for n in ast.walk(tree):
            if (isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr in ('dump', 'dumps')
                    and isinstance(n.func.value, ast.Name) and n.func.value.id == 'json'
                    and not any(k.arg == 'sort_keys' for k in n.keywords)):
                found[t] += 1
    out = [f'{t}: {n} json.dump(s) call(s) without sort_keys (kept: {JSON_ORDER_KEPT.get(t, 0)}); pass '
           'sort_keys=True' for t, n in sorted(found.items()) if n > JSON_ORDER_KEPT.get(t, 0)]
    out += [f'{t}: JSON_ORDER_KEPT says {k}, the file has {found.get(t, 0)} (lower it)'
            for t, k in sorted(JSON_ORDER_KEPT.items()) if found.get(t, 0) < k]
    return out


def python_status_problems() -> list[str]:
    """Every .py file of the suite (tracked, or new and not ignored) has its row in rust/python_status.py's TABLE, and
    no row names a file that is gone (PYTHON_STATUS.md is generated from it); the same check on stand-in lists must
    name exactly the stray file and the gone row."""
    sys.path.insert(0, os.path.join(SUITE, 'rust'))
    import python_status as ps
    row = ps.Row('a.py', ps.LIB, 'live', '', '', '', '')
    got = ps.table_problems(['a.py', 'stray.py'], (row, row._replace(path='gone.py')))
    want = ['stray.py: not in', 'gone.py: in']
    ok = len(got) == 2 and all(g.startswith(w) for g, w in zip(got, want))
    return ([] if ok else [f'the check on stand-in lists said {got}']) + ps.table_problems(ps.python_files(SUITE),
                                                                                          ps.TABLE)


def main(argv: list[str]) -> int:
    results = []
    results.append(('every .py file is classified in rust/python_status.py (PYTHON_STATUS.md), and every row names '
                    'a file that exists', python_status_problems()))
    results.append(('every test_*.py runs in a tier of run_tests.py (or is allowed out with its reason)',
                    untiered_test_files()))
    results.append(('no new runner entry named by a round code (O4:, F2a, E3o ...); every entry has a stable id',
                    round_coded_entries()))
    results.append(("no round code in bfbc2.exe's usage or the crates' descriptions", round_codes_shown_to_users()))
    results.append(('JSON written with its keys in a fixed order (sort_keys=True; older calls kept in JSON_ORDER_KEPT)',
                    unsorted_json_writes()))
    results.append(('no new Rust test file, test data or module named by a round code (gr4.rs, mod f2a ...)',
                    round_coded_rust()))
    results.append(('every tests/*.rs of a crate with autotests = false is a module of its test binary (and the '
                    'check finds one that is not)',
                    registration_self_check() + unregistered_test_files(os.path.join(SUITE, 'rust'))))

    blobs = git('rev-list', '--objects', '--all').splitlines()
    sizes = subprocess.run(['git', '-C', SUITE, 'cat-file', '--batch-check=%(objecttype) %(objectsize) %(rest)'],
                           input='\n'.join(l.split(' ')[0] for l in blobs), capture_output=True, text=True).stdout
    paths = {}
    for line in blobs:
        sha, _, path = line.partition(' ')
        if path:
            paths[sha] = path
    game_files, large = set(), set()
    for line, sha in zip(sizes.splitlines(), (l.split(' ')[0] for l in blobs)):
        kind, size, _ = (line.split(' ', 2) + [''])[:3]
        path = paths.get(sha, '')
        if kind != 'blob' or not path:
            continue
        if GAME_TYPES.search(path):
            game_files.add(path)
        if int(size) > MAX_BLOB and path not in LARGE_ALLOWED and sha not in LARGE_ALLOWED_BLOBS:
            large.add(f'{path} ({int(size):,} bytes)')
    results.append(('no game file types in the history', sorted(game_files)))
    results.append((f'no files over {MAX_BLOB:,} bytes in the history (allowed: {", ".join(sorted(LARGE_ALLOWED))})', sorted(large)))
    # round SPLIT1: a warning (not a failure) for any file of HEAD past 80% of the limit, so a growing generated file
    # (spec/rules.json reached 977,029 bytes) is split before a merge meets the limit
    for line in git('ls-tree', '-r', '-l', 'HEAD').splitlines():
        meta, _, path = line.partition('\t')
        size = meta.split()[3] if len(meta.split()) > 3 else ''
        if size.isdigit() and WARN_BLOB < int(size) <= MAX_BLOB and path not in LARGE_ALLOWED:
            print(f'warn {path} is {int(size):,} bytes, over 80% of the {MAX_BLOB:,}-byte limit: split it before it grows')

    messages = git('log', '--all', '--format=%H%x00%B%x01').split('\x01')
    bad = [m.split('\x00')[0][:10] for m in messages if '\x00' in m and ATTRIBUTION.search(m.split('\x00', 1)[1])]
    results.append(('no co-author / generated-by lines in commit messages', bad))
    authors = sorted(set(git('log', '--all', '--format=%an <%ae>').splitlines()))
    results.append((f'one author ({authors[0] if authors else "none"})', authors[1:]))

    tracked = git('ls-files').splitlines()
    results.append(('no build output or caches tracked', [t for t in tracked if TRACKED_NEVER.search(t)]))
    # the application icon (round I1): a personal override never; the committed one with its source recorded
    icons = [t for t in tracked if t.startswith('design/icons/') and t.lower().endswith(('.ico', '.png'))]
    unsourced = [t for t in icons if t.startswith('design/icons/local/') or 'design/icons/README.md' not in tracked]
    results.append(('no application icon tracked without design/icons/README.md, none from design/icons/local', unsourced))

    folders = [d for d in sorted(os.listdir(SUITE)) if os.path.isdir(os.path.join(SUITE, d)) and (d.startswith('bfbc2-') or d == 'core')]
    results.append(('a README.md in every tool folder', [d for d in folders if not os.path.isfile(os.path.join(SUITE, d, 'README.md'))]))
    results.append(('LICENSE at the root', [] if os.path.isfile(os.path.join(SUITE, 'LICENSE')) else ['LICENSE missing']))

    broken = []
    with tempfile.TemporaryDirectory() as tmp:
        for t in tracked:
            if t.endswith(('.py', '.pyw')):
                try:
                    py_compile.compile(os.path.join(SUITE, t), cfile=os.path.join(tmp, 'x.pyc'), doraise=True)
                except py_compile.PyCompileError as e:
                    broken.append(f'{t}: {str(e).strip().splitlines()[-1]}')
    results.append((f'every Python file compiles ({sum(t.endswith((".py", ".pyw")) for t in tracked)} files)', broken))

    failed = 0
    for name, problems in results:
        failed += bool(problems)
        print(f'{"FAIL" if problems else "ok  "} {name}')
        for p in problems[:15]:
            print('       ' + p)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
