//! The Linux side: a GTK drag offering the files as `text/uri-list`, which GNOME Files, Dolphin,
//! the desktop and other programs take. Files from a server are offered once their downloads
//! are in, so GTK keeps the windows running while the program they were dropped on waits.
//! Dolphin and the KDE desktop are also told of KDE's extract-on-drop protocol, which Ark uses:
//! they answer over D-Bus with the folder the files were dropped in, and Poros downloads them
//! straight there.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gtk::glib::translate::{FromGlib, FromGlibPtrFull, ToGlibPtr};
use gtk::glib::{self, SignalHandlerId, SourceId};
use gtk::prelude::*;
use gtk::{gdk, TargetList};
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::{oneshot, OnceCell};

use super::{drag_failed, file_uri, DragOutOutcome, Dragged, Staging, STAGING_POLL};
use crate::error::AppResult;
use crate::events::{Events, LogLevel};
use crate::transfer::{Direction, EnqueueRequest, TransferManager};

/// Ark's formats: the D-Bus name to call and the object there that takes the drop's folder.
const ARK_SERVICE_TARGET: &str = "application/x-kde-ark-dndextract-service";
const ARK_PATH_TARGET: &str = "application/x-kde-ark-dndextract-path";
const URIS: u32 = 1;
const ARK_SERVICE: u32 = 2;
const ARK_PATH: u32 = 3;
/// How long after a drop a KDE program may still ask for the files.
const ARK_ANSWER_LIMIT: Duration = Duration::from_secs(60);
/// GTK's timer that gives up on a drop the program it landed on has not finished, and how long
/// that timer runs.
const GTK_DROP_TIMER: &str = "[gtk+] gtk_drag_abort_timeout";
const GTK_DROP_LIMIT: Duration = Duration::from_secs(300);

/// KDE programs read the dragged files' list before asking where to download them over D-Bus,
/// and give up on the list after a second or so, so in a KDE session the list is never held
/// back until the downloads are in.
fn is_kde_session() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| names_kde(&desktop))
}

fn names_kde(desktop: &str) -> bool {
    desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("KDE"))
}

/// Where a KDE program sends the folder a drop landed in.
#[derive(Debug, Clone)]
struct ArkOffer {
    service: String,
    path: String,
}

/// The session bus connection that takes KDE drops, opened on the first drag from a server.
#[derive(Default)]
pub(crate) struct KdeDrops {
    connection: OnceCell<Option<zbus::Connection>>,
    next_id: AtomicU64,
}

impl KdeDrops {
    async fn connection(&self) -> Option<&zbus::Connection> {
        self.connection
            .get_or_init(|| async {
                zbus::Connection::session()
                    .await
                    .inspect_err(|error| log::info!("No session bus for KDE drops: {error}"))
                    .ok()
            })
            .await
            .as_ref()
    }

    /// Listens for a KDE program naming the folder `staging`'s files were dropped in.
    async fn offer(&self, app: &AppHandle, staging: &Arc<Staging>) -> Option<ArkOffer> {
        let connection = self.connection().await?;
        let service = connection.unique_name()?.to_string();
        let path = format!(
            "/org/poros/DragOut/{}",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let extract = DndExtract {
            app: app.clone(),
            staging: staging.clone(),
            taken: AtomicBool::new(false),
        };
        match connection.object_server().at(path.as_str(), extract).await {
            Ok(true) => Some(ArkOffer { service, path }),
            Ok(false) => None,
            Err(error) => {
                log::info!("Could not listen for KDE drops: {error}");
                None
            }
        }
    }

    /// Stops listening a while after the drag ends: the program the files were dropped on asks
    /// for them once the drop is over.
    fn withdraw_later(&self, offer: ArkOffer) {
        let Some(Some(connection)) = self.connection.get().cloned() else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(ARK_ANSWER_LIMIT).await;
            let _ = connection
                .object_server()
                .remove::<DndExtract, _>(offer.path.as_str())
                .await;
        });
    }
}

/// Ark's `org.kde.ark.DndExtract`, which Dolphin and the Plasma desktop call with the folder
/// a drop landed in.
struct DndExtract {
    app: AppHandle,
    staging: Arc<Staging>,
    taken: AtomicBool,
}

#[zbus::interface(name = "org.kde.ark.DndExtract")]
impl DndExtract {
    #[zbus(name = "extractSelectedFilesTo")]
    async fn extract_selected_files_to(&self, folder: String) -> zbus::fdo::Result<()> {
        if !Path::new(&folder).is_absolute() || !Path::new(&folder).is_dir() {
            return Err(zbus::fdo::Error::Failed(format!(
                "Poros can only download into a folder on this computer, not {folder}"
            )));
        }
        if self.taken.swap(true, Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed(
                "These files were already dropped".into(),
            ));
        }
        self.staging.discard();
        let staging = &self.staging;
        let request = EnqueueRequest {
            session_id: staging.session_id.clone(),
            source_session_id: None,
            direction: Direction::Download,
            target_directory: folder.clone(),
            items: staging.items.clone(),
        };
        let transfers = self.app.state::<TransferManager>();
        let events = self.app.state::<Events>();
        match transfers.enqueue(request).await {
            Ok(count) => {
                events.log(
                    LogLevel::Info,
                    Some(&staging.session_id),
                    format!(
                        "Downloading {count} dropped {} to {folder}",
                        if count == 1 { "item" } else { "items" }
                    ),
                );
                Ok(())
            }
            Err(error) => {
                events.log(LogLevel::Error, Some(&staging.session_id), &error.message);
                Err(zbus::fdo::Error::Failed(error.message))
            }
        }
    }
}

/// One GTK drag, read and changed only on the thread the windows run on.
struct GtkDrag {
    dragged: DraggedFiles,
    ark: Option<ArkOffer>,
    /// The file list waits for the downloads to finish.
    hold_list: bool,
    /// The id GLib gave the first event source added once the drag began.
    first_source: Cell<u32>,
    /// The program the files were dropped on is waiting for the downloads.
    waiting: Cell<bool>,
    /// The pointer is over a Poros window. Its page asks for the files at once and takes the
    /// drop itself, so it gets the list without waiting.
    over_own: Cell<bool>,
    /// Why GTK said the drag failed, if it did.
    failed: Cell<Option<gtk::DragResult>>,
    ended: Cell<bool>,
    handlers: RefCell<Vec<(gtk::Widget, SignalHandlerId)>>,
    /// The pages of the window the drag began in, one of which the mouse button went down on.
    pressed_pages: RefCell<Vec<gtk::Widget>>,
    done: RefCell<Option<oneshot::Sender<AppResult<DragOutOutcome>>>>,
}

enum DraggedFiles {
    Local(Vec<PathBuf>),
    Remote(Arc<Staging>),
}

impl GtkDrag {
    fn provide(&self, data: &gtk::SelectionData, info: u32) {
        match info {
            ARK_SERVICE => {
                if let Some(ark) = &self.ark {
                    data.set(&data.target(), 8, ark.service.as_bytes());
                }
            }
            ARK_PATH => {
                if let Some(ark) = &self.ark {
                    data.set(&data.target(), 8, ark.path.as_bytes());
                }
            }
            URIS => {
                let paths = match &self.dragged {
                    DraggedFiles::Local(paths) => paths.clone(),
                    DraggedFiles::Remote(staging) => {
                        if self.hold_list {
                            self.wait_for(staging);
                        }
                        // The drop was given up on before everything arrived.
                        if self.ended.get() && !staging.is_finished() {
                            Vec::new()
                        } else {
                            staging.arrived()
                        }
                    }
                };
                let uris: Vec<String> = paths.iter().map(|path| file_uri(path)).collect();
                let uris: Vec<&str> = uris.iter().map(String::as_str).collect();
                data.set_uris(&uris);
            }
            _ => {}
        }
    }

    /// Keeps the windows running until the downloads are in. The program that asked for the
    /// files waits for them, and the drop lands then.
    fn wait_for(&self, staging: &Staging) {
        let context = glib::MainContext::default();
        let mut held = HeldDropTimer::new(self.first_source.get());
        self.waiting.set(true);
        let due = Rc::new(Cell::new(true));
        let tick = {
            let due = due.clone();
            glib::timeout_add_local(STAGING_POLL, move || {
                due.set(true);
                glib::ControlFlow::Continue
            })
        };
        loop {
            // A Poros window's own page asks while the pointer is over it, and takes no files.
            if self.ended.get() || self.over_own.get() {
                break;
            }
            if due.replace(false) {
                held.hold(&context);
                if staging.is_finished() {
                    break;
                }
            }
            context.iteration(true);
        }
        self.waiting.set(false);
        tick.remove();
    }

    fn finish(&self) {
        if self.ended.replace(true) {
            return;
        }
        let unfinished = self.waiting.get()
            && matches!(&self.dragged, DraggedFiles::Remote(staging) if !staging.is_finished());
        // A drop on a Poros window counts as dropped too: its page takes it from there.
        let outcome = match self.failed.get() {
            None | Some(gtk::DragResult::TimeoutExpired) if unfinished => Err(drag_failed(
                "the program they were dropped on stopped waiting for the download. Use Download for files that take long to arrive.",
            )),
            None => Ok(DragOutOutcome::Dropped),
            Some(_) => Ok(DragOutOutcome::Cancelled),
        };
        for (widget, handler) in self.handlers.borrow_mut().drain(..) {
            widget.disconnect(handler);
        }
        for page in self.pressed_pages.borrow_mut().drain(..) {
            release_button(&page);
        }
        if let Some(done) = self.done.borrow_mut().take() {
            let _ = done.send(outcome);
        }
    }
}

/// The page the drag began on saw the mouse button go down but never come back up, since the
/// system's drag took the release, and would take the next press for a second button held
/// with the first, which pages ignore. It is sent a release away from everything on it.
fn release_button(page: &gtk::Widget) {
    let Some(window) = page.window() else {
        return;
    };
    let Some(pointer) = page
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer())
    else {
        return;
    };
    // SAFETY: `gdk_event_new` returns a new button release, so `button` is the union member in
    // use. The handle owns the event, and the window reference it takes is released with it.
    let mut event: gdk::Event = unsafe {
        let raw = gdk::ffi::gdk_event_new(gdk::ffi::GDK_BUTTON_RELEASE);
        let button = &mut (*raw).button;
        button.window = window.to_glib_full();
        button.send_event = 1;
        button.x = -1.0;
        button.y = -1.0;
        button.state = gdk::ffi::GDK_BUTTON1_MASK;
        button.button = 1;
        FromGlibPtrFull::from_glib_full(raw)
    };
    event.set_device(Some(&pointer));
    page.event(&event);
}

/// The id GLib gives the next event source added to `context`.
fn next_source_id(context: &glib::MainContext) -> u32 {
    let marker = glib::idle_source_new(None, glib::Priority::LOW, || glib::ControlFlow::Break);
    let id = marker.attach(Some(context));
    let raw = id.as_raw();
    id.remove();
    raw
}

/// GTK gives up on a drop five minutes after the button is let go when the program it landed on
/// has not finished taking it, which a large download can outlast. Its timer is held back while
/// the program waits for the downloads, and runs again from the start once they are in.
struct HeldDropTimer {
    /// Event sources from this id on have not been looked at yet.
    unchecked: u32,
    timer: Option<glib::Source>,
}

impl HeldDropTimer {
    /// Looks for the timer among the event sources added from id `first` on.
    fn new(first: u32) -> Self {
        Self {
            unchecked: first.max(1),
            timer: None,
        }
    }

    /// Holds the timer once GTK has started it. Programs such as GNOME Files ask for the files
    /// while the pointer is still over them, before the button is let go and the timer starts,
    /// so this is called again until the timer is found.
    fn hold(&mut self, context: &glib::MainContext) {
        if self.timer.is_some() {
            return;
        }
        let next = next_source_id(context);
        let timer = (self.unchecked..next).find_map(|raw| {
            // SAFETY: the id is only looked up, never removed through this handle.
            let id = unsafe { SourceId::from_glib(raw) };
            let source = context.find_source_by_id(&id)?;
            (source.name().as_deref() == Some(GTK_DROP_TIMER)).then_some(source)
        });
        self.unchecked = next;
        if let Some(timer) = &timer {
            // SAFETY: `timer` is a live source; a ready time of -1 keeps it from firing.
            unsafe { glib::ffi::g_source_set_ready_time(timer.to_glib_none().0, -1) };
        }
        self.timer = timer;
    }
}

impl Drop for HeldDropTimer {
    fn drop(&mut self) {
        let Some(timer) = self.timer.take() else {
            return;
        };
        if timer.is_destroyed() {
            return;
        }
        let limit = i64::try_from(GTK_DROP_LIMIT.as_micros()).unwrap_or(i64::MAX);
        let due = glib::monotonic_time().saturating_add(limit);
        // SAFETY: `timer` is a live source, kept alive by this handle.
        unsafe { glib::ffi::g_source_set_ready_time(timer.to_glib_none().0, due) };
    }
}

/// Every widget below `widget` that shows a page, where a drop on a Poros window lands.
fn web_views(widget: &gtk::Widget, found: &mut Vec<gtk::Widget>) {
    if widget.type_().name() == "WebKitWebView" {
        found.push(widget.clone());
        return;
    }
    if let Some(container) = widget.downcast_ref::<gtk::Container>() {
        for child in container.children() {
            web_views(&child, found);
        }
    }
}

fn begin(window: &WebviewWindow, windows: &[WebviewWindow], drag: Rc<GtkDrag>) -> AppResult<()> {
    let source = window.gtk_window().map_err(drag_failed)?;
    let targets = TargetList::new(&[]);
    targets.add_uri_targets(URIS);
    if drag.ark.is_some() {
        targets.add(&gdk::Atom::intern(ARK_SERVICE_TARGET), 0, ARK_SERVICE);
        targets.add(&gdk::Atom::intern(ARK_PATH_TARGET), 0, ARK_PATH);
    }
    let source: gtk::Widget = source.upcast();
    let mut handlers = Vec::new();
    {
        let drag = drag.clone();
        let handler = source.connect_drag_data_get(move |_, _, data, info, _| {
            drag.provide(data, info);
        });
        handlers.push((source.clone(), handler));
    }
    {
        let drag = drag.clone();
        let handler = source.connect_drag_failed(move |_, _, result| {
            drag.failed.set(Some(result));
            glib::Propagation::Proceed
        });
        handlers.push((source.clone(), handler));
    }
    {
        let drag = drag.clone();
        let handler = source.connect_drag_end(move |_, _| {
            let drag = drag.clone();
            // After the signal, so its own handler is not disconnected while it runs.
            glib::idle_add_local_once(move || drag.finish());
        });
        handlers.push((source.clone(), handler));
    }
    let mut pressed = Vec::new();
    web_views(&source, &mut pressed);
    *drag.pressed_pages.borrow_mut() = pressed;
    let mut pages = Vec::new();
    for other in windows {
        if let Ok(other) = other.gtk_window() {
            web_views(other.upcast_ref(), &mut pages);
        }
    }
    for page in pages {
        // Before the page's own handler, which asks for the data at once.
        let over = drag.clone();
        let handler = page.connect_drag_motion(move |_, _, _, _, _| {
            over.over_own.set(true);
            false
        });
        handlers.push((page.clone(), handler));
        let away = drag.clone();
        let handler = page.connect_drag_leave(move |_, _, _| away.over_own.set(false));
        handlers.push((page.clone(), handler));
    }
    *drag.handlers.borrow_mut() = handlers;

    drag.first_source
        .set(next_source_id(&glib::MainContext::default()));
    let actions = match drag.dragged {
        // A program that moves the downloaded files leaves nothing behind to clean up.
        DraggedFiles::Remote(_) => gdk::DragAction::COPY | gdk::DragAction::MOVE,
        DraggedFiles::Local(_) => gdk::DragAction::COPY,
    };
    let Some(context) = source.drag_begin_with_coordinates(&targets, actions, 1, None, -1, -1)
    else {
        for (widget, handler) in drag.handlers.borrow_mut().drain(..) {
            widget.disconnect(handler);
        }
        return Err(drag_failed("the system did not start the drag"));
    };
    let single_folder = match &drag.dragged {
        DraggedFiles::Local(paths) => paths.len() == 1 && paths[0].is_dir(),
        DraggedFiles::Remote(staging) => staging.items.len() == 1 && staging.items[0].is_dir,
    };
    let count = match &drag.dragged {
        DraggedFiles::Local(paths) => paths.len(),
        DraggedFiles::Remote(staging) => staging.items.len(),
    };
    let icon = if single_folder {
        "folder"
    } else if count > 1 {
        "edit-copy"
    } else {
        "text-x-generic"
    };
    context.drag_set_icon_name(icon, 0, 0);
    Ok(())
}

pub(super) async fn drag(
    app: &AppHandle,
    window: &WebviewWindow,
    dragged: &Dragged,
    kde: &KdeDrops,
) -> AppResult<DragOutOutcome> {
    let (files, ark) = match dragged {
        Dragged::Local(paths) => (DraggedFiles::Local(paths.clone()), None),
        Dragged::Remote(staging) => (
            DraggedFiles::Remote(staging.clone()),
            kde.offer(app, staging).await,
        ),
    };
    let (sender, receiver) = oneshot::channel();
    let hold_list = !is_kde_session();
    let windows: Vec<WebviewWindow> = app.webview_windows().into_values().collect();
    let source = window.clone();
    let offer = ark.clone();
    window
        .run_on_main_thread(move || {
            let drag = Rc::new(GtkDrag {
                dragged: files,
                ark: offer,
                hold_list,
                first_source: Cell::new(0),
                waiting: Cell::new(false),
                over_own: Cell::new(false),
                failed: Cell::new(None),
                ended: Cell::new(false),
                handlers: RefCell::new(Vec::new()),
                pressed_pages: RefCell::new(Vec::new()),
                done: RefCell::new(None),
            });
            *drag.done.borrow_mut() = Some(sender);
            if let Err(error) = begin(&source, &windows, drag.clone()) {
                if let Some(done) = drag.done.borrow_mut().take() {
                    let _ = done.send(Err(error));
                }
            }
        })
        .map_err(drag_failed)?;
    let outcome = receiver
        .await
        .unwrap_or_else(|_| Err(drag_failed("the drag ended without an answer")));
    if let Some(ark) = ark {
        kde.withdraw_later(ark);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kde_is_found_among_the_desktop_names() {
        assert!(names_kde("KDE"));
        assert!(names_kde("ubuntu:KDE"));
        assert!(!names_kde("ubuntu:GNOME"));
        assert!(!names_kde("X-Cinnamon"));
        assert!(!names_kde(""));
    }
}
