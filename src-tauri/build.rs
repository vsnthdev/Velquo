fn main() {
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let plist_path = std::path::Path::new(&manifest_dir).join("Info.plist");
        println!("cargo:rerun-if-changed={}", plist_path.display());

        let mr_src = std::path::Path::new(&manifest_dir)
            .join("src")
            .join("kdeconnect")
            .join("velquo_mr_bridge.m");
        println!("cargo:rerun-if-changed={}", mr_src.display());
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let dylib_out = std::path::Path::new(&out_dir).join("libvelquo_mr_bridge.dylib");

        let status = std::process::Command::new("/usr/bin/clang")
            .args([
                "-dynamiclib",
                "-O2",
                "-fobjc-arc",
                "-framework",
                "Foundation",
                mr_src.to_str().unwrap(),
                "-o",
                dylib_out.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to invoke clang to compile libvelquo_mr_bridge.dylib");
        assert!(
            status.success(),
            "Failed to compile velquo_mr_bridge.m into libvelquo_mr_bridge.dylib"
        );
    }
    tauri_build::build()
}
