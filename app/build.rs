use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CLEARBAR_TAP_DLL");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dll = std::env::var_os("CLEARBAR_TAP_DLL").map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../target/release/clearbar_tap.dll"));
    let dll = dll.canonicalize().expect("Build the embedded TAP first: cargo build --release -p clearbar-tap --locked");
    println!("cargo:rerun-if-changed={}", dll.display());
    println!("cargo:rustc-env=CLEARBAR_EMBEDDED_TAP={}", dll.display());
}
