//! The windows open when Poros last closed: each one's tabs, as its page hands them over, and
//! where it was on screen. The main window takes its tabs back on the next start and the other
//! windows open again around it.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalSize, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::desktop;
use crate::error::{AppError, AppResult};
use crate::events::{Events, LogLevel};
use crate::storage;

/// Mirrored in `src/lib/ipc.ts`.
pub const MAIN_WINDOW: &str = "main";

/// Where a window was: its outer top left corner in physical pixels, the size of its content in
/// logical pixels, and whether it filled the screen. The corner and size are those it had
/// before it was maximized, so it can be restored to them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFrame {
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub maximized: bool,
}

/// A monitor's area in physical pixels, with its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// How much of a window's top bar must be on a monitor for the window to open where it was:
/// enough to take hold of it and drag it, in logical pixels. The bar is measured from the
/// window's outer corner, which an invisible resize border can put a few pixels above the screen.
const REACHABLE_WIDTH: f64 = 120.0;
const REACHABLE_HEIGHT: f64 = 12.0;
const TOP_BAR_HEIGHT: f64 = 24.0;

/// Whether a window opened at `frame` could be taken hold of: a stretch of its top edge lies on
/// one of the monitors. Monitors that were unplugged since leave windows nowhere to be seen.
pub fn reachable(frame: &WindowFrame, monitors: &[MonitorArea]) -> bool {
    let (x, y) = (f64::from(frame.x), f64::from(frame.y));
    monitors.iter().any(|monitor| {
        let (left, top) = (f64::from(monitor.x), f64::from(monitor.y));
        let right = left + f64::from(monitor.width);
        let bottom = top + f64::from(monitor.height);
        let width = frame.width * monitor.scale;
        let shown_width = (x + width).min(right) - x.max(left);
        let shown_height = (y + TOP_BAR_HEIGHT * monitor.scale).min(bottom) - y.max(top);
        shown_width >= (REACHABLE_WIDTH * monitor.scale).min(width)
            && shown_height >= REACHABLE_HEIGHT * monitor.scale
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SavedWindow {
    /// What the window's page handed over, which only the page reads.
    #[serde(default)]
    layout: Option<serde_json::Value>,
    #[serde(default)]
    frame: Option<WindowFrame>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SavedWorkspace {
    #[serde(default)]
    windows: BTreeMap<String, SavedWindow>,
}

#[derive(Default)]
struct State {
    windows: BTreeMap<String, SavedWindow>,
    /// Set once the main window has gone, as every other window then closes with it and is kept.
    exiting: bool,
    /// The other windows open again once a run, also when the main window's page loads again.
    reopened: bool,
}

/// A window put back where it was: its layout and frame.
pub struct ReopenedWindow {
    pub layout: serde_json::Value,
    pub frame: Option<WindowFrame>,
}

pub struct WorkspaceStore {
    file: PathBuf,
    state: Mutex<State>,
}

impl WorkspaceStore {
    pub fn load(file: PathBuf, events: &Events) -> Self {
        let windows = match storage::read_json::<SavedWorkspace>(&file) {
            Ok(saved) => saved.unwrap_or_default().windows,
            Err(problem) => {
                events.log(
                    LogLevel::Warn,
                    None,
                    format!("{problem} Poros started without the tabs it had open."),
                );
                BTreeMap::new()
            }
        };
        let windows = windows
            .into_iter()
            .filter(|(_, window)| window.layout.is_some())
            .collect();
        Self {
            file,
            state: Mutex::new(State {
                windows,
                ..State::default()
            }),
        }
    }

    pub fn frame(&self, label: &str) -> Option<WindowFrame> {
        let state = self.state.lock().unwrap();
        state.windows.get(label).and_then(|window| window.frame)
    }

    /// Keeps a window's layout, or forgets it for `None`. `frame` is where the window is now, for
    /// one not seen moving yet: a window that has only just shown can report a corner it is not
    /// at, so a frame its moves and the main window's closing gave stands.
    pub fn save(
        &self,
        label: &str,
        layout: Option<serde_json::Value>,
        frame: Option<FrameUpdate>,
    ) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        let window = state.windows.entry(label.to_string()).or_default();
        window.layout = layout;
        if window.frame.is_none() {
            window.frame = frame.and_then(|update| update.apply(None));
        }
        self.write(&state)
    }

    /// Where a window opened again was put, which it keeps until it moves.
    pub fn placed(&self, label: &str, frame: WindowFrame) {
        let mut state = self.state.lock().unwrap();
        state.windows.entry(label.to_string()).or_default().frame = Some(frame);
    }

    /// Notes a window's move or resize, as it reports it, without writing: the frames go to disk
    /// with the next layout, and when the main window closes. The rest of the frame stays as last
    /// seen, since a window that has not moved since it showed may not know where it is.
    pub fn note(&self, label: &str, change: FrameChange, update: FrameUpdate) {
        let mut state = self.state.lock().unwrap();
        let window = state.windows.entry(label.to_string()).or_default();
        window.frame = change.apply(window.frame, update);
    }

    /// Notes which windows are maximized and writes it all, as the main window closes. Where each
    /// window is comes from its moves; a reading taken now only stands in for one never seen.
    pub fn keep(&self, updates: Vec<(String, FrameUpdate)>) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        for (label, update) in updates {
            let window = state.windows.entry(label).or_default();
            window.frame = update.refresh(window.frame);
        }
        self.write(&state)
    }

    /// The other windows are closing because Poros is: they are kept, to open again.
    pub fn exiting(&self) {
        let mut state = self.state.lock().unwrap();
        state.exiting = true;
        if let Err(error) = self.write(&state) {
            log::warn!("Could not keep the open tabs: {}", error.message);
        }
    }

    /// A window the user closed opens no more, unless Poros is closing.
    pub fn forget(&self, label: &str) {
        let mut state = self.state.lock().unwrap();
        if state.exiting || state.windows.remove(label).is_none() {
            return;
        }
        if let Err(error) = self.write(&state) {
            log::warn!("Could not forget a closed window: {}", error.message);
        }
    }

    /// Takes the windows other than `main` with a layout to open again. Each opens under a new
    /// name and keeps its layout under that.
    pub fn take_others(&self, main: &str) -> AppResult<Vec<ReopenedWindow>> {
        let mut state = self.state.lock().unwrap();
        if std::mem::replace(&mut state.reopened, true) {
            return Ok(Vec::new());
        }
        let labels: Vec<String> = state
            .windows
            .keys()
            .filter(|label| label.as_str() != main)
            .cloned()
            .collect();
        let reopened = labels
            .into_iter()
            .filter_map(|label| state.windows.remove(&label))
            .filter_map(|window| {
                window.layout.map(|layout| ReopenedWindow {
                    layout,
                    frame: window.frame,
                })
            })
            .collect();
        self.write(&state)?;
        Ok(reopened)
    }

    pub fn layout(&self, label: &str) -> Option<serde_json::Value> {
        let state = self.state.lock().unwrap();
        state
            .windows
            .get(label)
            .and_then(|window| window.layout.clone())
    }

    pub fn clear(&self) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        state.windows.clear();
        self.write(&state)
    }

    /// Writes the windows that have a layout; a frame alone is not worth keeping.
    fn write(&self, state: &State) -> AppResult<()> {
        let windows = state
            .windows
            .iter()
            .filter(|(_, window)| window.layout.is_some())
            .map(|(label, window)| (label.clone(), window.clone()))
            .collect();
        storage::write_json(&self.file, &SavedWorkspace { windows })
    }
}

/// A move or resize a window reports: its outer corner in physical pixels, or the size of its
/// content in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameChange {
    Moved { x: i32, y: i32 },
    Resized { width: f64, height: f64 },
}

impl FrameChange {
    /// `update` is the window's whole frame as read now, which fills in what was never seen.
    fn apply(self, previous: Option<WindowFrame>, update: FrameUpdate) -> Option<WindowFrame> {
        let FrameUpdate::Normal(current) = update else {
            return update.apply(previous);
        };
        let frame = WindowFrame {
            maximized: false,
            ..previous.unwrap_or(current)
        };
        Some(match self {
            FrameChange::Moved { x, y } => WindowFrame { x, y, ..frame },
            FrameChange::Resized { width, height } => WindowFrame {
                width,
                height,
                ..frame
            },
        })
    }
}

/// What a window's frame is now. A maximized or minimized window keeps the corner and size it had
/// before, which it goes back to when restored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameUpdate {
    Normal(WindowFrame),
    /// With the frame it fills, for a window that was never seen otherwise.
    Maximized(WindowFrame),
    Minimized,
}

impl FrameUpdate {
    fn apply(self, previous: Option<WindowFrame>) -> Option<WindowFrame> {
        match self {
            FrameUpdate::Normal(frame) => Some(frame),
            FrameUpdate::Maximized(filled) => Some(WindowFrame {
                maximized: true,
                ..previous.unwrap_or(filled)
            }),
            FrameUpdate::Minimized => previous,
        }
    }

    /// Updates whether a window is maximized but keeps where it was seen last, which a reading
    /// that is not a move may not know: Linux reports a window that has not moved since it was
    /// shown at the screen's corner.
    fn refresh(self, previous: Option<WindowFrame>) -> Option<WindowFrame> {
        match (self, previous) {
            (FrameUpdate::Normal(_), Some(frame)) => Some(WindowFrame {
                maximized: false,
                ..frame
            }),
            _ => self.apply(previous),
        }
    }

    /// The frame a window has now, hidden or not.
    pub fn of(window: &WebviewWindow) -> Option<FrameUpdate> {
        if window.is_minimized().unwrap_or(false) {
            return Some(FrameUpdate::Minimized);
        }
        let scale = window.scale_factor().ok()?;
        let size = window.inner_size().ok()?.to_logical::<f64>(scale);
        // Wayland keeps positions from windows; the corner is then never used.
        let corner = window.outer_position().unwrap_or_default();
        let frame = WindowFrame {
            x: corner.x,
            y: corner.y,
            width: size.width,
            height: size.height,
            maximized: false,
        };
        if window.is_maximized().unwrap_or(false) {
            return Some(FrameUpdate::Maximized(frame));
        }
        Some(FrameUpdate::Normal(frame))
    }
}

fn monitor_areas(app: &AppHandle) -> Vec<MonitorArea> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| MonitorArea {
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
            scale: monitor.scale_factor(),
        })
        .collect()
}

/// Puts a window that has not been shown yet back where it was. A window whose corner is on no
/// monitor any more keeps the place it opened at. Maximizing a hidden window maximizes it as it
/// shows.
pub fn place(app: &AppHandle, window: &WebviewWindow, frame: &WindowFrame) -> tauri::Result<()> {
    let placed = desktop::can_place_windows() && reachable(frame, &monitor_areas(app));
    if placed {
        window.set_position(PhysicalPosition::new(frame.x, frame.y))?;
    }
    // After the move, so the size uses the scale of the monitor the window is now on.
    window.set_size(LogicalSize::new(frame.width, frame.height))?;
    if !placed {
        window.center()?;
    }
    if frame.maximized {
        window.maximize()?;
    }
    Ok(())
}

/// Layouts handed from a window to a window it opens: one a tab was torn out into, or one put back
/// as Poros starts.
#[derive(Default)]
pub struct PendingWindows(Mutex<HashMap<String, serde_json::Value>>);

impl PendingWindows {
    pub fn take(&self, label: &str) -> Option<serde_json::Value> {
        self.0.lock().unwrap().remove(label)
    }
}

/// Where a new window goes.
pub enum Opening {
    Centered,
    /// Its content's top left corner at this point, in physical pixels.
    ContentAt(PhysicalPosition<i32>),
    /// Where a window was when Poros closed.
    Frame(WindowFrame),
}

/// Opens a window showing `layout`, hidden until its page has drawn; returns its label.
pub fn open_window(
    app: &AppHandle,
    pending: &PendingWindows,
    layout: serde_json::Value,
    size: LogicalSize<f64>,
    opening: Opening,
) -> AppResult<String> {
    let label = format!(
        "workspace-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    pending.0.lock().unwrap().insert(label.clone(), layout);
    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("Poros")
        .decorations(false)
        .visible(false)
        .inner_size(size.width, size.height)
        .min_inner_size(480.0, 360.0);
    let builder = match opening {
        Opening::ContentAt(corner) => {
            let opening = desktop::opening_corner(app, corner);
            builder.position(opening.x, opening.y)
        }
        Opening::Centered | Opening::Frame(_) => builder.center(),
    };
    let opened = builder.build().and_then(|window| match opening {
        Opening::ContentAt(corner) => {
            let inset = desktop::content_inset(&window)?;
            desktop::place(&window, corner, inset, size)
        }
        Opening::Frame(frame) => place(app, &window, &frame),
        Opening::Centered => Ok(()),
    });
    if let Err(error) = opened {
        pending.0.lock().unwrap().remove(&label);
        return Err(AppError::invalid(format!(
            "Could not open a window: {error}"
        )));
    }
    Ok(label)
}

/// The size a window put back opens with before its frame is applied, when it has none.
const REOPENED_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 640.0);

/// Opens the windows other than the main one that were open when Poros closed.
pub fn reopen_windows(
    app: &AppHandle,
    store: &WorkspaceStore,
    pending: &PendingWindows,
    main: &str,
) -> AppResult<()> {
    for window in store.take_others(main)? {
        let (size, opening) = match window.frame {
            Some(frame) => (
                LogicalSize::new(frame.width, frame.height),
                Opening::Frame(frame),
            ),
            None => (REOPENED_SIZE, Opening::Centered),
        };
        match open_window(app, pending, window.layout, size, opening) {
            Ok(label) => {
                if let Some(frame) = window.frame {
                    store.placed(&label, frame);
                }
            }
            Err(error) => log::warn!("{}", error.message),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: i32, y: i32, width: u32, height: u32) -> WindowFrame {
        WindowFrame {
            x,
            y,
            width: f64::from(width),
            height: f64::from(height),
            maximized: false,
        }
    }

    const FULL_HD: MonitorArea = MonitorArea {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        scale: 1.0,
    };
    /// A 4K monitor at 150% to the right of the first.
    const SCALED: MonitorArea = MonitorArea {
        x: 1920,
        y: 0,
        width: 3840,
        height: 2160,
        scale: 1.5,
    };

    #[test]
    fn a_window_on_a_monitor_is_reachable() {
        assert!(reachable(&frame(100, 100, 1200, 800), &[FULL_HD]));
        assert!(reachable(&frame(2500, 300, 1200, 800), &[FULL_HD, SCALED]));
    }

    #[test]
    fn a_window_mostly_off_the_side_is_reachable_while_enough_of_its_top_shows() {
        assert!(reachable(&frame(-1000, 100, 1200, 800), &[FULL_HD]));
        assert!(!reachable(&frame(-1150, 100, 1200, 800), &[FULL_HD]));
        assert!(reachable(&frame(1780, 100, 1200, 800), &[FULL_HD]));
    }

    #[test]
    fn a_window_against_the_top_of_the_screen_is_reachable() {
        assert!(reachable(&frame(100, -8, 1200, 800), &[FULL_HD]));
    }

    #[test]
    fn a_window_whose_top_is_off_every_monitor_is_not_reachable() {
        assert!(!reachable(&frame(100, -400, 1200, 800), &[FULL_HD]));
        assert!(!reachable(&frame(100, 1075, 1200, 800), &[FULL_HD]));
        assert!(!reachable(&frame(100, 100, 1200, 800), &[]));
    }

    #[test]
    fn a_window_left_on_an_unplugged_monitor_is_not_reachable() {
        assert!(!reachable(&frame(2500, 300, 1200, 800), &[FULL_HD]));
    }

    #[test]
    fn a_maximized_window_keeps_the_frame_it_had_before() {
        let before = frame(10, 20, 900, 700);
        let filled = frame(-8, -8, 1920, 1040);
        assert_eq!(
            FrameUpdate::Maximized(filled).apply(Some(before)),
            Some(WindowFrame {
                maximized: true,
                ..before
            })
        );
        assert_eq!(FrameUpdate::Minimized.apply(Some(before)), Some(before));
        assert_eq!(
            FrameUpdate::Maximized(filled).apply(None),
            Some(WindowFrame {
                maximized: true,
                ..filled
            })
        );
        let after = frame(30, 40, 800, 600);
        assert_eq!(FrameUpdate::Normal(after).apply(Some(before)), Some(after));

        // Closing keeps the frame the moves gave, and whether the window is maximized now.
        let maximized = WindowFrame {
            maximized: true,
            ..before
        };
        assert_eq!(
            FrameUpdate::Normal(frame(0, 0, 900, 700)).refresh(Some(maximized)),
            Some(before)
        );
        assert_eq!(
            FrameUpdate::Maximized(filled).refresh(Some(before)),
            Some(maximized)
        );
        assert_eq!(FrameUpdate::Normal(after).refresh(None), Some(after));
    }

    fn store(folder: &tempfile::TempDir) -> WorkspaceStore {
        WorkspaceStore::load(
            folder.path().join("workspace.json"),
            &Events::keeping_startup_log(),
        )
    }

    #[test]
    fn layouts_and_frames_survive_a_restart() {
        let folder = tempfile::tempdir().unwrap();
        let first = store(&folder);
        first.placed("main", frame(5, 6, 1300, 850));
        first
            .save("main", Some(serde_json::json!({ "tabs": 2 })), None)
            .unwrap();
        first
            .save(
                "workspace-a",
                Some(serde_json::json!({ "tabs": 1 })),
                Some(FrameUpdate::Normal(frame(900, 100, 700, 500))),
            )
            .unwrap();
        first
            .keep(vec![(
                "main".into(),
                FrameUpdate::Maximized(frame(0, 0, 1920, 1040)),
            )])
            .unwrap();

        let second = store(&folder);
        assert_eq!(
            second.layout("main"),
            Some(serde_json::json!({ "tabs": 2 }))
        );
        assert_eq!(
            second.frame("main"),
            Some(WindowFrame {
                maximized: true,
                ..frame(5, 6, 1300, 850)
            })
        );
        let others = second.take_others("main").unwrap();
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].layout, serde_json::json!({ "tabs": 1 }));
        assert_eq!(others[0].frame, Some(frame(900, 100, 700, 500)));

        // Taken windows open under new names, so they are gone from the file.
        assert!(store(&folder).take_others("main").unwrap().is_empty());

        // A page loaded again opens nothing twice.
        let third = store(&folder);
        third
            .save("workspace-b", Some(serde_json::json!({})), None)
            .unwrap();
        assert_eq!(third.take_others("main").unwrap().len(), 1);
        third
            .save("workspace-c", Some(serde_json::json!({})), None)
            .unwrap();
        assert!(third.take_others("main").unwrap().is_empty());
    }

    #[test]
    fn a_window_closed_by_the_user_is_forgotten_but_not_one_closed_with_poros() {
        let folder = tempfile::tempdir().unwrap();
        let workspace = store(&folder);
        let layout = Some(serde_json::json!({}));
        workspace.save("main", layout.clone(), None).unwrap();
        workspace.save("workspace-a", layout.clone(), None).unwrap();
        workspace.save("workspace-b", layout, None).unwrap();
        assert_eq!(store(&folder).take_others("main").unwrap().len(), 2);
        workspace
            .save("workspace-a", Some(serde_json::json!({})), None)
            .unwrap();
        workspace
            .save("workspace-b", Some(serde_json::json!({})), None)
            .unwrap();

        workspace.forget("workspace-a");
        workspace.exiting();
        workspace.forget("workspace-b");

        assert_eq!(store(&folder).take_others("main").unwrap().len(), 1);
    }

    #[test]
    fn moves_and_resizes_change_only_what_they_report() {
        let seen = frame(100, 200, 900, 700);
        let reading = FrameUpdate::Normal(frame(0, 0, 900, 700));
        assert_eq!(
            FrameChange::Resized {
                width: 1000.0,
                height: 800.0
            }
            .apply(Some(seen), reading),
            Some(frame(100, 200, 1000, 800))
        );
        assert_eq!(
            FrameChange::Moved { x: 300, y: 50 }.apply(Some(seen), reading),
            Some(frame(300, 50, 900, 700))
        );
        assert_eq!(
            FrameChange::Moved { x: 300, y: 50 }.apply(None, reading),
            Some(frame(300, 50, 900, 700))
        );
        // Maximizing moves and resizes the window too, which is not where it goes back to.
        let filled = FrameUpdate::Maximized(frame(-8, -8, 1920, 1040));
        assert_eq!(
            FrameChange::Moved { x: -8, y: -8 }.apply(Some(seen), filled),
            Some(WindowFrame {
                maximized: true,
                ..seen
            })
        );
        assert_eq!(
            FrameChange::Moved {
                x: -32000,
                y: -32000
            }
            .apply(Some(seen), FrameUpdate::Minimized),
            Some(seen)
        );
    }

    #[test]
    fn a_frame_from_moves_outlasts_a_reading_taken_as_the_window_shows() {
        let folder = tempfile::tempdir().unwrap();
        let workspace = store(&folder);
        let layout = Some(serde_json::json!({}));
        workspace.placed("workspace-a", frame(900, 100, 700, 500));
        workspace
            .save(
                "workspace-a",
                layout.clone(),
                Some(FrameUpdate::Normal(frame(0, 0, 700, 500))),
            )
            .unwrap();
        assert_eq!(
            workspace.frame("workspace-a"),
            Some(frame(900, 100, 700, 500))
        );

        workspace
            .save(
                "workspace-b",
                layout,
                Some(FrameUpdate::Normal(frame(20, 30, 640, 420))),
            )
            .unwrap();
        assert_eq!(
            workspace.frame("workspace-b"),
            Some(frame(20, 30, 640, 420))
        );
        workspace.note(
            "workspace-b",
            FrameChange::Moved { x: 40, y: 50 },
            FrameUpdate::Normal(frame(0, 0, 640, 420)),
        );
        assert_eq!(
            workspace.frame("workspace-b"),
            Some(frame(40, 50, 640, 420))
        );
    }

    #[test]
    fn a_window_without_a_layout_is_not_written() {
        let folder = tempfile::tempdir().unwrap();
        let workspace = store(&folder);
        workspace.placed("workspace-a", frame(1, 2, 700, 500));
        workspace
            .save("main", Some(serde_json::json!({})), None)
            .unwrap();
        workspace.save("main", None, None).unwrap();
        let written = std::fs::read_to_string(folder.path().join("workspace.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap(),
            serde_json::json!({ "windows": {} })
        );
    }
}
