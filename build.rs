//! Compiles the Swift parts (`swift/*.swift`: the SwiftUI menu bar panel and the snapshot
//! overlay) into a static library and links it, with the Swift runtime that ships in macOS. Needs only `swiftc`, which comes
//! with Apple's Command Line Tools.

use std::process::Command;

fn xcrun(args: &[&str]) -> String {
    let out = Command::new("xcrun").args(args).output().expect("run xcrun (install the Command Line Tools)");
    assert!(out.status.success(), "xcrun {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn main() {
    println!("cargo:rerun-if-changed=swift");
    let mut sources: Vec<String> = std::fs::read_dir("swift")
        .expect("swift/ folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "swift"))
        .map(|p| p.display().to_string())
        .collect();
    sources.sort();
    for s in &sources {
        println!("cargo:rerun-if-changed={s}");
    }
    let out = std::env::var("OUT_DIR").unwrap();
    let lib = format!("{out}/libReelPanel.a");
    let status = Command::new("xcrun")
        .args([
            "swiftc", "-parse-as-library", "-emit-library", "-static", "-O", "-wmo",
            "-module-name", "ReelPanel", "-target", "arm64-apple-macos13.0",
        ])
        .args(&sources)
        .args(["-o", &lib])
        .status()
        .expect("run swiftc");
    assert!(status.success(), "compiling the Swift sources failed");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=ReelPanel");

    // The Swift runtime and its back-deployment shims. The library names them itself
    // (auto-linking); these are just the places to find them.
    let sdk = xcrun(&["--show-sdk-path"]);
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    let swiftc = xcrun(&["--find", "swiftc"]);
    let toolchain = std::path::Path::new(&swiftc).parent().unwrap().parent().unwrap();
    println!("cargo:rustc-link-search=native={}/lib/swift/macosx", toolchain.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    // Swift's availability checks call into Clang's runtime library.
    let resource = xcrun(&["clang", "--print-resource-dir"]);
    println!("cargo:rustc-link-search=native={resource}/lib/darwin");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
