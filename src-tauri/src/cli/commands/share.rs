//! Share links: addresses that open a file for someone without an account.

use serde_json::json;

use super::{each, expand};
use crate::cli::args::{ShareArgs, ShareAudience, ShareKind};
use crate::cli::context::Context;
use crate::cli::output::{plural, time_iso, Reply};
use crate::error::{AppError, AppResult};
use crate::share::{self, Audience, LinkKind, ShareLink, ShareOptions};

fn kind(choice: ShareKind) -> LinkKind {
    match choice {
        ShareKind::View => LinkKind::View,
        ShareKind::Edit => LinkKind::Edit,
        ShareKind::Download => LinkKind::Download,
    }
}

fn audience(choice: ShareAudience) -> Audience {
    match choice {
        ShareAudience::Anyone => Audience::Anyone,
        ShareAudience::Organization => Audience::Organization,
        ShareAudience::Invited => Audience::Invited,
    }
}

/// A link as the JSON reply shows it. The address is the output itself, and is never
/// logged.
fn link_value(path: &str, link: &ShareLink) -> serde_json::Value {
    json!({
        "path": path,
        "url": link.url,
        "kind": link.kind,
        "audience": link.audience,
        "expiresAt": time_iso(link.expires_at),
        "hasPassword": link.has_password,
        "revocable": link.revocable,
        "notes": link.notes,
    })
}

/// Prints one link per path; what the user should know about a link goes to the log.
pub async fn share(context: &Context, arguments: &ShareArgs) -> AppResult<Reply> {
    let connected = context.connected().await?;
    let files = connected.files();
    let files = files.as_ref();
    let session_id = connected.info.id.as_str();
    let paths = expand(connected, &arguments.paths).await?;
    if arguments.revoke {
        share::check_revocable(files)?;
        return each(&paths, |path| async move {
            let links = share::links(files, path).await?;
            let mut removed: u64 = 0;
            for link in links.iter().filter(|link| link.revocable) {
                share::remove(files, &context.events, session_id, path, &link.id).await?;
                removed += 1;
            }
            let kept = links.len() as u64 - removed;
            if kept > 0 {
                return Err(AppError::unsupported(format!(
                    "{path} is shared through a folder above it; stop sharing that folder"
                ))
                .with_path(path));
            }
            let text = match removed {
                0 => format!("{path} was not shared"),
                removed => format!("Removed {} from {path}", plural(removed, "link")),
            };
            Ok((json!({ "path": path, "removed": removed }), text))
        })
        .await;
    }
    let options = ShareOptions {
        kind: kind(arguments.kind),
        audience: audience(arguments.audience),
        lifetime_secs: arguments.expires,
        password: None,
    };
    let options = &options;
    each(&paths, |path| async move {
        let link = share::create(files, &context.events, session_id, path, options).await?;
        for note in &link.notes {
            log::warn!("{path}: {note}");
        }
        Ok((link_value(path, &link), link.url))
    })
    .await
}
