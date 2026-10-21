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
/// Mirrored in `src/lib/ipc.ts`. Tells every window that the preview is showing a drag's tab,
/// with the drag, so the window it is dragged in can leave the tab to the preview.
pub const DRAG_PREVIEW_SHOWN_EVENT: &str = "poros://drag-preview-shown";
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

/// How far in from a window's outer corner its content starts, in physical pixels. Windows keeps
/// invisible borders around a frameless window that has a shadow, to resize it by, and moves a
/// window by its outer corner.
pub fn content_inset(window: &WebviewWindow) -> tauri::Result<PhysicalPosition<i32>> {
    let inner = window.inner_position()?;
    let outer = window.outer_position()?;
    Ok(PhysicalPosition::new(inner.x - outer.x, inner.y - outer.y))
}

/// Moves a window so its content's top left corner lands on a point in physical pixels, then
/// sizes its content in logical pixels. `inset` is how far in from the window's outer corner its
/// content starts. Logical positions mean different things on monitors scaled differently, and a
/// window converts them with the scale of the monitor it is leaving, so it would land elsewhere.
pub fn place(
    window: &WebviewWindow,
    corner: PhysicalPosition<i32>,
    inset: PhysicalPosition<i32>,
    size: LogicalSize<f64>,
) -> tauri::Result<()> {
    window.set_position(PhysicalPosition::new(
        corner.x - inset.x,
        corner.y - inset.y,
    ))?;
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
pub struct DragPreview {
    state: Mutex<PreviewState>,
    /// The preview is see-through around what it draws, so it draws the pane's shadow too.
    see_through: bool,
}

/// What the drag preview can do on this system.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSupport {
    /// It can follow the pointer.
    available: bool,
    /// It can be see-through, and so draw the pane as it looks in its window, shadow and all.
    see_through: bool,
}

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
    /// How far in from the window's outer corner its page starts.
    inset: PhysicalPosition<i32>,
}

impl DragPreview {
    pub fn new(see_through: bool) -> Self {
        Self {
            state: Mutex::default(),
            see_through,
        }
    }

    pub fn support(&self) -> PreviewSupport {
        PreviewSupport {
            available: can_place_windows(),
            see_through: self.see_through,
        }
    }

    /// Moves the preview and sends its page what to show; the page shows the window once it has
    /// drawn that, so the window never appears with the previous drag's tab.
    pub fn place(&self, app: &AppHandle, placement: PreviewPlacement) -> tauri::Result<()> {
        if !can_place_windows() {
            return Ok(());
        }
        let mut state = self.state.lock().unwrap();
        let corner = PhysicalPosition::new(placement.x, placement.y);
        let size = LogicalSize::new(placement.width, placement.height);
        let Some(window) = app.get_webview_window(DRAG_PREVIEW_WINDOW) else {
            state.content = Some(placement.content);
            state.ready = false;
            state.visible = false;
            state.inset = open_preview(app, corner, size, self.see_through)?;
            return Ok(());
        };
        place(&window, corner, state.inset, size)?;
        // Windows put on top since it showed, such as the taskbar, would cover it.
        if state.visible {
            raise_without_focus(&window)?;
        }
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
        let mut state = self.state.lock().unwrap();
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
        let mut state = self.state.lock().unwrap();
        if app.get_webview_window(DRAG_PREVIEW_WINDOW).is_some() {
            return Ok(());
        }
        state.ready = false;
        state.visible = false;
        state.inset = open_preview(
            app,
            PhysicalPosition::new(0, 0),
            LogicalSize::new(640.0, 420.0),
            self.see_through,
        )?;
        Ok(())
    }

    /// Keeps the preview up for the window a tab dropped on the desktop is opening in, until
    /// that window says it has drawn, or for a few seconds if it never does.
    pub fn hand_off(&self, app: &AppHandle, label: &str) {
        let mut state = self.state.lock().unwrap();
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
        let mut state = self.state.lock().unwrap();
        if state.successor.as_deref() != Some(label) {
            return Ok(());
        }
        hide_preview(app, &mut state)
    }

    /// Marks the preview page ready and returns what it should show, if anything.
    pub fn ready(&self) -> PreviewStart {
        let mut state = self.state.lock().unwrap();
        state.ready = true;
        PreviewStart {
            content: state.content.clone(),
            picture: state.picture.clone(),
        }
    }

    /// Shows the preview once its page has drawn the content, unless the drag left meanwhile,
    /// and tells the windows whose drag it shows.
    pub fn reveal(&self, window: &WebviewWindow) -> tauri::Result<()> {
        let mut state = self.state.lock().unwrap();
        let Some(content) = &state.content else {
            return Ok(());
        };
        if state.visible {
            return Ok(());
        }
        let drag = content.get("drag").cloned().unwrap_or_default();
        state.visible = true;
        show_without_focus(window)?;
        window.emit(DRAG_PREVIEW_SHOWN_EVENT, drag)
    }

    pub fn hide(&self, app: &AppHandle) -> tauri::Result<()> {
        let mut state = self.state.lock().unwrap();
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

/// Opens the preview window hidden, and returns how far in from its outer corner its page
/// starts. It never takes focus, which would end the drag, and stays above the window the tab is
/// dragged from. Where it can be see-through, the system draws no frame, corners or shadow of its
/// own: the page draws the pane as it looks in its window. It is resizable only so that it can be
/// sized as it follows the pane: some systems ignore new sizes for windows that are not.
fn open_preview(
    app: &AppHandle,
    corner: PhysicalPosition<i32>,
    size: LogicalSize<f64>,
    see_through: bool,
) -> tauri::Result<PhysicalPosition<i32>> {
    let opening = opening_corner(app, corner);
    let window = WebviewWindowBuilder::new(
        app,
        DRAG_PREVIEW_WINDOW,
        WebviewUrl::App("index.html".into()),
    )
    .title("Poros")
    .decorations(false)
    .transparent(see_through)
    .shadow(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .focused(false)
    .focusable(false)
    .visible(false)
    .inner_size(size.width, size.height)
    .position(opening.x, opening.y)
    .build()?;
    if see_through {
        undecorate(&window)?;
    }
    let inset = content_inset(&window)?;
    place(&window, corner, inset, size)?;
    Ok(inset)
}

/// Whether windows can be see-through: Windows composites every window, and Linux does under a
/// compositing window manager.
#[cfg(windows)]
pub fn can_see_through_windows() -> bool {
    true
}

#[cfg(target_os = "linux")]
pub fn can_see_through_windows() -> bool {
    gtk::gdk::Screen::default().is_some_and(|screen| screen.is_composited())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn can_see_through_windows() -> bool {
    false
}

/// Keeps Windows 11 from rounding a window's corners and drawing a border around it, which it
/// does for some frameless windows.
#[cfg(windows)]
fn undecorate(window: &WebviewWindow) -> tauri::Result<()> {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
    };
    set_corners(window, WindowCorners::Square)?;
    let hwnd = window.hwnd()?;
    let color: u32 = DWMWA_COLOR_NONE;
    // Windows 10 has no such border and refuses the attribute.
    unsafe {
        DwmSetWindowAttribute(
            hwnd.0,
            DWMWA_BORDER_COLOR as u32,
            (&color as *const u32).cast(),
            std::mem::size_of_val(&color) as u32,
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn undecorate(_window: &WebviewWindow) -> tauri::Result<()> {
    Ok(())
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

/// Shows the preview above every other window without taking focus from the window the tab is
/// dragged from, which would end the drag. The preview is opened once and only hidden between
/// drags, and showing a window leaves it where it was among the others: windows put on top in the
/// meantime, and anything that took its always on top away, would cover it. So it is put on top
/// each time it shows. Windows activates a window it shows unless asked not to.
#[cfg(windows)]
fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SWP_SHOWWINDOW;
    set_topmost(window, SWP_SHOWWINDOW)
}

#[cfg(not(windows))]
fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    window.set_always_on_top(true)?;
    window.show()
}

/// Puts the preview back on top of every other window, without focus.
#[cfg(windows)]
fn raise_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    set_topmost(window, 0)
}

#[cfg(not(windows))]
fn raise_without_focus(_window: &WebviewWindow) -> tauri::Result<()> {
    Ok(())
}

/// Moves a window to the top of the windows kept on top, and keeps it there, with `extra`
/// flags. Without waiting for the window's thread, as `ShowWindowAsync` did.
#[cfg(windows)]
fn set_topmost(
    window: &WebviewWindow,
    extra: windows_sys::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS,
) -> tauri::Result<()> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let hwnd = window.hwnd()?;
    let flags = SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | extra;
    unsafe { SetWindowPos(hwnd.0, HWND_TOPMOST, 0, 0, 0, 0, flags) };
    Ok(())
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
