//! zlib's deflate, byte for byte: a port of the compressor of madler zlib 1.3.1 (`deflate.c`: `fill_window`,
//! `longest_match`, `deflate_slow`; `trees.c`: the Huffman trees and the block writer) for the lazy-matching levels
//! 4-9, a 32 KB window and memLevel 8 (`deflateInit2(level, Z_DEFLATED, ±15, 8, Z_DEFAULT_STRATEGY)`).
//!
//! Why a port and not a library: a bundle FbrbTool packs is gzip from .NET Framework's `GZipStream`, which is zlib
//! at level 6, memLevel 8 (OPFOR's `fbrbedit.py` writes the same bytes with Python's `zlib`), and the suite's packer
//! must write the same bytes. The Rust deflaters (`zlib-rs`, a port of zlib-ng, and `miniz_oxide`) compress well but
//! choose other matches and blocks. zlib's output depends on every detail of its match finder and block
//! splitting, so this is a line-by-line port: the hash (15 bits, shift 5), the hash chains and their slide, the
//! lazy evaluation (`TOO_FAR`), `longest_match`'s comparison order (bytes past the input's end are part of it: they
//! read as zlib's window holds them, zeros where nothing was written yet and the old data after a slide), blocks
//! of 16,383 symbols, the choice between stored, fixed and dynamic blocks, and the trees as `trees.c` builds them
//! (heap order, depth ties, the bit-length overflow repair, the shared `Freq` / `Code` and `Dad` / `Len` fields).
//!
//! Proven against Python's `zlib` (madler zlib 1.3.1) on every level 4-9 and many inputs
//! (`core/tests/test_fbrb_pack_native.py`) and against FbrbTool on every bundle of the game copy.
//!
//! ```
//! let data = b"Battlefield: Bad Company 2, Battlefield: Bad Company 2".repeat(40);
//! let z = bc2_formats::zlib::deflate(&data, 6).expect("a lazy level");
//! assert!(z.len() < data.len() / 10);
//! assert_eq!(bc2_formats::zlib::deflate(&[], 6), Some(vec![0x03, 0x00]));
//! ```
// Every index below is bounded by the window's invariants (zlib's: `strstart <= 2 * W_SIZE - MIN_LOOKAHEAD`, hash
// values masked, positions below 2^16, tree indices below the fixed table sizes), not by the data compressed.
#![allow(clippy::indexing_slicing)]

use std::sync::OnceLock;

const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MIN_LOOKAHEAD: usize = MAX_MATCH + MIN_MATCH + 1;
const W_SIZE: usize = 1 << 15;
const W_MASK: usize = W_SIZE - 1;
const WINDOW_SIZE: usize = 2 * W_SIZE;
const MAX_DIST: usize = W_SIZE - MIN_LOOKAHEAD;
const HASH_SIZE: usize = 1 << 15; // memLevel 8 + 7 bits
const HASH_MASK: usize = HASH_SIZE - 1;
const HASH_SHIFT: usize = 5; // (hash_bits + MIN_MATCH - 1) / MIN_MATCH
const LIT_BUFSIZE: usize = 1 << 14; // 1 << (memLevel + 6)
const SYM_END: usize = (LIT_BUFSIZE - 1) * 3;
const TOO_FAR: usize = 4096;

const LENGTH_CODES: usize = 29;
const LITERALS: usize = 256;
const L_CODES: usize = LITERALS + 1 + LENGTH_CODES;
const D_CODES: usize = 30;
const BL_CODES: usize = 19;
const HEAP_SIZE: usize = 2 * L_CODES + 1;
const MAX_BITS: usize = 15;
const MAX_BL_BITS: usize = 7;
const END_BLOCK: usize = 256;
const REP_3_6: usize = 16;
const REPZ_3_10: usize = 17;
const REPZ_11_138: usize = 18;

const EXTRA_LBITS: [u8; LENGTH_CODES] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const EXTRA_DBITS: [u8; D_CODES] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];
const EXTRA_BLBITS: [u8; BL_CODES] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7];
const BL_ORDER: [usize; BL_CODES] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// One tree node as `trees.c`'s `ct_data`: `fc` is `Freq` and later `Code`, `dl` is `Dad` and later `Len` (the
/// unions are kept: zlib reads a field the other name wrote, e.g. `scan_tree`'s guard).
#[derive(Clone, Copy, Default)]
struct Ct {
    fc: u16,
    dl: u16,
}

/// `trees.c`'s static tables (`tr_static_init`).
struct Static {
    ltree: [Ct; L_CODES + 2],
    dtree: [Ct; D_CODES],
    dist_code: [u8; 512],
    length_code: [u8; 256],
    base_length: [u16; LENGTH_CODES],
    base_dist: [u16; D_CODES],
}

fn bi_reverse(mut code: u32, len: u32) -> u16 {
    let mut res = 0u32;
    for _ in 0..len {
        res = (res << 1) | (code & 1);
        code >>= 1;
    }
    res as u16
}

/// `gen_codes`: the codes of `tree[..=max_code]` from their lengths and the count of each length.
fn gen_codes(tree: &mut [Ct], max_code: usize, bl_count: &[u16; MAX_BITS + 1]) {
    let mut next_code = [0u16; MAX_BITS + 1];
    let mut code = 0u32;
    for bits in 1..=MAX_BITS {
        code = (code + u32::from(bl_count[bits - 1])) << 1;
        next_code[bits] = code as u16;
    }
    for n in tree.iter_mut().take(max_code + 1) {
        let len = usize::from(n.dl);
        if len == 0 {
            continue;
        }
        n.fc = bi_reverse(u32::from(next_code[len]), len as u32);
        next_code[len] = next_code[len].wrapping_add(1);
    }
}

fn statics() -> &'static Static {
    static S: OnceLock<Static> = OnceLock::new();
    S.get_or_init(|| {
        let mut s = Static {
            ltree: [Ct::default(); L_CODES + 2],
            dtree: [Ct::default(); D_CODES],
            dist_code: [0; 512],
            length_code: [0; 256],
            base_length: [0; LENGTH_CODES],
            base_dist: [0; D_CODES],
        };
        let mut length = 0usize;
        for code in 0..LENGTH_CODES - 1 {
            s.base_length[code] = length as u16;
            for _ in 0..(1usize << EXTRA_LBITS[code]) {
                s.length_code[length] = code as u8;
                length += 1;
            }
        }
        // length 255 (match length 258) has two codes, 284 + 5 bits or 285: zlib uses 285
        s.length_code[length - 1] = (LENGTH_CODES - 1) as u8;
        let mut dist = 0usize;
        for code in 0..16 {
            s.base_dist[code] = dist as u16;
            for _ in 0..(1usize << EXTRA_DBITS[code]) {
                s.dist_code[dist] = code as u8;
                dist += 1;
            }
        }
        dist >>= 7;
        for code in 16..D_CODES {
            s.base_dist[code] = (dist << 7) as u16;
            for _ in 0..(1usize << (EXTRA_DBITS[code] - 7)) {
                s.dist_code[256 + dist] = code as u8;
                dist += 1;
            }
        }
        let mut bl_count = [0u16; MAX_BITS + 1];
        for (n, c) in s.ltree.iter_mut().enumerate() {
            let len = match n {
                0..=143 | 280.. => 8,
                144..=255 => 9,
                _ => 7,
            };
            c.dl = len;
            bl_count[usize::from(len)] += 1;
        }
        gen_codes(&mut s.ltree, L_CODES + 1, &bl_count);
        for (n, c) in s.dtree.iter_mut().enumerate() {
            *c = Ct {
                fc: bi_reverse(n as u32, 5),
                dl: 5,
            };
        }
        s
    })
}

fn d_code(st: &Static, dist: usize) -> usize {
    usize::from(if dist < 256 {
        st.dist_code[dist]
    } else {
        st.dist_code[256 + (dist >> 7)]
    })
}

/// The bit writer: zlib's `send_bits` / `bi_windup` give the same bytes as any least-significant-bit-first
/// writer padded with zeros where zlib aligns.
struct Bits {
    out: Vec<u8>,
    buf: u64,
    n: u32,
}

impl Bits {
    fn send(&mut self, value: u32, len: u32) {
        self.buf |= u64::from(value) << self.n;
        self.n += len;
        while self.n >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.n -= 8;
        }
    }

    fn code(&mut self, c: usize, tree: &[Ct]) {
        self.send(u32::from(tree[c].fc), u32::from(tree[c].dl));
    }

    fn windup(&mut self) {
        if self.n > 0 {
            self.out.push(self.buf as u8);
        }
        self.buf = 0;
        self.n = 0;
    }
}

/// What a tree build reads besides the tree: `tree_desc` and `static_tree_desc`.
struct Desc<'a> {
    stree: Option<&'a [Ct]>,
    extra: &'a [u8],
    extra_base: usize,
    elems: usize,
    max_length: usize,
}

/// The block state of `deflate_state` that `trees.c` uses.
struct Trees {
    dyn_ltree: [Ct; HEAP_SIZE],
    dyn_dtree: [Ct; 2 * D_CODES + 1],
    bl_tree: [Ct; 2 * BL_CODES + 1],
    l_max_code: usize,
    d_max_code: usize,
    bl_count: [u16; MAX_BITS + 1],
    heap: [usize; 2 * L_CODES + 1],
    heap_len: usize,
    heap_max: usize,
    depth: [u8; 2 * L_CODES + 1],
    sym_buf: Vec<u8>,
    // `ulg` in zlib, where an intermediate value can wrap below zero (the dummy nodes of a tree with fewer than two
    // codes) and the sum comes back: wrapping arithmetic gives the same result
    opt_len: u64,
    static_len: u64,
}

fn smaller(tree: &[Ct], n: usize, m: usize, depth: &[u8]) -> bool {
    tree[n].fc < tree[m].fc || (tree[n].fc == tree[m].fc && depth[n] <= depth[m])
}

impl Trees {
    fn new() -> Self {
        let mut t = Self {
            dyn_ltree: [Ct::default(); HEAP_SIZE],
            dyn_dtree: [Ct::default(); 2 * D_CODES + 1],
            bl_tree: [Ct::default(); 2 * BL_CODES + 1],
            l_max_code: 0,
            d_max_code: 0,
            bl_count: [0; MAX_BITS + 1],
            heap: [0; 2 * L_CODES + 1],
            heap_len: 0,
            heap_max: 0,
            depth: [0; 2 * L_CODES + 1],
            sym_buf: Vec::with_capacity(SYM_END),
            opt_len: 0,
            static_len: 0,
        };
        t.init_block();
        t
    }

    fn init_block(&mut self) {
        for n in &mut self.dyn_ltree[..L_CODES] {
            n.fc = 0;
        }
        for n in &mut self.dyn_dtree[..D_CODES] {
            n.fc = 0;
        }
        for n in &mut self.bl_tree[..BL_CODES] {
            n.fc = 0;
        }
        self.dyn_ltree[END_BLOCK].fc = 1;
        self.opt_len = 0;
        self.static_len = 0;
        self.sym_buf.clear();
    }

    /// `_tr_tally_lit`; true when the block is full.
    fn tally_lit(&mut self, c: u8) -> bool {
        self.sym_buf.extend_from_slice(&[0, 0, c]);
        self.dyn_ltree[usize::from(c)].fc += 1;
        self.sym_buf.len() == SYM_END
    }

    /// `_tr_tally_dist` of a match `dist` back, `len - MIN_MATCH` given; true when the block is full.
    fn tally_dist(&mut self, st: &Static, dist: usize, len: usize) -> bool {
        let d = dist as u16;
        self.sym_buf.extend_from_slice(&[d as u8, (d >> 8) as u8, len as u8]);
        self.dyn_ltree[usize::from(st.length_code[len & 0xFF]) + LITERALS + 1].fc += 1;
        self.dyn_dtree[d_code(st, usize::from(d.wrapping_sub(1)))].fc += 1;
        self.sym_buf.len() == SYM_END
    }

    fn pqdownheap(&mut self, tree: &[Ct], mut k: usize) {
        let v = self.heap[k];
        let mut j = k << 1;
        while j <= self.heap_len {
            if j < self.heap_len && smaller(tree, self.heap[j + 1], self.heap[j], &self.depth) {
                j += 1;
            }
            if smaller(tree, v, self.heap[j], &self.depth) {
                break;
            }
            self.heap[k] = self.heap[j];
            k = j;
            j <<= 1;
        }
        self.heap[k] = v;
    }

    fn gen_bitlen(&mut self, tree: &mut [Ct], max_code: usize, desc: &Desc<'_>) {
        let mut overflow = 0i32;
        self.bl_count = [0; MAX_BITS + 1];
        tree[self.heap[self.heap_max]].dl = 0; // the root
        let mut h = self.heap_max + 1;
        while h < HEAP_SIZE {
            let n = self.heap[h];
            let mut bits = usize::from(tree[usize::from(tree[n].dl)].dl) + 1;
            if bits > desc.max_length {
                bits = desc.max_length;
                overflow += 1;
            }
            tree[n].dl = bits as u16;
            h += 1;
            if n > max_code {
                continue; // not a leaf
            }
            self.bl_count[bits] += 1;
            let xbits = if n >= desc.extra_base {
                usize::from(desc.extra[n - desc.extra_base])
            } else {
                0
            };
            let f = u64::from(tree[n].fc);
            self.opt_len = self.opt_len.wrapping_add(f * (bits + xbits) as u64);
            if let Some(stree) = desc.stree {
                self.static_len = self
                    .static_len
                    .wrapping_add(f * (usize::from(stree[n].dl) + xbits) as u64);
            }
        }
        if overflow == 0 {
            return;
        }
        loop {
            let mut bits = desc.max_length - 1;
            while self.bl_count[bits] == 0 {
                bits -= 1;
            }
            self.bl_count[bits] -= 1;
            self.bl_count[bits + 1] += 2;
            self.bl_count[desc.max_length] -= 1;
            overflow -= 2;
            if overflow <= 0 {
                break;
            }
        }
        for bits in (1..=desc.max_length).rev() {
            let mut n = self.bl_count[bits];
            while n != 0 {
                h -= 1;
                let m = self.heap[h];
                if m > max_code {
                    continue;
                }
                if usize::from(tree[m].dl) != bits {
                    let delta = (bits as u64).wrapping_sub(u64::from(tree[m].dl));
                    self.opt_len = self.opt_len.wrapping_add(delta.wrapping_mul(u64::from(tree[m].fc)));
                    tree[m].dl = bits as u16;
                }
                n -= 1;
            }
        }
    }

    /// `build_tree`: the tree's code lengths and codes; returns its `max_code`.
    fn build_tree(&mut self, tree: &mut [Ct], desc: &Desc<'_>) -> usize {
        let elems = desc.elems;
        let mut max_code: isize = -1;
        self.heap_len = 0;
        self.heap_max = HEAP_SIZE;
        for n in 0..elems {
            if tree[n].fc != 0 {
                self.heap_len += 1;
                self.heap[self.heap_len] = n;
                max_code = n as isize;
                self.depth[n] = 0;
            } else {
                tree[n].dl = 0;
            }
        }
        // at least two codes of non-zero frequency, so a code has at least one bit
        while self.heap_len < 2 {
            let node = if max_code < 2 {
                max_code += 1;
                max_code as usize
            } else {
                0
            };
            self.heap_len += 1;
            self.heap[self.heap_len] = node;
            tree[node].fc = 1;
            self.depth[node] = 0;
            self.opt_len = self.opt_len.wrapping_sub(1);
            let static_bits = desc.stree.map_or(0, |stree| u64::from(stree[node].dl));
            self.static_len = self.static_len.wrapping_sub(static_bits);
        }
        let max_code = max_code as usize;
        for n in (1..=self.heap_len / 2).rev() {
            self.pqdownheap(tree, n);
        }
        let mut node = elems;
        loop {
            // pqremove
            let n = self.heap[1];
            self.heap[1] = self.heap[self.heap_len];
            self.heap_len -= 1;
            self.pqdownheap(tree, 1);
            let m = self.heap[1];
            self.heap_max -= 1;
            self.heap[self.heap_max] = n;
            self.heap_max -= 1;
            self.heap[self.heap_max] = m;
            tree[node].fc = tree[n].fc.wrapping_add(tree[m].fc);
            self.depth[node] = self.depth[n].max(self.depth[m]).wrapping_add(1);
            tree[n].dl = node as u16;
            tree[m].dl = node as u16;
            self.heap[1] = node;
            node += 1;
            self.pqdownheap(tree, 1);
            if self.heap_len < 2 {
                break;
            }
        }
        self.heap_max -= 1;
        self.heap[self.heap_max] = self.heap[1];
        self.gen_bitlen(tree, max_code, desc);
        let bl_count = self.bl_count;
        gen_codes(tree, max_code, &bl_count);
        max_code
    }

    /// `scan_tree`: the code-length codes' frequencies of `tree`.
    fn scan_tree(&mut self, tree: &mut [Ct], max_code: usize) {
        let mut prevlen: isize = -1;
        let mut nextlen = usize::from(tree[0].dl);
        let mut count = 0usize;
        let (mut max_count, mut min_count) = if nextlen == 0 { (138, 3) } else { (7, 4) };
        tree[max_code + 1].dl = 0xFFFF; // guard
        for n in 0..=max_code {
            let curlen = nextlen;
            nextlen = usize::from(tree[n + 1].dl);
            count += 1;
            if count < max_count && curlen == nextlen {
                continue;
            }
            if count < min_count {
                self.bl_tree[curlen].fc += count as u16;
            } else if curlen != 0 {
                if curlen as isize != prevlen {
                    self.bl_tree[curlen].fc += 1;
                }
                self.bl_tree[REP_3_6].fc += 1;
            } else if count <= 10 {
                self.bl_tree[REPZ_3_10].fc += 1;
            } else {
                self.bl_tree[REPZ_11_138].fc += 1;
            }
            count = 0;
            prevlen = curlen as isize;
            (max_count, min_count) = if nextlen == 0 {
                (138, 3)
            } else if curlen == nextlen {
                (6, 3)
            } else {
                (7, 4)
            };
        }
    }

    /// `send_tree`: `tree`'s code lengths in the code-length code (the guard is `scan_tree`'s).
    fn send_tree(&self, tree: &[Ct], max_code: usize, bits: &mut Bits) {
        let bl = &self.bl_tree;
        let mut prevlen: isize = -1;
        let mut nextlen = usize::from(tree[0].dl);
        let mut count = 0usize;
        let (mut max_count, mut min_count) = if nextlen == 0 { (138, 3) } else { (7, 4) };
        for n in 0..=max_code {
            let curlen = nextlen;
            nextlen = usize::from(tree[n + 1].dl);
            count += 1;
            if count < max_count && curlen == nextlen {
                continue;
            }
            if count < min_count {
                for _ in 0..count {
                    bits.code(curlen, bl);
                }
            } else if curlen != 0 {
                if curlen as isize != prevlen {
                    bits.code(curlen, bl);
                    count -= 1;
                }
                bits.code(REP_3_6, bl);
                bits.send((count - 3) as u32, 2);
            } else if count <= 10 {
                bits.code(REPZ_3_10, bl);
                bits.send((count - 3) as u32, 3);
            } else {
                bits.code(REPZ_11_138, bl);
                bits.send((count - 11) as u32, 7);
            }
            count = 0;
            prevlen = curlen as isize;
            (max_count, min_count) = if nextlen == 0 {
                (138, 3)
            } else if curlen == nextlen {
                (6, 3)
            } else {
                (7, 4)
            };
        }
    }

    fn compress_block(&self, st: &Static, ltree: &[Ct], dtree: &[Ct], bits: &mut Bits) {
        for &[d0, d1, lc] in self.sym_buf.as_chunks::<3>().0 {
            let dist = usize::from(d0) | (usize::from(d1) << 8);
            let lc = usize::from(lc);
            if dist == 0 {
                bits.code(lc, ltree);
                continue;
            }
            let code = usize::from(st.length_code[lc]);
            bits.code(code + LITERALS + 1, ltree);
            let extra = u32::from(EXTRA_LBITS[code]);
            if extra != 0 {
                bits.send((lc - usize::from(st.base_length[code])) as u32, extra);
            }
            let dist = dist - 1;
            let code = d_code(st, dist);
            bits.code(code, dtree);
            let extra = u32::from(EXTRA_DBITS[code]);
            if extra != 0 {
                bits.send((dist - usize::from(st.base_dist[code])) as u32, extra);
            }
        }
        bits.code(END_BLOCK, ltree);
    }

    /// `_tr_flush_block`: the block of the symbols tallied, `stored` the input it covers when it is still in the
    /// window (a stored block is possible only then).
    fn flush_block(&mut self, st: &Static, stored: Option<&[u8]>, stored_len: usize, last: bool, bits: &mut Bits) {
        let mut ltree = self.dyn_ltree;
        self.l_max_code = self.build_tree(
            &mut ltree,
            &Desc {
                stree: Some(&st.ltree),
                extra: &EXTRA_LBITS,
                extra_base: LITERALS + 1,
                elems: L_CODES,
                max_length: MAX_BITS,
            },
        );
        let mut dtree = self.dyn_dtree;
        self.d_max_code = self.build_tree(
            &mut dtree,
            &Desc {
                stree: Some(&st.dtree),
                extra: &EXTRA_DBITS,
                extra_base: 0,
                elems: D_CODES,
                max_length: MAX_BITS,
            },
        );
        // build_bl_tree
        self.scan_tree(&mut ltree, self.l_max_code);
        self.scan_tree(&mut dtree, self.d_max_code);
        let mut bl_tree = self.bl_tree;
        self.build_tree(
            &mut bl_tree,
            &Desc {
                stree: None,
                extra: &EXTRA_BLBITS,
                extra_base: 0,
                elems: BL_CODES,
                max_length: MAX_BL_BITS,
            },
        );
        self.bl_tree = bl_tree;
        let mut max_blindex = BL_CODES - 1;
        while max_blindex >= 3 && self.bl_tree[BL_ORDER[max_blindex]].dl == 0 {
            max_blindex -= 1;
        }
        self.opt_len = self.opt_len.wrapping_add(3 * (max_blindex as u64 + 1) + 5 + 5 + 4);
        self.dyn_ltree = ltree;
        self.dyn_dtree = dtree;

        let mut opt_lenb = self.opt_len.wrapping_add(3 + 7) >> 3;
        let static_lenb = self.static_len.wrapping_add(3 + 7) >> 3;
        if static_lenb <= opt_lenb {
            opt_lenb = static_lenb;
        }
        let last_bit = u32::from(last);
        match stored {
            Some(buf) if stored_len as u64 + 4 <= opt_lenb => {
                bits.send(last_bit, 3); // STORED_BLOCK << 1
                bits.windup();
                let len = stored_len as u16;
                bits.out.extend_from_slice(&len.to_le_bytes());
                bits.out.extend_from_slice(&(!len).to_le_bytes());
                bits.out.extend_from_slice(buf);
            }
            _ if static_lenb == opt_lenb => {
                bits.send(2 + last_bit, 3); // STATIC_TREES << 1
                self.compress_block(st, &st.ltree, &st.dtree, bits);
            }
            _ => {
                bits.send(4 + last_bit, 3); // DYN_TREES << 1
                let (lcodes, dcodes, blcodes) = (self.l_max_code + 1, self.d_max_code + 1, max_blindex + 1);
                bits.send((lcodes - 257) as u32, 5);
                bits.send((dcodes - 1) as u32, 5);
                bits.send((blcodes - 4) as u32, 4);
                for &o in &BL_ORDER[..blcodes] {
                    bits.send(u32::from(self.bl_tree[o].dl), 3);
                }
                self.send_tree(&self.dyn_ltree, lcodes - 1, bits);
                self.send_tree(&self.dyn_dtree, dcodes - 1, bits);
                self.compress_block(st, &self.dyn_ltree, &self.dyn_dtree, bits);
            }
        }
        self.init_block();
        if last {
            bits.windup();
        }
    }
}

/// `configuration_table`: `good_length`, `max_lazy`, `nice_length`, `max_chain` of the lazy levels.
#[derive(Clone, Copy)]
struct Config {
    good: usize,
    lazy: usize,
    nice: usize,
    chain: usize,
}

fn config(level: u8) -> Option<Config> {
    let (good, lazy, nice, chain) = match level {
        4 => (4, 4, 16, 16),
        5 => (8, 16, 32, 32),
        6 => (8, 16, 128, 128),
        7 => (8, 32, 128, 256),
        8 => (32, 128, 258, 1024),
        9 => (32, 258, 258, 4096),
        _ => return None,
    };
    Some(Config {
        good,
        lazy,
        nice,
        chain,
    })
}

/// `deflate_state`'s window and matcher.
struct State<'a> {
    input: &'a [u8],
    next_in: usize,
    window: Vec<u8>,
    prev: Vec<u16>,
    head: Vec<u16>,
    ins_h: usize,
    strstart: usize,
    block_start: isize,
    lookahead: usize,
    // zlib's `uInt`s: match_start can wrap below zero in a slide and is then only used in differences
    match_start: usize,
    match_length: usize,
    prev_length: usize,
    prev_match: usize,
    match_available: bool,
    cfg: Config,
    trees: Trees,
    bits: Bits,
    st: &'static Static,
}

/// The number of equal bytes at the start of `a` and `b` (as long), eight at a time.
fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    let (ca, ra) = a.as_chunks::<8>();
    let (cb, rb) = b.as_chunks::<8>();
    for (i, (x, y)) in ca.iter().zip(cb).enumerate() {
        let d = u64::from_le_bytes(*x) ^ u64::from_le_bytes(*y);
        if d != 0 {
            return 8 * i + (d.trailing_zeros() / 8) as usize;
        }
    }
    8 * ca.len() + ra.iter().zip(rb).take_while(|(x, y)| x == y).count()
}

fn update_hash(h: usize, c: u8) -> usize {
    ((h << HASH_SHIFT) ^ usize::from(c)) & HASH_MASK
}

impl State<'_> {
    /// `INSERT_STRING`: the string at `s` into its hash chain; returns the chain's previous head.
    fn insert_string(&mut self, s: usize) -> usize {
        self.ins_h = update_hash(self.ins_h, self.window[s + MIN_MATCH - 1]);
        let head = self.head[self.ins_h];
        self.prev[s & W_MASK] = head;
        self.head[self.ins_h] = s as u16;
        usize::from(head)
    }

    fn slide_hash(&mut self) {
        for p in self.head.iter_mut().chain(self.prev.iter_mut()) {
            let m = usize::from(*p);
            *p = if m >= W_SIZE { (m - W_SIZE) as u16 } else { 0 };
        }
    }

    /// `fill_window` (1.3.1) with the whole input available, as a one-shot compression has it:
    ///
    /// - its `high_water` zeroing is the zeroed window here (zlib zeroes only bytes no data was ever written to);
    /// - `s->insert` is 0 throughout (zlib sets it only when a `deflate` call ends, for the strings the next call
    ///   must hash), so the hash is started from the two bytes at `strstart`, as zlib starts it then;
    /// - its loop runs once: the read fills the window to its end, which leaves more than `MIN_LOOKAHEAD` bytes
    ///   ahead unless the input has ended.
    fn fill_window(&mut self) {
        let mut more = WINDOW_SIZE - self.lookahead - self.strstart;
        if self.strstart >= W_SIZE + MAX_DIST {
            // the upper half to the lower, up to the end of the data (what lies after it stays)
            self.window.copy_within(W_SIZE..2 * W_SIZE - more, 0);
            self.match_start = self.match_start.wrapping_sub(W_SIZE);
            self.strstart -= W_SIZE;
            self.block_start -= W_SIZE as isize;
            self.slide_hash();
            more += W_SIZE;
        }
        let n = more.min(self.input.len() - self.next_in);
        if n == 0 {
            return;
        }
        let at = self.strstart + self.lookahead;
        self.window[at..at + n].copy_from_slice(&self.input[self.next_in..self.next_in + n]);
        self.next_in += n;
        self.lookahead += n;
        if self.lookahead >= MIN_MATCH {
            self.ins_h = update_hash(usize::from(self.window[self.strstart]), self.window[self.strstart + 1]);
        }
    }

    /// `longest_match` (the portable version): the longest match at `strstart` along the hash chain from
    /// `cur_match`, longer than `prev_length`; sets `match_start`.
    fn longest_match(&mut self, mut cur_match: usize) -> usize {
        let w = &self.window;
        let mut chain_length = self.cfg.chain;
        let scan = self.strstart;
        let mut best_len = self.prev_length;
        let mut nice_match = self.cfg.nice;
        let limit = self.strstart.saturating_sub(MAX_DIST);
        let mut scan_end1 = w[scan + best_len - 1];
        let mut scan_end = w[scan + best_len];
        if self.prev_length >= self.cfg.good {
            chain_length >>= 2;
        }
        if nice_match > self.lookahead {
            nice_match = self.lookahead;
        }
        loop {
            let m = cur_match;
            // byte 2 is not compared: equal hashes and bytes 0 and 1 make it equal (zlib's own shortcut)
            if w[m + best_len] == scan_end
                && w[m + best_len - 1] == scan_end1
                && w[m] == w[scan]
                && w[m + 1] == w[scan + 1]
            {
                // bytes 3.. compared up to MAX_MATCH (zlib also compares byte 258, which cannot change the length)
                let len = 3 + common_prefix(&w[scan + 3..scan + MAX_MATCH], &w[m + 3..m + MAX_MATCH]);
                if len > best_len {
                    self.match_start = cur_match;
                    best_len = len;
                    if len >= nice_match {
                        break;
                    }
                    scan_end1 = w[scan + best_len - 1];
                    scan_end = w[scan + best_len];
                }
            }
            cur_match = usize::from(self.prev[cur_match & W_MASK]);
            if cur_match <= limit {
                break;
            }
            chain_length -= 1;
            if chain_length == 0 {
                break;
            }
        }
        best_len.min(self.lookahead)
    }

    fn flush_block(&mut self, last: bool) {
        let stored_len = (self.strstart as isize - self.block_start) as usize;
        let stored = usize::try_from(self.block_start)
            .ok()
            .map(|b| &self.window[b..b + stored_len]);
        self.trees
            .flush_block(self.st, stored, stored_len, last, &mut self.bits);
        self.block_start = self.strstart as isize;
    }

    /// `deflate_slow` run to the end of the input with `Z_FINISH` (the bytes are the same as zlib's `Z_NO_FLUSH`
    /// calls followed by `Z_FINISH`: the same decisions, only paused).
    fn deflate_slow(&mut self) {
        loop {
            if self.lookahead < MIN_LOOKAHEAD {
                self.fill_window();
                if self.lookahead == 0 {
                    break;
                }
            }
            let mut hash_head = 0;
            if self.lookahead >= MIN_MATCH {
                hash_head = self.insert_string(self.strstart);
            }
            self.prev_length = self.match_length;
            self.prev_match = self.match_start;
            self.match_length = MIN_MATCH - 1;
            if hash_head != 0 && self.prev_length < self.cfg.lazy && self.strstart - hash_head <= MAX_DIST {
                self.match_length = self.longest_match(hash_head);
                if self.match_length <= 5
                    && self.match_length == MIN_MATCH
                    && self.strstart.wrapping_sub(self.match_start) > TOO_FAR
                {
                    self.match_length = MIN_MATCH - 1;
                }
            }
            if self.prev_length >= MIN_MATCH && self.match_length <= self.prev_length {
                let max_insert = self.strstart + self.lookahead - MIN_MATCH;
                let dist = (self.strstart - 1).wrapping_sub(self.prev_match);
                let bflush = self.trees.tally_dist(self.st, dist, self.prev_length - MIN_MATCH);
                self.lookahead -= self.prev_length - 1;
                self.prev_length -= 2;
                loop {
                    self.strstart += 1;
                    if self.strstart <= max_insert {
                        self.insert_string(self.strstart);
                    }
                    self.prev_length -= 1;
                    if self.prev_length == 0 {
                        break;
                    }
                }
                self.match_available = false;
                self.match_length = MIN_MATCH - 1;
                self.strstart += 1;
                if bflush {
                    self.flush_block(false);
                }
            } else if self.match_available {
                if self.trees.tally_lit(self.window[self.strstart - 1]) {
                    self.flush_block(false);
                }
                self.strstart += 1;
                self.lookahead -= 1;
            } else {
                self.match_available = true;
                self.strstart += 1;
                self.lookahead -= 1;
            }
        }
        if self.match_available {
            self.trees.tally_lit(self.window[self.strstart - 1]);
            self.match_available = false;
        }
        self.flush_block(true);
    }
}

/// `data` as a raw deflate stream, the bytes zlib 1.3.1 writes for
/// `deflateInit2(level, Z_DEFLATED, -15, 8, Z_DEFAULT_STRATEGY)` (Python's `zlib.compressobj(level, DEFLATED, -15,
/// 8)`), or `None` for a level other than the lazy levels 4-9 (zlib's levels 1-3 use another matcher, not ported).
#[must_use]
pub fn deflate(data: &[u8], level: u8) -> Option<Vec<u8>> {
    let cfg = config(level)?;
    let mut s = State {
        input: data,
        next_in: 0,
        window: vec![0; WINDOW_SIZE],
        prev: vec![0; W_SIZE],
        head: vec![0; HASH_SIZE],
        ins_h: 0,
        strstart: 0,
        block_start: 0,
        lookahead: 0,
        match_start: 0,
        match_length: MIN_MATCH - 1,
        prev_length: MIN_MATCH - 1,
        prev_match: 0,
        match_available: false,
        cfg,
        trees: Trees::new(),
        bits: Bits {
            out: Vec::with_capacity(data.len() / 3 + 64),
            buf: 0,
            n: 0,
        },
        st: statics(),
    };
    s.deflate_slow();
    Some(s.bits.out)
}

/// `data` as a gzip member as .NET Framework's `GZipStream` writes it (what FbrbTool's bundles hold): the 10-byte
/// header `1F 8B 08 00 00000000 04 00` (no name, time 0, extra flags 4, OS 0), zlib's level-6 deflate, the CRC-32
/// and the length modulo 2^32. No data gives no bytes at all: `GZipStream` writes nothing when nothing was
/// written to it (FbrbTool's bundles of *deleted* markers only have an empty payload part).
#[must_use]
pub fn gzip_net(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 4, 0];
    out.extend(deflate(data, 6).unwrap_or_default());
    let mut crc = flate2::Crc::new();
    crc.update(data);
    out.extend(crc.sum().to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn inflate(z: &[u8]) -> Vec<u8> {
        use std::io::Read as _;
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(z).read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn known_outputs() {
        // zlib 1.3.1's bytes (Python: zlib.compressobj(6, zlib.DEFLATED, -15, 8))
        assert_eq!(deflate(b"", 6).unwrap(), [0x03, 0x00]);
        assert_eq!(deflate(b"a", 6).unwrap(), [0x4B, 0x04, 0x00]);
        assert_eq!(
            deflate(b"hello hello hello hello", 6).unwrap(),
            [0xCB, 0x48, 0xCD, 0xC9, 0xC9, 0x57, 0xC8, 0x40, 0x27, 0x01]
        );
        assert!(deflate(b"x", 3).is_none() && deflate(b"x", 10).is_none());
    }

    #[test]
    fn round_trips() {
        let mut seed = 0x1234_5678u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let noise: Vec<u8> = (0..200_000).map(|_| rnd() as u8).collect();
        let text: Vec<u8> = (0..300_000u32)
            .map(|i| b"abcdefgh"[(i.wrapping_mul(i) % 7) as usize])
            .collect();
        let runs: Vec<u8> = (0..150_000u32)
            .map(|i| if (i / 1000) % 2 == 0 { 0 } else { (i % 3) as u8 })
            .collect();
        // short tokens: nearly every symbol a match, so blocks fill on a match as well as on a literal
        let tokens: Vec<u8> = (0..60_000).flat_map(|_| (rnd() % 64).to_le_bytes()).collect();
        for data in [noise, text, runs, tokens, vec![7u8; 100_000]] {
            for level in 4..=9 {
                assert_eq!(inflate(&deflate(&data, level).unwrap()), data, "level {level}");
            }
        }
    }

    #[test]
    fn common_prefixes() {
        let a: Vec<u8> = (0..255u8).collect();
        for k in [0, 1, 7, 8, 9, 100, 248, 249, 254] {
            let mut b = a.clone();
            b[k] ^= 1;
            assert_eq!(common_prefix(&a, &b), k);
        }
        assert_eq!(common_prefix(&a, &a), 255);
    }

    #[test]
    fn gzip_frame() {
        assert_eq!(gzip_net(b""), Vec::<u8>::new());
        let z = gzip_net(b"abc");
        assert_eq!(&z[..10], &[0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 4, 0]);
        assert_eq!(&z[z.len() - 4..], &3u32.to_le_bytes());
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(z.as_slice()), &mut out).unwrap();
        assert_eq!(out, b"abc");
    }
}
