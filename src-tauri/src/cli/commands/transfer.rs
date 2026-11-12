//! `get` and `put`, with `cp`'s rules for where things land: into a folder that exists, or
//! under a new name when one item goes to a path that does not.

use std::path::{Path, PathBuf};

use super::{absolute_local, expand, make_dirs, missing};
use crate::cli::args::TransferArgs;
use crate::cli::context::Context;
use crate::cli::output::Reply;
use crate::cli::transfers;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::protocol;
use crate::remote_path;
use crate::transfer::{Direction, TransferItem};

/// Where the items go, and the name the single item takes there when it is renamed.
#[derive(Debug, PartialEq, Eq)]
struct Placement {
    folder: String,
    rename_to: Option<String>,
}

/// What is at the destination, which decides the placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    Folder,
    File,
    Nothing,
}

/// `cp`'s rules. A trailing slash says the destination is meant as a folder.
fn placement(
    destination: &str,
    found: Destination,
    sources: &[TransferItem],
    meant_as_folder: bool,
    parent_of: impl Fn(&str) -> Option<String>,
    name_of: impl Fn(&str) -> String,
) -> AppResult<Placement> {
    let into_folder = Placement {
        folder: destination.to_string(),
        rename_to: None,
    };
    let renamed = || -> AppResult<Placement> {
        let folder = parent_of(destination)
            .ok_or_else(|| AppError::invalid(format!("{destination} has no folder above it")))?;
        Ok(Placement {
            folder,
            rename_to: Some(name_of(destination)),
        })
    };
    match (found, sources) {
        (Destination::Folder, _) => Ok(into_folder),
        (Destination::File, [single]) if !single.is_dir && !meant_as_folder => renamed(),
        (Destination::File, _) => Err(AppError::new(
            ErrorKind::AlreadyExists,
            format!("{destination} is a file, not a folder"),
        )
        .with_path(destination)),
        (Destination::Nothing, [_]) if !meant_as_folder => renamed(),
        (Destination::Nothing, _) => Ok(into_folder),
    }
}

pub async fn upload(context: &Context, arguments: &TransferArgs) -> AppResult<Reply> {
    let (destination, sources) = arguments
        .paths
        .split_last()
        .ok_or_else(|| AppError::invalid("Name what to upload and where"))?;
    let mut items = Vec::with_capacity(sources.len());
    for source in sources {
        items.push(local_item(source)?);
    }
    let connected = context.connected().await?;
    let files = connected.files();
    let target = connected.resolve(destination);
    let found = match files.stat(&target).await? {
        Some(stat) if stat.is_dir => Destination::Folder,
        Some(_) => Destination::File,
        None => Destination::Nothing,
    };
    let placement = placement(
        &target,
        found,
        &items,
        destination.ends_with('/'),
        remote_path::parent,
        |path| remote_path::file_name(path).to_string(),
    )?;
    if let Some(name) = &placement.rename_to {
        items[0].name = name.clone();
    }
    match files.stat(&placement.folder).await? {
        Some(stat) if stat.is_dir => {}
        Some(_) => return Err(protocol::not_a_folder(&placement.folder)),
        None if arguments.parents => {
            make_dirs(&*files, &placement.folder).await?;
        }
        None => return Err(missing_folder(&placement.folder)),
    }
    let summary = transfers::run(
        context,
        &connected.info.id,
        Direction::Upload,
        items,
        placement.folder,
        &arguments.options,
    )
    .await?;
    Ok(summary.into_reply("uploaded"))
}

pub async fn download(context: &Context, arguments: &TransferArgs) -> AppResult<Reply> {
    let (destination, sources) = arguments
        .paths
        .split_last()
        .ok_or_else(|| AppError::invalid("Name what to download and where"))?;
    let connected = context.connected().await?;
    let files = connected.files();
    let mut items = Vec::new();
    for source in expand(connected, sources).await? {
        let stat = files.stat(&source).await?.ok_or_else(|| missing(&source))?;
        let name = remote_path::file_name(&source).to_string();
        if name.is_empty() || name == "/" {
            return Err(AppError::invalid(format!(
                "Name a folder inside {source} rather than the top of the server"
            )));
        }
        items.push(TransferItem {
            path: source,
            name,
            is_dir: stat.is_dir,
            size: stat.size,
        });
    }
    let target = absolute_local(destination)?;
    let found = match std::fs::metadata(&target) {
        Ok(metadata) if metadata.is_dir() => Destination::Folder,
        Ok(_) => Destination::File,
        Err(_) => Destination::Nothing,
    };
    let local_text = |path: &Path| path.to_string_lossy().into_owned();
    let placement = placement(
        &local_text(&target),
        found,
        &items,
        destination.ends_with('/') || destination.ends_with(std::path::MAIN_SEPARATOR),
        |path| Path::new(path).parent().map(local_text),
        |path| {
            Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        },
    )?;
    if let Some(name) = &placement.rename_to {
        items[0].name = name.clone();
    }
    let folder = PathBuf::from(&placement.folder);
    if !folder.is_dir() {
        if folder.exists() {
            return Err(protocol::not_a_folder(&placement.folder));
        }
        if !arguments.parents {
            return Err(missing_folder(&placement.folder));
        }
        std::fs::create_dir_all(&folder)
            .map_err(|error| AppError::from(error).with_path(placement.folder.clone()))?;
    }
    let summary = transfers::run(
        context,
        &connected.info.id,
        Direction::Download,
        items,
        placement.folder,
        &arguments.options,
    )
    .await?;
    Ok(summary.into_reply("downloaded"))
}

fn local_item(source: &str) -> AppResult<TransferItem> {
    let path = absolute_local(source)?;
    let shown = path.to_string_lossy().into_owned();
    let metadata = std::fs::metadata(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            missing(&shown)
        } else {
            AppError::from(error).with_path(shown.clone())
        }
    })?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| {
            AppError::invalid(format!("Name a folder inside {shown} rather than the top"))
        })?;
    Ok(TransferItem {
        path: shown,
        name,
        is_dir: metadata.is_dir(),
        size: if metadata.is_dir() { 0 } else { metadata.len() },
    })
}

fn missing_folder(folder: &str) -> AppError {
    AppError::new(
        ErrorKind::NotFound,
        format!("The folder {folder} does not exist; add --parents to create it"),
    )
    .with_path(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, is_dir: bool) -> TransferItem {
        TransferItem {
            path: format!("/local/{name}"),
            name: name.into(),
            is_dir,
            size: 0,
        }
    }

    fn place(
        destination: &str,
        found: Destination,
        sources: &[TransferItem],
        meant_as_folder: bool,
    ) -> AppResult<Placement> {
        placement(
            destination,
            found,
            sources,
            meant_as_folder,
            remote_path::parent,
            |path| remote_path::file_name(path).to_string(),
        )
    }

    #[test]
    fn items_go_into_folders_or_take_a_new_name_as_with_cp() {
        let one = [item("a.txt", false)];
        let two = [item("a.txt", false), item("b", true)];
        let into = |folder: &str| Placement {
            folder: folder.into(),
            rename_to: None,
        };
        let renamed = |folder: &str, name: &str| Placement {
            folder: folder.into(),
            rename_to: Some(name.into()),
        };
        assert_eq!(
            place("/srv", Destination::Folder, &two, false).unwrap(),
            into("/srv")
        );
        assert_eq!(
            place("/srv/new.txt", Destination::Nothing, &one, false).unwrap(),
            renamed("/srv", "new.txt")
        );
        assert_eq!(
            place("/srv/old.txt", Destination::File, &one, false).unwrap(),
            renamed("/srv", "old.txt")
        );
        assert_eq!(
            place("/srv/new", Destination::Nothing, &one, true).unwrap(),
            into("/srv/new")
        );
        assert_eq!(
            place("/srv/new", Destination::Nothing, &two, false).unwrap(),
            into("/srv/new")
        );
        let error = place("/srv/file", Destination::File, &two, false).unwrap_err();
        assert_eq!(error.kind, ErrorKind::AlreadyExists);
        let folder_onto_file = [item("site", true)];
        assert!(place("/srv/file", Destination::File, &folder_onto_file, false).is_err());
    }
}
