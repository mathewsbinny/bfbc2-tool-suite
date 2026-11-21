//! Kynapse path data (`pathdata.hpd`): the file's header, its sector table and each sector's route graph (nodes and
//! edges) with the sector's runtime-mesh record left as bytes ([`crate::kymesh`] reads that). A port of NavKit's
//! `bfbc2-auto-navkit/tools/hpdtool.py` (`parse` / `write`; layout in `research/KYNAPSE_SPEC.md`), round F0.
//!
//! The same values as the reference, in the same order, with every float's bits. Where the reference raises (not a
//! path-data file, a read past the end of the file or of a sector's block, a runtime mesh not where expected, blocks
//! that do not cover the file) this returns an [`Error`] and the Python reads the file itself, so the error stays the
//! reference's. A NaN anywhere is also left to the reference (its float conversion is the interpreter's).
//!
//! ```
//! use bc2_formats::hpd::{self, HpdFile, Sector};
//!
//! let mesh = b"KynogonRuntimeMesh\0\0rest";
//! let file = HpdFile {
//!     header: [0; 36],
//!     sectors: vec![Sector { id: 0x0102, bounds: [0.0; 6], reserved: 0, date: 100_007, time: 0, nodes: vec![],
//!                            edges: vec![], mesh }],
//! };
//! let bytes = hpd::write(&file)?;
//! assert_eq!(hpd::parse(&bytes)?.sectors[0].mesh, mesh);
//! assert_eq!(hpd::write(&hpd::parse(&bytes)?)?, bytes);
//! # Ok::<(), hpd::Error>(())
//! ```

use crate::bytes::array;

/// The file's first 12 bytes.
pub const MAGIC: &[u8; 12] = b"Kynogon HPD\0";
/// The header's `u32` words after the magic.
pub const HEADER_WORDS: usize = 36;
/// The header's size in bytes.
pub const HEADER_SIZE: usize = 12 + HEADER_WORDS * 4;
/// A sector table entry's size.
pub const TABLE_ENTRY: usize = 48;
/// A route-graph node's size.
pub const NODE: usize = 36;
/// A route-graph edge's size.
pub const EDGE: usize = 12;
/// How every sector's runtime mesh starts.
pub const MESH_MARKER: &[u8] = b"KynogonRuntimeMesh";

/// Why a file is left to the reference reader (or cannot be written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not a Kynogon HPD file.
    Magic,
    /// A read past the end of the file or of a sector's block.
    Short,
    /// Sector `.0` (id `.1`): its runtime mesh is not where expected.
    Mesh(usize, u32),
    /// The sector blocks do not cover the file.
    Cover,
    /// A NaN value.
    Nan,
    /// A sector block's offset or size does not fit the table's `u32`.
    TooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => f.write_str("not a Kynogon HPD file"),
            Self::Short => f.write_str("a read past the end of the file"),
            Self::Mesh(i, sid) => write!(f, "sector {i} ({sid:#x}): runtime mesh not where expected"),
            Self::Cover => f.write_str("sector blocks do not cover the file"),
            Self::Nan => f.write_str("a NaN value"),
            Self::TooLarge => f.write_str("a sector block too large for the table"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

/// A route-graph node (`hpdtool.Node`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// Node id; the high bit marks a border node shared with a neighbouring sector.
    pub id: u32,
    /// World position, Y up.
    pub pos: [f32; 3],
    /// Always `0x8000_0000` so far.
    pub flags: u32,
    /// `DistEdgeMax` on a sector's first node, else 0.
    pub value: f32,
    /// The node's first outgoing edge + 1.
    pub a: u32,
    /// The node's entries in the lists sorted by x (low half) and y (high half).
    pub b: u32,
    /// The entry sorted by z (low half); bit 16: border node.
    pub c: u32,
}

/// A route-graph edge (`hpdtool.Edge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// The cost: `0x8000_0000 | floor(distance * 16)`.
    pub tag: u32,
    /// The source node.
    pub src: u32,
    /// The target node.
    pub dst: u32,
}

/// One sector (`hpdtool.Sector`); `mesh` borrows the runtime-mesh record.
#[derive(Debug, Clone, PartialEq)]
pub struct Sector<'a> {
    /// Grid coordinates, `0xXXYY`.
    pub id: u32,
    /// min x, max x, min y, max y, min z, max z.
    pub bounds: [f32; 6],
    /// Always 0 so far.
    pub reserved: u32,
    /// Build date `YYMMDD`, the month zero-based.
    pub date: u32,
    /// Build time `HHMMSScc`.
    pub time: u32,
    /// The route graph's nodes.
    pub nodes: Vec<Node>,
    /// The route graph's edges.
    pub edges: Vec<Edge>,
    /// The runtime-mesh record (starts with [`MESH_MARKER`]).
    pub mesh: &'a [u8],
}

/// A whole file (`hpdtool.HpdFile`).
#[derive(Debug, Clone, PartialEq)]
pub struct HpdFile<'a> {
    /// The 36 header words (word 2, the sector count, is rewritten by [`write`]).
    pub header: [u32; HEADER_WORDS],
    /// The sectors in table order.
    pub sectors: Vec<Sector<'a>>,
}

fn u32_at(d: &[u8], at: usize) -> Result<u32> {
    array::<4>(d, at).map(u32::from_le_bytes).ok_or(Error::Short)
}

fn f32_at(d: &[u8], at: usize) -> Result<f32> {
    let v = array::<4>(d, at).map(f32::from_le_bytes).ok_or(Error::Short)?;
    if v.is_nan() { Err(Error::Nan) } else { Ok(v) }
}

fn floats<const N: usize>(d: &[u8], at: usize) -> Result<[f32; N]> {
    let mut out = [0f32; N];
    for (k, o) in out.iter_mut().enumerate() {
        *o = f32_at(d, at + k * 4)?;
    }
    Ok(out)
}

fn node(b: &[u8], at: usize) -> Result<Node> {
    if at + NODE > b.len() {
        return Err(Error::Short);
    }
    Ok(Node {
        id: u32_at(b, at)?,
        pos: floats::<3>(b, at + 4)?,
        flags: u32_at(b, at + 16)?,
        value: f32_at(b, at + 20)?,
        a: u32_at(b, at + 24)?,
        b: u32_at(b, at + 28)?,
        c: u32_at(b, at + 32)?,
    })
}

/// `hpdtool.parse(data)`.
///
/// # Errors
/// Where the reference raises, or a value is a NaN.
pub fn parse(data: &[u8]) -> Result<HpdFile<'_>> {
    if data.get(..12) != Some(MAGIC.as_slice()) {
        return Err(Error::Magic);
    }
    if data.len() < HEADER_SIZE {
        return Err(Error::Short);
    }
    let mut header = [0u32; HEADER_WORDS];
    for (k, h) in header.iter_mut().enumerate() {
        *h = u32_at(data, 12 + k * 4)?;
    }
    let count = header[2] as usize;
    let blocks = (count as u64) * (TABLE_ENTRY as u64) + HEADER_SIZE as u64;
    let mut sectors = Vec::with_capacity(count.min(data.len() / TABLE_ENTRY));
    let mut covered = 0u64;
    for i in 0..count {
        let e = HEADER_SIZE + i * TABLE_ENTRY;
        if e + TABLE_ENTRY > data.len() {
            return Err(Error::Short);
        }
        let id = u32_at(data, e)?;
        let bounds = floats::<6>(data, e + 4)?;
        let reserved = u32_at(data, e + 28)?;
        let size = u64::from(u32_at(data, e + 32)?);
        let offset = u64::from(u32_at(data, e + 36)?);
        let (built_on, built_at) = (u32_at(data, e + 40)?, u32_at(data, e + 44)?);
        // data[start:start + size], cut at the file's end as a slice is
        let start = (blocks + offset).min(data.len() as u64) as usize;
        let end = (blocks + offset + size).min(data.len() as u64) as usize;
        let block = data.get(start..end).unwrap_or(&[]);
        let (n_nodes, n_edges) = (u32_at(block, 0)? as usize, u32_at(block, 4)? as usize);
        if n_nodes.saturating_mul(NODE) > block.len() || n_edges.saturating_mul(EDGE) > block.len() {
            return Err(Error::Short);
        }
        let mut pos = 8;
        let mut nodes = Vec::with_capacity(n_nodes);
        for _ in 0..n_nodes {
            nodes.push(node(block, pos)?);
            pos += NODE;
        }
        let mut edges = Vec::with_capacity(n_edges);
        for _ in 0..n_edges {
            if pos + EDGE > block.len() {
                return Err(Error::Short);
            }
            edges.push(Edge {
                tag: u32_at(block, pos)?,
                src: u32_at(block, pos + 4)?,
                dst: u32_at(block, pos + 8)?,
            });
            pos += EDGE;
        }
        let mesh = block.get(pos..).unwrap_or(&[]);
        if !mesh.starts_with(MESH_MARKER) {
            return Err(Error::Mesh(i, id));
        }
        covered += block.len() as u64;
        sectors.push(Sector {
            id,
            bounds,
            reserved,
            date: built_on,
            time: built_at,
            nodes,
            edges,
            mesh,
        });
    }
    if blocks + covered != data.len() as u64 {
        return Err(Error::Cover);
    }
    Ok(HpdFile { header, sectors })
}

/// Where [`parse_exact`] stops: the reference's own error at the same read (round RT3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactError {
    /// `ValueError('not a Kynogon HPD file')`.
    Magic,
    /// `ValueError`: sector `.0` (id `.1`), its runtime mesh not where expected.
    Mesh(usize, u32),
    /// `ValueError('sector blocks do not cover the file')`.
    Cover,
    /// `struct.error`: `size` bytes at `offset` of a buffer of `len` (the file, or a sector's block).
    Struct {
        /// The bytes the read takes (144 the header, 48 a table entry, 8 the counts, 36 a node, 12 an edge).
        size: u64,
        /// Where the read starts.
        offset: u64,
        /// The buffer's length.
        len: u64,
    },
}

impl ExactError {
    /// Whether this is the reference's `struct.error` (else its `ValueError`).
    #[must_use]
    pub fn is_struct(&self) -> bool {
        matches!(self, Self::Struct { .. })
    }
}

impl std::fmt::Display for ExactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => Error::Magic.fmt(f),
            Self::Mesh(i, sid) => Error::Mesh(*i, *sid).fmt(f),
            Self::Cover => Error::Cover.fmt(f),
            // CPython 3.13's words (Modules/_struct.c, unpack_from)
            Self::Struct { size, offset, len } => write!(
                f,
                "unpack_from requires a buffer of at least {} bytes for unpacking {size} bytes at offset {offset} \
                 (actual buffer size is {len})",
                size + offset
            ),
        }
    }
}

impl std::error::Error for ExactError {}

/// `struct.unpack_from` of `size` bytes at `at` (never negative here): the bytes, or the reference's `struct.error`.
fn unpack(d: &[u8], at: u64, size: u64) -> std::result::Result<&[u8], ExactError> {
    let len = d.len() as u64;
    if len < at || len - at < size {
        return Err(ExactError::Struct { size, offset: at, len });
    }
    // within the buffer: both ends are at most its length
    Ok(d.get(at as usize..(at + size) as usize).unwrap_or(&[]))
}

fn word(b: &[u8], k: usize) -> u32 {
    array::<4>(b, 4 * k).map_or(0, u32::from_le_bytes)
}

fn float(b: &[u8], k: usize) -> f32 {
    f32::from_bits(word(b, k))
}

/// `hpdtool.parse(data)` as the reference reads it, read for read (round RT3, so `parse_reference` could be retired):
/// every float as the file holds it (a NaN too: the caller converts it as CPython's `struct` does,
/// [`crate::hpdlayout::c_double`]), and where the reference raises, its error at the same read
/// ([`ExactError`]: `ValueError` or `struct.error` in CPython's words).
///
/// # Errors
/// Where the reference raises.
pub fn parse_exact(data: &[u8]) -> std::result::Result<HpdFile<'_>, ExactError> {
    if data.get(..12) != Some(MAGIC.as_slice()) {
        return Err(ExactError::Magic);
    }
    let h = unpack(data, 12, (HEADER_WORDS * 4) as u64)?;
    let mut header = [0u32; HEADER_WORDS];
    for (k, w) in header.iter_mut().enumerate() {
        *w = word(h, k);
    }
    let count = u64::from(header[2]);
    let blocks = HEADER_SIZE as u64 + count * TABLE_ENTRY as u64;
    let len = data.len() as u64;
    let mut sectors = Vec::with_capacity((count as usize).min(data.len() / TABLE_ENTRY));
    let mut covered = 0u64;
    for i in 0..count {
        let e = unpack(data, HEADER_SIZE as u64 + i * TABLE_ENTRY as u64, TABLE_ENTRY as u64)?;
        let id = word(e, 0);
        let mut bounds = [0f32; 6];
        for (k, b) in bounds.iter_mut().enumerate() {
            *b = float(e, 1 + k);
        }
        let (reserved, size, offset, built_on, built_at) = (
            word(e, 7),
            u64::from(word(e, 8)),
            u64::from(word(e, 9)),
            word(e, 10),
            word(e, 11),
        );
        // data[start:start + size], cut at the file's end as a slice is
        let start = (blocks + offset).min(len) as usize;
        let end = (blocks + offset + size).min(len) as usize;
        let block = data.get(start..end).unwrap_or(&[]);
        let counts = unpack(block, 0, 8)?;
        let (n_nodes, n_edges) = (u64::from(word(counts, 0)), u64::from(word(counts, 1)));
        let blen = block.len() as u64;
        let node_end = 8 + n_nodes * NODE as u64;
        let pos = node_end + n_edges * EDGE as u64;
        if pos > blen {
            // the reference reads one record at a time here: the first that does not fit raises
            if node_end > blen {
                let k = (blen.saturating_sub(8) / NODE as u64).min(n_nodes);
                return Err(ExactError::Struct {
                    size: NODE as u64,
                    offset: 8 + k * NODE as u64,
                    len: blen,
                });
            }
            let k = (blen - node_end) / EDGE as u64;
            return Err(ExactError::Struct {
                size: EDGE as u64,
                offset: node_end + k * EDGE as u64,
                len: blen,
            });
        }
        let nodes = (0..n_nodes as usize)
            .map(|k| {
                let r = block.get(8 + k * NODE..8 + (k + 1) * NODE).unwrap_or(&[]);
                Node {
                    id: word(r, 0),
                    pos: [float(r, 1), float(r, 2), float(r, 3)],
                    flags: word(r, 4),
                    value: float(r, 5),
                    a: word(r, 6),
                    b: word(r, 7),
                    c: word(r, 8),
                }
            })
            .collect();
        let edges = (0..n_edges as usize)
            .map(|k| {
                let at = node_end as usize + k * EDGE;
                let r = block.get(at..at + EDGE).unwrap_or(&[]);
                Edge {
                    tag: word(r, 0),
                    src: word(r, 1),
                    dst: word(r, 2),
                }
            })
            .collect();
        let mesh = block.get(pos as usize..).unwrap_or(&[]);
        if !mesh.starts_with(MESH_MARKER) {
            return Err(ExactError::Mesh(i as usize, id));
        }
        covered += blen;
        sectors.push(Sector {
            id,
            bounds,
            reserved,
            date: built_on,
            time: built_at,
            nodes,
            edges,
            mesh,
        });
    }
    if blocks + covered != len {
        return Err(ExactError::Cover);
    }
    Ok(HpdFile { header, sectors })
}

fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn putf(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// A sector's block: the counts, the nodes, the edges and the mesh (`hpdtool._block`).
fn block_into(out: &mut Vec<u8>, s: &Sector<'_>) -> Result<()> {
    put(out, u32::try_from(s.nodes.len()).map_err(|_| Error::TooLarge)?);
    put(out, u32::try_from(s.edges.len()).map_err(|_| Error::TooLarge)?);
    for n in &s.nodes {
        put(out, n.id);
        for &p in &n.pos {
            putf(out, p);
        }
        put(out, n.flags);
        putf(out, n.value);
        put(out, n.a);
        put(out, n.b);
        put(out, n.c);
    }
    for e in &s.edges {
        put(out, e.tag);
        put(out, e.src);
        put(out, e.dst);
    }
    out.extend_from_slice(s.mesh);
    Ok(())
}

/// The size of a sector's block.
fn block_len(s: &Sector<'_>) -> usize {
    8 + s.nodes.len() * NODE + s.edges.len() * EDGE + s.mesh.len()
}

/// `hpdtool.write(f)`: the header (its word 2 set to the sector count), the table and the blocks.
///
/// # Errors
/// [`Error::TooLarge`] where a block's size or offset, or the sector count, does not fit a `u32` (the reference's
/// `struct.error`).
pub fn write(f: &HpdFile<'_>) -> Result<Vec<u8>> {
    let total: usize = f.sectors.iter().map(block_len).sum();
    let mut out = Vec::with_capacity(HEADER_SIZE + f.sectors.len() * TABLE_ENTRY + total);
    out.extend_from_slice(MAGIC);
    for (k, &h) in f.header.iter().enumerate() {
        put(
            &mut out,
            if k == 2 {
                u32::try_from(f.sectors.len()).map_err(|_| Error::TooLarge)?
            } else {
                h
            },
        );
    }
    let mut offset = 0usize;
    for s in &f.sectors {
        let size = block_len(s);
        put(&mut out, s.id);
        for &b in &s.bounds {
            putf(&mut out, b);
        }
        put(&mut out, s.reserved);
        put(&mut out, u32::try_from(size).map_err(|_| Error::TooLarge)?);
        put(&mut out, u32::try_from(offset).map_err(|_| Error::TooLarge)?);
        put(&mut out, s.date);
        put(&mut out, s.time);
        offset += size;
    }
    for s in &f.sectors {
        block_into(&mut out, s)?;
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn sample() -> HpdFile<'static> {
        let n = |id: u32, x: f32| Node {
            id,
            pos: [x, 1.5, -x],
            flags: 0x8000_0000,
            value: if id == 0 { 15.0 } else { 0.0 },
            a: id + 1,
            b: id | (id << 16),
            c: id,
        };
        let mut header = [0u32; HEADER_WORDS];
        for (k, h) in header.iter_mut().enumerate() {
            *h = k as u32 * 3;
        }
        HpdFile {
            header,
            sectors: vec![
                Sector {
                    id: 0x0001,
                    bounds: [0.0, 64.0, -3.0, 9.5, 0.0, 64.0],
                    reserved: 0,
                    date: 100_007,
                    time: 12_345_678,
                    nodes: vec![n(0, 1.0), n(1, 2.0)],
                    edges: vec![
                        Edge {
                            tag: 0x8000_0010,
                            src: 0,
                            dst: 1,
                        },
                        Edge {
                            tag: 0x8000_0010,
                            src: 1,
                            dst: 0,
                        },
                    ],
                    mesh: b"KynogonRuntimeMesh\0\0abcd",
                },
                Sector {
                    id: 0x0100,
                    bounds: [64.0, 128.0, -3.0, 9.5, 0.0, 64.0],
                    reserved: 0,
                    date: 100_007,
                    time: 1,
                    nodes: vec![],
                    edges: vec![],
                    mesh: b"KynogonRuntimeMesh",
                },
            ],
        }
    }

    #[test]
    fn round_trip() {
        let f = sample();
        let bytes = write(&f).unwrap();
        let back = parse(&bytes).unwrap();
        assert_eq!(back.header[2], 2);
        let mut want = f.clone();
        want.header[2] = 2;
        assert_eq!(back, want);
        assert_eq!(write(&back).unwrap(), bytes);
    }

    #[test]
    fn refusals() {
        let bytes = write(&sample()).unwrap();
        assert_eq!(parse(b"Kynogon HPD"), Err(Error::Magic));
        assert_eq!(parse(&bytes[..100]), Err(Error::Short));
        for cut in [HEADER_SIZE, HEADER_SIZE + 40, bytes.len() - 1] {
            assert!(parse(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        // the table's sizes bound the blocks: a byte past the last one is not covered
        assert_eq!(parse(&longer), Err(Error::Cover));
        let mut bad = bytes.clone();
        let first_mesh = HEADER_SIZE + 2 * TABLE_ENTRY + 8 + 2 * NODE + 2 * EDGE;
        bad[first_mesh] = b'X';
        assert_eq!(parse(&bad), Err(Error::Mesh(0, 1)));
        let mut nan = bytes;
        nan[HEADER_SIZE + 4..HEADER_SIZE + 8].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(parse(&nan), Err(Error::Nan));
    }

    #[test]
    fn exact_errors_and_nans() {
        let bytes = write(&sample()).unwrap();
        assert_eq!(parse_exact(&bytes).unwrap(), parse(&bytes).unwrap());
        assert_eq!(
            parse_exact(&bytes[..100]).unwrap_err().to_string(),
            "unpack_from requires a buffer of at least 156 bytes for unpacking 144 bytes at offset 12 (actual buffer \
             size is 100)"
        );
        // a node record cut short: the first node that does not fit, at its offset in the sector's block
        let first_block = HEADER_SIZE + 2 * TABLE_ENTRY;
        let mut cut = bytes.clone();
        cut[first_block..first_block + 4].copy_from_slice(&5u32.to_le_bytes());
        let e = parse_exact(&cut).unwrap_err();
        assert!(e.is_struct(), "{e}");
        assert!(e.to_string().contains("for unpacking 36 bytes at offset"), "{e}");
        let mut nan = bytes.clone();
        nan[HEADER_SIZE + 4..HEADER_SIZE + 8].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse_exact(&nan).unwrap().sectors[0].bounds[0].is_nan());
        assert_eq!(parse_exact(b"nope").unwrap_err(), ExactError::Magic);
    }

    #[test]
    fn display() {
        assert_eq!(
            Error::Mesh(3, 0x102).to_string(),
            "sector 3 (0x102): runtime mesh not where expected"
        );
    }
}
