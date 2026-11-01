//! PuTTY and KiTTY sessions: an export of their registry key on Windows, or PuTTY's own
//! session files on Linux (`~/.putty/sessions`).

use super::registry::{self, Section};
use super::{parse_port, percent_decode, split_user_host, Candidate, Input, Parsed};
use crate::protocol::Protocol;

pub const REGISTRY_KEYS: [&str; 2] = [
    "HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions",
    "HKEY_CURRENT_USER\\Software\\9bis.com\\KiTTY\\Sessions",
];

pub fn looks_like_session_file(text: &str) -> bool {
    let has = |key: &str| text.lines().any(|line| line.trim_start().starts_with(key));
    has("HostName=") && (has("Protocol=") || has("PortNumber="))
}

pub fn parse(input: &Input) -> Parsed {
    let mut parsed = Parsed::default();
    if registry::is_export(&input.text) {
        for section in registry::parse_export(&input.text) {
            if let Some((_, name)) = section.name.rsplit_once("\\Sessions\\") {
                session(&section, &percent_decode(name), &mut parsed);
            }
        }
    } else {
        let mut sections = registry::parse_ini(&input.text);
        if let Some(section) = sections.first_mut() {
            let file_name = input
                .path
                .as_deref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            session(section, &percent_decode(&file_name), &mut parsed);
        }
    }
    parsed
}

fn session(section: &Section, name: &str, parsed: &mut Parsed) {
    let protocol = section.text("Protocol").unwrap_or("ssh");
    let (host_user, host) = split_user_host(section.text("HostName").unwrap_or_default());
    if host.is_empty() {
        if name != "Default Settings" {
            parsed.skip(name, "It has no server address");
        }
        return;
    }
    if !protocol.eq_ignore_ascii_case("ssh") {
        parsed.skip(
            name,
            format!("It is a {protocol} session; Poros connects over SSH"),
        );
        return;
    }
    let username = section.text("UserName").unwrap_or(host_user);
    let port = section.text("PortNumber").and_then(parse_port);
    let mut candidate = Candidate::new(Protocol::Sftp, name, host, port, username);
    candidate.use_key_file(section.text("PublicKeyFile").unwrap_or_default());
    parsed.candidates.push(candidate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::AuthType;
    use std::path::PathBuf;

    #[test]
    fn reads_registry_exports() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\Default%20Settings]\r\n\
            \"HostName\"=\"\"\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\My%20Server]\r\n\
            \"HostName\"=\"admin@srv.example.com\"\r\n\"PortNumber\"=dword:00000801\r\n\
            \"Protocol\"=\"ssh\"\r\n\"PublicKeyFile\"=\"C:\\\\keys\\\\srv.ppk\"\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\router]\r\n\
            \"HostName\"=\"192.168.1.1\"\r\n\"Protocol\"=\"telnet\"\r\n";
        let parsed = parse(&Input::from_text("putty.reg", reg));
        assert_eq!(parsed.candidates.len(), 1);
        let server = &parsed.candidates[0].connection;
        assert_eq!(server.name, "My Server");
        assert_eq!(
            (server.host.as_str(), server.port, server.username.as_str()),
            ("srv.example.com", 2049, "admin")
        );
        assert_eq!(server.auth_type, AuthType::PublicKey);
        assert_eq!(server.key_path.as_deref(), Some("C:\\keys\\srv.ppk"));
        assert_eq!(parsed.skipped.len(), 1);
        assert_eq!(parsed.skipped[0].name, "router");
    }

    #[test]
    fn reads_linux_session_files() {
        let mut input = Input::from_text(
            "",
            "HostName=box.example.com\nUserName=pi\nPortNumber=22\nProtocol=ssh\n",
        );
        input.path = Some(PathBuf::from(
            "/home/u/.putty/sessions/box.example.com%20pi",
        ));
        assert!(looks_like_session_file(&input.text));
        let parsed = parse(&input);
        let server = &parsed.candidates[0].connection;
        assert_eq!(server.name, "box.example.com pi");
        assert_eq!(server.username, "pi");
        assert_eq!(server.auth_type, AuthType::Password);
    }
}
