"""The resource kinds and loose files that had no format of their own before round SPG4, measured on DICE's files
(read-only; spec/formats/occluder.md OCC-*, soot.md SOOT-*, animtree.md ATREE-*, bink.md BINK-*, audio.md IR-*,
havok.md HAVOK-RAG-1, game_folder.md GAME-FILE-*).

    python spec/probes/resources.py [--opfor <folder>]

Reads, from Project OPFOR BFBC2's game copy (DICE's files only, dated before 2013): every distinct `OccluderMesh`,
`SootMesh`, `AnimTreeInfo`, `ImpulseResponse`, `RagdollResource` and `BinkMemory` entry of DICE's bundles (each name
once, the first bundle in path order), the movies and the aerial height map under `Output/win32`, and the loose files
of the game folder (`database.dbmanifest`, `dfe`, `InstallationData.xml`, `UpdateHistory.log`,
`dialoglogo128x128.jpg`, `BFBC2Game.exe`'s names for them). Prints structure and counts only (sizes, counts, field
words that are counts or versions; no payload). `checks(game)` recomputes the stable answers the rules cite for
core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import math
import os
import re
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import fbrb, paths  # noqa: E402

TYPES = ('OccluderMesh', 'SootMesh', 'AnimTreeInfo', 'ImpulseResponse', 'RagdollResource', 'BinkMemory')
Counter = collections.Counter

EXPECTED: dict[str, Any] = {
    'n': {'AnimTreeInfo': 1926, 'BinkMemory': 56, 'ImpulseResponse': 5, 'OccluderMesh': 361, 'RagdollResource': 1,
          'SootMesh': 40},
    # OCC-HDR-1, OCC-TRI-1, OCC-VTX-1
    'occ_layout': 361, 'occ_rest': 361, 'occ_in_box': 361, 'occ_index_ok': 360, 'occ_empty': 1,
    'occ_tri_block': {'absent': 39, 'present': 321, 'present, empty': 1},
    'occ_tri_records': {'one a triangle': 253, 'more': 68},
    'occ_w': {'1.0, block': 321, 'k x 12.75, no block': 39},
    # OCC-TRI-1 (round SPG4-rest): the block at 16, one 192-bit mask a triangle padded to an even count
    'occ_first16': 361, 'occ_tri_even': 321, 'occ_tri_pad_zero': 321, 'occ_tri_masks': (82, 5812, 22998, 106),
    # SOOT-HDR-1, SOOT-VTX-1, SOOT-TAIL-1
    'soot_layout': 40, 'soot_version': [18], 'soot_stride': {12: 34, 16: 6}, 'soot_boxes': [2, 4, 8, 10, 27],
    'soot_index_ok': 40, 'soot_tail': 40,
    # SOOT-FLAG-1, SOOT-SLOT-1 (round SPG4-rest)
    'soot_flags': {'0,1,12': 12, '1,0,16': 6, '1,1,12': 22}, 'soot_slot_words': 40, 'soot_vertex_slots': 40,
    'soot_vertex_boxes': 40, 'soot_vertex_in_box': 40,
    # ATREE-HDR-1, ATREE-PTR-1
    'atree_version': [6], 'atree_layout': 1926, 'atree_aligned': 1926, 'atree_pointers_inside': 1926,
    # ATREE-BLK-1 - 3
    'atree_node_table': 1926, 'atree_nodes': 1926, 'atree_b_head': 1926, 'atree_c_records': 1926,
    'atree_node_types': (1, 26, 26), 'atree_node_count': 59725,
    # ATREE-NODE-1, ATREE-BLK-4 (round SPG2-rest5)
    'atree_type_names': True,
    'atree_type_counts': {1: 28254, 2: 7756, 3: 5684, 4: 2863, 5: 1074, 6: 749, 7: 27, 8: 3882, 9: 42, 10: 18, 11: 66,
                          12: 12, 13: 280, 14: 134, 15: 4559, 16: 239, 17: 37, 18: 105, 19: 2974, 20: 18, 21: 26,
                          22: 24, 23: 326, 24: 72, 25: 487, 26: 17},
    'atree_nodes_in_order': 1926, 'atree_b_entries': 1926, 'atree_anim_index': 1926,
    'atree_b_lengths': (28254, 0.0333, 182.3333),
    # ATREE-NODE-2 - 4, ATREE-BLK-5, 6 (round SPG2-rest6)
    'atree_node_header': 1926, 'atree_child_refs': 1926, 'atree_anim_flags': 1926,
    'atree_is_cyclic_looping': (332, 332), 'atree_c_curves': 1926,
    'atree_c_modes': {'0@0x40': 15537, '0@0x44': 26257, '0@0x48': 22237, '0@0x4c': 24787, '0@0x50': 25110,
                      '1@0x4c': 3, '2@0x40': 445, '2@0x44': 1997, '2@0x48': 6017, '2@0x4c': 3409, '2@0x50': 1040,
                      '3@0x40': 12272, '3@0x4c': 55, '3@0x50': 949, '4@0x50': 1155},
    'atree_b_cycle': (700, 0.2083, 5.2213, 96, 45.0, 90.0, 0),
    # WAVE-IR-1
    'ir_header': [('irf1', 1, 2, 48000, 1024, 1024, 2048, 1)], 'ir_layout': 5,
    # IR-PART-1 (round SPG4-rest): two channel blocks a partition, a block-scale head, 1,024 (re, im) bins
    'ir_block_heads': 518, 'ir_dc_imag_zero': 518,
    'ir_scales': {1023: 5, 2047: 17, 4095: 44, 8191: 59, 16383: 85, 32767: 308}, 'ir_scaled_full': (210, 210),
    # HAVOK-RAG-1
    'ragdoll': [(16944, True, 'Havok-5.5.0-r1')],
    # BINK-HDR-1, BINK-RES-1
    'bink_res': {('BIKi', 1280, 720, 25.0, 0): 40, ('BIKi', 1280, 720, 29.97, 0): 16}, 'bink_res_size': 56,
    # BINK-OUT-1
    'movies': {('BinkMemory', None, 'BIKi', 0): 22, ('BinkTexture', (0, 0), 'BIKi', 0): 3,
               ('BinkTexture', (0, 0), 'BIKi', 1): 7, ('BinkTexture', (1, 1), 'BIKi', 0): 1},
    'movies_size': 33, 'movies_lengths_zero': 33,
    # GAME-FILE-3
    'aerial': [(8.0, 512, 512, -2048.0, -2048.0, True)],
    # GAME-FILE-4 - 7
    'dbmanifest': ('Database', 2, 2), 'dfe': (1000, True),
    'installation': ['CurrentVersion', 'EulaVersion', 'InstalledHashes', 'PatcherVersion', 'PbEulaVersion', 'Sku'],
    'installation_version': '795745', 'update_history': [('patch', '515195', '795745')],
    'jpeg': (128, 128),
    'exe_names': {'AerialHeightmap.res': True, 'InstallationData': False, 'UpdateHistory': False, 'binkw32.dll': True,
                  'database.dbmanifest': True, 'dialoglogo': False},
}


def dice(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def entries(game: Path) -> dict[str, list[tuple[str, bytes]]]:
    """{type: [(name, bytes)]} of the TYPES, each name once."""
    out: dict[str, list[tuple[str, bytes]]] = collections.defaultdict(list)
    seen: set[str] = set()
    for p in sorted((p for p in game.glob('**/*.fbrb') if dice(p)), key=lambda p: str(p).lower()):
        es = [e for e in fbrb.entries(p) if not e.deleted and e.type in TYPES and e.name.lower() not in seen]
        if not es:
            continue
        seen.update(e.name.lower() for e in es)
        got = fbrb.read(p, [e.name for e in es])
        for e in es:
            b = got[e.name]
            # BinkMemory: only the header is measured (the videos are large)
            out[e.type].append((e.name, b[:64] + struct.pack('<I', len(b)) if e.type == 'BinkMemory' else b))
    return out


def occluders(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """OCC-*: box (6 f32), u32 rest length, a 16-byte part header {u16 vertices, u16 indices, u32 vertex offset,
    u32 index offset, u32 triangle-block offset} (offsets from byte 28), the triangle block, vertices, indices."""
    layout = rest = in_box = index_ok = empty = 0
    block: Counter[str] = Counter()
    recs: Counter[str] = Counter()
    w: Counter[str] = Counter()
    first16 = even = padzero = mask_blocks = mask_tris = all_tris = 0
    top = -1
    for _, d in items:
        mn, mx = struct.unpack_from('<3f', d, 0), struct.unpack_from('<3f', d, 12)
        rest += struct.unpack_from('<I', d, 24)[0] == len(d) - 28
        nv, ni, vo, io, to = struct.unpack_from('<HHIII', d, 28)
        if 28 + vo + 16 * nv == 28 + io and 28 + io + 2 * ni == len(d) and ni % 3 == 0:
            layout += 1
        vs = [struct.unpack_from('<4f', d, 28 + vo + 16 * k) for k in range(nv)]
        in_box += all(mn[a] - 1e-3 <= v[a] <= mx[a] + 1e-3 for v in vs for a in range(3))
        idx = struct.unpack_from(f'<{ni}H', d, 28 + io)
        if not idx:
            empty += 1
        else:
            index_ok += max(idx) < nv
        size = vo - to if to else 0
        block['absent' if not to else 'present, empty' if size == 0 else 'present'] += 1
        if size:
            recs['one a triangle' if size == 24 * (ni // 3) else
                 'more' if size % 24 == 0 and size > 24 * (ni // 3) else 'other'] += 1
        first16 += (to or vo) == 16
        if to and vo > to:
            nt, nrec = ni // 3, (vo - to) // 24
            even += nrec == nt + (nt & 1)
            padzero += not any(d[28 + to + 24 * nt:28 + vo])
            masks = [d[28 + to + 24 * k:28 + to + 24 * k + 24] for k in range(nt)]
            hit = [int.from_bytes(m, 'little') for m in masks if any(m)]
            mask_blocks += bool(hit)
            mask_tris += len(hit)
            all_tris += nt
            top = max([top] + [h.bit_length() - 1 for h in hit])
        ws = {v[3] for v in vs}
        if ws:
            w['1.0, block' if ws == {1.0} and size else
              'k x 12.75, no block' if not to and len(ws) == 1 and (next(iter(ws)) / 12.75).is_integer() else 'other'] += 1
    out.update(occ_layout=layout, occ_rest=rest, occ_in_box=in_box, occ_index_ok=index_ok, occ_empty=empty,
               occ_tri_block=dict(sorted(block.items())), occ_tri_records=dict(sorted(recs.items())),
               occ_w=dict(sorted(w.items())), occ_first16=first16, occ_tri_even=even, occ_tri_pad_zero=padzero,
               occ_tri_masks=(mask_blocks, mask_tris, all_tris, top))


def soots(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """SOOT-*: 5 u32 (version, stride, two flags, n boxes), n x 6 f32, 4 u32 {1, vertex bytes, index bytes, 0},
    vertices, u16 indices, 64 u32 tail {0, n, triangles, 60, 60 words}."""
    layout = index_ok = tail = slot_words = vslots = vboxes = inside = 0
    versions, boxes = set(), set()
    stride: Counter[int] = Counter()
    flags: Counter[str] = Counter()
    for _, d in items:
        ver, st, _f1, _f2, n = struct.unpack_from('<5I', d, 0)
        versions.add(ver)
        boxes.add(n)
        stride[st] += 1
        h = 0x14 + 24 * n
        one, vb, ib, zero = struct.unpack_from('<4I', d, h)
        if (one, zero) == (1, 0) and h + 16 + vb + ib + 256 == len(d) and vb % st == 0:
            layout += 1
        idx = struct.unpack_from(f'<{ib // 2}H', d, h + 16 + vb)
        index_ok += bool(idx) and max(idx) < vb // st
        t = struct.unpack_from('<4I', d, len(d) - 256)
        tail += t == (0, n, ib // 6, 60)
        # SOOT-FLAG-1: (per-pixel, half positions, stride); SOOT-SLOT-1: the trailer's slot words and the vertices'
        # (slot, box) halves of their last u32
        flags[f'{_f1},{_f2},{st}'] += 1
        words = struct.unpack_from('<64I', d, len(d) - 256)[4:4 + t[3]]
        used = [x for x in words if x != 0xFFFFFFFF]
        k = len(used)
        slot_words += (all(x == 0xFFFFFFFF for x in words[k:]) and len(set(used)) == k and all(x < 192 for x in used))
        vo, nv = h + 16, vb // st
        lo = [struct.unpack_from('<H', d, vo + st * i + st - 4)[0] for i in range(nv)]
        hi = [struct.unpack_from('<H', d, vo + st * i + st - 2)[0] for i in range(nv)]
        vslots += set(lo) == set(range(k))
        vboxes += set(hi) == set(range(n))
        bx = [struct.unpack_from('<6f', d, 0x14 + 24 * j) for j in range(n)]
        fmt = '<3f' if st == 16 else '<3e'
        inside += all(hi[i] < n and all(bx[hi[i]][a] - 1e-2 <= struct.unpack_from(fmt, d, vo + st * i)[a]
                                        <= bx[hi[i]][3 + a] + 1e-2 for a in range(3)) for i in range(nv))
    out.update(soot_layout=layout, soot_version=sorted(versions), soot_stride=dict(sorted(stride.items())),
               soot_boxes=sorted(boxes), soot_index_ok=index_ok, soot_tail=tail,
               soot_flags=dict(sorted(flags.items())), soot_slot_words=slot_words, soot_vertex_slots=vslots,
               soot_vertex_boxes=vboxes, soot_vertex_in_box=inside)


def anim_trees(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """ATREE-*: {u32 version, u32 base address, u32 sizes of blocks A, B, C}, the blocks; a word in
    [base, base + blocks] is a pointer into them."""
    versions = set()
    layout = aligned = inside = 0
    for _, d in items:
        ver, base, a, b, c = struct.unpack_from('<5I', d, 0)
        versions.add(ver)
        layout += 20 + a + b + c == len(d)
        aligned += a % 16 == 0 and b % 16 == 0 and c % 16 == 0
        words = struct.unpack_from(f'<{len(d) // 4}I', d, 0)[5:]
        rel = [x - base for x in words if base <= x < base + len(d)]
        inside += all(r % 4 == 0 and r <= a + b + c for r in rel)
    out.update(atree_version=sorted(versions), atree_layout=layout, atree_aligned=aligned,
               atree_pointers_inside=inside)
    anim_tree_blocks(items, out)
    anim_tree_fields(items, out)


# ATREE-BLK-1 - 3: the loader's relocation walkers (BFBC2Game.exe 0xF598E0 block A, 0xF599F0 block B, 0xF59A30
# block C): the node types whose records hold pointers, and where (0xF59730 - 0xF598C0 by the type map at 0xF599D4).
ATREE_NODE_POINTERS = {2: (0x20, 0x28, 0x30, 0x38), 3: (0x20, 0x28, 0x30, 0x38), 4: (0x24,), 6: (0x20, 0x28),
                       7: (0x20,), 13: (0x20,), 16: (0x110,), 23: (0x20, 0x28), 25: (0x20,)}
ATREE_INDIRECT = {2: 0x38, 16: 0x110}    # the record's pointer leads to one more pointer the walker relocates
ATREE_C_RECORD = 0x60
# ATREE-NODE-1: the executable's names of the node types (BFBC2Game.exe build 795745, PE time stamp 0x4E55445D: the
# function 0xF490A0 returns, for type t = 1 - 26, `mov eax, <name>` through its jump table 0xF4915C).
ATREE_BUILD = 0x4E55445D
ATREE_TYPE_NAMES_FN = 0xF4915C
ATREE_TYPE_NAMES = ('AnimationNodeInfo', 'IfNodeInfo', 'SelectorNodeInfo', 'WeightFactorNodeInfo', 'AimNodeInfo',
                    'EnumeratedSwitchNodeInfo', 'AutoEndingEnumeratedSwitchNodeInfo', 'TripleStateNodeInfo',
                    'TimeSynchronizerNodeInfo', 'TimeSynchronizerToParentNodeInfo', 'MovementNodeInfo',
                    'Movement4WayNodeInfo', 'TransitionNodeInfo', 'PingPongNodeInfo', 'AutoEndingLoopNodeInfo',
                    'WeaponHandsIkNodeInfo', 'GenericHandsIkNodeInfo', 'BoneToBoneIkNodeInfo',
                    'TimeControllerNodeInfo', 'ProceduralLeaningNodeInfo', 'LerpNodeInfo', 'TurnNodeInfo',
                    'RandomSelectorNodeInfo', 'SelectorByPoseNodeInfo', 'XorSelectorNodeInfo', 'RagdollNodeInfo')


class _Tree:
    """A compiled tree's blocks: (file offset, offset from block A, size) each, with its base address."""

    def __init__(self, d: bytes) -> None:
        self.d = d
        _, self.base, a, b, c = struct.unpack_from('<5I', d, 0)
        self.blocks = ((20, 0, a), (20 + a, a, b), (20 + a + b, a + b, c))

    def u(self, o: int) -> int:
        return int(struct.unpack_from('<I', self.d, o)[0])

    def own(self, w: int, blk: int) -> bool:
        """w is 0 or points into block blk or at its end."""
        _, lo, size = self.blocks[blk]
        return w == 0 or self.base + lo <= w <= self.base + lo + size

    def at(self, w: int, blk: int) -> int:
        """The file offset a pointer into block blk names."""
        start, lo, _ = self.blocks[blk]
        return start + w - self.base - lo


def anim_tree_blocks(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """ATREE-BLK-*: block A starts with (node table, node count), each node a u32 type 1 - 26 and pointers at the
    type's places; block B starts with one pointer; block C with (record table, count) of 0x60-byte records. Every
    pointer is 0 or points into its own block or at its end. Counts and types only."""
    head = nodes = b_head = c_recs = 0
    in_order = b_entries = anim_index = 0
    lengths: list[float] = []
    types: Counter[int] = Counter()
    for _, d in items:
        tr = _Tree(d)
        u, own, at, base = tr.u, tr.own, tr.at, tr.base
        a, b, c = (size for _s, _l, size in tr.blocks)
        table, n = u(20), u(24)
        ok = own(table, 0) and (n == 0 or table != 0) and at(table, 0) + 4 * n <= 20 + a
        all_nodes = ok
        for i in range(n if ok else 0):
            p = u(at(table, 0) + 4 * i)
            if not base <= p < base + a:
                all_nodes = False
                break
            no = at(p, 0)
            t = u(no)
            types[t] += 1
            for off in ATREE_NODE_POINTERS.get(t, ()):
                all_nodes &= own(u(no + off), 0)
            ind = ATREE_INDIRECT.get(t)
            if ind is not None and u(no + ind):
                all_nodes &= own(u(at(u(no + ind), 0)), 0)
        head += ok
        nodes += all_nodes
        # ATREE-BLK-4: the nodes lie in table order; block B is (pointer to B + 16, count, 0, 0) and one 16-byte entry
        # per animation node (type 1, its length first), block C one record per animation node; an animation node's
        # word +0x20 is its index among them.
        offs = [at(u(at(table, 0) + 4 * i), 0) for i in range(n)] if all_nodes else []
        in_order += offs == sorted(offs)
        anim = [o for o in offs if u(o) == 1]
        cn = u(24 + a + b) if c else 0
        head_b = 20 + a
        b_entries += (b == 16 + 16 * cn and len(anim) == cn and (cn == 0 or (u(head_b) == base + a + 16
                      and u(head_b + 4) == cn and u(head_b + 8) == 0 and u(head_b + 12) == 0)))
        anim_index += all(u(o + 0x20) == k for k, o in enumerate(anim))
        lengths += [struct.unpack_from('<f', d, head_b + 16 + 16 * k)[0] for k in range(cn if b else 0)]
        b_head += b == 0 or own(u(20 + a), 1)
        good = c > 0
        if good:
            rt, rn = u(20 + a + b), u(24 + a + b)
            good = own(rt, 2) and (rn == 0 or rt != 0)
            r0 = at(rt, 2) if rt else 0
            good &= r0 + ATREE_C_RECORD * rn <= 20 + a + b + c
            for k in range(rn if good else 0):
                r = r0 + ATREE_C_RECORD * k
                offs = [8 * j for j in range(min(u(r + 0x10), 2))]
                offs += [0x14 + 8 * j for j in range(min(u(r + 0x24), 2))]
                for off in offs + [0x28, 0x30, 0x38]:
                    good &= own(u(r + off), 2)
        c_recs += good
    out.update(atree_node_table=head, atree_nodes=nodes, atree_b_head=b_head, atree_c_records=c_recs,
               atree_node_types=(min(types), max(types), len(types)), atree_node_count=sum(types.values()),
               atree_type_counts=dict(sorted(types.items())), atree_nodes_in_order=in_order,
               atree_b_entries=b_entries, atree_anim_index=anim_index,
               atree_b_lengths=(len(lengths), round(min(lengths), 4), round(max(lengths), 4)) if lengths else None)


# ATREE-NODE-4 (round SPG2-rest6): the u16 words of each node type that name a child node (a node whose parent,
# its word +0x16, is this node); True: the word may also be 0xFFFF (no node).
ATREE_CHILDREN: dict[int, tuple[tuple[int, bool], ...]] = {
    2: ((0x3C, False), (0x3E, True)), 4: ((0x20, False),), 7: ((0x2C, True),),
    8: ((0x20, True), (0x22, True), (0x24, True), (0x26, True)), 9: ((0x2C, False),), 10: ((0x20, False),),
    11: tuple((k, False) for k in range(0x22, 0x2E, 2)), 12: tuple((k, False) for k in range(0x22, 0x2A, 2)),
    13: ((0x28, True),), 14: ((0x20, False), (0x22, False), (0x24, False)), 15: ((0x20, False), (0x22, True)),
    16: ((0xC4, False),), 17: ((0xC4, False),), 19: ((0x20, False),), 21: ((0x20, False), (0x22, False)),
    22: tuple((k, False) for k in range(0x20, 0x28, 2)), 24: ((0x70, False), (0x72, False)), 25: ((0x28, True),)}
# ATREE-BLK-6: a C record's five property curves: (mode word, curve count word or None for one curve, pointers)
ATREE_C_CURVES = ((0x4C, 0x10, (0x00, 0x08)), (0x40, 0x24, (0x14, 0x1C)), (0x44, None, (0x28,)),
                  (0x48, None, (0x30,)), (0x50, None, (0x38,)))


def anim_tree_fields(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """ATREE-NODE-2 - 4, ATREE-BLK-5, 6: the node header (index, parent, the enabled byte, the effect pairs), the child
    references, the animation node's flags, block B's cycle distance and angle, block C's curve modes. Counts only."""
    header = children = triple = c_ok = 0
    cyclic = [0, 0]
    modes: Counter[str] = Counter()
    dist: list[float] = []
    angle: list[float] = []
    fourth = 0
    for name, d in items:
        tr = _Tree(d)
        u = tr.u
        a, b, c = (size for _s, _l, size in tr.blocks)
        table, n = u(20), u(24)
        offs = [tr.at(u(tr.at(table, 0) + 4 * i), 0) for i in range(n)]
        ends = sorted(offs) + [20 + a]
        end = {o: ends[ends.index(o) + 1] for o in offs}
        parent = [u(o + 0x14) >> 16 for o in offs]
        h_ok = k_ok = t_ok = True
        for i, o in enumerate(offs):
            pairs = tr.at(u(o + 4), 0)
            h_ok &= (u(o + 0x14) & 0xFFFF == i and (parent[i] == 0xFFFF) == (i == 0) and (i == 0 or parent[i] < i)
                     and d[o + 0x1A] == 1 and d[o + 0x1B] == 0 and u(o + 0x1C) == 0
                     and u(o + 8) <= 3 and o + 0x20 <= pairs and pairs + 8 * u(o + 8) <= end[o])
            for k, may_none in ATREE_CHILDREN.get(u(o), ()):
                v = struct.unpack_from('<H', d, o + k)[0]
                k_ok &= (may_none and v == 0xFFFF) or (v < n and parent[v] == i)
            if u(o) == 1:
                t_ok &= u(o + 0x24) <= 2 and u(o + 0x28) <= 2 and u(o + 0x2C) in (0, 0xFFFFFFFF)
                if 'looping' in name.lower():
                    cyclic[0] += u(o + 0x24) == 1
                    cyclic[1] += 1
        header += h_ok
        children += k_ok
        triple += t_ok
        good = True
        if c:
            r0, rn = tr.at(u(20 + a + b), 2), u(24 + a + b)
            for k in range(rn):
                r = r0 + ATREE_C_RECORD * k
                for mode_at, count_at, ptrs in ATREE_C_CURVES:
                    m = u(r + mode_at)
                    modes[f'{m}@{mode_at:#x}'] += 1
                    curve = m in (3, 4)
                    cnt_ok = count_at is None or u(r + count_at) == (2 if curve else 0)
                    good &= m <= 4 and (m != 4 or mode_at == 0x50) and cnt_ok and all(
                        (u(r + p) != 0) == curve for p in ptrs)
        c_ok += good
        for k in range(u(20 + a + 4) if b else 0):
            e = 20 + a + 16 + 16 * k
            x, y = struct.unpack_from('<2f', d, e + 4)
            if x:
                dist.append(x)
            if y:
                angle.append(y)
            fourth += u(e + 12) != 0
    out.update(atree_node_header=header, atree_child_refs=children, atree_anim_flags=triple,
               atree_is_cyclic_looping=tuple(cyclic), atree_c_curves=c_ok, atree_c_modes=dict(sorted(modes.items())),
               atree_b_cycle=(len(dist), round(min(dist), 4), round(max(dist), 4), len(angle), round(min(angle), 4),
                              round(max(angle), 4), fourth) if dist and angle else None)


def atree_type_names(exe: Path) -> bool | None:
    """ATREE-NODE-1: the executable's name function gives ATREE_TYPE_NAMES for types 1 - 26 (None: another build)."""
    b = exe.read_bytes()
    pe = struct.unpack_from('<I', b, 0x3C)[0]
    if struct.unpack_from('<I', b, pe + 8)[0] != ATREE_BUILD:
        return None

    def off(va: int) -> int:                               # .text / .rdata: file offset = va - image base
        return va - 0x400000

    names = []
    for t in range(26):
        case = struct.unpack_from('<I', b, off(ATREE_TYPE_NAMES_FN + 4 * t))[0]
        if b[off(case)] != 0xB8:                           # mov eax, imm32
            return False
        s = off(struct.unpack_from('<I', b, off(case) + 1)[0])
        names.append(b[s:b.index(b'\x00', s)].decode('ascii', 'replace'))
    return tuple(names) == ATREE_TYPE_NAMES


def impulse_responses(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """WAVE-IR-1: big-endian 48-byte header, then ceil(frames / block) blocks of 32 + 4 x FFT-size bytes."""
    heads = set()
    layout = block_heads = dc = big = small = 0
    scales: Counter[int] = Counter()
    for _, d in items:
        magic, ver, ch, frames, rate, blk, blk2, fft, one = struct.unpack_from('>4s8I', d, 0)
        heads.add((magic.decode('latin-1'), ver, ch, rate, blk, blk2, fft, one))
        layout += 48 + math.ceil(frames / blk) * (32 + 2 * ch * fft) == len(d)
        # IR-PART-1: a partition is ch blocks of 16 + fft x 2 bytes: a u16 scale 2^k - 1, 14 zero bytes, fft / 2
        # big-endian int16 (re, im) pairs
        bs = 16 + 2 * fft
        for p in range(math.ceil(frames / blk)):
            for c in range(ch):
                o = 48 + p * ch * bs + c * bs
                sc = struct.unpack_from('>H', d, o)[0]
                block_heads += sc > 0 and (sc + 1) & sc == 0 and not any(d[o + 2:o + 16])
                scales[sc] += 1
                q = struct.unpack_from(f'>{fft}h', d, o + 16)
                dc += q[1] == 0
                if sc < 32767:
                    small += 1
                    big += max(map(abs, q)) > 16383
    out.update(ir_header=sorted(heads), ir_layout=layout, ir_block_heads=block_heads, ir_dc_imag_zero=dc,
               ir_scales=dict(sorted(scales.items())), ir_scaled_full=(big, small))


def ragdolls(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    out['ragdoll'] = sorted((len(d), d[:8] == bytes.fromhex('57e0e05710c0c010'), d[40:54].decode('latin-1'))
                            for _, d in items)


def bink(d: bytes, at: int) -> tuple[str, int, int, float, int, bool]:
    """BINK-HDR-1: signature, width, height, frame rate, audio tracks, whether the size word fits."""
    sig, size, frames, _largest, frames2, wd, ht, num, den, _flags, tracks = struct.unpack_from('<4s10I', d, at)
    return sig.decode('latin-1'), wd, ht, round(num / den, 2), tracks, frames == frames2 and size


def bink_resources(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    got: Counter[tuple[str, int, int, float, int]] = Counter()
    fits = 0
    for _, d in items:
        sig, wd, ht, fps, tracks, size = bink(d, 0)
        got[(sig, wd, ht, fps, tracks)] += 1
        fits += size + 8 == struct.unpack_from('<I', d, 64)[0]
    out.update(bink_res=dict(sorted(got.items())), bink_res_size=fits)


def movies(game: Path, out: dict[str, Any]) -> None:
    """BINK-OUT-1: the RES wrapper with both length words 0; BinkTexture carries 8 bytes more."""
    got: Counter[tuple[str, Any, str, int]] = Counter()
    size = zero = 0
    for p in sorted((game / 'Output' / 'win32' / 'movies').glob('**/*.res')):
        if not dice(p):
            continue
        with open(p, 'rb') as f:
            d = f.read(128)
        n = p.stat().st_size
        kind = d[4:56].rstrip(b'\0').decode('latin-1')
        at = 72 if kind == 'BinkTexture' else 64
        pre = struct.unpack_from('<2I', d, 64) if kind == 'BinkTexture' else None
        sig, _wd, _ht, _fps, tracks, sz = bink(d, at)
        got[(kind, pre, sig, tracks)] += 1
        size += sz + 8 + at == n
        zero += struct.unpack_from('<2I', d, 56) == (0, 0) and d[:4] == b'RES\x02'
    out.update(movies=dict(sorted(got.items(), key=str)), movies_size=size, movies_lengths_zero=zero)


def loose(game: Path, out: dict[str, Any]) -> None:
    """GAME-FILE-*: the aerial height map, database.dbmanifest, dfe, the patch record, the dialog logo."""
    aerial = []
    for p in sorted((game / 'Output' / 'win32' / 'levels').glob('*/aerialheightmap.res')):
        d = p.read_bytes()
        cell, w, h, x0, z0, _scale, _offset = struct.unpack_from('<fIIffff', d, 0)
        aerial.append((cell, w, h, x0, z0, 28 + 2 * w * h == len(d)))
    out['aerial'] = aerial
    root = (game / 'database.dbmanifest').read_bytes()
    m = re.search(rb'<([A-Za-z]+)[\s>]', root)
    same = 0
    copies = 0
    for b in (game / 'Dist' / 'win32' / 'build_overlay-00.fbrb', game / 'Package' / 'build_overlay-00.fbrb'):
        got = fbrb.read(b, ['database.dbmanifest'])
        copies += len(got)
        same += got.get('database.dbmanifest') == root
    out['dbmanifest'] = (m.group(1).decode() if m else '', copies, same)
    d = (game / 'dfe').read_bytes()
    c = Counter(d)
    out['dfe'] = (len(d), -sum(n / len(d) * math.log2(n / len(d)) for n in c.values()) > 7.5)
    x = (game / 'InstallationData.xml').read_text(encoding='latin-1')
    out['installation'] = sorted(set(re.findall(r'<([A-Za-z]+)>', x)) - {'InstallationData'})
    v = re.search(r'<CurrentVersion>(\d+)</CurrentVersion>', x)
    out['installation_version'] = v.group(1) if v else ''
    log = (game / 'UpdateHistory.log').read_text(encoding='latin-1').splitlines()
    out['update_history'] = [m2.groups() for line in log
                             for m2 in [re.search(r'/(patch)_(\d+)_(\d+)\s*$', line)] if m2]
    j = (game / 'dialoglogo128x128.jpg').read_bytes()
    sof = re.search(rb'\xff[\xc0\xc2]..\x08(..)(..)', j, re.S)
    out['jpeg'] = struct.unpack('>HH', sof.group(2) + sof.group(1)) if sof and j[:2] == b'\xff\xd8' else None
    exe = (game / 'BFBC2Game.exe').read_bytes()
    out['exe_names'] = {n: n.encode().lower() in exe.lower() for n in EXPECTED['exe_names']}


def analyse(game: Path) -> dict[str, Any]:
    items = entries(game)
    out: dict[str, Any] = {'n': {t: len(v) for t, v in sorted(items.items())}}
    occluders(items['OccluderMesh'], out)
    soots(items['SootMesh'], out)
    anim_trees(items['AnimTreeInfo'], out)
    out['atree_type_names'] = atree_type_names(game / 'BFBC2Game.exe')
    impulse_responses(items['ImpulseResponse'], out)
    ragdolls(items['RagdollResource'], out)
    bink_resources(items['BinkMemory'], out)
    movies(game, out)
    loose(game, out)
    return out


def checks(game: Path) -> list[tuple[bool, str]]:
    got = analyse(game)
    return [(got.get(k) == want, f'resources probe: {k} = {want!r} (got {got.get(k)!r})')
            for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    t = time.time()
    for k, v in sorted(analyse(game).items()):
        print(f'{k}: {v}')
    print(f'({time.time() - t:.1f} s)')


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    main()
