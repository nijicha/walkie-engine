// ONNX Runtime's prebuilt static library checks the macOS version with @available, which needs
// ___isPlatformVersionAtLeast from clang's runtime library; rustc does not link that one.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::process::Command::new("xcrun").args(["clang", "--print-resource-dir"]).output();
    if let Ok(out) = out {
        let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let lib = std::path::Path::new(&dir).join("lib/darwin/libclang_rt.osx.a");
        if lib.exists() {
            println!("cargo:rustc-link-arg={}", lib.display());
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
