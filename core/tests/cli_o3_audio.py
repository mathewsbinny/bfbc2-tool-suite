r"""Round O3's audio group: the command lines test_cli_o3.py replays for audio.py (bfbc2-console-convert: an Xbox 360
Wave resource, BFBC2's or Battlefield 1943's, as BFBC2's PC Wave).

cases(): the quick tier's (synthetic Waves made in code: EA-XMA streams of made-up XMA2 packets, which the in-process
XMA2 decoder - ffmpeg's xma2 decoder ported, round PR1b-1c, XMA-DEC-1 - turns into deterministic samples; no game data,
no ffmpeg: recorded again in PR1b-1c with no ffmpeg on PATH, as GitHub's runner has none). opfor_calls(opfor): the --game tier's
(Project OPFOR does not run audio.py as a process - it imports audio.convert - so the replay is the command line on
real Xbox Waves of its console extracts, copied to the case folder).
"""
from __future__ import annotations

import os
import random
import struct
import sys
from typing import Any

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, str], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}
PACKET = 2048


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


# ------------------------------------------------------------------------------------------- synthetic Waves

def _packets(rng: random.Random, n: int, frames: int = 4) -> bytes:
    """n made-up XMA2 packets: a packet header word (frame count, the first frame's bit offset), then noise."""
    out = bytearray()
    for _ in range(n):
        head = (frames << 26) | (32 << 11)
        out += struct.pack('>I', head) + bytes(rng.getrandbits(8) for _ in range(PACKET - 4))
    return bytes(out)


def _xma_block(rng: random.Random, channels: int, packets: int, cut: int = 0) -> bytes:
    """An EA-XMA block's data: per XMA stream (2 channels each) u32 n << 2, then n - 4 bytes of packets."""
    out = bytearray()
    for _ in range((channels + 1) // 2):
        p = _packets(rng, packets)[:packets * PACKET - cut]
        out += struct.pack('>I', (len(p) + 4) << 2) + p
    return bytes(out)


def _header(version: int, codec: int, channels: int, rate: int, type_: int, samples: int) -> bytes:
    return struct.pack('>II', version << 28 | codec << 24 | (channels - 1) << 18 | rate, type_ << 30 | samples)


def sps(codec: int, channels: int, rate: int, blocks: list[tuple[int, bytes]], samples: int | None = None) -> bytes:
    """An SPS stream (v1): 0x48 header, 0x44 blocks, 0x45 end."""
    n = sum(k for k, _ in blocks) if samples is None else samples
    out = b'\x48\x00\x00\x0c' + _header(1, codec, channels, rate, 1, n)
    for k, d in blocks:
        out += struct.pack('>II', 0x44000000 | (8 + len(d)), k) + d
    return out + b'\x45\x00\x00\x04'


def snr(codec: int, channels: int, rate: int, blocks: list[tuple[int, bytes]], streamed: bool = False) -> bytes:
    """An SNR stream (v0, BF1943): the header, then (in memory) its blocks."""
    n = sum(k for k, _ in blocks)
    out = _header(0, codec, channels, rate, 0, n)
    if streamed:
        return out
    for i, (k, d) in enumerate(blocks):
        out += struct.pack('>II', (0x80 if i == len(blocks) - 1 else 0) << 24 | (8 + len(d)), k) + d
    return out


def sns(blocks: list[tuple[int, bytes]]) -> bytes:
    out = b''
    for i, (k, d) in enumerate(blocks):
        out += struct.pack('>II', (0x80 if i == len(blocks) - 1 else 0) << 24 | (8 + len(d)), k) + d
    return out


def _blob(b: bytes, e: str) -> bytes:
    return struct.pack(e + 'I', len(b)) + b + b'\xad\xde\xad'[:(-len(b)) % 4]


def wave(segments: list[bytes | None], channels: int, total: int, intro: int = 0, outro: int = 0, word14: int = 0,
         flags_id: int = 0x00140000, e: str = '>', info: bytes = b'') -> bytes:
    """A BFBC2 Wave resource (Xbox big-endian by default)."""
    out = struct.pack(e + '6I', flags_id, total, intro, channels, outro, word14)
    for s in segments:
        out += struct.pack(e + 'I', 0) if s is None else _blob(s, e) + _blob(info, e)
    return out


def wave_1943(segments: list[bytes | None], channels: int, total: int, rate: float = 24000.0, loop_start: int = 0,
              loop_end: int | None = None, infos: list[bytes] | None = None) -> bytes:
    """A Battlefield 1943 Wave resource: >f6I rate, word 4, channels, total, loop start, loop end, flags | id."""
    out = struct.pack('>f6I', rate, 7, channels, total, loop_start, total if loop_end is None else loop_end, 0x00200001)
    for i, s in enumerate(segments):
        out += struct.pack('>I', 0) if s is None else _blob(s, '>') + _blob((infos or [b''] * 3)[i], '>')
    return out


def _sounds() -> dict[str, bytes]:
    rng = random.Random(1943)
    out: dict[str, bytes] = {}
    blk = lambda ch, n=1, cut=0: _xma_block(rng, ch, n, cut)                                     # noqa: E731
    out['mono'] = wave([None, sps(3, 1, 24000, [(4096, blk(1))]), None], 1, 4096)
    out['stereo'] = wave([None, sps(3, 2, 48000, [(5000, blk(2)), (3000, blk(2, 1, 700))]), None], 2, 8000,
                         word14=0x1234)
    out['three'] = wave([None, sps(3, 3, 44100, [(4000, blk(3))]), None], 3, 4000)
    out['segments'] = wave([sps(3, 1, 22050, [(1000, blk(1))]), sps(3, 1, 22050, [(2048, blk(1, 2))]),
                            sps(3, 1, 22050, [(300, blk(1))])], 1, 3348, intro=1000, outro=300)
    out['long'] = wave([None, sps(3, 2, 32000, [(9000, blk(2)) for _ in range(6)]), None], 2, 54000)
    loop = sps(3, 1, 24000, [(6000, blk(1, 2))])
    out['streamed'] = wave([None, struct.pack('>I', 6000), None], 1, 6000, info=b'\x01\x02\x03')
    out['streamed.sns'] = loop
    out['xas'] = wave([None, sps(4, 1, 24000, [(128, bytes(76))]), None], 1, 128)
    out['pcm'] = wave([None, sps(2, 1, 24000, [(4, bytes(8))]), None], 1, 4)
    out['lps'] = wave([None, sps(5, 1, 24000, [(4, bytes(8))]), None], 1, 4)
    out['chans'] = wave([None, sps(3, 2, 24000, [(4096, blk(2))]), None], 1, 4096)
    out['head'] = wave([None, sps(3, 1, 24000, [(4096, blk(1))], samples=9000), None], 1, 9000)
    out['badxma'] = wave([None, sps(3, 1, 24000, [(4096, struct.pack('>I', 9999 << 2) + bytes(20))]), None], 1, 4096)
    out['zeros'] = wave([None, sps(3, 1, 24000, [(4096, struct.pack('>I', (PACKET + 4) << 2) + bytes(PACKET))]), None],
                        1, 4096)
    out['short'] = b'\x00' * 20
    out['cut'] = out['mono'][:-9]
    out['noloop'] = wave([sps(3, 1, 24000, [(4096, blk(1))]), None, None], 1, 4096)
    out['pc'] = wave([None, sps(3, 1, 24000, [(4096, blk(1))]), None], 1, 4096, e='<')
    # Battlefield 1943: in memory (SNR with its blocks), streamed (the 8-byte header, the blocks in SNS)
    b0 = [(3000, blk(1)), (1000, blk(1, 1, 900))]
    out['bf_mem'] = wave_1943([None, snr(3, 1, 24000, b0), None], 1, 4000)
    out['bf_loop'] = wave_1943([snr(3, 2, 32000, [(500, blk(2))]), snr(3, 2, 32000, [(4000, blk(2))]),
                                snr(3, 2, 32000, [(700, blk(2))])], 2, 5200, rate=32000.0, loop_start=500,
                               loop_end=4500)
    sb = [(4096, blk(2)), (2000, blk(2))]
    out['bf_stream'] = wave_1943([None, snr(3, 2, 48000, sb, streamed=True), None], 2, 6096, rate=48000.0,
                                 infos=[b'', b'\x00\x01', b''])
    out['bf_stream.sns'] = sns(sb)
    out['bf_streamhead'] = wave_1943([None, snr(3, 1, 24000, [(4096, blk(1))]) + b'', None], 1, 8000,
                                     infos=[b'', b'\x05', b''])
    out['bf_nan'] = wave_1943([None, snr(3, 1, 24000, b0), None], 1, 4000, rate=float('nan'))
    out['bf_xas'] = wave_1943([None, snr(4, 1, 24000, [(128, bytes(76))]), None], 1, 128)
    return out


SOUNDS = _sounds()


def _files(*names: str) -> dict[str, Any]:
    return {n: SOUNDS[n] for n in names}


def cases() -> list[Case]:
    rows: list[Case] = []

    def add(key: str, files: dict[str, Any], args: list[str], env: dict[str, str] | None = None) -> None:
        rows.append((key, 'audio', files, args, env or {}, False))
    # the command line: argparse's usage, help and errors
    add('usage', {}, [])
    add('help', {}, ['-h'])
    add('help long', {}, ['--help'])
    add('help after', {}, ['bfbc2', 'a', '-h'])
    add('bad source', {}, ['xbox', 'a', 'b'])
    add('bad source empty', {}, ['', 'a', 'b'])
    add('missing output', {}, ['bfbc2', 'a'])
    add('missing two', {}, ['bf1943'])
    add('extra', {}, ['bfbc2', 'a', 'b', 'c'])
    add('extra two', {}, ['bfbc2', 'a', 'b', 'c', 'd'])
    add('unknown option', {}, ['bfbc2', 'a', 'b', '--nope'])
    add('unknown option eq', {}, ['--nope=1', 'bfbc2', 'a', 'b'])
    add('stream no value', {}, ['bfbc2', 'a', 'b', '--stream'])
    add('stream option value', {}, ['--stream', '--x', 'bfbc2', 'a', 'b'])
    add('stream abbreviation', {}, ['--str', '1=x', 'bfbc2', 'a', 'b'])
    add('options between', _files('mono'), ['bfbc2', '{d}\\mono', '--stream', '5=nothing', '{d}\\out.res'])
    add('negative number arg', {}, ['bfbc2', '-1', '{d}\\o'])
    add('dash alone', {}, ['bfbc2', '-', '{d}\\o'])
    add('double dash', _files('mono'), ['bfbc2', '--', '{d}\\mono', '{d}\\o'])
    add('bad choice and extra', {}, ['nope', 'a', 'b', 'c'])
    add('missing and unknown', {}, ['bfbc2', '--zz'])
    add('stream eq form', _files('streamed', 'streamed.sns'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream=1={d}\\streamed.sns'])
    # files
    add('no input', {}, ['bfbc2', '{d}\\nothing.res', '{d}\\out.res'])
    add('input is a folder', {'sub': None}, ['bfbc2', '{d}\\sub', '{d}\\out.res'])
    add('stream missing file', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\out.res', '--stream', '1={d}\\none'])
    add('stream not a number', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\out.res', '--stream', 'x={d}\\mono'])
    add('stream no equals', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\out.res', '--stream', '1'])
    add('stream spaces', _files('streamed', 'streamed.sns'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream', ' +1 ={d}\\streamed.sns'])
    add('stream underscore', _files('streamed', 'streamed.sns'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream', '0_1={d}\\streamed.sns'])
    add('stream twice', _files('streamed', 'streamed.sns', 'mono'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream', '1={d}\\mono', '--stream', '1={d}\\streamed.sns'])
    add('stream other segment', _files('streamed', 'streamed.sns'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream', '2={d}\\streamed.sns'])
    add('output folder missing', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\no\\out.res'])
    add('output is a folder', dict(_files('mono'), sub=None), ['bfbc2', '{d}\\mono', '{d}\\sub'])
    add('output name cp1252', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\café.res'])
    add('output name outside cp1252', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\жук.res'])
    add('output name emoji', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\\U0001F600.res'])
    # sounds
    for n in ('mono', 'stereo', 'three', 'segments', 'long', 'xas', 'pcm', 'lps', 'chans', 'head', 'badxma',
              'short', 'cut', 'noloop', 'pc', 'streamed'):
        add(f'bfbc2 {n}', _files(n), ['bfbc2', f'{{d}}\\{n}', '{d}\\out.res'])
    add('bfbc2 streamed with sns', _files('streamed', 'streamed.sns'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream', '1={d}\\streamed.sns'])
    add('bfbc2 streamed bad sns', _files('streamed', 'mono'),
        ['bfbc2', '{d}\\streamed', '{d}\\out.res', '--stream', '1={d}\\mono'])
    for n in ('bf_mem', 'bf_loop', 'bf_stream', 'bf_streamhead', 'bf_nan', 'bf_xas'):
        add(f'bf1943 {n}', _files(n), ['bf1943', f'{{d}}\\{n}', '{d}\\out.res'])
    add('bf1943 stream with sns', _files('bf_stream', 'bf_stream.sns'),
        ['bf1943', '{d}\\bf_stream', '{d}\\out.res', '--stream', '1={d}\\bf_stream.sns'])
    add('bf1943 given a bfbc2 wave', _files('mono'), ['bf1943', '{d}\\mono', '{d}\\out.res'])
    add('bfbc2 given a bf1943 wave', _files('bf_mem'), ['bfbc2', '{d}\\bf_mem', '{d}\\out.res'])
    # no ffmpeg (round PR1b-1c: EA-XMA is decoded in process, so these convert as the others do; until then they were
    # audio.py's "ffmpeg not found" / ffmpeg's errors): a PATH without it and no $FFMPEG (the WinGet folder is under the
    # run's own LOCALAPPDATA), $FFMPEG naming nothing, $FFMPEG naming another program
    pydir = os.path.dirname(sys.executable)
    add('no ffmpeg on PATH', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\out.res'], {'PATH': pydir})
    add('no ffmpeg on PATH, no xma', _files('xas'), ['bfbc2', '{d}\\xas', '{d}\\out.res'], {'PATH': pydir})
    add('FFMPEG names nothing', _files('mono'), ['bfbc2', '{d}\\mono', '{d}\\out.res'], {'FFMPEG': '{d}\\nope.exe'})
    add('FFMPEG names another program', dict(_files('mono'), **{'fake.cmd': '@exit /b 3\r\n'}),
        ['bfbc2', '{d}\\mono', '{d}\\out.res'], {'FFMPEG': '{d}\\fake.cmd'})
    picked = ['help', 'bad source', 'bfbc2 stereo', 'bfbc2 xas', 'output name outside cp1252', 'no input',
              'output name emoji', 'bfbc2 short']
    return with_variants(rows, picked)


def opfor_calls(opfor: str) -> list[Call]:
    """Real Xbox 360 Waves from OPFOR's console extracts (work\\xbox: BFBC2's Xbox bundles unpacked), copied to the
    case folder: a spread of sizes, mono and stereo, with intro / outro segments where found; streamed ones with
    their waveDataSns entry."""
    calls: list[Call] = []
    xbox = os.path.join(opfor, 'work', 'xbox')
    if not os.path.isdir(xbox):
        return bf1943_calls()
    found: list[str] = []
    for dp, _ds, fs in os.walk(xbox):
        for f in fs:
            if f.lower().endswith('_wave.res'):
                found.append(os.path.join(dp, f))
        if len(found) > 4000:
            break
    found.sort()
    by_size = sorted(found, key=os.path.getsize)
    picks = by_size[:: max(1, len(by_size) // 10)][:10] + by_size[-2:]
    for p in picks:
        name = os.path.basename(p)
        calls.append((f'audio bfbc2 {name}', 'audio', ['bfbc2', '{d}\\in.res', '{d}\\out.res'], {'in.res': p}, {}, False))
    return calls + bf1943_calls()


def bf1943_calls() -> list[Call]:
    """Battlefield 1943's Waves as OPFOR's wake_vo / wake_level convert them (bf1943import.Source over Project Rising
    Sun's extracted bundles; read-only): voice-over lines with their streamed waveDataSns entries (--stream), and
    Wake's in-memory effects and ambiences."""
    suite = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
    if suite not in sys.path:
        sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('bf1943-import', 'core')
    try:
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception:                                   # noqa: BLE001 - no BF1943 copy here: no such calls
        return []
    waves = [k for k, (_, e) in sorted(src.entries.items()) if e.get('type') == 'Wave']
    vo = [k for k in waves if k.startswith('sound/vo/') and k[:-len('_wave.res')] + '_wavedatasns_1.res' in src.entries]
    sfx = [k for k in waves if k.startswith('sound/beach_sfx/')]
    calls: list[Call] = []
    for k in vo[:: max(1, len(vo) // 4)][:4] + sfx[:: max(1, len(sfx) // 6)][:6]:
        files: dict[str, Any] = {'in.res': (src.data(k), 0.0)}
        args = ['bf1943', '{d}\\in.res', '{d}\\out.res']
        for i in range(3):
            s = k[:-len('_wave.res')] + f'_wavedatasns_{i}.res'
            if s in src.entries:
                files[f'sns{i}.res'] = (src.data(s), 0.0)
                args += ['--stream', f'{i}={{d}}\\sns{i}.res']
        calls.append((f'audio bf1943 {k.rsplit("/", 1)[-1]}', 'audio', args, files, {}, True))
    return calls
