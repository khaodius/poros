//! Window behavior a page cannot set up from inside: corners the system draws, and the preview
//! that follows a tab dragged out of Poros onto the desktop.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

/// The window showing where a tab dragged out of Poros will open.
pub const DRAG_PREVIEW_WINDOW: &str = "drag-preview";
/// Mirrored in `src/lib/ipc.ts`. Tells the preview window what to show.
pub const DRAG_PREVIEW_EVENT: &str = "poros://drag-preview";
/// Mirrored in `src/lib/ipc.ts`. Gives the preview window the picture of the dragged pane.
pub const DRAG_PREVIEW_PICTURE_EVENT: &str = "poros://drag-preview-picture";
/// How long the preview waits for the window a tab dropped on the desktop opens in, in case it
/// never says it has drawn.
const HAND_OFF_LIMIT: Duration = Duration::from_secs(3);

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

/// The preview of a dragged tab, drawn where the window it is dragged in cannot draw: past its
/// edge and out on the desktop. Its window opens on the first tab drag and is hidden, not
/// closed, between drags so it shows at once the next time.
#[derive(Default)]
pub struct DragPreview(Mutex<PreviewState>);

#[derive(Default)]
struct PreviewState {
    /// What the preview shows while a drag wants it; none while it is hidden.
    content: Option<serde_json::Value>,
    /// The latest drag's picture of its pane, which the page draws the pane from.
    picture: Option<serde_json::Value>,
    /// The preview page has loaded and listens for content.
    ready: bool,
    /// The window is showing; it is shown once per time it is wanted.
    visible: bool,
    /// The window a tab dropped on the desktop is opening in. The preview stays in its place
    /// until it has drawn, so nothing flickers in between.
    successor: Option<String>,
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
            state.visible = false;
            open_preview(app, corner, size)?;
            return Ok(());
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

    /// Keeps the picture of the dragged pane and passes it to the page.
    pub fn picture(&self, app: &AppHandle, picture: serde_json::Value) -> tauri::Result<()> {
        if !can_place_windows() {
            return Ok(());
        }
        let mut state = self.0.lock().unwrap();
        if state.ready {
            if let Some(window) = app.get_webview_window(DRAG_PREVIEW_WINDOW) {
                window.emit_to(DRAG_PREVIEW_WINDOW, DRAG_PREVIEW_PICTURE_EVENT, &picture)?;
            }
        }
        state.picture = Some(picture);
        Ok(())
    }

    /// Opens the preview hidden and empty, so its page has loaded by the time a drag first
    /// needs it.
    pub fn prepare(&self, app: &AppHandle) -> tauri::Result<()> {
        if !can_place_windows() {
            return Ok(());
        }
        let mut state = self.0.lock().unwrap();
        if app.get_webview_window(DRAG_PREVIEW_WINDOW).is_some() {
            return Ok(());
        }
        state.ready = false;
        state.visible = false;
        open_preview(
            app,
            PhysicalPosition::new(0, 0),
            LogicalSize::new(640.0, 420.0),
        )?;
        Ok(())
    }

    /// Keeps the preview up for the window a tab dropped on the desktop is opening in, until
    /// that window says it has drawn, or for a few seconds if it never does.
    pub fn hand_off(&self, app: &AppHandle, label: &str) {
        let mut state = self.0.lock().unwrap();
        if state.content.is_none() {
            return;
        }
        state.successor = Some(label.to_owned());
        let app = app.clone();
        let label = label.to_owned();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(HAND_OFF_LIMIT).await;
            let _ = app.state::<DragPreview>().taken_over(&app, &label);
        });
    }

    /// A window has drawn: if the preview was standing in for it, the preview leaves.
    pub fn taken_over(&self, app: &AppHandle, label: &str) -> tauri::Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.successor.as_deref() != Some(label) {
            return Ok(());
        }
        hide_preview(app, &mut state)
    }

    /// Marks the preview page ready and returns what it should show, if anything.
    pub fn ready(&self) -> PreviewStart {
        let mut state = self.0.lock().unwrap();
        state.ready = true;
        PreviewStart {
            content: state.content.clone(),
            picture: state.picture.clone(),
        }
    }

    /// Shows the preview once its page has drawn the content, unless the drag left meanwhile.
    pub fn reveal(&self, window: &WebviewWindow) -> tauri::Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.content.is_none() || state.visible {
            return Ok(());
        }
        state.visible = true;
        show_without_focus(window)
    }

    pub fn hide(&self, app: &AppHandle) -> tauri::Result<()> {
        let mut state = self.0.lock().unwrap();
        hide_preview(app, &mut state)
    }
}

/// What the preview page shows when it has loaded.
#[derive(Serialize)]
pub struct PreviewStart {
    content: Option<serde_json::Value>,
    picture: Option<serde_json::Value>,
}

fn hide_preview(app: &AppHandle, state: &mut PreviewState) -> tauri::Result<()> {
    state.successor = None;
    state.visible = false;
    if state.content.take().is_none() {
        return Ok(());
    }
    match app.get_webview_window(DRAG_PREVIEW_WINDOW) {
        Some(window) => hide_window(&window),
        None => Ok(()),
    }
}

/// Opens the preview window hidden. It never takes focus, which would end the drag, and stays
/// above the window the tab is dragged from. It is resizable only so that it can be sized as it
/// follows the pane: some systems ignore new sizes for windows that are not.
fn open_preview(
    app: &AppHandle,
    corner: PhysicalPosition<i32>,
    size: LogicalSize<f64>,
) -> tauri::Result<WebviewWindow> {
    let opening = opening_corner(app, corner);
    let window = WebviewWindowBuilder::new(
        app,
        DRAG_PREVIEW_WINDOW,
        WebviewUrl::App("index.html".into()),
    )
    .title("Poros")
    .decorations(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .focused(false)
    .focusable(false)
    .visible(false)
    .inner_size(size.width, size.height)
    .position(opening.x, opening.y)
    .build()?;
    place(&window, corner, size)?;
    Ok(window)
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
