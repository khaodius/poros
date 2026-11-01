//! The Windows side: an OLE data object that Explorer and other programs read the files from.
//! Files on this computer go as an ordinary file list (`CF_HDROP`). Files from a server go as
//! virtual files (`FileGroupDescriptorW` and `FileContents`), which a program reads only once
//! they are dropped on it. The data object lives in the multithreaded apartment, so those reads
//! wait there for the downloads without holding up the windows, and it lets Explorer read on a
//! thread of its own (`IDataObjectAsyncCapability`) so Explorer is not held up either.

use std::mem::ManuallyDrop;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ::windows::core::{implement, AgileReference, Ref, BOOL, HRESULT, HSTRING};
use ::windows::Win32::Foundation::{
    GlobalFree, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_DVASPECT,
    DV_E_FORMATETC, DV_E_LINDEX, DV_E_TYMED, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, FILETIME,
    HANDLE, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, POINT, S_OK,
};
use ::windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use ::windows::Win32::System::Com::{
    CoIncrementMTAUsage, CoInitializeEx, CoUninitialize, IAdviseSink, IBindCtx, IDataObject,
    IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, IStream, COINIT_MULTITHREADED, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, STGM_READ, STGM_SHARE_DENY_WRITE,
    TYMED_HGLOBAL, TYMED_ISTREAM,
};
use ::windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use ::windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleDuplicateData, ReleaseStgMedium, CF_HDROP,
    CLIPBOARD_FORMAT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use ::windows::Win32::System::SystemServices::{
    MK_CONTROL, MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS,
};
use ::windows::Win32::UI::Shell::{
    IDataObjectAsyncCapability, IDataObjectAsyncCapability_Impl, SHCreateStdEnumFmtEtc,
    SHCreateStreamOnFileEx, CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_PREFERREDDROPEFFECT,
    DROPFILES, FD_ATTRIBUTES, FD_FILESIZE, FD_PROGRESSUI, FD_UNICODE, FD_WRITESTIME,
    FILEDESCRIPTORW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOT,
};
use tauri::{Manager, WebviewWindow};
use tokio::sync::oneshot;

use super::{
    drag_failed, staged_entries, DragOutOutcome, Dragged, PagePoint, StagedEntry, Staging,
};
use crate::error::AppResult;

/// Explorer reads the files after the drop, on a thread of its own, and says when it is done.
/// The staging folder is deleted then, or after this long if it never says.
const EXTRACTION_LIMIT: Duration = Duration::from_secs(12 * 60 * 60);
/// `FILEDESCRIPTORW` names, with the folders above them, fit in this many UTF-16 units.
const MAX_DESCRIPTOR_NAME: usize = 259;
/// 100-nanosecond intervals from 1601, where Windows file times start, to 1970.
const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;

struct Formats {
    descriptor: u16,
    contents: u16,
    preferred_effect: u16,
}

fn formats() -> &'static Formats {
    static FORMATS: OnceLock<Formats> = OnceLock::new();
    FORMATS.get_or_init(|| unsafe {
        Formats {
            descriptor: RegisterClipboardFormatW(CFSTR_FILEDESCRIPTORW) as u16,
            contents: RegisterClipboardFormatW(CFSTR_FILECONTENTS) as u16,
            preferred_effect: RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) as u16,
        }
    })
}

enum Files {
    Local(Vec<PathBuf>),
    Remote(Arc<Staging>),
}

/// What the data object and the drop source share.
struct DragState {
    files: Files,
    /// Let go over another program, which reads the files from now on.
    dropped: AtomicBool,
    /// The program reads the files on a thread of its own and says when it is done.
    extracting: AtomicBool,
    extraction_ended: Mutex<Option<oneshot::Sender<()>>>,
}

fn format_entry(format: u16, tymed: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: tymed as u32,
    }
}

fn global_from(bytes: &[u8]) -> ::windows::core::Result<HGLOBAL> {
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let target = GlobalLock(global);
        if target.is_null() {
            let _ = GlobalFree(Some(global));
            return Err(E_OUTOFMEMORY.into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        // Fails with no error when the memory is unlocked, which is what is wanted.
        let _ = GlobalUnlock(global);
        Ok(global)
    }
}

fn global_medium(global: HGLOBAL) -> STGMEDIUM {
    STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: global },
        pUnkForRelease: ManuallyDrop::new(None),
    }
}

fn as_bytes<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}

/// A `CF_HDROP` file list: a `DROPFILES` header, then the paths, each ending in a zero, and one
/// more zero after the last.
fn drop_files(paths: &[PathBuf]) -> Vec<u8> {
    let header = DROPFILES {
        pFiles: size_of::<DROPFILES>() as u32,
        fWide: true.into(),
        ..Default::default()
    };
    let mut bytes = as_bytes(&header).to_vec();
    for path in paths {
        for unit in path.as_os_str().encode_wide() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn file_time(time: SystemTime) -> FILETIME {
    let since_1970 = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() / 100) as u64;
    let ticks = FILETIME_UNIX_EPOCH + since_1970;
    FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    }
}

fn descriptor_name(entry: &StagedEntry) -> Vec<u16> {
    entry.relative.as_os_str().encode_wide().collect()
}

fn descriptor(entry: &StagedEntry) -> FILEDESCRIPTORW {
    let mut descriptor = FILEDESCRIPTORW::default();
    let mut flags = FD_ATTRIBUTES.0 | FD_PROGRESSUI.0 | FD_UNICODE.0;
    if entry.is_dir {
        descriptor.dwFileAttributes = FILE_ATTRIBUTE_DIRECTORY.0;
    } else {
        flags |= FD_FILESIZE.0;
        descriptor.dwFileAttributes = FILE_ATTRIBUTE_NORMAL.0;
        descriptor.nFileSizeHigh = (entry.size >> 32) as u32;
        descriptor.nFileSizeLow = entry.size as u32;
    }
    if let Some(modified) = entry.modified {
        flags |= FD_WRITESTIME.0;
        descriptor.ftLastWriteTime = file_time(modified);
    }
    descriptor.dwFlags = flags as u32;
    let name = descriptor_name(entry);
    // The struct is packed, so its name is filled in whole rather than through a reference.
    let mut file_name = [0u16; 260];
    file_name[..name.len()].copy_from_slice(&name);
    descriptor.cFileName = file_name;
    descriptor
}

/// A `FILEGROUPDESCRIPTORW`: the count, then one `FILEDESCRIPTORW` for each entry.
fn group_descriptor(entries: &[StagedEntry]) -> Vec<u8> {
    let mut bytes = (entries.len() as u32).to_le_bytes().to_vec();
    for entry in entries {
        bytes.extend_from_slice(as_bytes(&descriptor(entry)));
    }
    bytes
}

/// What another program set on the data object, such as the picture Explorer drags.
struct Stored {
    format: u16,
    global: HGLOBAL,
}

#[implement(IDataObject, IDataObjectAsyncCapability, Agile = false)]
struct DataObject {
    drag: Arc<DragState>,
    /// The entries described last, which `FileContents` requests count from.
    described: Mutex<Arc<Vec<StagedEntry>>>,
    stored: Mutex<Vec<Stored>>,
    async_mode: AtomicBool,
}

impl DataObject {
    fn offered(&self) -> Vec<FORMATETC> {
        let formats = formats();
        let mut offered = match &self.drag.files {
            Files::Local(_) => vec![format_entry(CF_HDROP.0, TYMED_HGLOBAL.0)],
            Files::Remote(_) => vec![
                format_entry(formats.descriptor, TYMED_HGLOBAL.0),
                format_entry(formats.contents, TYMED_ISTREAM.0),
            ],
        };
        offered.push(format_entry(formats.preferred_effect, TYMED_HGLOBAL.0));
        offered
    }

    /// Before the drop, only the dragged items themselves, which may not have arrived yet; after
    /// it, once the downloads are in, everything in them.
    fn describe(&self, staging: &Staging) -> Arc<Vec<StagedEntry>> {
        let mut entries = if self.drag.dropped.load(Ordering::SeqCst) {
            staging.wait_blocking();
            let names: Vec<String> = staging.items.iter().map(|item| item.name.clone()).collect();
            staged_entries(&staging.folder, &names)
        } else {
            staging
                .items
                .iter()
                .map(|item| StagedEntry {
                    relative: PathBuf::from(&item.name),
                    path: staging.folder.join(&item.name),
                    is_dir: item.is_dir,
                    size: item.size,
                    modified: None,
                })
                .collect()
        };
        entries.retain(|entry| {
            let fits = descriptor_name(entry).len() <= MAX_DESCRIPTOR_NAME;
            if !fits {
                log::warn!(
                    "{} is too deep in its folders for Explorer to take it from a drop",
                    entry.relative.display()
                );
            }
            fits
        });
        let entries = Arc::new(entries);
        *self.described.lock().unwrap() = entries.clone();
        entries
    }

    fn contents(&self, staging: &Staging, index: i32) -> ::windows::core::Result<STGMEDIUM> {
        // Programs read contents once the files are dropped; asking before would hold them up.
        if !self.drag.dropped.load(Ordering::SeqCst) {
            return Err(DV_E_FORMATETC.into());
        }
        staging.wait_blocking();
        let described = self.described.lock().unwrap().clone();
        let entry = usize::try_from(index)
            .ok()
            .and_then(|index| described.get(index))
            .filter(|entry| !entry.is_dir)
            .ok_or(DV_E_LINDEX)?;
        let stream: IStream = unsafe {
            SHCreateStreamOnFileEx(
                &HSTRING::from(entry.path.as_path()),
                (STGM_READ | STGM_SHARE_DENY_WRITE).0,
                0,
                false,
                None,
            )?
        };
        Ok(STGMEDIUM {
            tymed: TYMED_ISTREAM.0 as u32,
            u: STGMEDIUM_0 {
                pstm: ManuallyDrop::new(Some(stream)),
            },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }

    fn stored_copy(&self, format: u16) -> Option<::windows::core::Result<STGMEDIUM>> {
        let stored = self.stored.lock().unwrap();
        let entry = stored.iter().find(|entry| entry.format == format)?;
        Some(duplicate(entry.global, format).map(global_medium))
    }
}

fn duplicate(global: HGLOBAL, format: u16) -> ::windows::core::Result<HGLOBAL> {
    let copy =
        unsafe { OleDuplicateData(HANDLE(global.0), CLIPBOARD_FORMAT(format), GMEM_MOVEABLE) };
    if copy.is_invalid() {
        return Err(E_OUTOFMEMORY.into());
    }
    Ok(HGLOBAL(copy.0))
}

impl Drop for DataObject {
    fn drop(&mut self) {
        for entry in self.stored.get_mut().unwrap().drain(..) {
            let _ = unsafe { GlobalFree(Some(entry.global)) };
        }
    }
}

impl IDataObject_Impl for DataObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> ::windows::core::Result<STGMEDIUM> {
        let format = unsafe { format.as_ref() }.ok_or(E_INVALIDARG)?;
        if format.dwAspect != DVASPECT_CONTENT.0 {
            return Err(DV_E_DVASPECT.into());
        }
        let formats = formats();
        let wants = |tymed: i32| format.tymed & tymed as u32 != 0;
        match &self.drag.files {
            Files::Local(paths) if format.cfFormat == CF_HDROP.0 => {
                if !wants(TYMED_HGLOBAL.0) {
                    return Err(DV_E_TYMED.into());
                }
                return global_from(&drop_files(paths)).map(global_medium);
            }
            Files::Remote(staging) if format.cfFormat == formats.descriptor => {
                if !wants(TYMED_HGLOBAL.0) {
                    return Err(DV_E_TYMED.into());
                }
                let entries = self.describe(staging);
                return global_from(&group_descriptor(&entries)).map(global_medium);
            }
            Files::Remote(staging) if format.cfFormat == formats.contents => {
                if !wants(TYMED_ISTREAM.0) {
                    return Err(DV_E_TYMED.into());
                }
                return self.contents(staging, format.lindex);
            }
            _ => {}
        }
        if format.cfFormat == formats.preferred_effect && wants(TYMED_HGLOBAL.0) {
            return global_from(&DROPEFFECT_COPY.0.to_le_bytes()).map(global_medium);
        }
        self.stored_copy(format.cfFormat)
            .unwrap_or(Err(DV_E_FORMATETC.into()))
    }

    fn GetDataHere(
        &self,
        _format: *const FORMATETC,
        _medium: *mut STGMEDIUM,
    ) -> ::windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        let Some(format) = (unsafe { format.as_ref() }) else {
            return E_INVALIDARG;
        };
        let offered = self
            .offered()
            .into_iter()
            .find(|offered| offered.cfFormat == format.cfFormat);
        match offered {
            Some(offered) if offered.tymed & format.tymed != 0 => S_OK,
            Some(_) => DV_E_TYMED,
            None if self
                .stored
                .lock()
                .unwrap()
                .iter()
                .any(|entry| entry.format == format.cfFormat) =>
            {
                S_OK
            }
            None => DV_E_FORMATETC,
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _format_in: *const FORMATETC,
        _format_out: *mut FORMATETC,
    ) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(
        &self,
        format: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> ::windows::core::Result<()> {
        let (Some(format), Some(given)) = (unsafe { format.as_ref() }, unsafe { medium.as_ref() })
        else {
            return Err(E_INVALIDARG.into());
        };
        if given.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let copy = duplicate(unsafe { given.u.hGlobal }, format.cfFormat);
        if release.as_bool() {
            unsafe { ReleaseStgMedium(medium.cast_mut()) };
        }
        let mut stored = self.stored.lock().unwrap();
        if let Some(index) = stored
            .iter()
            .position(|entry| entry.format == format.cfFormat)
        {
            let replaced = stored.remove(index);
            let _ = unsafe { GlobalFree(Some(replaced.global)) };
        }
        stored.push(Stored {
            format: format.cfFormat,
            global: copy?,
        });
        Ok(())
    }

    fn EnumFormatEtc(&self, direction: u32) -> ::windows::core::Result<IEnumFORMATETC> {
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let mut formats = self.offered();
        for entry in self.stored.lock().unwrap().iter() {
            formats.push(format_entry(entry.format, TYMED_HGLOBAL.0));
        }
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }

    fn DAdvise(
        &self,
        _format: *const FORMATETC,
        _advise: u32,
        _sink: Ref<IAdviseSink>,
    ) -> ::windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _connection: u32) -> ::windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> ::windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

impl IDataObjectAsyncCapability_Impl for DataObject_Impl {
    fn SetAsyncMode(&self, async_mode: BOOL) -> ::windows::core::Result<()> {
        self.async_mode
            .store(async_mode.as_bool(), Ordering::SeqCst);
        Ok(())
    }

    fn GetAsyncMode(&self) -> ::windows::core::Result<BOOL> {
        Ok(self.async_mode.load(Ordering::SeqCst).into())
    }

    fn StartOperation(&self, _context: Ref<IBindCtx>) -> ::windows::core::Result<()> {
        self.drag.dropped.store(true, Ordering::SeqCst);
        self.drag.extracting.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn InOperation(&self) -> ::windows::core::Result<BOOL> {
        Ok(self.drag.extracting.load(Ordering::SeqCst).into())
    }

    fn EndOperation(
        &self,
        _result: HRESULT,
        _context: Ref<IBindCtx>,
        _effects: u32,
    ) -> ::windows::core::Result<()> {
        if let Some(ended) = self.drag.extraction_ended.lock().unwrap().take() {
            let _ = ended.send(());
        }
        Ok(())
    }
}

/// Where the drag was let go over a Poros window: the point on screen, and whether Ctrl was
/// held.
#[derive(Clone, Copy)]
struct Release {
    point: POINT,
    copy: bool,
}

#[implement(IDropSource)]
struct DropSource {
    drag: Arc<DragState>,
    source_window: isize,
    own_windows: Vec<isize>,
    returned: Arc<Mutex<Option<Release>>>,
}

impl DropSource {
    /// The Poros window under the pointer, if it is over one.
    fn own_window_under(&self) -> Option<(isize, POINT)> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }.ok()?;
        let root = unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) };
        let root = root.0 as isize;
        self.own_windows.contains(&root).then_some((root, point))
    }
}

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        // Escape or another button cancels, as Windows' own drags do.
        if escape.as_bool() || keys.0 & MK_RBUTTON.0 != 0 {
            return DRAGDROP_S_CANCEL;
        }
        if keys.0 & MK_LBUTTON.0 != 0 {
            return S_OK;
        }
        match self.own_window_under() {
            Some((window, point)) => {
                if window == self.source_window {
                    *self.returned.lock().unwrap() = Some(Release {
                        point,
                        copy: keys.0 & MK_CONTROL.0 != 0,
                    });
                }
                DRAGDROP_S_CANCEL
            }
            None => {
                self.drag.dropped.store(true, Ordering::SeqCst);
                DRAGDROP_S_DROP
            }
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Keeps the multithreaded apartment for the life of the app, so data objects made there stay
/// reachable after the thread that made them ends.
fn keep_multithreaded_apartment() -> AppResult<()> {
    static KEPT: OnceLock<Result<(), String>> = OnceLock::new();
    KEPT.get_or_init(|| {
        unsafe { CoIncrementMTAUsage() }
            .map(|_cookie| ())
            .map_err(|error| error.to_string())
    })
    .clone()
    .map_err(drag_failed)
}

/// Makes the data object in the multithreaded apartment, where other programs' requests are
/// served on threads of their own.
fn data_object(drag: Arc<DragState>) -> AppResult<AgileReference<IDataObject>> {
    keep_multithreaded_apartment()?;
    std::thread::spawn(move || unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let object: IDataObject = DataObject {
            drag,
            described: Mutex::new(Arc::new(Vec::new())),
            stored: Mutex::new(Vec::new()),
            async_mode: AtomicBool::new(true),
        }
        .into();
        let reference = AgileReference::new(&object);
        drop(object);
        CoUninitialize();
        reference
    })
    .join()
    .map_err(|_| drag_failed("the data object could not be made"))?
    .map_err(drag_failed)
}

/// How the system's drag ended on the thread the windows run on.
struct Ended {
    result: HRESULT,
    effect: DROPEFFECT,
    returned: Option<Release>,
}

fn run_drag(
    data: &AgileReference<IDataObject>,
    drag: Arc<DragState>,
    source_window: isize,
    own_windows: Vec<isize>,
) -> AppResult<Ended> {
    let data = data.resolve().map_err(drag_failed)?;
    let returned = Arc::new(Mutex::new(None));
    let source: IDropSource = DropSource {
        drag,
        source_window,
        own_windows,
        returned: returned.clone(),
    }
    .into();
    let mut effect = DROPEFFECT_NONE;
    let result = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
    let returned = *returned.lock().unwrap();
    Ok(Ended {
        result,
        effect,
        returned,
    })
}

fn page_point(window: &WebviewWindow, point: POINT) -> Option<PagePoint> {
    let corner = window.inner_position().ok()?;
    let scale = window.scale_factor().ok()?;
    Some(PagePoint {
        x: f64::from(point.x - corner.x) / scale,
        y: f64::from(point.y - corner.y) / scale,
    })
}

fn window_handle(window: &WebviewWindow) -> Option<isize> {
    window.hwnd().ok().map(|hwnd| hwnd.0 as isize)
}

pub(super) async fn drag(window: &WebviewWindow, dragged: &Dragged) -> AppResult<DragOutOutcome> {
    let (ended_sender, extraction_ended) = oneshot::channel();
    let drag = Arc::new(DragState {
        files: match dragged {
            Dragged::Local(paths) => Files::Local(paths.clone()),
            Dragged::Remote(staging) => Files::Remote(staging.clone()),
        },
        dropped: AtomicBool::new(false),
        extracting: AtomicBool::new(false),
        extraction_ended: Mutex::new(Some(ended_sender)),
    });
    let data = data_object(drag.clone())?;
    let source_window = window_handle(window).ok_or_else(|| drag_failed("no window"))?;
    let own_windows: Vec<isize> = window
        .app_handle()
        .webview_windows()
        .values()
        .filter_map(window_handle)
        .collect();
    let (sender, receiver) = oneshot::channel();
    let state = drag.clone();
    window
        .run_on_main_thread(move || {
            let _ = sender.send(run_drag(&data, state, source_window, own_windows));
        })
        .map_err(drag_failed)?;
    let ended = receiver
        .await
        .unwrap_or_else(|_| Err(drag_failed("the drag ended without an answer")))?;
    if let Some(release) = ended.returned {
        return Ok(DragOutOutcome::Returned {
            at: page_point(window, release.point),
            copy: release.copy,
        });
    }
    let extracting = drag.extracting.load(Ordering::SeqCst);
    let dropped =
        ended.result == DRAGDROP_S_DROP && (ended.effect != DROPEFFECT_NONE || extracting);
    if !dropped {
        return Ok(DragOutOutcome::Cancelled);
    }
    if let Dragged::Remote(staging) = dragged {
        if extracting {
            let staging = staging.clone();
            tauri::async_runtime::spawn(async move {
                let _ = tokio::time::timeout(EXTRACTION_LIMIT, extraction_ended).await;
                staging.discard();
            });
        } else {
            // The program read the files before the drag returned.
            staging.discard();
        }
    }
    Ok(DragOutOutcome::Dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_times_count_from_1601() {
        let time = UNIX_EPOCH + Duration::from_secs(1);
        let converted = file_time(time);
        let ticks =
            (u64::from(converted.dwHighDateTime) << 32) | u64::from(converted.dwLowDateTime);
        assert_eq!(ticks, FILETIME_UNIX_EPOCH + 10_000_000);
    }

    #[test]
    fn file_lists_end_in_two_zeros() {
        let bytes = drop_files(&[PathBuf::from(r"C:\a"), PathBuf::from(r"C:\b")]);
        let header = size_of::<DROPFILES>();
        let units: Vec<u16> = bytes[header..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let expected: Vec<u16> = "C:\\a\0C:\\b\0\0".encode_utf16().collect();
        assert_eq!(units, expected);
    }

    #[test]
    fn group_descriptors_hold_one_descriptor_per_entry() {
        let entries = vec![
            StagedEntry {
                relative: PathBuf::from("photos"),
                path: PathBuf::from(r"C:\staging\photos"),
                is_dir: true,
                size: 0,
                modified: None,
            },
            StagedEntry {
                relative: PathBuf::from(r"photos\beach.jpg"),
                path: PathBuf::from(r"C:\staging\photos\beach.jpg"),
                is_dir: false,
                size: 5_000_000_000,
                modified: Some(UNIX_EPOCH),
            },
        ];
        let bytes = group_descriptor(&entries);
        assert_eq!(bytes.len(), 4 + 2 * size_of::<FILEDESCRIPTORW>());
        assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 2);
        let file = descriptor(&entries[1]);
        let (high, low, attributes) =
            (file.nFileSizeHigh, file.nFileSizeLow, file.dwFileAttributes);
        assert_eq!(high, 1);
        assert_eq!(low, (5_000_000_000u64 & 0xFFFF_FFFF) as u32);
        assert_eq!(attributes, FILE_ATTRIBUTE_NORMAL.0);
        let file_name = file.cFileName;
        let name: Vec<u16> = r"photos\beach.jpg".encode_utf16().collect();
        assert_eq!(&file_name[..name.len()], name.as_slice());
        assert_eq!(file_name[name.len()], 0);
        let folder_attributes = descriptor(&entries[0]).dwFileAttributes;
        assert_eq!(folder_attributes, FILE_ATTRIBUTE_DIRECTORY.0);
    }
}
