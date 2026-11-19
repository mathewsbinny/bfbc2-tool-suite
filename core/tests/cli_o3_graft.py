r"""Round O3's graft group: the command lines test_cli_o3.py replays for aptgraft (bfbc2-menu-tools\aptgraft.py).

cases(): the quick tier's, on movies built here (bfbc2-menu-tools\tests\test_aptgraft.py's builders and a few of
this module's own: mode switches, sprite timelines with labels; no game data). opfor_calls(opfor): the --game tier's
(Project OPFOR's console_hud.py steps on the PC movies as its stage keeps them before patching - the bundles'
.opfor-hud-orig backups read in place, and the same movies copied out as .res -, the console HUD from work\xbox_dlc;
every output into the case folder).
"""
from __future__ import annotations

import glob
import os
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
if SUITE not in sys.path:
    sys.path.insert(0, SUITE)
import suite_paths  # noqa: E402

suite_paths.use('core', 'menu-tools', 'apt')
sys.path.insert(0, os.path.join(SUITE, 'bfbc2-menu-tools', 'tests'))
import aptgraft as ag  # noqa: E402
import test_aptgraft as tag  # noqa: E402
from aptgraft import Code, Ins, Ref, Words  # noqa: E402

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
# files: {name in the case folder: a source path, or (contents, modification time)}
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}
T = 'aptgraft'


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


def res(m: Any, order: str | None = None) -> bytes:
    return ag.write_bigf(m.write(order=order))


S = tag.S


def modes_movie(name: str = 'modes') -> Any:
    """Two functions: getMapDesc (a mode switch with its fallback, mp_objectives' shape; twice, in two actions sharing
    one pool's shape) and getTeamName / showGameModeComponents (switches on the mode: PushValueOfVar, the first compare
    through SetRegister, PushData of a register), and an anonymous function with a "conquest" switch."""
    m = ag.Movie(tag.blank(name))
    names = ['gameModeId', 'conquest', 'KEY_CQ', 'Missing DESC tag!! ', 'goldrush', 'tdm', 'Missing GM tag: ',
             'KEY_GR', 'KEY_TDM']

    def pool() -> Ins:
        return Ins(0x88, [len(names), Ref(Words('apt', None, [m.const_index(('s', t.encode())) for t in names],
                                                'poollist'))])

    def map_desc(fallback: int) -> list[Ins]:
        end, fb, l1 = Ins(0xB9, [2]), Ins(0xA2, [fallback]), Ins(0xA2, [2])
        return [Ins(0xAE, [0]), Ins(0xA2, [1]), Ins(0x66), Ins(0x9D, target=l1), Ins(0x99, target=fb),
                l1, Ins(0x87, [2]), Ins(0x17), Ins(0x99, target=end),
                fb, Ins(0xAE, [0]), Ins(0x47), Ins(0x87, [2]), Ins(0x17), Ins(0x99, target=end), end, Ins(0x3E)]

    def fn(name: str, body: list[Ins]) -> Ins:
        return Ins(0x8E, [S(name), 0, 3 | (0x29 << 16), Ref(Words('apt', None, [], 'f2params')), *ag.FN_MAGIC],
                   body=body)
    reg1 = Ref(Words('apt', None, [m.const_index((4, 1))], 'pushlist'))
    end2, a1, a2 = Ins(0x3E), Ins(0xA2, [7]), Ins(0xA2, [8])
    team = [Ins(0xAE, [0]), Ins(0x87, [1]), Ins(0xA2, [4]), Ins(0x66), Ins(0x9D, target=a1),
            Ins(0x96, [1, reg1]), Ins(0xA2, [5]), Ins(0x49), Ins(0x9D, target=a2), Ins(0x99, target=end2),
            a1, Ins(0x99, target=end2), a2, end2]
    show = [Ins(0xAE, [0]), Ins(0xA2, [5]), Ins(0x66), Ins(0x9D, target=None), Ins(0x3E)]
    show[3].target = show[4]
    anon_end = Ins(0x3E)
    anon = [Ins(0xAE, [0]), Ins(0xA2, [1]), Ins(0x66), Ins(0x9D, target=anon_end), anon_end]
    code1 = Code('apt', None, [pool(), fn('getMapDesc', map_desc(3)), fn('getTeamName', team),
                               fn('showGameModeComponents', show),
                               Ins(0x9B, [S(''), 0, Ref(Words('apt', None, [], 'f1params')), *ag.FN_MAGIC], body=anon),
                               Ins(0)])
    code2 = Code('apt', None, [pool(), fn('getHeadline', map_desc(6)), fn('getMapDesc', map_desc(3)), Ins(0)])
    m.add_item(0, Words('apt', None, [1, Ref(code1)], 'action'))
    m.add_item(0, Words('apt', None, [1, Ref(code2)], 'action'))
    return ag.Movie(m.write(), name)


def labels_movie(name: str = 'icons') -> Any:
    """A main timeline and two sprites with frame labels (PBLB on frame 1, "Type95 Boat" already in one sprite)."""
    m = ag.Movie(tag.blank(name))
    for f, lab in ((0, 'start'), (1, 'PBLB'), (2, 'MRKV')):
        m.add_item(f, Words('apt', None, [2, S(lab), 0x70000, f], 'label'))
    for k in range(2):
        sp = Words('apt', None, [5, tag.SIG, 1, 0, 0], 'sprite')
        m.add_char(sp)
        m.add_item(0, Words('apt', None, [2, S('idle'), 0, 0], 'label'), sp)
        m.add_item(2, Words('apt', None, [2, S('PBLB'), 0x70000, 2], 'label'), sp)
        if k:
            m.add_item(2, Words('apt', None, [2, S('Type95 Boat'), 0x70000, 2], 'label'), sp)
    return ag.Movie(m.write(), name)


def inputs() -> dict[str, bytes]:
    src, imp, sid, spid = tag.build_source()
    built = ag.write_bigf(src.write())
    big = ag.Movie(src.write(order='>'), 'big')
    target = tag.target()
    dead = src.write()
    dead = [(dead[0][0], dead[0][1] + b'\x11\x22\x33\x44')] + dead[1:]
    ch_src = tag.crosshair_movie('src', ['Tank_cannon', 'rifle'], 0x29, image=True)
    ch_dst = tag.crosshair_movie('dst', ['rifle'], 0x129)
    ch_full = tag.crosshair_movie('full', ['rifle'], 0x129, empty=1)
    lm = src.write()
    return {'built.res': built, 'big.res': res(big), 'target.res': res(target), 'dead.res': ag.write_bigf(dead),
            'modes.res': res(modes_movie()), 'icons.res': res(labels_movie()),
            'ch_src.res': res(ch_src, '>'), 'ch_dst.res': res(ch_dst), 'ch_full.res': res(ch_full),
            'notbigf.res': b'RIFF' + bytes(60), 'loose\\built.apt': dict(lm)['src.apt'],
            'loose\\built.const': dict(lm)['src.const'], 'loose\\built.geo': dict(lm)['src.geo']}


def cases() -> list[Case]:
    files = inputs()
    rows: list[Case] = []

    def add(key: str, args: list[str], env: dict[str, str] | None = None, f: dict[str, Any] | None = None) -> None:
        rows.append((key, T, files if f is None else f, args, env or {}, False))

    add('usage', [])
    add('unknown command', ['frobnicate', '{d}\\built.res'])
    # roundtrip
    add('roundtrip one', ['roundtrip', '{d}\\built.res'])
    add('roundtrip --pack', ['roundtrip', '{d}\\built.res', '--pack'])
    add('roundtrip --quiet several', ['roundtrip', '--quiet', '{d}\\built.res', '{d}\\big.res', '{d}\\target.res',
                                      '{d}\\built.res'])
    add('roundtrip --pack several', ['roundtrip', '{d}\\built.res', '{d}\\big.res', '{d}\\modes.res',
                                     '{d}\\icons.res', '{d}\\ch_src.res', '--pack'])
    add('roundtrip dead data', ['roundtrip', '{d}\\dead.res'])
    add('roundtrip dead data --pack', ['roundtrip', '{d}\\dead.res', '--pack'])
    add('roundtrip a folder', ['roundtrip', '{d}'])
    add('roundtrip a folder --quiet', ['roundtrip', '{d}', '--quiet'])
    add('roundtrip a .apt', ['roundtrip', '{d}\\loose\\built.apt'])
    add('roundtrip not bigf', ['roundtrip', '{d}\\notbigf.res', '{d}\\built.res'])
    add('roundtrip missing', ['roundtrip', '{d}\\nowhere.res'])
    add('roundtrip nothing', ['roundtrip', '{d}\\empty'], f={**files, 'empty': None})
    add('roundtrip no args', ['roundtrip'])
    manifest = {**files, 'unp\\fbrb.manifest': '# entries\nMenu/a.res|SwfMovie|1\nMenu/b.res|Other|2\n'
                'Menu/gone.res|SwfMovie|3\n', 'unp\\Menu\\a.res': files['modes.res'],
                'unp\\Menu\\b.res': files['built.res'], 'unp\\sub\\c.res': files['big.res']}
    add('roundtrip a manifest folder', ['roundtrip', '{d}\\unp'], f=manifest)
    # convert
    add('convert le to be', ['convert', '{d}\\built.res', '{d}\\out.res'])
    add('convert be to le', ['convert', '{d}\\big.res', '{d}\\out.res'])
    add('convert --le same order', ['convert', '{d}\\built.res', '{d}\\out.res', '--le'])
    add('convert --be --pack', ['convert', '--be', '--pack', '{d}\\built.res', '{d}\\out.res'])
    add('convert a .apt', ['convert', '{d}\\loose\\built.apt', '{d}\\out.res'])
    add('convert dead data', ['convert', '{d}\\dead.res', '{d}\\out.res'])
    add('convert wrong count', ['convert', '{d}\\built.res'])
    add('convert missing', ['convert', '{d}\\nowhere.res', '{d}\\out.res'])
    add('convert not bigf', ['convert', '{d}\\notbigf.res', '{d}\\out.res'])
    add('convert into a missing folder', ['convert', '{d}\\built.res', '{d}\\no\\such\\out.res'])
    # deps
    for c in ('3', '0x3', '1', '2', '5', '99', '-1'):
        add(f'deps {c}', ['deps', '{d}\\big.res', c])
    add('deps crosshair source', ['deps', '{d}\\ch_src.res', '2'])
    add('deps wrong count', ['deps', '{d}\\big.res'])
    # graft
    add('graft named', ['graft', '{d}\\target.res', '{d}\\big.res', '3', '{d}\\out.res', '--name', 'widget', '--x', '10',
                        '--y', '20'])
    add('graft plain', ['graft', '{d}\\target.res', '{d}\\built.res', '3', '{d}\\out.res'])
    add('graft depth', ['graft', '{d}\\target.res', '{d}\\big.res', '0x3', '{d}\\out.res', '--depth', '9', '--frame',
                        '0'])
    add('graft the shape', ['graft', '{d}\\target.res', '{d}\\big.res', '2', '{d}\\out.res'])
    add('graft the import', ['graft', '{d}\\target.res', '{d}\\big.res', '1', '{d}\\out.res'])
    add('graft into itself', ['graft', '{d}\\built.res', '{d}\\big.res', '3', '{d}\\out.res'])
    add('graft crosshair image', ['graft', '{d}\\ch_dst.res', '{d}\\ch_src.res', '3', '{d}\\out.res'])
    add('graft missing char', ['graft', '{d}\\target.res', '{d}\\big.res', '99', '{d}\\out.res'])
    add('graft the movie', ['graft', '{d}\\target.res', '{d}\\big.res', '0', '{d}\\out.res'])
    add('graft bad frame', ['graft', '{d}\\target.res', '{d}\\big.res', '3', '{d}\\out.res', '--frame', '4'])
    add('graft wrong count', ['graft', '{d}\\target.res', '{d}\\big.res', '3'])
    add('graft value missing', ['graft', '{d}\\target.res', '{d}\\big.res', '3', '{d}\\out.res', '--name'])
    # mode-case
    add('mode-case getMapDesc', ['mode-case', '{d}\\modes.res', '{d}\\out.res', '--function', 'getMapDesc', '--mode',
                                 'onslaught', '--key', 'MP_LOADING_OBJECTIVE_ONSLAUGHT'])
    add('mode-case headline fallback', ['mode-case', '{d}\\modes.res', '{d}\\out.res', '--function', 'getHeadline',
                                        '--mode', 'onslaught', '--key', 'MAINMENU_COOP', '--fallback',
                                        'Missing GM tag: '])
    add('mode-case existing key', ['mode-case', '{d}\\modes.res', '{d}\\out.res', '--function', 'getMapDesc', '--mode',
                                   'conquest', '--key', 'KEY_CQ'])
    add('mode-case wrong fallback', ['mode-case', '{d}\\modes.res', '{d}\\out.res', '--function', 'getMapDesc',
                                     '--mode', 'onslaught', '--key', 'K', '--fallback', 'nope'])
    add('mode-case no function', ['mode-case', '{d}\\modes.res', '{d}\\out.res', '--function', 'getNothing', '--mode',
                                  'onslaught', '--key', 'K'])
    add('mode-case no key', ['mode-case', '{d}\\modes.res', '{d}\\out.res', '--function', 'getMapDesc', '--mode',
                             'onslaught'])
    add('mode-case in place', ['mode-case', '{d}\\modes.res', '{d}\\modes.res', '--function', 'getMapDesc', '--mode',
                               'onslaught', '--key', 'KEY_ONS'])
    # mode-alias
    add('mode-alias all', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'onslaught', '--like', 'conquest'])
    add('mode-alias team', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'onslaught', '--like',
                            'goldrush', '--function', 'getTeamName'])
    add('mode-alias tdm two', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'onslaught', '--like', 'tdm',
                               '--function', 'getTeamName,showGameModeComponents'])
    add('mode-alias existing', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'tdm', '--like', 'goldrush'])
    add('mode-alias none', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'onslaught', '--like', 'rush'])
    add('mode-alias none in', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'onslaught', '--like',
                               'conquest', '--function', 'nope,never'])
    add('mode-alias missing like', ['mode-alias', '{d}\\modes.res', '{d}\\out.res', '--mode', 'onslaught'])
    # label-alias
    add('label-alias one', ['label-alias', '{d}\\icons.res', '{d}\\out.res', '--like', 'PBLB', '--alias',
                            'Type95 Boat'])
    add('label-alias several', ['label-alias', '{d}\\icons.res', '{d}\\out.res', '--like', 'PBLB', '--alias',
                                'A,B,Type95 Boat'])
    add('label-alias main only', ['label-alias', '{d}\\icons.res', '{d}\\out.res', '--like', 'MRKV', '--alias', 'X'])
    add('label-alias none', ['label-alias', '{d}\\icons.res', '{d}\\out.res', '--like', 'NOPE', '--alias', 'X'])
    add('label-alias crosshair', ['label-alias', '{d}\\ch_dst.res', '{d}\\out.res', '--like', 'rifle', '--alias',
                                  'rifle2'])
    add('label-alias wrong count', ['label-alias', '{d}\\icons.res', '--like', 'PBLB', '--alias', 'X'])
    # crosshair-add
    add('crosshair-add', ['crosshair-add', '{d}\\ch_dst.res', '{d}\\ch_src.res', '{d}\\out.res', '--id',
                          'Tank_cannon'])
    add('crosshair-add frame', ['crosshair-add', '{d}\\ch_dst.res', '{d}\\ch_src.res', '{d}\\out.res', '--id',
                                'Tank_cannon', '--frame', '5'])
    add('crosshair-add has it', ['crosshair-add', '{d}\\ch_dst.res', '{d}\\ch_src.res', '{d}\\out.res', '--id', 'rifle'])
    add('crosshair-add full', ['crosshair-add', '{d}\\ch_full.res', '{d}\\ch_src.res', '{d}\\out.res', '--id',
                               'Tank_cannon'])
    add('crosshair-add unknown id', ['crosshair-add', '{d}\\ch_dst.res', '{d}\\ch_src.res', '{d}\\out.res', '--id',
                                     'nope'])
    add('crosshair-add two with frame', ['crosshair-add', '{d}\\ch_dst.res', '{d}\\ch_src.res', '{d}\\out.res',
                                         '--id', 'a,b', '--frame', '3'])
    # onslaught-timer (the console movie is the game's: here only the refusals)
    add('onslaught-timer order', ['onslaught-timer', '{d}\\big.res', '{d}\\built.res', '{d}\\out.res'])
    add('onslaught-timer not console', ['onslaught-timer', '{d}\\built.res', '{d}\\big.res', '{d}\\out.res'])
    add('onslaught-timer wrong count', ['onslaught-timer', '{d}\\built.res', '{d}\\big.res'])
    # <bundle.fbrb>:<name> specs: the movie read out of a bundle, then (fbrb._budget) the caches' overall budget -
    # its lock and stamp in the cache root, the least recently used entries of any cache deleted beyond
    # BFBC2_CACHE_TOTAL_GB, nothing when the stamp is fresh
    from bfbc2core import fbrb
    bundle = fbrb.build([('Menu/modes.res', 'SwfMovie', files['modes.res']),
                         ('Menu/built.res', 'SwfMovie', files['built.res'])])
    bun = {**files, 'b.fbrb': bundle, 'b.fbrb.opfor-hud-orig': bundle}
    cache = '{lad}/bfbc2-tool-suite/cache/'
    full = {**bun, cache + 'collision/old.bin': (bytes(2000), 1_600_000_000.0),
            cache + 'culltree/mid.pkl': (bytes(2000), 1_650_000_000.0),
            cache + 'fbrb/new.payload': (bytes(2000), 1_700_000_000.0),
            cache + 'fbrb/x.payload.tmp-1': (bytes(5000), 1_500_000_000.0),
            cache + 'collision/.hidden.bin': (bytes(5000), 1_500_000_000.0),
            cache + 'collision/note.txt': (bytes(5000), 1_500_000_000.0)}
    tiny = {'BFBC2_CACHE_TOTAL_GB': '0.000003'}
    add('bundle spec mode-case', ['mode-case', '{d}\\b.fbrb:modes', '{d}\\out.res', '--function', 'getMapDesc',
                                  '--mode', 'onslaught', '--key', 'K'], f=bun)
    add('bundle spec backup suffix', ['mode-alias', '{d}\\b.fbrb.opfor-hud-orig:modes', '{d}\\out.res', '--mode',
                                      'onslaught', '--like', 'conquest'], f=bun)
    add('bundle spec entry name', ['deps', '{d}\\b.fbrb:Menu/built.res', '3'], f=bun)
    add('bundle spec roundtrip', ['roundtrip', '{d}\\b.fbrb:modes', '{d}\\built.res'], f=bun)
    add('bundle spec missing entry', ['mode-case', '{d}\\b.fbrb:nope', '{d}\\out.res', '--function', 'getMapDesc',
                                      '--mode', 'onslaught', '--key', 'K'], f=bun)
    add('bundle spec missing bundle', ['deps', '{d}\\gone.fbrb:modes', '3'], f=bun)
    add('bundle spec over budget', ['mode-case', '{d}\\b.fbrb:modes', '{d}\\out.res', '--function', 'getMapDesc',
                                    '--mode', 'onslaught', '--key', 'K'], tiny, f=full)
    add('bundle spec over budget, stamp fresh', ['deps', '{d}\\b.fbrb:modes', '3'], tiny,
        f={**full, cache + '.pruned-total': b''})
    add('bundle spec over budget, stamp old', ['deps', '{d}\\b.fbrb:modes', '3'], tiny,
        f={**full, cache + '.pruned-total': (b'', 1_600_000_000.0)})
    add('bundle spec under budget', ['deps', '{d}\\b.fbrb:modes', '3'], f=full)
    add('file spec over budget', ['deps', '{d}\\modes.res', '3'], tiny, f=full)
    # encodings: a path outside cp1252 (stdout fails), and in it
    uni = {**files, 'Ω\\built.res': files['built.res'], 'é\\modes.res': files['modes.res']}
    add('roundtrip path outside cp1252', ['roundtrip', '{d}\\Ω\\built.res'], f=uni)
    add('mode-case path in cp1252', ['mode-case', '{d}\\é\\modes.res', '{d}\\é\\out.res', '--function', 'getMapDesc',
                                     '--mode', 'onslaught', '--key', 'K'], f=uni)
    add('label-alias alias outside cp1252', ['label-alias', '{d}\\icons.res', '{d}\\out.res', '--like', 'PBLB',
                                             '--alias', 'Ω'])
    picked = ['usage', 'roundtrip --pack several', 'roundtrip path outside cp1252', 'mode-case path in cp1252',
              'deps 3', 'graft named', 'crosshair-add', 'roundtrip not bigf']
    return with_variants(rows, picked)


# ------------------------------------------------------------------------------------------- Project OPFOR's calls

def _onslaught_steps() -> list[tuple[str, str, list[str]]]:
    """console_hud.py's steps: (bundle, entry, [command, options...]) - its PATCHES and LOADING_SCREEN."""
    ons = ['--mode', 'onslaught']
    loading = [['mode-case', '--function', 'getHeadline', *ons, '--key', 'MAINMENU_COOP', '--fallback',
                'Missing GM tag: '],
               ['mode-case', '--function', 'getMapDesc', *ons, '--key', 'MP_LOADING_OBJECTIVE_ONSLAUGHT'],
               ['mode-alias', *ons, '--like', 'goldrush', '--function', 'getTeamName'],
               ['mode-alias', *ons, '--like', 'tdm', '--function', 'showGameModeComponents']]
    out = [('ingame-00', 'mp_objectives', ['mode-case', '--function', 'getMapDesc', *ons, '--key',
                                           'MP_LOADING_OBJECTIVE_ONSLAUGHT']),
           ('ingame-00', 'hud_root', ['mode-alias', *ons, '--like', 'conquest'])]
    out += [('loader-00', 'loadingscreen_mp', s) for s in loading]
    return out


def opfor_calls(opfor: str) -> list[Call]:
    from bfbc2core import fbrb
    out: list[Call] = []
    game = os.path.join(opfor, 'game')
    con = os.path.join(opfor, 'work', 'xbox_dlc', 'hud', 'con_ingame', 'Menu', 'hudlayer_shared.res')
    for lv in ('ons_mp_002', 'ons_mp_004', 'ons_mp_005', 'ons_mp_008'):
        for name in ('ingame-00', 'loader-00'):
            pristine = os.path.join(game, 'Package', 'levels', lv, name + '.fbrb.opfor-hud-orig')
            if not os.path.isfile(pristine):
                pristine = os.path.join(game, 'Package', 'levels', lv, name + '.fbrb')
            if not os.path.isfile(pristine):
                continue
            names = fbrb.names(pristine)
            for bundle, movie, step in _onslaught_steps():
                if bundle != name or f'menu/{movie}.res' not in names:
                    continue
                real = names[f'menu/{movie}.res'].name
                data = fbrb.read(pristine, [real])[real]
                cmd, opts = step[0], step[1:]
                # as console_hud.patch_movie runs it: <movie.res> <out> <options>, and the bundle spec form
                out.append((f'{lv} {movie} {" ".join(step)}', T, [cmd, '{d}\\' + movie + '.res', '{d}\\out.res', *opts],
                            {movie + '.res': (data, 1_700_000_000.0)}, {}, True))
                out.append((f'{lv} {movie} {" ".join(step)} (bundle spec)', T,
                            [cmd, f'{pristine}:{movie}', '{d}\\out.res', *opts], {}, {}, True))
            if name == 'ingame-00' and os.path.isfile(con) and 'menu/hudlayer_shared.res' in names:
                real = names['menu/hudlayer_shared.res'].name
                data = fbrb.read(pristine, [real])[real]
                with open(con, 'rb') as f:
                    cdata = f.read()
                files = {'hudlayer_shared.res': (data, 1_700_000_000.0), 'con.res': (cdata, 1_700_000_000.0)}
                for extra in ([], ['--no-lms'], ['--no-place']):
                    out.append((f'{lv} onslaught-timer {extra}', T,
                                ['onslaught-timer', '{d}\\hudlayer_shared.res', '{d}\\con.res', '{d}\\out.res', *extra],
                                files, {}, True))
    # roundtrip of every menu movie OPFOR's stages keep unpacked (work\console_hud), one call per level
    for lv_dir in sorted(glob.glob(os.path.join(opfor, 'work', 'console_hud', 'ons_mp_*', 'ingame-00'))):
        out.append((f'roundtrip {os.path.basename(os.path.dirname(lv_dir))} ingame-00', T,
                    ['roundtrip', lv_dir, '--quiet'], {}, {}, True))
    # roundtrip --pack of the console HUD, convert and deps on it
    if os.path.isfile(con):
        with open(con, 'rb') as f:
            cdata = f.read()
        out.append(('roundtrip --pack console hudlayer_shared', T, ['roundtrip', '{d}\\con.res', '--pack'],
                    {'con.res': (cdata, 1_700_000_000.0)}, {}, True))
        out.append(('convert console hudlayer_shared', T, ['convert', '{d}\\con.res', '{d}\\out.res'],
                    {'con.res': (cdata, 1_700_000_000.0)}, {}, True))
        for c in ('184', '146'):
            out.append((f'deps console {c}', T, ['deps', '{d}\\con.res', c], {'con.res': (cdata, 1_700_000_000.0)},
                        {}, True))
    return out
