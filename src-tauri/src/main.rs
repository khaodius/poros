// Prevents an additional console window on Windows in release. Do not remove.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    linux_webview_workarounds();
    poros_lib::run()
}

/// Fixes blank windows (DMA-BUF renderer) and "Error 71" aborts (NVIDIA explicit sync)
/// under Wayland. User-set values win. Must run before any threads start.
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
