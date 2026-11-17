//! Rebuilds the module when `BC2_SOURCE_HASH` changes: `lib.rs` bakes it in as `__source_hash__`
//! (`rust/build_native.py` passes the sources' fingerprint; README.md, "The Rust core").

fn main() {
    println!("cargo:rerun-if-env-changed=BC2_SOURCE_HASH");
}
