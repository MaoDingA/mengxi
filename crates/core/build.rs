fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let moonbit_lib = workspace_root.join("moonbit").join("target").join("libmoonbit_core.a");

    if moonbit_lib.exists() {
        println!(
            "cargo:rustc-link-search=native={}",
            moonbit_lib.parent().unwrap().display()
        );
        println!("cargo:rustc-link-lib=static=moonbit_core");
        println!("cargo:rerun-if-changed={}", moonbit_lib.display());

        // Link MoonBit runtime backtrace support
        let moon_home =
            std::env::var("MOONBIT_HOME").unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                format!("{}/.moon", home)
            });
        let moon_lib_path = std::path::Path::new(&moon_home).join("lib");
        if moon_lib_path.join("libbacktrace.a").exists() {
            println!(
                "cargo:rustc-link-search=native={}",
                moon_lib_path.display()
            );
            println!("cargo:rustc-link-lib=static=backtrace");
        }
    }
}
