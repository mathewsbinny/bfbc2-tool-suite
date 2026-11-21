r"""The repository's own rules, checked before anything is pushed (no game data needed):

- no game file types and no large files anywhere in the history (EA / DICE content is never published);
- no co-author or generated-by lines in commit messages (the author is the only author);
- no build output or caches tracked (.exe, .pyc, __pycache__);
- no application icon tracked from design/icons/local (a personal override), nor one in design/icons without the
  README.md recording its source;
- a README in every tool folder, a LICENSE at the root;
- every Python file compiles;
- every tests/*.rs of a Rust crate with `autotests = false` is a `mod` of its test binary's main.rs (round RT3: a
  file left off that list silently never runs).

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
LARGE_ALLOWED = {'bfbc2-dbx-tools/tests/fingerprints_795745.txt'}       # SHA-1 hashes only
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


def main(argv: list[str]) -> int:
    results = []
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
        if int(size) > MAX_BLOB and path not in LARGE_ALLOWED:
            large.add(f'{path} ({int(size):,} bytes)')
    results.append(('no game file types in the history', sorted(game_files)))
    results.append((f'no files over {MAX_BLOB:,} bytes in the history (allowed: {", ".join(sorted(LARGE_ALLOWED))})', sorted(large)))

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
