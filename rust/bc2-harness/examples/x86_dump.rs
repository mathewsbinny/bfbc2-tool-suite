//! The first instruction at every byte of an exe's code sections, in the format of the capstone dump the parity run
//! compares with (`<va> <size> <mnemonic>\t<op_str>\t<operands>`, `<va> -` where nothing decodes).
//!
//!   cargo run --release -p bc2-harness --example x86_dump -- <exe> <out> [stride]

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use std::fmt::Write as _;
use std::io::Write as _;

use bc2_harness::pe::Image;
use bc2_harness::x86::{Disassembler, Operand, reg_name};
use rayon::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = std::fs::read(&args[1]).expect("read the exe");
    let stride: usize = args.get(3).map_or(1, |s| s.parse().expect("stride"));
    let img = Image::parse(&file).expect("map the exe");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&args[2]).expect("create the output"));
    let mut n = 0usize;
    for (lo, hi) in img.code_ranges() {
        let starts: Vec<u32> = (lo..hi).step_by(stride).collect();
        let chunks: Vec<String> = starts
            .par_chunks(1 << 16)
            .map(|chunk| {
                let d = Disassembler::new();
                let mut s = String::new();
                for &a in chunk {
                    let o = (a - img.base) as usize;
                    let end = (o + 16).min(img.data.len());
                    match d.decode_unchecked(&img.data[o.min(end)..end], a) {
                        None => writeln!(s, "{a:x} -").unwrap(),
                        Some(i) => {
                            let ops: Vec<String> = i
                                .operands
                                .iter()
                                .map(|op| match op {
                                    Operand::Imm(v) => format!("I{:x}", (*v as u64) & 0xFFFF_FFFF),
                                    Operand::Mem { base, index, disp } => format!(
                                        "M{},{},{:x}",
                                        if *base == iced_x86::Register::None {
                                            String::new()
                                        } else {
                                            reg_name(*base)
                                        },
                                        if *index == iced_x86::Register::None {
                                            String::new()
                                        } else {
                                            reg_name(*index)
                                        },
                                        (*disp as u64) & 0xFFFF_FFFF
                                    ),
                                })
                                .collect();
                            writeln!(s, "{a:x} {} {}\t{}\t{}", i.size, i.mnemonic, i.op_str, ops.join(" ")).unwrap();
                        }
                    }
                }
                s
            })
            .collect();
        for c in chunks {
            out.write_all(c.as_bytes()).unwrap();
        }
        n += starts.len();
    }
    println!("{n} starts");
}
