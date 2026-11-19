//! Generated aim poses (`AimAnimation` resources, `*_generatedaimanimation.res`): a port of
//! `bfbc2-anim-tools/aimanim.py` (`parse`, `write`, `renumbered`, `to_pc`), round F3. The layout is in that module's
//! docstring: a 0x1c-byte header (version 1, an address, the data size, the address + 0x10, the bone count, the pose
//! count), a table of `(bone, kind)` bytes, zeros to a 16-byte boundary counted from 0x0c, then per bone a fixed
//! (kind 0), rotation (kind 1) or moving (kind 2) block of 32-bit floats.
//!
//! The same values and the same bytes as the reference. Where the reference raises (an `AimError`, a read past the
//! end, a value `struct` cannot pack) this answers [`Refused`] and the Python runs, so every error stays its own. A
//! NaN read or written is refused too: `struct`'s conversion of a NaN between float widths is the interpreter's to
//! define.
//!
//! ```
//! use bc2_formats::aimanim::{self, Aim, Block};
//!
//! let a = Aim { addr: 0x1000, poses: 1, bones: vec![(3, 0, Block::Fixed([0.0, 1.0, 2.0, 1.0], [0.0, 0.0, 0.0, 1.0]))] };
//! let b = aimanim::write(&a)?;
//! assert_eq!(aimanim::parse(&b, false)?, a);
//! # Ok::<(), aimanim::Refused>(())
//! ```

/// The input is left to the Python reference (it raises, or a value is a NaN).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("left to the Python reference")
    }
}

impl std::error::Error for Refused {}

type Result<T> = std::result::Result<T, Refused>;

/// Four 32-bit floats (a position `x, y, z, w` or a quaternion `x, y, z, w`).
pub type Quad = [f32; 4];

/// A bone's block of poses.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Kind 0: a position and a rotation (`(pos4, quat4)`).
    Fixed(Quad, Quad),
    /// Kind 1: a position and a rotation per pose (`(pos4, [quat4])`).
    Rotation(Quad, Vec<Quad>),
    /// Kind 2 (any other kind when written): a position and a rotation per pose (`[(pos4, quat4)]`).
    Moving(Vec<(Quad, Quad)>),
}

/// An aim animation: `aimanim.parse`'s dict.
#[derive(Debug, Clone, PartialEq)]
pub struct Aim {
    /// `addr`: the build tool's address (word 0x04).
    pub addr: u32,
    /// `poses`: the pose count (byte 0x14).
    pub poses: u8,
    /// `bones`: `(skeleton bone index, kind, block)` in table order.
    pub bones: Vec<(u8, u8, Block)>,
}

/// `_data_at(nb)`: where the blocks start after a table of `nb` bones.
fn data_at(nb: usize) -> usize {
    let o = 0x1c + 2 * nb;
    o + (16 - (o - 0x0c) % 16) % 16
}

fn word(b: &[u8], at: usize, big: bool) -> Result<u32> {
    let w: [u8; 4] = b
        .get(at..at.checked_add(4).ok_or(Refused)?)
        .ok_or(Refused)?
        .try_into()
        .map_err(|_| Refused)?;
    Ok(if big {
        u32::from_be_bytes(w)
    } else {
        u32::from_le_bytes(w)
    })
}

fn quad(b: &[u8], at: usize, big: bool) -> Result<Quad> {
    let mut q = [0f32; 4];
    for (k, v) in q.iter_mut().enumerate() {
        *v = f32::from_bits(word(b, at + 4 * k, big)?);
        if v.is_nan() {
            return Err(Refused);
        }
    }
    Ok(q)
}

/// `aimanim.parse(b, big_endian)`.
///
/// # Errors
///
/// [`Refused`] where the reference raises, or a value is a NaN.
pub fn parse(b: &[u8], big_endian: bool) -> Result<Aim> {
    if b.len() < 0x1c {
        return Err(Refused);
    }
    let w = |at: usize| word(b, at, big_endian);
    let (ver, addr, size, addr2, nb) = (w(0)?, w(4)?, w(8)?, w(12)?, w(16)?);
    if ver != 1 || u64::from(size) != b.len() as u64 - 12 || addr2 != addr.wrapping_add(0x10) {
        return Err(Refused);
    }
    let n = *b.get(0x14).ok_or(Refused)?;
    let nb = usize::try_from(nb).map_err(|_| Refused)?;
    let table = b.get(0x1c..0x1c + 2 * nb).ok_or(Refused)?;
    let mut o = data_at(nb);
    let mut bones = Vec::with_capacity(nb);
    let poses = usize::from(n);
    for &[bone, kind] in table.as_chunks::<2>().0 {
        let block = match kind {
            0 => {
                let blk = Block::Fixed(quad(b, o, big_endian)?, quad(b, o + 16, big_endian)?);
                o += 32;
                blk
            }
            1 => {
                let pos = quad(b, o, big_endian)?;
                let rots = (0..poses)
                    .map(|k| quad(b, o + 16 + 16 * k, big_endian))
                    .collect::<Result<Vec<_>>>()?;
                o += 16 + 16 * poses;
                Block::Rotation(pos, rots)
            }
            2 => {
                let ps = (0..poses)
                    .map(|k| Ok((quad(b, o + 32 * k, big_endian)?, quad(b, o + 32 * k + 16, big_endian)?)))
                    .collect::<Result<Vec<_>>>()?;
                o += 32 * poses;
                Block::Moving(ps)
            }
            _ => return Err(Refused),
        };
        bones.push((bone, kind, block));
    }
    if o != b.len() {
        return Err(Refused);
    }
    Ok(Aim { addr, poses: n, bones })
}

fn put_quad(out: &mut Vec<u8>, q: &Quad) -> Result<()> {
    for v in q {
        if v.is_nan() {
            return Err(Refused);
        }
        out.extend_from_slice(&v.to_le_bytes());
    }
    Ok(())
}

/// `aimanim.write(a)`: the PC (little-endian) bytes. A bone's kind is written as it is; its block decides the layout
/// as the reference's branches do (kind 0 the fixed block, 1 the rotation block, any other the moving block), so a
/// block that does not match its kind is refused.
///
/// # Errors
///
/// [`Refused`] where the reference raises (a block of another pose count, a block that does not match its kind), or
/// a value is a NaN.
pub fn write(a: &Aim) -> Result<Vec<u8>> {
    let nb = a.bones.len();
    let n = usize::from(a.poses);
    let nb32 = u32::try_from(nb).map_err(|_| Refused)?;
    let mut out = Vec::with_capacity(data_at(nb) + 32 * n * nb.max(1));
    for v in [1, a.addr, 0, a.addr.wrapping_add(0x10), nb32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.push(a.poses);
    out.extend_from_slice(&[0; 7]);
    for (bone, kind, _) in &a.bones {
        out.push(*bone);
        out.push(*kind);
    }
    out.resize(data_at(nb), 0);
    for (_, kind, block) in &a.bones {
        match (kind, block) {
            (0, Block::Fixed(p, q)) => {
                put_quad(&mut out, p)?;
                put_quad(&mut out, q)?;
            }
            (1, Block::Rotation(p, qs)) => {
                if qs.len() != n {
                    return Err(Refused);
                }
                put_quad(&mut out, p)?;
                for q in qs {
                    put_quad(&mut out, q)?;
                }
            }
            (k, Block::Moving(ps)) if *k > 1 => {
                if ps.len() != n {
                    return Err(Refused);
                }
                for (p, q) in ps {
                    put_quad(&mut out, p)?;
                    put_quad(&mut out, q)?;
                }
            }
            _ => return Err(Refused),
        }
    }
    let size = u32::try_from(out.len() - 12).map_err(|_| Refused)?;
    out.get_mut(8..12).ok_or(Refused)?.copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// `aimanim.renumbered(a, bones)`: the bone table renumbered (`map(old) = new`) and sorted by the new index (a
/// stable sort, as Python's `sorted`).
///
/// # Errors
///
/// [`Refused`] when a bone has no counterpart (the reference's `AimError`).
pub fn renumbered(a: &Aim, map: impl Fn(u8) -> Option<u8>) -> Result<Aim> {
    let mut bones = a
        .bones
        .iter()
        .map(|(bone, kind, block)| Ok((map(*bone).ok_or(Refused)?, *kind, block.clone())))
        .collect::<Result<Vec<_>>>()?;
    bones.sort_by_key(|t| t.0);
    Ok(Aim {
        addr: a.addr,
        poses: a.poses,
        bones,
    })
}

/// `aimanim.to_pc(b, bones)`: Battlefield 1943's (big-endian) aim animation as a PC resource, renumbered with `map`
/// when given.
///
/// # Errors
///
/// [`Refused`] where the reference raises.
pub fn to_pc(b: &[u8], map: Option<&dyn Fn(u8) -> Option<u8>>) -> Result<Vec<u8>> {
    let a = parse(b, true)?;
    match map {
        Some(m) => write(&renumbered(&a, m)?),
        None => write(&a),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::cast_precision_loss)]
    use super::*;

    fn sample() -> Aim {
        let q = |k: usize| [0.0, (k as f32 / 10.0).sin(), 0.0, (k as f32 / 10.0).cos()];
        Aim {
            addr: 0x08A2_55B0,
            poses: 3,
            bones: vec![
                (0, 0, Block::Fixed([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0])),
                (2, 1, Block::Rotation([0.1, 0.2, 0.3, 0.0], (0..3).map(q).collect())),
                (
                    7,
                    2,
                    Block::Moving((0..3).map(|k| ([k as f32, 1.4, 0.2, 1.0], q(k))).collect()),
                ),
            ],
        }
    }

    fn big(b: &[u8], nb: usize) -> Vec<u8> {
        let mut out = b.to_vec();
        for o in (0..b.len()).step_by(4) {
            if (0x14..0x1c + 2 * nb).contains(&o) {
                continue;
            }
            out[o..o + 4].reverse();
        }
        out
    }

    #[test]
    fn round_trip_both_orders() -> Result<()> {
        let a = sample();
        let b = write(&a)?;
        assert_eq!(b.len(), data_at(3) + 32 + (16 + 16 * 3) + 32 * 3);
        assert_eq!(parse(&b, false)?, a);
        assert_eq!(parse(&big(&b, 3), true)?, a);
        assert_eq!(to_pc(&big(&b, 3), None)?, b);
        Ok(())
    }

    #[test]
    fn renumbering_sorts_and_refuses_missing() -> Result<()> {
        let a = sample();
        let m = renumbered(&a, |b| [(0, 0), (2, 9), (7, 4)].iter().find(|p| p.0 == b).map(|p| p.1))?;
        assert_eq!(
            m.bones.iter().map(|t| (t.0, t.1)).collect::<Vec<_>>(),
            vec![(0, 0), (4, 2), (9, 1)]
        );
        assert_eq!(renumbered(&a, |b| (b != 7).then_some(b)), Err(Refused));
        Ok(())
    }

    #[test]
    fn refusals() -> Result<()> {
        let b = write(&sample())?;
        assert_eq!(parse(&b[..0x1b], false), Err(Refused));
        let mut longer = b.clone();
        longer.extend_from_slice(&[0; 16]);
        assert_eq!(parse(&longer, false), Err(Refused));
        let mut nan = b.clone();
        let at = data_at(3);
        nan[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(parse(&nan, false), Err(Refused));
        let mut bad = sample();
        bad.bones[1].1 = 0;
        assert_eq!(write(&bad), Err(Refused));
        Ok(())
    }
}
