//! Sites written as addresses such as `sftp://user@host:2222/path`: GNOME Files bookmarks
//! (`gtk-3.0/bookmarks`, one address and an optional label per line), KDE Dolphin places
//! (`user-places.xbel`), and the lists SmartFTP exports.

use reqwest::Url;

use super::xml::{self, Element};
use super::{percent_decode, Candidate, Parsed};
use crate::error::AppResult;
use crate::protocol::Protocol;

pub fn parse(text: &str) -> AppResult<Parsed> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let mut parsed = Parsed::default();
    if text.starts_with('<') {
        places(&xml::parse(text)?, &mut Vec::new(), &mut parsed);
        return Ok(parsed);
    }
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (address, label) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if address.contains("://") {
            add(address, label.trim(), &[], &mut parsed);
        }
    }
    Ok(parsed)
}

/// KDE's places file: bookmarks with a title, possibly inside folders.
fn places(element: &Element, folder: &mut Vec<String>, parsed: &mut Parsed) {
    for child in &element.children {
        match child.name.as_str() {
            "bookmark" => {
                if let Some(address) = child.attribute("href") {
                    let title = child.child_text("title").unwrap_or_default();
                    add(address, title, folder, parsed);
                }
            }
            "folder" => {
                folder.push(child.child_text("title").unwrap_or_default().to_string());
                places(child, folder, parsed);
                folder.pop();
            }
            _ => places(child, folder, parsed),
        }
    }
}

fn add(address: &str, label: &str, folder: &[String], parsed: &mut Parsed) {
    let Ok(url) = Url::parse(address) else {
        parsed.skip(
            if label.is_empty() { address } else { label },
            "The address is not valid",
        );
        return;
    };
    let protocol = match url.scheme() {
        "sftp" | "ssh" | "scp" | "fish" => Protocol::Sftp,
        "ftp" => Protocol::Ftp,
        "ftpes" => Protocol::Ftps,
        "ftps" => Protocol::FtpsImplicit,
        "dav" | "webdav" => Protocol::WebDav,
        "davs" | "webdavs" => Protocol::WebDavs,
        // Local folders and web pages are not servers.
        "file" | "http" | "https" | "trash" | "recent" | "remote" | "computer" | "network" => {
            return
        }
        other => {
            let name = if label.is_empty() { address } else { label };
            parsed.skip(name, format!("Poros does not connect with {other}://"));
            return;
        }
    };
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches('[')
        .trim_end_matches(']');
    if host.is_empty() {
        return;
    }
    let mut candidate = Candidate::new(
        protocol,
        label,
        host,
        url.port(),
        &percent_decode(url.username()),
    );
    candidate.folder = folder.to_vec();
    candidate.set_secret(url.password().map(percent_decode));
    candidate.set_remote_path(&percent_decode(url.path()));
    parsed.candidates.push(candidate);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_gtk_bookmarks() {
        let parsed = parse(
            "file:///home/u/Music\n\
             sftp://alice@files.example.com:2222/srv/My%20Files Work files\n\
             davs://cloud.example.com/remote.php/webdav\n\
             ftp://anon:secret@[2001:db8::1]/pub\n\
             smb://nas/share NAS\n",
        )
        .unwrap();
        assert_eq!(parsed.candidates.len(), 3);
        let work = &parsed.candidates[0].connection;
        assert_eq!(
            (
                work.name.as_str(),
                work.host.as_str(),
                work.port,
                work.username.as_str()
            ),
            ("Work files", "files.example.com", 2222, "alice")
        );
        assert_eq!(work.remote_path.as_deref(), Some("/srv/My Files"));
        let cloud = &parsed.candidates[1].connection;
        assert_eq!(cloud.host, "cloud.example.com/remote.php/webdav");
        assert_eq!(cloud.port, 443);
        let ftp = &parsed.candidates[2];
        assert_eq!(ftp.connection.host, "2001:db8::1");
        assert_eq!(ftp.secret.as_deref(), Some("secret"));
        assert_eq!(parsed.skipped.len(), 1);
        assert_eq!(parsed.skipped[0].name, "NAS");
    }

    #[test]
    fn reads_kde_places() {
        let parsed = parse(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xbel>\n\
             <xbel xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\">\n\
             <bookmark href=\"file:///home/u\"><title>Home</title></bookmark>\n\
             <folder><title>Servers</title>\n\
             <bookmark href=\"fish://root@router.lan/\"><title>Router</title>\
             <info><metadata owner=\"http://freedesktop.org\"><bookmark:icon name=\"network-server\"/></metadata></info></bookmark>\n\
             </folder></xbel>",
        )
        .unwrap();
        assert_eq!(parsed.candidates.len(), 1);
        let router = &parsed.candidates[0];
        assert_eq!(router.connection.name, "Router");
        assert_eq!(router.connection.protocol, Protocol::Sftp);
        assert_eq!(router.folder, vec!["Servers"]);
        assert_eq!(router.connection.remote_path, None);
    }
}
