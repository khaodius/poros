use std::path::Path;

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Writes through a temporary file and a rename, so a crash never leaves a half-written file.
pub fn write_json(file: &Path, value: &impl Serialize) -> AppResult<()> {
    let text = serde_json::to_string_pretty(value).map_err(|error| {
        AppError::invalid(format!("Could not encode {}: {error}", file.display()))
    })?;
    write_text(file, &text)
}

pub fn write_text(file: &Path, text: &str) -> AppResult<()> {
    let with_path =
        |error: std::io::Error| AppError::from(error).with_path(file.display().to_string());
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(with_path)?;
    }
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".tmp");
    std::fs::write(&temporary, text).map_err(with_path)?;
    std::fs::rename(&temporary, file).map_err(with_path)
}
