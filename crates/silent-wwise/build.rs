//! Generates the exported functions from `exports.txt` into `OUT_DIR`.

#[path = "src/list.rs"]
mod list;

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=exports.txt");
    println!("cargo:rerun-if-changed=src/list.rs");
    let text = std::fs::read_to_string("exports.txt").expect("exports.txt is readable");
    let entries = list::parse(&text).unwrap_or_else(|error| panic!("exports.txt: {error}"));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out.join("entries.rs"), list::generate(&entries)).expect("OUT_DIR is writable");
}
