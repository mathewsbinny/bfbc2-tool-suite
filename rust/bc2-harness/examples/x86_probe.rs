//! The decode of each 16-byte record of a probe file at 0x401000, in the capstone probe dump's format
//! (`<index> <size> <mnemonic>\t<op_str>\t<operands>`, `<index> -`).
//!
//!   cargo run --release -p bc2-harness --example x86_probe -- <probe.bin> <out.txt>

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

use bc2_harness::x86::{Disassembler, Operand, reg_name};
use rayon::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("read the probes");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&args[2]).expect("create the output"));
    let chunks: Vec<String> = data
        .par_chunks(16 << 12)
        .enumerate()
        .map(|(c, chunk)| {
            let d = Disassembler::new();
            let mut s = String::new();
            for (k, rec) in chunk.chunks(16).enumerate() {
                let n = (c << 12) + k;
                let refuse = std::env::args().any(|a| a == "--refuse");
                if refuse && d.decode(rec, 0x40_1000).is_err() {
                    writeln!(s, "{n:x} U").unwrap();
                    continue;
                }
                match d.decode_unchecked(rec, 0x40_1000) {
                    None => writeln!(s, "{n:x} -").unwrap(),
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
                        writeln!(s, "{n:x} {} {}\t{}\t{}", i.size, i.mnemonic, i.op_str, ops.join(" ")).unwrap();
                    }
                }
            }
            s
        })
        .collect();
    for c in chunks {
        out.write_all(c.as_bytes()).unwrap();
    }
    println!("{} probes", data.len() / 16);
}
