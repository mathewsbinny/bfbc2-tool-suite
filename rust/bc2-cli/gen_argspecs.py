r"""Writes rust\bc2-cli\src\argspecs.rs: the argparse parsers of the command lines whose help and usage texts
bfbc2.exe formats at the console's width (round RT2, src\argfmt.rs), as the width-independent pieces argparse's
HelpFormatter builds them from.

    python rust\bc2-cli\gen_argspecs.py            write src\argspecs.rs
    python rust\bc2-cli\gen_argspecs.py --check    exit 1 when src\argspecs.rs is not what the parsers give now

Each tool's main() runs with --help until it calls parse_args / parse_known_args; the parser it built is taken there
(nothing else of the tool runs). Every piece argparse makes without the width is computed here by argparse itself
(Python 3.13's HelpFormatter): the usage's parts (_get_actions_usage_parts_with_split, the mutually exclusive groups'
brackets), each action's invocation (_format_action_invocation) and expanded help (_expand_help), and the words
textwrap breaks a text into (TextWrapper._split_chunks). What depends on the width - the usage's wrapping, the help
column, textwrap's lines - is argfmt.rs's, proved against this Python's own output at many widths
(core\tests\test_argfmt.py).

Run it again after a change to one of these parsers (test_argfmt.py fails until then).
"""
import argparse
import os
import runpy
import sys
import textwrap
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
OUT = os.path.join(HERE, 'src', 'argspecs.rs')
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import native  # noqa: E402

# the tools whose help / usage errors bfbc2.exe formats at the console's width (BENCHMARKS.md, "RT2", item 4)
TOOLS = ('caches', 'convert', 'audio', 'shaderdb', 'renderqa', 'xdiff', 'dbxparity', 'portparity', 'texqa')


class _Captured(SystemExit):                     # a SystemExit: the tools' error guards pass it on
    def __init__(self, parser: argparse.ArgumentParser) -> None:
        super().__init__()
        self.parser = parser


def capture(tool: str) -> argparse.ArgumentParser:
    """The parser `tool`'s main() builds (the tool run as bfbc2.exe hands it to Python: python <tool>.py)."""
    script = native.CLI_TOOLS[tool]
    saved = (argparse.ArgumentParser.parse_args, argparse.ArgumentParser.parse_known_args, sys.argv[:], sys.path[:])

    def grab(self: argparse.ArgumentParser, *a: object, **k: object) -> None:
        raise _Captured(self)
    setattr(argparse.ArgumentParser, 'parse_args', grab)                 # noqa: B010 - patched for the capture
    setattr(argparse.ArgumentParser, 'parse_known_args', grab)           # noqa: B010
    sys.argv = [script, '--help']
    try:
        if tool == 'caches':                                     # python -m bfbc2core.caches
            runpy.run_module('bfbc2core.caches', run_name='__main__', alter_sys=True)
        else:
            sys.path.insert(0, os.path.dirname(script))
            runpy.run_path(script, run_name='__main__')
    except _Captured as c:
        return c.parser
    finally:
        setattr(argparse.ArgumentParser, 'parse_args', saved[0])           # noqa: B010
        setattr(argparse.ArgumentParser, 'parse_known_args', saved[1])     # noqa: B010
        sys.argv, sys.path = saved[2], saved[3]
    raise SystemExit(f'{tool}: main() did not reach parse_args')


def chunks(text: str) -> list[str]:
    return textwrap.TextWrapper()._split_chunks(text)


def text_spec(fmt: argparse.HelpFormatter, text: str | None) -> tuple[str, object] | None:
    """A description / epilog: ('raw', text) or ('fill', chunks) as _format_text gives it to _fill_text."""
    if text is None or text is argparse.SUPPRESS:
        return None
    if '%(prog)' in text:
        text = text % dict(prog=fmt._prog)
    if isinstance(fmt, argparse.RawDescriptionHelpFormatter):
        return ('raw', text)
    if type(fmt)._fill_text is not argparse.HelpFormatter._fill_text:
        raise SystemExit(f'{type(fmt).__name__}: a _fill_text argfmt.rs does not know')
    return ('fill', chunks(fmt._whitespace_matcher.sub(' ', text).strip()))


def item(fmt: argparse.HelpFormatter, action: argparse.Action) -> dict[str, object]:
    """An action's line(s) in the help: its invocation, its help ('missing': help falsy; 'blank': only whitespace;
    else the expanded help's chunks) and its subactions'."""
    if type(fmt)._split_lines is not argparse.HelpFormatter._split_lines:
        raise SystemExit(f'{type(fmt).__name__}: a _split_lines argfmt.rs does not know')
    if not action.help:
        help_: object = 'missing'
    elif not action.help.strip():
        help_ = 'blank'
    else:
        expanded = fmt._expand_help(action)
        help_ = chunks(fmt._whitespace_matcher.sub(' ', expanded).strip()) if expanded else []
    subs = []
    get = getattr(action, '_get_subactions', None)
    if get is not None:
        subs = [item(fmt, s) for s in get() if s.help is not argparse.SUPPRESS]
    return {'invocation': fmt._format_action_invocation(action), 'help': help_, 'subs': subs}


def spec(key: str, p: argparse.ArgumentParser, out: list[dict[str, object]]) -> None:
    fmt = p._get_formatter()
    if p.usage is not None:
        raise SystemExit(f'{key}: a usage= string argfmt.rs does not take')
    actions = p._actions
    optionals = [a for a in actions if a.option_strings]
    positionals = [a for a in actions if not a.option_strings]
    # Python 3.13.15's split of the usage's parts (gh-75949; not in typeshed)
    split = getattr(fmt, '_get_actions_usage_parts_with_split')         # noqa: B009
    parts, pos_start = split(optionals + positionals, p._mutually_exclusive_groups, len(optionals))
    usage = ' '.join(s for s in [p.prog, fmt._format_actions_usage(optionals + positionals,
                                                                    p._mutually_exclusive_groups)] if s)
    sections = []
    for g in p._action_groups:
        sections.append({'heading': g.title, 'text': text_spec(fmt, g.description),
                         'items': [item(fmt, a) for a in g._group_actions if a.help is not argparse.SUPPRESS]})
    out.append({'key': key, 'prog': p.prog, 'usage': usage, 'opt_parts': parts[:pos_start],
                'pos_parts': parts[pos_start:], 'description': text_spec(fmt, p.description), 'sections': sections,
                'epilog': text_spec(fmt, p.epilog)})
    for a in actions:
        if isinstance(a, argparse._SubParsersAction):
            if any(not x.option_strings for x in actions[:actions.index(a)]):
                raise SystemExit(f'{key}: positionals before the subcommands (their prog depends on the width)')
            for name, sp in a.choices.items():
                spec(f'{key} {name}', sp, out)


def rs(s: str) -> str:
    """A Rust string literal."""
    out = ['"']
    for ch in s:
        if ch == '\\':
            out.append('\\\\')
        elif ch == '"':
            out.append('\\"')
        elif ch == '\n':
            out.append('\\n')
        elif ch == '\r':
            out.append('\\r')
        elif ch == '\t':
            out.append('\\t')
        elif ord(ch) < 32 or ord(ch) == 127:
            out.append(f'\\u{{{ord(ch):x}}}')
        else:
            out.append(ch)
    out.append('"')
    return ''.join(out)


def rs_list(items: list[str]) -> str:
    return '&[' + ', '.join(rs(x) for x in items) + ']'


def rs_text(t: tuple[str, object] | None) -> str:
    if t is None:
        return 'None'
    kind, v = t
    return f'Some(Text::Raw({rs(v)}))' if kind == 'raw' else f'Some(Text::Fill({rs_list(v)}))'  # type: ignore[arg-type]


def rs_item(it: dict[str, Any], indent: str) -> str:
    h = it['help']
    help_ = 'Help::Missing' if h == 'missing' else 'Help::Blank' if h == 'blank' else f'Help::Chunks({rs_list(h)})'
    subs = it['subs']
    sub = '&[]' if not subs else '&[\n' + ''.join(f'{indent}    {rs_item(s, indent + "    ")},\n' for s in subs) + f'{indent}]'
    return f'Item {{ invocation: {rs(it["invocation"])}, help: {help_}, subs: {sub} }}'


def generate() -> str:
    specs: list[dict[str, object]] = []
    for tool in TOOLS:
        spec(tool, capture(tool), specs)
    lines = ['//! Generated by `rust/bc2-cli/gen_argspecs.py` from the Python parsers (round RT2): do not edit. The pieces',
             '//! of each parser\'s help and usage that do not depend on the width; `argfmt` lays them out at the',
             '//! console\'s width as argparse\'s `HelpFormatter` does.', '',
             '#![allow(clippy::too_many_lines, reason = "generated data")]', '',
             'use crate::argfmt::{Help, Item, Section, Spec, Text};', '',
             '/// Every parser, by its key: the tool, then its subcommand (`"shaderdb info"`).',
             'pub static SPECS: &[Spec] = &[']
    for s in specs:
        lines.append('    Spec {')
        lines.append(f'        key: {rs(s["key"])},')  # type: ignore[arg-type]
        lines.append(f'        prog: {rs(s["prog"])},')  # type: ignore[arg-type]
        lines.append(f'        usage: {rs(s["usage"])},')  # type: ignore[arg-type]
        lines.append(f'        opt_parts: {rs_list(s["opt_parts"])},')  # type: ignore[arg-type]
        lines.append(f'        pos_parts: {rs_list(s["pos_parts"])},')  # type: ignore[arg-type]
        lines.append(f'        description: {rs_text(s["description"])},')  # type: ignore[arg-type]
        lines.append('        sections: &[')
        for sec in s['sections']:  # type: ignore[attr-defined]
            heading = 'None' if sec['heading'] is None else f'Some({rs(sec["heading"])})'
            lines.append(f'            Section {{ heading: {heading}, text: {rs_text(sec["text"])}, items: &[')
            for it in sec['items']:
                lines.append(f'                {rs_item(it, "                ")},')
            lines.append('            ] },')
        lines.append('        ],')
        lines.append(f'        epilog: {rs_text(s["epilog"])},')  # type: ignore[arg-type]
        lines.append('    },')
    lines.append('];')
    return '\n'.join(lines) + '\n'


def main(argv: list[str]) -> int:
    text = generate()
    if '--check' in argv:
        try:
            with open(OUT, encoding='utf-8') as f:
                old = f.read().replace('\r\n', '\n')
        except OSError:
            old = ''
        if old != text:
            print(f'{OUT} is not what the parsers give now: run python rust\\bc2-cli\\gen_argspecs.py')
            return 1
        print(f'{OUT}: as the parsers give it')
        return 0
    with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
        f.write(text)
    print(f'wrote {OUT}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
