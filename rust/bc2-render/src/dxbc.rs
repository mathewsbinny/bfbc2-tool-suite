//! What a compiled Direct3D 10 / 11 shader (DXBC) declares: its `$Globals` variables and resource bindings from the
//! `RDEF` chunk (`pcbuild.rdef`, `renderqa.resources`), and whether its code can discard (`renderqa.discards`).

use crate::{cstr_at, u32_at};

/// A `$Globals` variable: its name, byte offset and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Var {
    /// Its name (`worldViewProjMatrix`, `constants` ...).
    pub name: String,
    /// Byte offset in the buffer.
    pub offset: u32,
    /// Size in bytes.
    pub size: u32,
}

/// A bound resource: a texture (kind 2), a sampler (3) or a constant buffer (0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// Its name (`texture_Texture2`, `sampler0` ...).
    pub name: String,
    /// `D3D_SHADER_INPUT_TYPE`: 0 cbuffer, 2 texture, 3 sampler.
    pub kind: u32,
    /// Its register (t / s / b).
    pub point: u32,
    /// `D3D_SRV_DIMENSION`: 4 2D, 9 cube.
    pub dim: u32,
}

/// A shader's reflection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reflection {
    /// `$Globals`' variables, in declaration order.
    pub globals: Vec<Var>,
    /// The bound resources.
    pub resources: Vec<Resource>,
}

impl Reflection {
    /// The size of `$Globals` the variables reach, in whole registers (at least one).
    #[must_use]
    pub fn globals_size(&self) -> usize {
        let end = self
            .globals
            .iter()
            .map(|v| (v.offset + v.size) as usize)
            .max()
            .unwrap_or(16);
        end.max(16).div_ceil(16) * 16
    }

    /// A variable by name.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<&Var> {
        self.globals.iter().find(|v| v.name == name)
    }
}

/// A chunk of a DXBC container by its tag.
#[must_use]
pub fn chunk<'a>(dxbc: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    if dxbc.get(0..4) != Some(b"DXBC") {
        return None;
    }
    let n = u32_at(dxbc, 28) as usize;
    (0..n.min(64)).find_map(|i| {
        let off = u32_at(dxbc, 32 + 4 * i) as usize;
        if dxbc.get(off..off + 4) != Some(&tag[..]) {
            return None;
        }
        let size = u32_at(dxbc, off + 4) as usize;
        dxbc.get(off + 8..off + 8 + size)
    })
}

/// The `RDEF` chunk read: `$Globals` and the resource bindings (empty when there is none).
#[must_use]
pub fn reflect(dxbc: &[u8]) -> Reflection {
    let Some(r) = chunk(dxbc, b"RDEF") else {
        return Reflection::default();
    };
    let (ncb, cbo, nb, bo) = (u32_at(r, 0), u32_at(r, 4), u32_at(r, 8), u32_at(r, 12));
    let var_size = if r.get(28..32) == Some(b"RD11") { 40 } else { 24 };
    let mut out = Reflection::default();
    for i in 0..ncb.min(64) as usize {
        let at = cbo as usize + 24 * i;
        if cstr_at(r, u32_at(r, at) as usize) != "$Globals" {
            continue;
        }
        let (nv, vo) = (u32_at(r, at + 4), u32_at(r, at + 8));
        for j in 0..nv.min(4096) as usize {
            let v = vo as usize + var_size * j;
            out.globals.push(Var {
                name: cstr_at(r, u32_at(r, v) as usize),
                offset: u32_at(r, v + 4),
                size: u32_at(r, v + 8),
            });
        }
    }
    for i in 0..nb.min(256) as usize {
        let at = bo as usize + 32 * i;
        out.resources.push(Resource {
            name: cstr_at(r, u32_at(r, at) as usize),
            kind: u32_at(r, at + 4),
            dim: u32_at(r, at + 12),
            point: u32_at(r, at + 20),
        });
    }
    out
}

/// The compiler that built a shader (its `RDEF` creator string): DICE's say `... 9.27.952.3022`.
#[must_use]
pub fn compiler(dxbc: &[u8]) -> String {
    chunk(dxbc, b"RDEF").map_or_else(String::new, |r| cstr_at(r, u32_at(r, 24) as usize))
}

/// Whether a shader's code has a `discard` instruction (its own alpha test): the `SHDR` / `SHEX` instruction stream
/// walked token by token (opcode 13 `discard`; 53 `customdata` carries its own length).
#[must_use]
pub fn discards(dxbc: &[u8]) -> bool {
    let Some(code) = chunk(dxbc, b"SHEX").or_else(|| chunk(dxbc, b"SHDR")) else {
        return false;
    };
    let total = (u32_at(code, 4) as usize).min(code.len() / 4);
    let mut at = 2;
    while at < total {
        let token = u32_at(code, 4 * at);
        let opcode = token & 0x7ff;
        let len = if opcode == 53 {
            u32_at(code, 4 * at + 4) as usize
        } else {
            ((token >> 24) & 0x7f) as usize
        };
        if opcode == 13 {
            return true;
        }
        if len == 0 {
            return false;
        }
        at += len;
    }
    false
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    /// A container with one chunk.
    fn container(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut d = b"DXBC".to_vec();
        d.extend([0u8; 20]);
        d.extend(36u32.to_le_bytes()); // total (not checked)
        d.extend(1u32.to_le_bytes());
        d.extend(36u32.to_le_bytes());
        d.extend(tag);
        d.extend(u32::try_from(body.len()).unwrap_or(0).to_le_bytes());
        d.extend(body);
        d
    }

    #[test]
    fn discard_found_in_the_instruction_stream() {
        // version, length, mov (len 5), discard_nz (len 3), ret (len 1)
        let words: [u32; 11] = [0x50, 11, 0x0500_0036, 0, 0, 0, 0, 0x0300_000d, 0, 0, 0x0100_003e];
        let body: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert!(discards(&container(b"SHEX", &body)));
        let words: [u32; 8] = [0x50, 8, 0x0500_0036, 0, 0, 0, 0, 0x0100_003e];
        let body: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert!(!discards(&container(b"SHEX", &body)));
        assert!(!discards(b"not a shader"));
    }

    #[test]
    fn globals_size_rounds_to_registers() {
        let r = Reflection {
            globals: vec![Var {
                name: "cameraPos".into(),
                offset: 16,
                size: 12,
            }],
            resources: Vec::new(),
        };
        assert_eq!(r.globals_size(), 32);
        assert_eq!(Reflection::default().globals_size(), 16);
    }
}
