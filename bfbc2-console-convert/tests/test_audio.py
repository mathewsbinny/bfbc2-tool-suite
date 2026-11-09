"""Sound conversion (audio.py) against BFBC2's own Xbox / PC twins, and on Battlefield 1943's sounds.

Needs the user's files: BFBC2's Xbox bundles unpacked by the console tools (OPFOR work\\xbox) and the PC game (OPFOR
game\\); Battlefield 1943 through the suite's bfbc2-bf1943-import; ffmpeg (decoding EA-XMA, and EA-XAS for the
comparison). Skips what is missing. About 5 minutes.
"""
import collections
import glob
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'bf1943-import')
import audio  # noqa: E402
from bfbc2core import fbrb  # noqa: E402

OPFOR = os.path.join(SUITE, '..', 'project-opfor-bfbc2')
END_CREDITS = 'sound/music/menu_loading/end_credits_01_wave.res'
WAKE = ['sound/beach_music/bmus_jploadingloop_wave.res',                          # Wake's loading music
        'sound/beach_sfx/ambiences/global_ambients/bsfx_amb_wake_global_01_wave.res',     # streamed ambiences
        'sound/beach_sfx/ambiences/material_ambients/bsfx_amb_wake_material_grass_01_wave.res',
        'sound/beach_sfx/ambiences/material_ambients/bsfx_amb_wake_material_water_03_wave.res',
        'sound/vo/en/mp/beach_mp_us/bvox_us_wakeisland_win_01_wave.res',          # voice-over (streamed)
        'sound/vo/en/mp/beach_mp_jap/bvox_jap_wakeisland_lose_01_wave.res',
        'sound/beach_sfx/vehicles/bomber/bsfx_ve_bomber_extdrone_001_wave.res']   # intro, loop, outro
SFX = ['sound/beach_sfx/weapons/', 'sound/beach_sfx/vehicles/', 'sound/explosions/']   # the first in-memory one of each


def waves(pattern):
    """{lower-case name: bytes} of every Wave resource in the bundles (one read per bundle)."""
    where: dict[str, tuple[str, str]] = {}
    for p in glob.glob(pattern, recursive=True):
        try:
            entries = fbrb.entries(p)
        except Exception:                                        # noqa: BLE001 - not a bundle this reader takes
            continue
        for e in entries:
            if e.type == 'Wave' and not e.deleted:
                where.setdefault(e.name.lower(), (p, e.name))
    by = collections.defaultdict(list)
    for p, n in where.values():
        by[p].append(n)
    out = {}
    for p, ns in by.items():
        out.update({n.lower(): b for n, b in fbrb.read(p, ns).items()})
    return out


def snr(ref, x):
    ref, x = ref.astype(float), x.astype(float)
    return 10 * np.log10(max(np.sum(ref ** 2), 1e-9) / max(np.sum((ref - x) ** 2), 1e-9))


def corr(a, b):
    a, b = a.astype(float).ravel(), b.astype(float).ravel()
    return float(np.corrcoef(a, b)[0, 1]) if a.std() and b.std() else float(np.array_equal(a, b))


def in_memory(w):
    return all(s is None or (len(s[0]) > 8 and not s[1]) for s in w.segments)


def layout(st):
    return [(n, len(b)) for n, b in st.blocks]


def dice_layout(samples, rate, ch):
    per = audio.xas_block_samples(rate)
    out = [(per, per // 128 * 76 * ch)] * (samples // per)
    rest = samples - per * len(out)
    return out + [(rest, -(-rest // 128) * 76 * ch)]


def best_xas_snr(pcm):
    """The SNR of EA-XAS with every group's best of all 64 coefficient sets / shifts (nibbles rounded): the bound
    the encoder's search is checked against."""
    n, ch = pcm.shape
    F = -(-n // 128)
    x = np.zeros((F * 128, ch), np.int64)
    x[:n] = pcm
    g = x.reshape(F, 4, 32, ch).transpose(0, 3, 1, 2).reshape(-1, 32)
    q0 = np.clip((g[:, 0] + 8) >> 4 << 4, -32768, 32752)
    q1 = np.clip((g[:, 1] + 8) >> 4 << 4, -32768, 32752)
    best = np.full(len(g), np.inf)
    for c in range(4):
        c0, c1 = int(audio.COEFS[c]), int(audio.COEFS[c + 4])
        for k in range(16):
            step, p2, p1, err = 1 << (20 - k), q0, q1, np.zeros(len(g))
            for t in range(30):
                pred = p1 * c0 + p2 * c1
                v = np.clip(np.rint((g[:, t + 2] * 256 - pred) / step), -8, 7).astype(np.int64)
                s = np.clip((v * step + pred + 128) >> 8, -32768, 32767)
                err += (s - g[:, t + 2]) ** 2
                p2, p1 = p1, s
            best = np.minimum(best, err)
    err = best + (g[:, 0] - q0) ** 2 + (g[:, 1] - q1) ** 2
    return 10 * np.log10(np.sum(g.astype(float) ** 2) / np.sum(err))


def main():
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    try:
        audio.ffmpeg_path()
        ff = True
    except audio.AudioError as e:
        print(f'SKIP decoding: {e}')
        ff = False

    X = waves(os.path.join(OPFOR, 'work', 'xbox', '**', '*.fbrb'))
    P = waves(os.path.join(OPFOR, 'game', '**', '*.fbrb'))
    both = sorted(set(X) & set(P))
    if not both:
        print('SKIP: no BFBC2 Xbox / PC Wave twins')
    else:
        # 1. the Frostbite wrapper, both platforms
        wx, wp, bad = {}, {}, []
        for k in both:
            try:
                wx[k], wp[k] = audio.read_wave(X[k], 'bfbc2-xbox'), audio.read_wave(P[k], 'bfbc2-pc')
            except audio.AudioError as e:
                bad.append((k, str(e)))
        check(not bad, f'wrapper: {len(wx)} of {len(both)} BFBC2 twins parse on the Xbox and the PC', str(bad[:2]))
        same = sum(audio.write_wave_pc(wp[k]) == P[k] for k in wp)
        check(same == len(wp), f'wrapper: {same} of {len(wp)} PC files written back byte for byte')
        names = ['flags|id', 'total samples', 'intro samples', 'channels', 'outro samples', 'word 0x14']
        eq = [sum(wx[k].words()[i] == wp[k].words()[i] for k in wx) for i in range(6)]
        print('     Xbox = PC, word by word: ' + ', '.join(f'{n} {e}' for n, e in zip(names, eq)))
        check(min(eq) >= len(wx) - 1, 'wrapper: every header word equal on both platforms (one total: DICE\'s edit)')
        pres = sum([s is None for s in wx[k].segments] == [s is None for s in wp[k].segments] for k in wx)
        check(pres == len(wx), f'wrapper: segments present alike in {pres} of {len(wx)}')
        kinds = collections.Counter(('stub' if len(w.segments[1][0]) == 4 else 'memory') +
                                    (' + stream info' if w.segments[1][1] else '') for w in wp.values())
        print(f'     PC loop segments: {dict(kinds)}')

        # EA stream headers of every in-memory twin segment
        pairs = []
        for k in wx:
            for i in range(3):
                sx, sp = wx[k].segments[i], wp[k].segments[i]
                if sx and sp and len(sx[0]) > 12 and len(sp[0]) > 12:
                    pairs.append((k, i, audio.read_sps(sx[0]), audio.read_sps(sp[0])))
        hdr = collections.Counter((a.codec, b.codec, a.version == b.version == 1, a.channels == b.channels,
                                   a.rate == b.rate, a.type == b.type, a.samples == b.samples) for _, _, a, b in pairs)
        ok = sum(v for (cx, cp, *r), v in hdr.items() if cx == 3 and cp == 4 and all(r))
        check(ok >= len(pairs) - 1, f'EA headers: {ok} of {len(pairs)} in-memory twin streams Xbox EA-XMA / PC '
              f'EA-XAS v1, channels, rate, type, samples equal', str(hdr))
        sums = sum(sum(n for n, _ in a.blocks) == a.samples and sum(n for n, _ in b.blocks) == b.samples
                   for _, _, a, b in pairs)
        check(sums == len(pairs), f'EA blocks: the block samples add up to the header on {sums} of {len(pairs)} twins')
        lay = sum(layout(b) == dice_layout(b.samples, b.rate, b.channels) for _, _, _, b in pairs)
        check(lay == len(pairs), f'EA-XAS: DICE\'s block layout (256 * ceil(rate / 1280) samples a block) on {lay} of '
              f'{len(pairs)} PC streams')

        # 2. EA-XMA -> PCM against the PC's EA-XAS, 3. the XAS decoder / encoder, 4. whole conversions
        if ff:
            full = [k for k in wx if in_memory(wx[k]) and in_memory(wp[k]) and wx[k].words() == wp[k].words()]
            sample = sorted(set(full[::40]) | {END_CREDITS} & set(full))
            exact_n, cs, ss, xas_eq, enc, conv, conv_c = 0, [], [], 0, [], 0, []
            for k in sample:
                w = wx[k]
                for i in range(3):
                    if not w.segments[i]:
                        continue
                    sx, sp = audio.segment_stream(w, i), audio.segment_stream(wp[k], i)
                    raw = audio.decode_xma(sx, trim=False)
                    exact_n += len(raw) == sx.samples
                    a = raw[:sx.samples]
                    b = audio.ffmpeg_decode_xas(sp)
                    mine = audio.decode_xas(sp)
                    xas_eq += np.array_equal(mine, b)
                    cs.append(corr(a, b))
                    ss.append(snr(b, a))
                    rt = audio.decode_xas(audio.encode_xas(a, sx.rate))
                    enc.append((snr(a, rt), k, i, a))
                pc = audio.convert(X[k], 'bfbc2')
                wc = audio.read_wave(pc, 'bfbc2-pc')
                same_struct = wc.words() == wp[k].words() and all(
                    (c is None) == (d is None) for c, d in zip(wc.segments, wp[k].segments))
                for i in range(3):
                    if wc.segments[i]:
                        c, d = audio.read_sps(wc.segments[i][0]), audio.read_sps(wp[k].segments[i][0])
                        same_struct &= c.header() == d.header() and layout(c) == layout(d)
                        conv_c.append(corr(audio.ffmpeg_decode_xas(c), audio.ffmpeg_decode_xas(d)))
                conv += same_struct and len(pc) == len(P[k])
            n = len(cs)
            check(exact_n == n, f'EA-XMA: ffmpeg gives the header\'s sample count on {exact_n} of {n} streams '
                  f'({len(sample)} files, every 40th in-memory twin and the end credits)')
            # XMA is perceptual: noise-like, high-frequency effects (impacts, cracks) match the PC's waveform least
            check(min(cs) > 0.9 and np.median(cs) > 0.98, f'EA-XMA vs the PC\'s EA-XAS: correlation min {min(cs):.4f},'
                  f' median {np.median(cs):.4f}; SNR min {min(ss):.1f} dB, median {np.median(ss):.1f} dB (lag 0)')
            check(xas_eq == n, f'EA-XAS: this decoder = ffmpeg\'s adpcm_ea_xas sample for sample on {xas_eq} of {n} PC '
                  f'streams')
            es = sorted(enc, key=lambda r: r[0])
            check(np.median([r[0] for r in es]) > 30, f'EA-XAS encoder: decoded XMA audio encoded and decoded back, '
                  f'SNR min {es[0][0]:.1f} dB, median {np.median([r[0] for r in es]):.1f} dB, max {es[-1][0]:.1f} dB')
            bound = [(r[0], best_xas_snr(r[3]), r[1].split('/')[-1]) for r in es[:3]]
            check(all(s >= b - 0.05 for s, b, _ in bound), 'EA-XAS encoder: the 3 lowest at the format\'s bound (every '
                  'group\'s best of all 64 coefficient sets / shifts): ' +
                  ', '.join(f'{nm} {s:.2f} / {b:.2f} dB' for s, b, nm in bound))
            check(conv == len(sample), f'convert: {conv} of {len(sample)} BFBC2 Xbox Waves as PC files with the PC '
                  f'twin\'s words, segments, EA headers, block layout and size')
            check(min(conv_c) > 0.9 and np.median(conv_c) > 0.98, f'convert: audio vs the PC twin, correlation min '
                  f'{min(conv_c):.4f}, median {np.median(conv_c):.4f}')

    # Battlefield 1943
    try:
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                       # noqa: BLE001
        print(f'SKIP Battlefield 1943: {e}')
        return 1 if fails else 0
    ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'Wave')
    stats: collections.Counter[tuple[int, int, bool, int] | str]
    ws, bad, stats = {}, [], collections.Counter()
    for k in ks:
        try:
            b = src.data(k)
            w = ws[k] = audio.read_wave(b, 'bf1943')
            for s in w.segments:
                if s:
                    h = audio._ea_header(*struct.unpack_from('>II', s[0], 0))
                    stats[(h['version'], h['codec'], h['rate'] == int(w.rate), w.word04)] += 1
                    stats['streamed' if len(s[0]) == 8 else 'memory'] += 1
        except audio.AudioError as e:
            bad.append((k, str(e)))
    check(not bad, f'Battlefield 1943: {len(ws)} of {len(ks)} Waves parse', str(bad[:2]))
    ok = sum(v for key, v in stats.items() if isinstance(key, tuple) and key == (0, 3, True, 2))
    check(ok == stats['streamed'] + stats['memory'], f'Battlefield 1943: {ok} segments EA-XMA v0, the f32 rate = the '
          f'EA rate, word 0x04 = 2 ({stats["memory"]} in memory, {stats["streamed"]} streamed)', str(stats))
    vo = [k for k in ks if k.startswith('sound/vo/')]
    sns = sum((k[:-len('_wave.res')] + '_wavedatasns_1.res') in src.entries for k in vo)
    check(sns == len(vo), f'Battlefield 1943: {sns} of {len(vo)} voice-over Waves have their stream '
          f'(streaming_sounds-00.fbrb, <name>_waveDataSns_1.res)')
    if not ff:
        return 1 if fails else 0
    sfx = [next((k for k in ks if k.startswith(p) and in_memory(ws[k])), None) for p in SFX]
    for k in WAKE + [k for k in sfx if k]:
        if k not in ws:
            print(f'SKIP {k}: not in the Battlefield 1943 files')
            continue
        b = src.data(k)
        streams = {}
        for i, s in enumerate(ws[k].segments):
            name = f'{k[:-len("_wave.res")]}_wavedatasns_{i}.res'
            if s and name in src.entries:
                streams[i] = src.data(name)
        a, rate = audio.decode(b, 'bf1943', streams)
        pc = audio.convert(b, 'bf1943', streams)
        w = audio.read_wave(pc, 'bfbc2-pc')
        sts = [audio.segment_stream(w, i) for i in range(3) if w.segments[i]]
        good = all(st.version == 1 and st.codec == 4 and layout(st) == dice_layout(st.samples, st.rate, st.channels)
                   for st in sts) and in_memory(w) and w.total == ws[k].total == len(a)
        segs = [audio.segment_stream(ws[k], i, streams).samples if ws[k].segments[i] else 0 for i in range(3)]
        good &= w.intro == segs[0] and w.outro == segs[2]
        d = np.vstack([audio.ffmpeg_decode_xas(st) for st in sts])
        s = snr(a, d)
        bound_note = f', the format\'s bound {best_xas_snr(a):.1f} dB' if s < 30 else ''
        check(good and len(d) == len(a) and s > 20 and corr(a, d) > 0.99,
              f'Battlefield 1943 {k.split("/")[-1]}: {w.channels} ch {rate} Hz, {len(a)} samples '
              f'{"(streamed) " if streams else ""}{"segments " + str(segs) + " " if segs[0] or segs[2] else ""}-> PC '
              f'Wave {len(pc)} bytes; ffmpeg\'s adpcm_ea_xas vs the XMA decode: SNR {s:.1f} dB{bound_note}, correlation '
              f'{corr(a, d):.5f}')
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main())
