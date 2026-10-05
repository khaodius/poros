// Prevents an additional console window on Windows in release. Do not remove.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    linux_webview_workarounds();
    poros_lib::run()
}

/// WebKitGTK's DMA-BUF renderer shows a blank or flickering window on many Wayland
/// setups (NVIDIA especially), and NVIDIA's explicit sync makes Wayland abort with
/// "Error 71 (Protocol error)". Both are opt-outs, so a user's own value always wins.
/// Runs before any threads exist, which `set_var` requires.
#[cfg(target_os = "linux")]
fn linux_webview_workarounds() {
    for (key, value) in [
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ("__NV_DISABLE_EXPLICIT_SYNC", "1"),
    ] {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
}
