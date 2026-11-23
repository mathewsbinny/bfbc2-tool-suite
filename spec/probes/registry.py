"""DICE's bundle registries as the data model describes them (read-only; spec/model/registration.md REG-MFT, REG-BM,
spec/model/bundles.md BNDL-KIND, BNDL-LOC, spec/engine/streaming.md STRM-REG).

    python spec/probes/registry.py [--opfor <folder>]

Reads the stock `package.mft` and `bundleManifest` OPFOR backed up before its first change
(`game/Dist/win32/package.mft.opfor-orig`, `bundleManifest.opfor-orig`, DICE's files) and which of the bundles they
name are in the game copy's `Dist/win32` and `Package` folders. Prints counts only. `checks(game)` recomputes the stable
counts the rules cite, for core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import re
import sys
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import paths  # noqa: E402

Counter = collections.Counter
LEVEL_KINDS = ('level', 'terrain', 'ingame', 'loader')
LANGS = ('de', 'en', 'es', 'fr', 'it', 'pl', 'ru')


def kind(source: str) -> str:
    """A registered bundle's kind: level / terrain / ingame / loader, vo/lang, async, menu or root."""
    p = source.lower()
    m = re.fullmatch(r'levels/[^/]+/(level|terrain|ingame|loader)(-00)?(\.fbrb)?', p)
    if m:
        return m.group(1)
    if re.fullmatch(r'levels/[^/]+/vo/[a-z]{2}(-00)?(\.fbrb)?', p):
        return 'vo/lang'
    if p.startswith('levels/'):
        return 'level other'
    for k in ('async', 'menu'):
        if p.startswith(k + '/'):
            return k
    return 'root'


def stem(p: str) -> str:
    """A registry name without `game:`, a leading slash and `-00.fbrb`, lower case."""
    p = p.lower().split(':', 1)[-1].lstrip('/')
    return re.sub(r'(-00)?\.fbrb$', '', p)


def analyse(game: Path) -> dict[str, object]:
    win = game / 'Dist' / 'win32'
    raw = (win / 'package.mft.opfor-orig').read_bytes()
    c: Counter[str] = Counter()
    c['mft: bytes'] = len(raw)
    c['mft: BOM'] = raw.startswith(b'\xef\xbb\xbf')
    text = raw.decode('ascii')
    lines = text.split('\r\n')
    if lines and lines[-1] == '':
        lines.pop()
    c['mft: bare LF'] = text.replace('\r\n', '').count('\n')
    c['mft: lines'] = len(lines)
    c['mft: line 1 Signature + 40 hex'] = bool(re.fullmatch(r'Signature [0-9a-f]{40}', lines[0]))
    c['mft: line 2 Authorative'] = lines[1] == 'Authorative'
    c['mft: blank lines'] = sum(1 for x in lines if x == '')
    sources: list[str] = []
    targets: dict[str, list[str]] = {}
    for i, x in enumerate(lines):
        if x.startswith('BundleSource '):
            c['mft: sources'] += 1
            c['mft: source after a blank line'] += lines[i - 1] == ''
            s = x.split(' ', 1)[1]
            c['mft: source game:'] += s.startswith('game:')
            sources.append(stem(s))
            targets[stem(s)] = []
        elif x.startswith('BundleTarget '):
            c['mft: targets'] += 1
            targets[sources[-1]].append(x.split(' ', 1)[1])
        elif x and i > 1:
            c['mft: other lines'] += 1
    c['mft: duplicate sources'] = len(sources) - len(set(sources))

    def where(s: str) -> str:
        d, p = (win / f'{s}-00.fbrb').is_file(), (game / 'Package' / f'{s}-00.fbrb').is_file()
        return {(True, True): 'both', (True, False): 'Dist', (False, True): 'Package', (False, False): 'none'}[(d, p)]

    levels: dict[str, set[str]] = collections.defaultdict(set)
    for s in sources:
        k = kind(s)
        c[f'mft kind: {k}'] += 1
        if k in LEVEL_KINDS:
            levels[s.split('/')[1]].add(k)
            c[f'mft level sources in: {where(s)}'] += 1
        if k == 'vo/lang':
            levels[s.split('/')[1]].add('vo/' + s.split('/')[-1])
            c[f'mft vo sources in: {where(s)}'] += 1
    c['mft: levels'] = len(levels)
    full = set(LEVEL_KINDS) | {f'vo/{lang}' for lang in LANGS}
    c['mft: levels with the four kinds and seven vo'] = sum(1 for v in levels.values() if v == full)
    for lang in LANGS:
        s = f'streaming_vo_{lang}'
        c[f'mft: {s} registered'] += s in targets
        c[f'mft: {s} {where(s)}'] += 1
        c[f'mft: {s} targets package: then game:'] += [t.split(':')[0] for t in targets.get(s, [])] == ['package', 'game']
    c['mft: streaming_sounds targets package: then game:'] = [
        t.split(':')[0] for t in targets.get('streaming_sounds', [])] == ['package', 'game']

    # bundleManifest
    bm = [x for x in (win / 'bundleManifest.opfor-orig').read_text(encoding='ascii').splitlines() if x]
    c['bm: lines'] = len(bm)
    names = [x.split('\t')[0] for x in bm]
    c['bm: lines path, zero guid, 1'] = sum(
        1 for x in bm if re.fullmatch(r'[^\t]+\t0{8}-0{4}-0{4}-0{4}-0{12}\t1', x) is not None)
    bm_levels: set[str] = set()
    mft = set(sources)
    for n in names:
        s = stem(n)
        k = kind(s)
        c[f'bm kind: {k}'] += 1
        c[f'bm: with a Dist file {(win / f"{s}-00.fbrb").is_file()}'] += 1
        c['bm: also a package.mft source'] += s in mft
        c['bm: also a package.mft source, as spelled'] += n.lower() in mft
        c['bm: async, as spelled'] += n.lower().startswith('async/')
        if k in LEVEL_KINDS:
            bm_levels.add(s.split('/')[1])
    c['bm: levels'] = len(bm_levels)
    c['bm: levels with Dist bundles'] = sum(1 for lv in bm_levels if (win / 'levels' / lv / 'level-00.fbrb').is_file())
    c['bm: sorted'] = names == sorted(names) or names == sorted(names, key=str.lower)
    c['bm: async/weapon'] = sum(1 for n in names if stem(n).startswith('async/weapon/'))
    c['bm: async/levels cutscene'] = sum(1 for n in names if re.fullmatch(r'async/levels/[^/]+/cutscene/c_[0-9a-f]{32}', stem(n)))
    spell = {n for n in names if 'streaming' in n.lower() or 'ondemand' in n.lower() or 'startup' in n.lower()
             or n.lower() == 'vo'}
    c['bm: global spellings'] = len(spell)
    want = {f'/streaming_vo_{lang}' for lang in LANGS} | {'/streaming_sounds', '/async/ondemand_sounds',
                                                          '/async/ondemand_awards', 'async/startup', 'VO'}
    c['bm: global spellings as described'] = spell == want
    return {'counts': c}


# The stable counts of DICE's stock files (2026-11-22).
EXPECTED: dict[str, object] = {
    'mft: bytes': 136251, 'mft: BOM': False, 'mft: bare LF': 0, 'mft: lines': 3493,
    'mft: line 1 Signature + 40 hex': True, 'mft: line 2 Authorative': True, 'mft: blank lines': 900,
    'mft: sources': 899, 'mft: source after a blank line': 899, 'mft: targets': 1692, 'mft: other lines': None,
    'mft: source game:': 899, 'mft: duplicate sources': 0,
    'mft kind: level': 74, 'mft kind: terrain': 74, 'mft kind: ingame': 74, 'mft kind: loader': 74,
    'mft kind: vo/lang': 518, 'mft kind: async': 50, 'mft kind: menu': 25, 'mft kind: root': 10,
    'mft kind: level other': None,
    'mft level sources in: both': 132, 'mft level sources in: Package': 164, 'mft level sources in: Dist': None,
    'mft level sources in: none': None, 'mft vo sources in: none': 518,
    'mft: levels': 74, 'mft: levels with the four kinds and seven vo': 74,
    'mft: streaming_vo_en both': 1,
    'mft: streaming_vo_de none': 1, 'mft: streaming_vo_es none': 1, 'mft: streaming_vo_fr none': 1,
    'mft: streaming_vo_it none': 1, 'mft: streaming_vo_pl none': 1, 'mft: streaming_vo_ru none': 1,
    **{f'mft: streaming_vo_{lang} targets package: then game:': 1 for lang in LANGS},
    'mft: streaming_sounds targets package: then game:': True,
    'bm: lines': 835, 'bm: lines path, zero guid, 1': 835, 'bm: levels': 33, 'bm: levels with Dist bundles': 33,
    'bm: with a Dist file True': 598, 'bm: with a Dist file False': 237, 'bm: also a package.mft source': 407,
    'bm: also a package.mft source, as spelled': 397, 'bm: async, as spelled': 443, 'bm kind: menu': 17,
    'bm kind: async': 445, 'bm: async/weapon': 323, 'bm: async/levels cutscene': 119,
    'bm: sorted': False, 'bm: global spellings as described': True,
}


def summary(a: dict[str, object]) -> dict[str, object]:
    c = a['counts']
    assert isinstance(c, Counter)
    return dict(c)


def checks(game: Path) -> list[tuple[bool, str]]:
    got = summary(analyse(game))
    return [(got.get(k) == want, f'registry probe: {k} = {want!r} (got {got.get(k)!r})') for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    for k, v in sorted(summary(analyse(game)).items()):
        print(f'{k}: {v}')


if __name__ == '__main__':
    main()
