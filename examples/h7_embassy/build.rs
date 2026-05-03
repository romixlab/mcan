use std::ffi::OsString;
use std::{env, fs};
//use std::fs::File;
//use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put `memory.x` in our output directory and ensure it's on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    // This is needed if your flash or ram addresses are not aligned to 0x10000 in memory.x
    // See https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    println!("cargo:rustc-link-arg=--nmagic");

    if env::var_os("RAM_LINK") == Some(OsString::from("1")) {
        fs::write(
            out.join("link_ram.x"),
            include_bytes!("../link_ram_cortex_m.x"),
        )
        .unwrap();
        println!(
            "cargo::warning=⚠️ \x1b[1;33mUsing RAM linking, old code will be run from FLASH on power-cycle"
        );
        println!("cargo:rustc-link-arg=-Tlink_ram.x");
    } else {
        println!("cargo:rustc-link-arg=-Tlink.x"); // provided by cortex-m-rt
    }

    println!("cargo:rustc-link-arg=-Tdefmt.x");
}
