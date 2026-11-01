//! FileZilla's Site Manager (`sitemanager.xml`), its exports, and `recentservers.xml`.

use base64::Engine;

use super::xml::{self, Element};
use super::{parse_port, Candidate, Parsed};
use crate::connections::AuthType;
use crate::error::{AppError, AppResult};
use crate::protocol::Protocol;

pub fn parse(text: &str) -> AppResult<Parsed> {
    let root = xml::parse(text)?;
    if !root.name.eq_ignore_ascii_case("FileZilla3") {
        return Err(AppError::invalid("This is not a FileZilla site file"));
    }
    let mut parsed = Parsed::default();
    walk(&root, &mut Vec::new(), &mut parsed);
    Ok(parsed)
}

fn walk(element: &Element, folder: &mut Vec<String>, parsed: &mut Parsed) {
    for child in &element.children {
        match child.name.as_str() {
            "Server" => server(child, folder, parsed),
            "Folder" => {
                folder.push(child.text.trim().to_string());
                walk(child, folder, parsed);
                folder.pop();
            }
            "Servers" | "RecentServers" => walk(child, folder, parsed),
            _ => {}
        }
    }
}

fn server(element: &Element, folder: &[String], parsed: &mut Parsed) {
    let host = element.child_text("Host").unwrap_or_default();
    // Versions before 3.10 kept the name as the element's own text.
    let name = element
        .child_text("Name")
        .or_else(|| Some(element.text.trim()).filter(|text| !text.is_empty()))
        .unwrap_or(host);
    let number: i64 = element
        .child_text("Protocol")
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let protocol = match number {
        0 | 4 => Protocol::Ftps,
        1 => Protocol::Sftp,
        3 => Protocol::FtpsImplicit,
        6 => Protocol::Ftp,
        7 => Protocol::S3,
        9 => Protocol::WebDavs,
        17 => Protocol::BackblazeB2,
        19 => Protocol::WebDav,
        unsupported => {
            parsed.skip(name, unsupported_reason(unsupported));
            return;
        }
    };
    if host.is_empty() && protocol != Protocol::BackblazeB2 {
        parsed.skip(name, "It has no server address");
        return;
    }
    let logon_type = element
        .child_text("Logontype")
        .and_then(|text| text.parse().ok())
        .unwrap_or(1);
    let username = match logon_type {
        0 => "anonymous",
        _ => element.child_text("User").unwrap_or_default(),
    };
    let port = element.child_text("Port").and_then(parse_port);
    let mut candidate = Candidate::new(protocol, name, host, port, username);
    candidate.folder = folder.to_vec();
    if number == 0 {
        candidate.note(
            "FileZilla used TLS only when the server offered it. Poros insists on it; \
             switch to plain FTP if this server has none.",
        );
    }
    match logon_type {
        5 if protocol == Protocol::Sftp => {
            candidate.use_key_file(element.child_text("Keyfile").unwrap_or_default());
            if candidate.connection.auth_type != AuthType::PublicKey {
                candidate.connection.auth_type = AuthType::Agent;
            }
        }
        1 | 4 | 6 => {
            if let Some(password) = element.child("Pass") {
                read_password(password, &mut candidate, parsed);
            }
            if logon_type == 4 {
                candidate.note("FileZilla also sent an FTP account name, which Poros does not.");
            }
        }
        _ => {}
    }
    if let Some(path) = element.child_text("RemoteDir").and_then(remote_dir) {
        candidate.set_remote_path(&path);
    }
    candidate.connection.ftp_active = element.child_text("PasvMode") == Some("MODE_ACTIVE");
    candidate.connection.bypass_proxy =
        protocol == Protocol::Sftp && element.child_text("BypassProxy") == Some("1");
    parsed.candidates.push(candidate);
}

fn read_password(element: &Element, candidate: &mut Candidate, parsed: &mut Parsed) {
    let text = element.text.trim();
    if text.is_empty() {
        return;
    }
    match element.attribute("encoding") {
        None | Some("") | Some("plain") => candidate.set_secret(Some(text.to_string())),
        Some("base64") => {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(text)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok());
            candidate.set_secret(decoded);
        }
        Some(_) => {
            candidate.note(
                "FileZilla's master password protects the password; type it when you connect.",
            );
            parsed.warn(
                "FileZilla's master password protects the passwords, so they were left behind.",
            );
        }
    }
}

fn unsupported_reason(protocol: i64) -> String {
    let name = match protocol {
        2 | 5 => return "It is a web (HTTP) address, not a file server".to_string(),
        14 | 16 | 18 => {
            return "Google Drive, OneDrive and Box need a browser sign-in; add it from the \
                    Connect dialog"
                .to_string()
        }
        8 | 21 => "Storj",
        10 | 11 => "Azure",
        12 => "OpenStack Swift",
        13 => "Google Cloud Storage",
        15 => "Dropbox",
        20 => "Rackspace",
        _ => "this protocol",
    };
    format!("Poros does not connect to {name}")
}

/// FileZilla's own path format: the server type, the prefix, then each folder name after its
/// length, such as `1 0 4 home 5 alice` for `/home/alice`.
fn remote_dir(text: &str) -> Option<String> {
    let mut rest = text.trim_start();
    let (_server_type, after_type) = number(rest)?;
    let (prefix_length, after_prefix_length) = number(after_type)?;
    rest = skip_characters(after_prefix_length, prefix_length)?;
    let mut segments = Vec::new();
    while !rest.trim().is_empty() {
        let (length, after_length) = number(rest)?;
        let segment: String = after_length.chars().take(length).collect();
        rest = skip_characters(after_length, length)?;
        segments.push(segment);
    }
    Some(format!("/{}", segments.join("/")))
}

/// A number and the text after the space that ends it.
fn number(text: &str) -> Option<(usize, &str)> {
    let text = text.strip_prefix(' ').unwrap_or(text);
    let end = text.find(' ').unwrap_or(text.len());
    let value = text[..end].parse().ok()?;
    Some((value, text.get(end + 1..).unwrap_or("")))
}

fn skip_characters(text: &str, count: usize) -> Option<&str> {
    if count == 0 {
        return Some(text);
    }
    let end = text
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(index, _)| index);
    if text[..end].chars().count() < count {
        return None;
    }
    Some(&text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<FileZilla3 version="3.67.0" platform="windows">
    <Servers>
        <Server>
            <Host>sftp.example.com</Host>
            <Port>2222</Port>
            <Protocol>1</Protocol>
            <Type>0</Type>
            <User>alice</User>
            <Pass encoding="base64">aHVudGVyMg==</Pass>
            <Logontype>1</Logontype>
            <PasvMode>MODE_DEFAULT</PasvMode>
            <EncodingType>Auto</EncodingType>
            <BypassProxy>1</BypassProxy>
            <Name>Web server</Name>
            <RemoteDir>1 0 3 var 3 www 8 my sites</RemoteDir>
            <SyncBrowsing>0</SyncBrowsing>
        </Server>
        <Folder expanded="1">Clients
            <Folder expanded="0">Acme &amp; Co
                <Server>
                    <Host>ftp.acme.test</Host>
                    <Port>21</Port>
                    <Protocol>0</Protocol>
                    <User>bob</User>
                    <Pass encoding="crypt" pubkey="abc">deadbeef</Pass>
                    <Logontype>1</Logontype>
                    <PasvMode>MODE_ACTIVE</PasvMode>
                    <Name>Acme FTP</Name>
                </Server>
                <Server>
                    <Host>keys.acme.test</Host>
                    <Protocol>1</Protocol>
                    <User>deploy</User>
                    <Keyfile>C:\keys\deploy.ppk</Keyfile>
                    <Logontype>5</Logontype>
                    <Name>Keys</Name>
                </Server>
            </Folder>
        </Folder>
        <Server>
            <Host>files.test</Host>
            <Protocol>3</Protocol>
            <Logontype>0</Logontype>
            Old style name&#x0A;
        </Server>
        <Server>
            <Host>drive.google.com</Host>
            <Protocol>14</Protocol>
            <Name>Drive</Name>
        </Server>
        <Server>
            <Host>dav.test</Host>
            <Protocol>9</Protocol>
            <User>carol</User>
            <Pass>plain</Pass>
            <RemoteDir>1 0 10 remote.php 3 dav</RemoteDir>
            <Name>Cloud</Name>
        </Server>
    </Servers>
</FileZilla3>"#;

    #[test]
    fn reads_sites_folders_and_passwords() {
        let parsed = parse(SITES).unwrap();
        let sites = &parsed.candidates;
        assert_eq!(sites.len(), 5);

        let web = &sites[0].connection;
        assert_eq!(
            (web.protocol, web.name.as_str(), web.host.as_str(), web.port),
            (Protocol::Sftp, "Web server", "sftp.example.com", 2222)
        );
        assert_eq!(web.remote_path.as_deref(), Some("/var/www/my sites"));
        assert!(web.bypass_proxy);
        assert_eq!(sites[0].secret.as_deref(), Some("hunter2"));
        assert!(sites[0].folder.is_empty());

        let acme = &sites[1];
        assert_eq!(acme.folder, vec!["Clients", "Acme & Co"]);
        assert_eq!(acme.connection.protocol, Protocol::Ftps);
        assert!(acme.connection.ftp_active);
        assert_eq!(acme.secret, None);
        assert_eq!(acme.notes.len(), 2);

        let keys = &sites[2].connection;
        assert_eq!(keys.auth_type, AuthType::PublicKey);
        assert_eq!(keys.key_path.as_deref(), Some("C:\\keys\\deploy.ppk"));
        assert_eq!(keys.port, 22);

        let old = &sites[3].connection;
        assert_eq!(old.name, "Old style name");
        assert_eq!(old.username, "anonymous");
        assert_eq!((old.protocol, old.port), (Protocol::FtpsImplicit, 990));

        let cloud = &sites[4];
        assert_eq!(cloud.connection.host, "dav.test/remote.php/dav");
        assert_eq!(cloud.connection.remote_path, None);
        assert_eq!(cloud.secret.as_deref(), Some("plain"));

        assert_eq!(parsed.skipped.len(), 1);
        assert_eq!(parsed.skipped[0].name, "Drive");
        assert_eq!(parsed.warnings.len(), 1);
    }

    #[test]
    fn reads_remote_dirs() {
        assert_eq!(remote_dir("1 0").as_deref(), Some("/"));
        assert_eq!(
            remote_dir("1 0 4 home 5 alice").as_deref(),
            Some("/home/alice")
        );
        assert_eq!(remote_dir("1 0 5 a b c").as_deref(), Some("/a b c"));
        assert_eq!(remote_dir("1 0 5 José").as_deref(), None);
        assert_eq!(remote_dir("1 0 4 José").as_deref(), Some("/José"));
        assert_eq!(remote_dir("x"), None);
    }

    #[test]
    fn refuses_other_xml() {
        assert!(parse("<plist/>").is_err());
    }
}
