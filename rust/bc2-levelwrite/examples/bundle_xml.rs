//! Study helper (round E3): a bundle partition's XML, or the entries whose names contain a text.
//!
//! `cargo run -p bc2-levelwrite --example bundle_xml -- <bundle> <entry> <out.xml>`
//! `cargo run -p bc2-levelwrite --example bundle_xml -- <bundle> --list <text>`
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use bc2_formats::{dbx, fbrb};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a[2] == "--list" {
        let h = fbrb::entries_path(&a[1]).expect("header");
        let f = a[3].to_ascii_lowercase();
        for e in &h.entries {
            if e.name.to_ascii_lowercase().contains(&f) {
                println!(
                    "{}{} {} flags={:#x} size={} size2={}",
                    if e.is_deleted() { "D " } else { "  " },
                    e.name,
                    e.type_name,
                    e.flags,
                    e.size,
                    e.size2
                );
            }
        }
        return;
    }
    let data = fbrb::read_path_uncached(&a[1], &[a[2].as_str()]).expect("read");
    let x = dbx::to_xml(&data[0]).expect("xml");
    std::fs::write(&a[3], x).expect("write");
    println!("{} -> {}", a[2], a[3]);
}
