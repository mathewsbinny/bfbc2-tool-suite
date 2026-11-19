//! Embeds the suite's application icon in mapexport.exe (round I1: `rust/app_icon.rs`, README.md "The application
//! icon"); the library and its tests are not touched.
#![allow(clippy::print_stdout)]

include!("../app_icon.rs");

fn main() {
    embed_app_icon(&["mapexport"]);
}
