//! The microcode listing of `bfbc2-shader-tools/xenos.py`'s `disasm`, round F3: the labels, each control-flow
//! instruction and each ALU / fetch instruction it executes, line for line as `_cf_text` and `disasm_instruction`
//! write them. The header comment lines (constant table, literals, declarations) stay in Python, which reads them
//! from the tree it holds; this is the listing after them, the part that grows with the program.
//!
//! ```
//! use bc2_shader::disasm;
//!
//! assert_eq!(disasm::pred(true, false), "(!p0) ");
//! ```

use std::fmt::Write as _;

use crate::xenos::{Cf, CfKind, CfType, Dest, Fetch, FetchKind, File, Instr, InstrKind, Operand, Rel, Shader};

/// `_pred(p, c)`.
#[must_use]
pub fn pred(p: bool, c: bool) -> &'static str {
    match (p, c) {
        (false, _) => "      ",
        (true, true) => " (p0) ",
        (true, false) => "(!p0) ",
    }
}

fn reg(file: &str, index: u32, rel: Rel) -> String {
    match rel {
        Some(r) => format!("{file}[{index}+{r}]"),
        None => format!("{file}{index}"),
    }
}

/// `_src(o)`.
fn src(o: &Operand) -> String {
    let mut s = String::new();
    if o.negate {
        s.push('-');
    }
    let f = if o.file == File::Temp { "r" } else { "c" };
    if o.abs {
        s.push('|');
        s.push_str(&reg(f, o.index, o.rel));
        s.push('|');
    } else {
        s.push_str(&reg(f, o.index, o.rel));
    }
    let comps = if let Some(c) = &o.components {
        c.as_str()
    } else {
        let sw = o.swizzle();
        if sw == "xyzw" {
            return s;
        }
        sw
    };
    s.push('.');
    s.push_str(comps);
    s
}

/// `_dst(d)` of a destination's file, index, rel and write mask.
fn dst_of(file: File, index: Option<u32>, rel: Rel, write: &str) -> String {
    let mut s = match (file, index) {
        (File::Temp, i) => reg("r", i.unwrap_or(0), rel),
        (f, None) => f.name().to_owned(),
        (f, Some(i)) => format!("{}{i}", f.name()),
    };
    if write == "xyzw" {
        return s;
    }
    if write.bytes().all(|c| c == b'_') {
        s.push_str("._");
    } else {
        s.push('.');
        s.push_str(write);
    }
    s
}

fn dst(d: &Dest) -> String {
    dst_of(d.file, d.index, d.rel, &d.write)
}

/// Python's `'%g' % v` for the values a fetch prints (`k / 16` and `k / 2` for small `k`: exact, at most six
/// significant digits, so Rust's shortest form is the same text).
fn g(v: f64) -> String {
    format!("{v}")
}

fn fetch_lines(f: &Fetch) -> String {
    let p = pred(f.predicated, f.pred_condition);
    match &f.kind {
        FetchKind::Vertex(v) => {
            let mut s = format!(
                "{p}{} {}",
                f.op,
                dst_of(File::Temp, Some(f.dst), f.dst_rel, &f.dst_swizzle)
            );
            if !v.is_mini {
                let _ = write!(s, ", {}.{}, vf{}", reg("r", f.src, f.src_rel), v.src_swizzle, v.vf());
                if v.is_index_rounded {
                    s.push_str(", RoundIndex=true");
                }
            }
            if v.exp_adjust != 0 {
                let _ = write!(s, ", ExpAdjust={}", v.exp_adjust);
            }
            if v.offset != 0 {
                let _ = write!(s, ", Offset={}", v.offset);
            }
            s.push_str(", DataFormat=");
            s.push_str(&v.format_name());
            if !v.is_mini && v.stride != 0 {
                let _ = write!(s, ", Stride={}", v.stride);
            }
            if v.is_signed {
                s.push_str(", Signed=true");
            }
            if v.is_integer {
                s.push_str(", NumFormat=integer");
            }
            if v.prefetch_count != 0 {
                let _ = write!(s, ", PrefetchCount={}", v.prefetch_count + 1);
            }
            s
        }
        FetchKind::Texture(t) => {
            let mut parts = Vec::with_capacity(3);
            if t.has_dest {
                parts.push(dst_of(File::Temp, Some(f.dst), f.dst_rel, &f.dst_swizzle));
            }
            parts.push(format!("{}.{}", reg("r", f.src, f.src_rel), t.src_swizzle));
            if t.has_const {
                parts.push(format!("tf{}", t.const_index));
            }
            let mut s = format!("{p}{} {}", f.op, parts.join(", "));
            if !t.fetch_valid_only {
                s.push_str(", FetchValidOnly=false");
            }
            if t.unnormalized {
                s.push_str(", UnnormalizedTextureCoords=true");
            }
            for (nm, v) in [
                ("MagFilter", t.mag_filter),
                ("MinFilter", t.min_filter),
                ("MipFilter", t.mip_filter),
                ("AnisoFilter", t.aniso_filter),
                ("VolMagFilter", t.vol_mag_filter),
                ("VolMinFilter", t.vol_min_filter),
            ] {
                if v != "keep" {
                    let _ = write!(s, ", {nm}={v}");
                }
            }
            if !t.use_computed_lod {
                s.push_str(", UseComputedLOD=false");
            }
            if t.use_register_lod {
                s.push_str(", UseRegisterLOD=true");
            }
            if t.use_register_gradients {
                s.push_str(", UseRegisterGradients=true");
            }
            if t.sample_location != "centroid" {
                s.push_str(", SampleLocation=center");
            }
            if t.lod_bias != 0.0 {
                let _ = write!(s, ", LODBias={}", g(t.lod_bias));
            }
            for (nm, v) in [
                ("OffsetX", t.offset_x),
                ("OffsetY", t.offset_y),
                ("OffsetZ", t.offset_z),
            ] {
                if v != 0.0 {
                    let _ = write!(s, ", {nm}={}", g(v));
                }
            }
            s
        }
    }
}

/// `disasm_instruction(ins)`: one line, or for an ALU instruction with both halves two.
#[must_use]
pub fn instruction(ins: &Instr) -> Vec<String> {
    match &ins.kind {
        InstrKind::Alu(a) => {
            let p = pred(a.predicated, a.pred_condition);
            if a.vector_nop && a.scalar_nop {
                return vec![format!("{p}nop")];
            }
            let mut lines = Vec::with_capacity(2);
            let sat = |s: bool| if s { "_sat" } else { "" };
            if !a.vector_nop {
                let ops: Vec<String> = std::iter::once(dst(&a.vector_dest))
                    .chain(a.vector_operands.iter().map(src))
                    .collect();
                lines.push(format!(
                    "{p}{}{} {}",
                    a.vector_op(),
                    sat(a.vector_dest.saturate),
                    ops.join(", ")
                ));
            }
            if !a.scalar_nop {
                let ops: Vec<String> = std::iter::once(dst(&a.scalar_dest))
                    .chain(a.scalar_operands.iter().map(src))
                    .collect();
                let plus = if a.vector_nop { "" } else { "   + " };
                lines.push(format!(
                    "{plus}{p}{}{} {}",
                    a.scalar_op(),
                    sat(a.scalar_dest.saturate),
                    ops.join(", ")
                ));
            }
            lines
        }
        InstrKind::Fetch(f) => vec![fetch_lines(f)],
    }
}

fn cond(c: Option<u32>) -> bool {
    c.unwrap_or(0) != 0
}

/// `_cf_text(cf, is_pixel)`.
#[must_use]
pub fn cf_text(cf: &Cf, is_pixel: bool) -> String {
    let p0 = |c: bool| if c { " (p0) " } else { "(!p0) " };
    match &cf.kind {
        _ if cf.opcode == 0 => "      cnop".into(),
        CfKind::Exec {
            address,
            count,
            is_yield,
            is_end,
            type_,
            bool_index,
            condition,
            is_predicate_clean,
            ..
        } => {
            let name = if *is_end { "exece" } else { "exec" };
            let mut s = match type_ {
                CfType::Conditional => format!(
                    "      c{name} {}b{}",
                    if cond(*condition) { "" } else { "!" },
                    bool_index.unwrap_or(0)
                ),
                CfType::Predicated => format!("{}{name}", p0(cond(*condition))),
                CfType::Unconditional => format!("      {name}"),
            };
            let _ = write!(s, "    ADDR({address}) CNT({count})");
            if *is_yield != 0 {
                s.push_str(" Yield=true");
            }
            if !is_predicate_clean {
                s.push_str(" PredicateClean=false");
            }
            s
        }
        CfKind::LoopStart {
            address,
            is_repeat,
            loop_id,
        } => format!(
            "      loop i{loop_id}, L{address}{}",
            if *is_repeat != 0 { ", Repeat=true" } else { "" }
        ),
        CfKind::LoopEnd {
            address,
            loop_id,
            is_predicated_break,
            condition,
        } => {
            let p = if *is_predicated_break != 0 {
                p0(*condition != 0)
            } else {
                "      "
            };
            format!("{p}endloop i{loop_id}, L{address}")
        }
        CfKind::Jump {
            address,
            bool_index,
            condition,
            type_,
            ..
        } => {
            let nm = if cf.opcode == 9 { "call" } else { "jmp" };
            match type_ {
                CfType::Unconditional => format!("      {nm} L{address}"),
                CfType::Predicated => format!("{}{nm} L{address}", p0(*condition != 0)),
                CfType::Conditional => format!(
                    "      c{nm} {}b{bool_index}, L{address}",
                    if *condition != 0 { "" } else { "!" }
                ),
            }
        }
        CfKind::Alloc { size, alloc_type, .. } => {
            let what = match alloc_type {
                0 => "none".to_owned(),
                1 => "position".to_owned(),
                2 => if is_pixel { "colors" } else { "interpolators" }.to_owned(),
                _ => format!("export = {size}"),
            };
            if *alloc_type == 3 {
                format!("      alloc {what}")
            } else {
                format!("      alloc {what} (size {size})")
            }
        }
        CfKind::Plain => match cf.opcode {
            10 => "      ret".into(),
            15 => "      mark_vs_fetch_done".into(),
            op => format!("      cf_{op}"),
        },
    }
}

/// `disasm`'s listing after the header: a label line before each CF a loop or jump targets, each CF's line and the
/// lines of the instructions it executes (`show_raw`: each first line padded to 60 characters and the three words
/// after it). `is_pixel` is the tree's `is_pixel`, as `_cf_text` reads it.
#[must_use]
pub fn body(sh: &Shader, show_raw: bool, is_pixel: bool) -> Vec<String> {
    let mut labels = std::collections::HashSet::new();
    for cf in &sh.cf {
        match &cf.kind {
            CfKind::LoopStart { address, .. } | CfKind::LoopEnd { address, .. } | CfKind::Jump { address, .. }
                if matches!(cf.opcode, 7 | 8 | 9 | 11) =>
            {
                labels.insert(*address);
            }
            _ => {}
        }
    }
    let mut out = Vec::with_capacity(sh.cf.len() + sh.instructions.len() * 2);
    for cf in &sh.cf {
        if labels.contains(&cf.index) {
            out.push(format!("                label L{}", cf.index));
        }
        out.push(format!(
            "/* {:4}.{} */ {}",
            cf.index / 2,
            cf.index % 2,
            cf_text(cf, is_pixel)
        ));
        for &i in cf.instructions.as_deref().unwrap_or_default() {
            let Some(ins) = sh.instructions.get(i) else { continue };
            for (k, mut line) in instruction(ins).into_iter().enumerate() {
                if k == 0 {
                    if ins.serialize {
                        line.push_str("    // serialize");
                    }
                    if show_raw {
                        let pad = 60usize.saturating_sub(line.chars().count());
                        line.extend(std::iter::repeat_n(' ', pad));
                        let [a, b, c] = ins.raw;
                        let _ = write!(line, " // {a:08x} {b:08x} {c:08x}");
                    }
                    out.push(format!("/* {:4}   */    {line}", ins.address));
                } else {
                    out.push(format!("                {line}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values_print_as_percent_g() {
        for k in -64..64 {
            let v = f64::from(k) / 16.0;
            let s = g(v);
            assert!(!s.contains('e') && !s.ends_with(".0"), "{s}");
        }
        assert_eq!(g(-0.5), "-0.5");
        assert_eq!(g(3.9375), "3.9375");
        assert_eq!(g(-4.0), "-4");
    }

    #[test]
    fn predicates() {
        assert_eq!(pred(false, true), "      ");
        assert_eq!(pred(true, true), " (p0) ");
    }
}
