fn main() {
    println!("cargo:rustc-link-arg=--nmagic");
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-arg=-Tcnt.x");
    println!("cargo:rustc-link-arg-tests=-Tembedded-test.x");
    println!("cargo::rustc-check-cfg=cfg(rust_analyzer)");
}
