//! Cyberduck and Mountain Duck bookmarks: one property list (`.duck` file) per site.

use std::collections::HashMap;

use super::xml::{self, Element};
use super::{parse_port, Candidate, Input, Parsed};
use crate::error::{AppError, AppResult};
use crate::protocol::Protocol;

pub fn parse(input: &Input) -> AppResult<Parsed> {
    let root = xml::parse(&input.text)?;
    let dict = if root.name == "dict" {
        Some(&root)
    } else {
        root.child("dict")
    }
    .ok_or_else(|| AppError::invalid("This is not a Cyberduck bookmark"))?;
    let mut parsed = Parsed::default();
    bookmark(&dictionary(dict), &input.stem(), &mut parsed);
    Ok(parsed)
}

/// A property list dictionary's keys and their text values; nested values are left out.
fn dictionary(dict: &Element) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let mut key: Option<&str> = None;
    for child in &dict.children {
        if child.name == "key" {
            key = Some(child.text.trim());
        } else if let Some(name) = key.take() {
            let value = match child.name.as_str() {
                "true" | "false" => child.name.clone(),
                _ => child.text.trim().to_string(),
            };
            values.insert(name.to_string(), value);
        }
    }
    values
}

fn bookmark(values: &HashMap<String, String>, file_stem: &str, parsed: &mut Parsed) {
    let value = |key: &str| values.get(key).map(String::as_str).unwrap_or_default();
    let host = value("Hostname");
    let name = [value("Nickname"), host, file_stem]
        .into_iter()
        .find(|name| !name.trim().is_empty())
        .unwrap_or_default();
    let identifier = value("Protocol").to_ascii_lowercase();
    let protocol = match identifier.as_str() {
        "sftp" => Protocol::Sftp,
        "ftp" => Protocol::Ftp,
        "ftps" => Protocol::Ftps,
        "dav" => Protocol::WebDav,
        "davs" => Protocol::WebDavs,
        "s3" => Protocol::S3,
        "b2" => Protocol::BackblazeB2,
        "googledrive" | "onedrive" | "box" | "sharepoint" | "dropbox" => {
            parsed.skip(
                name,
                "Cloud drives need a browser sign-in; add it from the Connect dialog",
            );
            return;
        }
        _ => {
            parsed.skip(name, "Poros does not connect with this protocol");
            return;
        }
    };
    if host.trim().is_empty() && !matches!(protocol, Protocol::S3 | Protocol::BackblazeB2) {
        parsed.skip(name, "It has no server address");
        return;
    }
    let anonymous = value("Anonymous Login") == "true";
    let username = if anonymous {
        "anonymous"
    } else {
        value("Username")
    };
    let mut candidate = Candidate::new(protocol, name, host, parse_port(value("Port")), username);
    candidate.use_key_file(value("Private Key File"));
    candidate.set_remote_path(value("Path"));
    candidate.connection.ftp_active =
        protocol.is_ftp() && value("FTP Connect Mode").eq_ignore_ascii_case("active");
    parsed.candidates.push(candidate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::AuthType;
    use std::path::PathBuf;

    fn duck(entries: &str) -> Input {
        let mut input = Input::from_text(
            "x.duck",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\"><dict>{entries}</dict></plist>"
            ),
        );
        input.path = Some(PathBuf::from("/b/3F2A.duck"));
        input
    }

    #[test]
    fn reads_bookmarks() {
        let parsed = parse(&duck(
            "<key>Protocol</key><string>sftp</string>\
             <key>Nickname</key><string>Build box</string>\
             <key>Hostname</key><string>build.example.com</string>\
             <key>Port</key><string>2200</string>\
             <key>Username</key><string>ci</string>\
             <key>Path</key><string>/srv/build</string>\
             <key>Private Key File</key><string>~/.ssh/id_ed25519</string>\
             <key>Labels</key><array><string>work</string></array>",
        ))
        .unwrap();
        let site = &parsed.candidates[0].connection;
        assert_eq!(
            (
                site.name.as_str(),
                site.host.as_str(),
                site.port,
                site.username.as_str()
            ),
            ("Build box", "build.example.com", 2200, "ci")
        );
        assert_eq!(site.remote_path.as_deref(), Some("/srv/build"));
        assert_eq!(site.auth_type, AuthType::PublicKey);

        let parsed = parse(&duck(
            "<key>Protocol</key><string>ftp</string>\
             <key>Hostname</key><string>ftp.example.com</string>\
             <key>Anonymous Login</key><true/>\
             <key>FTP Connect Mode</key><string>active</string>",
        ))
        .unwrap();
        let site = &parsed.candidates[0].connection;
        assert_eq!(site.username, "anonymous");
        assert!(site.ftp_active);
        assert_eq!(site.port, 21);

        let parsed = parse(&duck(
            "<key>Protocol</key><string>googledrive</string>\
             <key>Hostname</key><string>www.googleapis.com</string>",
        ))
        .unwrap();
        assert!(parsed.candidates.is_empty());
        assert_eq!(parsed.skipped[0].name, "www.googleapis.com");
    }
}
