//! Embeds the application icon and version info into the Windows executable (UI-050).

fn main() {
    println!("cargo:rerun-if-changed=../../assets/brand/inkra.ico");
    println!("cargo:rerun-if-changed=build.rs");
    let target_windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if target_windows && cfg!(windows) {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/brand/inkra.ico")
            .set("ProductName", "Inkra")
            .set("FileDescription", "Inkra")
            .set("InternalName", "inkra")
            .set("OriginalFilename", "inkra.exe")
            .set("LegalCopyright", "Inkra contributors, MIT OR Apache-2.0");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows resources: {e}");
        }
    }
}
