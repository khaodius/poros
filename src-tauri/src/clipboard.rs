//! The system clipboard, for text copied with no click behind it: the web view lets a page write
//! the clipboard only while it handles a click or key press on Linux.

use std::sync::{Arc, Mutex, PoisonError};

use arboard::Clipboard;

use crate::error::{AppError, AppResult, ErrorKind};

/// Opened on first use and then kept open: on X11 this process serves the copied text to other
/// programs only for as long as its clipboard stays open.
#[derive(Clone, Default)]
pub struct SystemClipboard(Arc<Mutex<Option<Clipboard>>>);

impl SystemClipboard {
    pub fn write_text(&self, text: String) -> AppResult<()> {
        let mut opened = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let clipboard = match opened.as_mut() {
            Some(clipboard) => clipboard,
            None => opened.insert(Clipboard::new().map_err(clipboard_error)?),
        };
        clipboard.set_text(text).map_err(clipboard_error)
    }
}

fn clipboard_error(error: arboard::Error) -> AppError {
    AppError::new(
        ErrorKind::Io,
        format!("Could not use the clipboard: {error}"),
    )
}
