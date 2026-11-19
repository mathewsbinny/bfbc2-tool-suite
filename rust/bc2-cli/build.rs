//! Embeds the suite's application icon in bfbc2.exe (round I1: `rust/app_icon.rs`, README.md "The application icon").
#![allow(clippy::print_stdout)]

include!("../app_icon.rs");

fn main() {
    embed_app_icon(&["bfbc2"]);
}
