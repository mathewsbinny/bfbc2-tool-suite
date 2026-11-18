//! For each path on stdin: FbrbTool's `Gunzip` of the whole file as `fbrb::netgzip` reads it, printed as
//! `<len> <sha1>` or `EXC <type>: <message>` (the format of the C# probe the tests compare it with).
#![allow(clippy::print_stdout, clippy::unwrap_used, missing_docs)]

use std::io::BufRead;

use bc2_formats::fbrb::netgzip::{self, NetGzipError};

fn main() {
    let managed = std::env::args().any(|a| a == "--managed");
    for line in std::io::stdin().lock().lines() {
        let path = line.unwrap();
        let data = std::fs::read(&path).unwrap();
        let r = if managed {
            netgzip::gunzip_managed(&data)
        } else {
            netgzip::gunzip(&data)
        };
        match r {
            Ok(o) => println!("{} {}", o.len(), sha1_smol::Sha1::from(&o).digest()),
            Err(NetGzipError::InvalidData(m)) => println!("EXC InvalidDataException: {m}"),
            Err(NetGzipError::IndexOutOfRange) => {
                println!("EXC IndexOutOfRangeException: Index was outside the bounds of the array.");
            }
        }
    }
}
