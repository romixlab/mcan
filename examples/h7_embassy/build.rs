use std::{env, fs};
//use std::fs::File;
//use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put `memory.x` in our output directory and ensure it's on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();
    fs::write(
        out.join("link_ram.x"),
        include_bytes!("../link_ram_cortex_m.x"),
    )
    .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg=--nmagic");
    //println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rustc-link-arg=-Tlink_ram.x");
    println!("cargo:rustc-link-arg=-Tdefmt.x");
}
