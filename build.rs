use embed_manifest::manifest::{DpiAwareness, ExecutionLevel};
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    // Reading the USN journal needs administrator rights, same as the C# version.
    // The manifest only goes into the binaries, so `cargo test` and examples run unelevated.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest(
            new_manifest("JournalTrace")
                .requested_execution_level(ExecutionLevel::RequireAdministrator)
                .dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("unable to embed manifest");
    }
    // Exe icon and version info. Needs rc.exe from the Windows SDK; without
    // it the exe just has no icon, so this is a warning, not an error.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("FileDescription", "JournalTrace")
            .set("ProductName", "JournalTrace")
            .set("LegalCopyright", "GPL-3.0");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // Optional background picture: embedded when assets/background.png exists.
    println!("cargo::rustc-check-cfg=cfg(has_background)");
    if std::path::Path::new("assets/background.png").exists() {
        println!("cargo::rustc-cfg=has_background");
    }
    println!("cargo:rerun-if-changed=assets/background.png");
    println!("cargo:rerun-if-changed=build.rs");
}
