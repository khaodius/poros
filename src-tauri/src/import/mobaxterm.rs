//! MobaXterm sessions, from `MobaXterm.ini` or a `.mxtsessions` export. Each session is one line
//! under a `[Bookmarks...]` section: `Name=#icon#type%host%port%user%...#terminal#...`.

use super::registry;
use super::{parse_port, Candidate, Parsed};
use crate::connections::AuthType;
use crate::protocol::Protocol;

const SSH: &str = "0";
const FTP: &str = "6";
const SFTP: &str = "7";

pub fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    for section in registry::parse_ini(text) {
        if !section.name.to_ascii_lowercase().starts_with("bookmarks") {
            continue;
        }
        let folder: Vec<String> = section
            .get("SubRep")
            .unwrap_or_default()
            .split('\\')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect();
        for (name, value) in &section.values {
            if name.eq_ignore_ascii_case("SubRep") || name.eq_ignore_ascii_case("ImgNum") {
                continue;
            }
            if let Some(index) = session(name.trim(), value, &mut parsed) {
                parsed.candidates[index].folder = folder.clone();
            }
        }
    }
    parsed
}

/// Adds the session's site; returns where it went.
fn session(name: &str, value: &str, parsed: &mut Parsed) -> Option<usize> {
    let connection = value.trim().strip_prefix('#')?.split('#').nth(1)?;
    let fields: Vec<String> = connection.split('%').map(unescape).collect();
    let field = |index: usize| fields.get(index).map(String::as_str).unwrap_or_default();
    let (protocol, key_field, folder_field) = match field(0) {
        SSH => (Protocol::Sftp, Some(14), None),
        SFTP => (Protocol::Sftp, Some(9), Some(6)),
        FTP => (Protocol::Ftp, None, None),
        _ => {
            parsed.skip(name, "It is not an SSH, SFTP or FTP session");
            return None;
        }
    };
    let host = field(1);
    if host.is_empty() {
        parsed.skip(name, "It has no server address");
        return None;
    }
    let username = match field(3) {
        "<default>" => "",
        username => username,
    };
    let mut candidate = Candidate::new(protocol, name, host, parse_port(field(2)), username);
    if let Some(index) = key_field {
        candidate.use_key_file(&expand_folders(field(index)));
    }
    if let Some(index) = folder_field {
        candidate.set_remote_path(field(index));
    }
    let index = parsed.candidates.len();
    parsed.candidates.push(candidate);
    if field(0) == SSH {
        gateways(&fields, index, parsed);
    }
    Some(index)
}

/// The SSH gateways (jump hosts) of an SSH session, outermost first: hosts, ports and users
/// in three lists separated by `__PIPE__`.
fn gateways(fields: &[String], index: usize, parsed: &mut Parsed) {
    let list = |position: usize| -> Vec<String> {
        fields
            .get(position)
            .map(|field| {
                field
                    .split('|')
                    .map(|part| part.trim().to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let (hosts, ports, users) = (list(8), list(9), list(10));
    let mut previous = None;
    for (position, host) in hosts.iter().enumerate() {
        if host.is_empty() {
            continue;
        }
        let port = ports.get(position).and_then(|port| parse_port(port));
        let user = users.get(position).map(String::as_str).unwrap_or_default();
        let mut gateway = Candidate::new(Protocol::Sftp, "", host, port, user);
        gateway.connection.name = format!("{host} (gateway)");
        gateway.connection.auth_type = AuthType::Password;
        gateway.jump = previous;
        previous = Some(parsed.jump_host(gateway));
    }
    if let Some(jump) = previous {
        parsed.link_jump(index, jump);
    }
}

/// MobaXterm's stand-ins for characters its file format uses itself.
fn unescape(field: &str) -> String {
    field
        .replace("__PTVIRG__", ";")
        .replace("__DIEZE__", "#")
        .replace("__PIPE__", "|")
        .trim()
        .to_string()
}

/// MobaXterm's placeholders for the user's folders in key paths.
fn expand_folders(path: &str) -> String {
    let home = dirs::home_dir().map(|home| home.to_string_lossy().into_owned());
    let documents = dirs::document_dir().map(|documents| documents.to_string_lossy().into_owned());
    let mut expanded = path.to_string();
    if let Some(home) = home {
        expanded = expanded.replace("_ProfileDir_", &home);
    }
    if let Some(documents) = documents {
        expanded = expanded.replace("_MyDocuments_", &documents);
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sessions_with_folders_and_gateways() {
        let parsed = parse(
            "[Bookmarks]\r\nSubRep=\r\nImgNum=42\r\n\
             Deck=#109#0%192.168.137.40%22%deck%%-1%-1%%%%%0%0%0%%%-1%0%0%0%%1080%%0%0%1#MobaFont%10%0#0# #-1\r\n\
             Desktop=#91#4%pc.lan%3389%%0%-1#MobaFont#0# #-1\r\n\
             [Bookmarks_1]\r\nSubRep=Work\\Servers\r\nImgNum=41\r\n\
             Files=#140#7%files.example.com%2222%<default>%-1%-1%/srv/data%0%0%_ProfileDir_\\.ssh\\id.ppk%#MobaFont#0# #-1\r\n\
             Inner=#109#0%10.0.0.9%22%app%%-1%-1%%bastion.example.com__PIPE__gate.lan%22__PIPE__2200%jump__PIPE__ops%#MobaFont#0# #-1\r\n",
        );
        assert_eq!(parsed.skipped.len(), 1);
        let deck = &parsed.candidates[0];
        assert_eq!(
            (
                deck.connection.name.as_str(),
                deck.connection.host.as_str(),
                deck.connection.username.as_str()
            ),
            ("Deck", "192.168.137.40", "deck")
        );
        assert!(deck.folder.is_empty());

        let files = &parsed.candidates[1];
        assert_eq!(files.folder, vec!["Work", "Servers"]);
        assert_eq!(files.connection.port, 2222);
        assert_eq!(files.connection.username, "");
        assert_eq!(files.connection.remote_path.as_deref(), Some("/srv/data"));
        assert_eq!(files.connection.auth_type, AuthType::PublicKey);
        assert!(!files
            .connection
            .key_path
            .as_deref()
            .unwrap()
            .contains("_ProfileDir_"));

        let inner = parsed
            .candidates
            .iter()
            .find(|site| site.connection.name == "Inner")
            .unwrap();
        assert_eq!(inner.folder, vec!["Work", "Servers"]);
        let gate = &parsed.candidates[inner.jump.unwrap()];
        assert_eq!(
            (
                gate.connection.host.as_str(),
                gate.connection.port,
                gate.connection.username.as_str()
            ),
            ("gate.lan", 2200, "ops")
        );
        let bastion = &parsed.candidates[gate.jump.unwrap()];
        assert_eq!(bastion.connection.host, "bastion.example.com");
    }
}
