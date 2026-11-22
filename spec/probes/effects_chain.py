"""The emitters' processor chains as DICE's files hold them (read-only; spec/engine/effects.md section 4).

    python spec/probes/effects_chain.py [--opfor <folder>]

Reads the partitions named `.../fx/...` of the DICE bundles of Project OPFOR BFBC2's game copy
(`game/**/levels/*/*.fbrb` dated before 2013), each partition name once (the first bundle that holds it), and for
every `Emitter.EmitterDocument`:

- the template (`EmitterTemplateData`): `EmittableType`, the counts and lifetimes;
- the two roots (the document's `RootProcessor`, the template's `RootProcessor`) and the chain each starts by
  `NextProcessor`: length, whether one runs into the other, the processor classes in chain order, `ProcessorType`
  against the class name, `Enable` / `Deprecated` flags, `EvaluatorInput`;
- the evaluators a processor's `Pre` / `Post` names (classes, nesting).

Prints structure, counts and value ranges only.
"""
from __future__ import annotations

import argparse
import collections
import re
import sys
import time
from pathlib import Path

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import dbx, fbrb, paths

Counter = collections.Counter


def stock_bundles(game: Path) -> list[Path]:
    return sorted((p for p in game.glob('**/levels/*/*.fbrb') if time.gmtime(p.stat().st_mtime).tm_year < 2013),
                  key=lambda p: str(p).lower())


def ref(e: dbx.Element | None) -> str:
    return 'null' if e is None else (e.attr('ref') or 'null')


def text(e: dbx.Element | None) -> str:
    return '' if e is None else dbx.value(e)


def kind(e: dbx.Element) -> str:
    return (e.attr('type') or '').split('.')[-1]


def chain(start: str, by: dict[str, dbx.Element]) -> list[dbx.Element]:
    out: list[dbx.Element] = []
    seen: set[str] = set()
    g = start.lower()
    while g != 'null' and g in by and g not in seen:
        seen.add(g)
        e = by[g]
        out.append(e)
        g = ref(e.child('NextProcessor')).lower()
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    bundles = stock_bundles(game)
    seen: set[str] = set()
    docs = 0
    emittable: Counter[str] = Counter()
    doc_len: Counter[int] = Counter()
    tmpl_len: Counter[int] = Counter()
    relation: Counter[str] = Counter()
    first_class: Counter[str] = Counter()
    tmpl_first: Counter[str] = Counter()
    classes: Counter[str] = Counter()
    type_matches: Counter[bool] = Counter()
    disabled: Counter[str] = Counter()
    deprecated = 0
    inputs: Counter[str] = Counter()
    evaluators: Counter[str] = Counter()
    eval_nested = 0
    order_spawn_first: Counter[bool] = Counter()
    unchained: Counter[str] = Counter()
    pos_after: Counter[str] = Counter()
    for path in bundles:
        names = [e.name for e in fbrb.entries(path) if not e.deleted and e.name.lower().endswith('.dbx')
                 and re.search(r'(^|/)fx/', e.name.lower()) and e.name.lower() not in seen]
        if not names:
            continue
        got = fbrb.read(path, names)
        for n in names:
            seen.add(n.lower())
            f = dbx.parse(got[n])
            insts = f.instances
            doc = next((i for i in insts if kind(i) == 'EmitterDocument'), None)
            if doc is None:
                continue
            docs += 1
            by = {(i.attr('guid') or '').lower(): i for i in insts}
            tmpl = by.get(ref(doc.child('TemplateData')).lower())
            if tmpl is not None:
                emittable[text(tmpl.child('EmittableType'))] += 1
            dc = chain(ref(doc.child('RootProcessor')), by)
            tc = chain(ref(tmpl.child('RootProcessor')), by) if tmpl is not None else []
            doc_len[len(dc)] += 1
            tmpl_len[len(tc)] += 1
            dset = {id(x) for x in dc}
            tset = {id(x) for x in tc}
            relation['same chain' if dset == tset else 'template chain inside document chain' if tset < dset
                     else 'document chain inside template chain' if dset < tset
                     else 'disjoint' if not dset & tset else 'overlapping'] += 1
            if dc:
                first_class[kind(dc[0])] += 1
            if tc:
                tmpl_first[kind(tc[0])] += 1
            both = dc + [x for x in tc if id(x) not in dset]
            procs = [i for i in insts if i.child('ProcessorType') is not None]
            for p in procs:
                if id(p) not in {id(x) for x in both}:
                    unchained[kind(p)] += 1
            for p in both:
                k = kind(p)
                classes[k] += 1
                pt = text(p.child('ProcessorType'))
                type_matches['Pt' + k[:-4] == pt] += 1
                if text(p.child('Enable')) == 'false':
                    disabled[k] += 1
                deprecated += text(p.child('Deprecated')) == 'true'
                inputs[text(p.child('EvaluatorInput'))] += 1
                for slot in ('Pre', 'Post'):
                    ev = by.get(ref(p.child(slot)).lower())
                    if ev is not None:
                        evaluators[f'{slot} {kind(ev)}'] += 1
                        eval_nested += any(c.attr('ref') not in (None, 'null') for c in ev.children or ())
            for c in (tc, dc):
                ks = [kind(x) for x in c]
                spawn = [j for j, k in enumerate(ks) if k.startswith('Spawn')]
                upd = [j for j, k in enumerate(ks) if k.startswith('Update')]
                if spawn and upd:
                    order_spawn_first[max(spawn) < min(upd)] += 1
            ks = [kind(x) for x in tc]
            for j in range(1, len(ks)):
                pos_after[f'{ks[j - 1]} -> {ks[j]}'] += 0
    print(f'{docs} emitter documents (fx partitions of DICE bundles, each name once)')
    print(f'EmittableType: {dict(emittable)}')
    print(f'document chain lengths: {dict(sorted(doc_len.items()))}')
    print(f'template chain lengths: {dict(sorted(tmpl_len.items()))}')
    print(f'the two chains: {dict(relation)}')
    print(f'document chain first class: {dict(first_class.most_common(8))}')
    print(f'template chain first class: {dict(tmpl_first.most_common(8))}')
    print(f'processor classes in the chains: {len(classes)} - {dict(classes.most_common())}')
    print(f'ProcessorType = "Pt" + class name without "Data": {dict(type_matches)}')
    print(f'processors with Enable false: {sum(disabled.values())} ({dict(disabled.most_common(8))}); Deprecated true: '
          f'{deprecated}')
    print(f'EvaluatorInput: {dict(inputs.most_common())}')
    print(f'evaluators named by Pre / Post: {dict(evaluators.most_common(16))}; with references of their own: '
          f'{eval_nested}')
    print(f'chains with spawn and update processors: every spawn before every update {dict(order_spawn_first)}')
    print(f'processors in no chain: {dict(unchained)}')


if __name__ == '__main__':
    main()
