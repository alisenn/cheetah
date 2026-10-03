use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=src/blocklist.txt");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("blocklist.txt");
    let list = fs::read_to_string("src/blocklist.txt").unwrap_or_default();
    fs::write(out, list).unwrap();
}
