//! Window behavior a page cannot set up from inside: corners the system draws, and the preview
//! that follows a tab dragged out of Poros onto the desktop.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

/// The window showing where a tab dragged out of Poros will open.
pub const DRAG_PREVIEW_WINDOW: &str = "drag-preview";
/// Mirrored in `src/lib/ipc.ts`. Tells the preview window what to show.
pub const DRAG_PREVIEW_EVENT: &str = "poros://drag-preview";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowCorners {
    Square,
    Small,
    Round,
}

/// Rounds a window's corners on Windows 11, which draws them in two sizes. Other systems keep
/// frameless windows square.
pub fn set_corners(window: &WebviewWindow, corners: WindowCorners) -> tauri::Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND,
            DWMWCP_ROUNDSMALL,
        };
        let preference = match corners {
            WindowCorners::Square => DWMWCP_DONOTROUND,
            WindowCorners::Small => DWMWCP_ROUNDSMALL,
            WindowCorners::Round => DWMWCP_ROUND,
        };
        let hwnd = window.hwnd()?;
        // Windows 10 has no corner preference and refuses it; its corners stay square.
        unsafe {
            DwmSetWindowAttribute(
                hwnd.0,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&preference as *const i32).cast(),
                std::mem::size_of_val(&preference) as u32,
            );
        }
    }
    #[cfg(not(windows))]
    let _ = (window, corners);
    Ok(())
}

/// Where to ask for a new window to open so it lands at a point in physical pixels: the point in
/// the logical pixels of the monitor holding it. Some systems still pick a monitor by other
/// rules, so [`place`] moves the window there once it exists.
pub fn opening_corner(app: &AppHandle, corner: PhysicalPosition<i32>) -> LogicalPosition<f64> {
    let monitor = app
        .monitor_from_point(corner.x.into(), corner.y.into())
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    corner.to_logical(monitor.map_or(1.0, |monitor| monitor.scale_factor()))
}

/// Moves a window's top left corner to a point in physical pixels, then sizes it in logical
/// pixels. Logical positions mean different things on monitors scaled differently, and a window
/// converts them with the scale of the monitor it is leaving, so it would land elsewhere.
pub fn place(
    window: &WebviewWindow,
    corner: PhysicalPosition<i32>,
    size: LogicalSize<f64>,
) -> tauri::Result<()> {
    window.set_position(corner)?;
    // After the move, so the size uses the scale of the monitor the window is now on.
    window.set_size(size)
}

/// Where the preview sits on screen and what it shows: its top left corner in physical pixels,
/// its size in logical pixels.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPlacement {
    pub content: serde_json::Value,
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}

/// The preview of a tab dragged onto the desktop. Its window opens on the first such drag and
/// is hidden, not closed, between drags so it shows at once the next time.
#[derive(Default)]
pub struct DragPreview(Mutex<PreviewState>);

#[derive(Default)]
struct PreviewState {
    /// What the preview shows while a drag wants it; none while it is hidden.
    content: Option<serde_json::Value>,
    /// The preview page has loaded and listens for content.
    ready: bool,
}

impl DragPreview {
    /// Moves the preview and sends its page what to show; the page shows the window once it has
    /// drawn that, so the window never appears with the previous drag's tab.
    pub fn place(&self, app: &AppHandle, placement: PreviewPlacement) -> tauri::Result<()> {
        if !can_place_windows() {
            return Ok(());
        }
        let mut state = self.0.lock().unwrap();
        let corner = PhysicalPosition::new(placement.x, placement.y);
        let size = LogicalSize::new(placement.width, placement.height);
        let Some(window) = app.get_webview_window(DRAG_PREVIEW_WINDOW) else {
            state.content = Some(placement.content);
            state.ready = false;
            let opening = opening_corner(app, corner);
            let window = WebviewWindowBuilder::new(
                app,
                DRAG_PREVIEW_WINDOW,
                WebviewUrl::App("index.html".into()),
            )
            .title("Poros")
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .inner_size(placement.width, placement.height)
            .position(opening.x, opening.y)
            .build()?;
            return place(&window, corner, size);
        };
        place(&window, corner, size)?;
        if state.content.as_ref() != Some(&placement.content) {
            if state.ready {
                window.emit_to(DRAG_PREVIEW_WINDOW, DRAG_PREVIEW_EVENT, &placement.content)?;
            }
            state.content = Some(placement.content);
        }
        Ok(())
    }

    /// Marks the preview page ready and returns what it should show, if anything.
    pub fn ready(&self) -> Option<serde_json::Value> {
        let mut state = self.0.lock().unwrap();
        state.ready = true;
        state.content.clone()
    }

    /// Shows the preview once its page has drawn the content, unless the drag left meanwhile.
    pub fn reveal(&self, window: &WebviewWindow) -> tauri::Result<()> {
        let state = self.0.lock().unwrap();
        if state.content.is_none() {
            return Ok(());
        }
        show_without_focus(window)
    }

    pub fn hide(&self, app: &AppHandle) -> tauri::Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.content.take().is_none() {
            return Ok(());
        }
        match app.get_webview_window(DRAG_PREVIEW_WINDOW) {
            Some(window) => hide_window(&window),
            None => Ok(()),
        }
    }
}

/// Wayland leaves window positions to the compositor, so a preview could not follow the pointer.
#[cfg(target_os = "linux")]
pub fn can_place_windows() -> bool {
    let x11 = std::env::var("GDK_BACKEND").is_ok_and(|backend| backend.starts_with("x11"));
    x11 || std::env::var_os("WAYLAND_DISPLAY").is_none()
}

#[cfg(not(target_os = "linux"))]
pub fn can_place_windows() -> bool {
    true
}

/// Shows the preview without taking focus from the window the tab is dragged from, which would
/// end the drag. Windows activates a window it shows unless asked not to.
#[cfg(windows)]
fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_SHOWNOACTIVATE};
    let hwnd = window.hwnd()?;
    unsafe { ShowWindowAsync(hwnd.0, SW_SHOWNOACTIVATE) };
    Ok(())
}

#[cfg(not(windows))]
fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()
}

/// Hides the preview the same way it was shown.
#[cfg(windows)]
fn hide_window(window: &WebviewWindow) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_HIDE};
    let hwnd = window.hwnd()?;
    unsafe { ShowWindowAsync(hwnd.0, SW_HIDE) };
    Ok(())
}

#[cfg(not(windows))]
fn hide_window(window: &WebviewWindow) -> tauri::Result<()> {
    window.hide()
}
