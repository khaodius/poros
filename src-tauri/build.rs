use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        embed_windows_manifest_in_every_binary();
        let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("failed to run tauri-build");
    } else {
        tauri_build::build();
    }
}

/// tauri-build links the manifest into the app binary only. Test executables need it too: the
/// dialog plugin imports `TaskDialogIndirect`, which exists only in Common Controls v6, and
/// without the manifest they abort with STATUS_ENTRYPOINT_NOT_FOUND.
fn embed_windows_manifest_in_every_binary() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
