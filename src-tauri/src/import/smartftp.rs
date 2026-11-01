//! SmartFTP favorites: the list of addresses its Favorites window exports (Tools > Export), or
//! the XML file per favorite that versions before 4 kept. Newer versions keep favorites in a
//! format of their own, which only SmartFTP reads.

use super::xml;
use super::{parse_port, urls, Candidate, Input, Parsed};
use crate::error::AppResult;
use crate::protocol::Protocol;

pub fn parse(input: &Input) -> AppResult<Parsed> {
    let text = input.text.trim_start_matches('\u{feff}').trim_start();
    if !text.starts_with('<') {
        return urls::parse(text);
    }
    let root = xml::parse(text)?;
    let mut parsed = Parsed::default();
    let find = |names: &[&str]| {
        root.descendants().into_iter().find_map(|element| {
            names
                .iter()
                .any(|name| element.name.eq_ignore_ascii_case(name))
                .then(|| element.text.trim())
                .filter(|text| !text.is_empty())
        })
    };
    let host = find(&["Host", "Server", "Address"]).unwrap_or_default();
    let stem = input.stem();
    let name = find(&["Name", "Title"]).unwrap_or(&stem);
    if host.is_empty() {
        parsed.skip(name, "It has no server address");
        return Ok(parsed);
    }
    let port = find(&["Port"]).and_then(parse_port);
    let protocol = protocol(find(&["Protocol"]).unwrap_or_default(), port);
    let mut candidate = Candidate::new(
        protocol,
        name,
        host,
        port,
        find(&["User", "UserName", "Login"]).unwrap_or_default(),
    );
    candidate.set_remote_path(find(&["Path", "RemotePath", "RemoteFolder"]).unwrap_or_default());
    if find(&["Password"]).is_some() {
        candidate.note(
            "SmartFTP encrypts its passwords for this computer only; type it when you connect.",
        );
    }
    parsed.candidates.push(candidate);
    Ok(parsed)
}

/// SmartFTP names protocols in words in some versions and by number in others; the port
/// decides when the name says nothing.
fn protocol(name: &str, port: Option<u16>) -> Protocol {
    let name = name.to_ascii_lowercase();
    if name.contains("sftp") || name.contains("ssh") {
        Protocol::Sftp
    } else if name.contains("implicit") {
        Protocol::FtpsImplicit
    } else if name.contains("ftps") || name.contains("ssl") || name.contains("tls") {
        Protocol::Ftps
    } else if name.contains("webdav") || name.contains("http") {
        if port == Some(80) {
            Protocol::WebDav
        } else {
            Protocol::WebDavs
        }
    } else {
        match port {
            Some(22) => Protocol::Sftp,
            Some(990) => Protocol::FtpsImplicit,
            _ => Protocol::Ftp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reads_old_xml_favorites() {
        let mut input = Input::from_text(
            "x",
            "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n<FavoriteItem>\r\n\
             <Host>ftp.example.com</Host>\r\n<Port>21</Port>\r\n<User>web</User>\r\n\
             <Password>9722972FC57CAE5A3EDB8B4E6296</Password>\r\n<Path>/public_html</Path>\r\n\
             </FavoriteItem>",
        );
        input.path = Some(PathBuf::from("/fav/Shop site.xml"));
        let parsed = parse(&input).unwrap();
        let site = &parsed.candidates[0];
        assert_eq!(site.connection.name, "Shop site");
        assert_eq!(site.connection.protocol, Protocol::Ftp);
        assert_eq!(site.connection.username, "web");
        assert_eq!(site.connection.remote_path.as_deref(), Some("/public_html"));
        assert_eq!(site.secret, None);
        assert_eq!(site.notes.len(), 1);
    }

    #[test]
    fn reads_exported_address_lists() {
        let parsed = parse(&Input::from_text(
            "x",
            "ftp://web@ftp.example.com/public_html\r\nsftp://root@10.0.0.2:2222/\r\n",
        ))
        .unwrap();
        assert_eq!(parsed.candidates.len(), 2);
        assert_eq!(parsed.candidates[1].connection.port, 2222);
    }

    #[test]
    fn picks_protocols() {
        assert_eq!(protocol("SFTP over SSH2", None), Protocol::Sftp);
        assert_eq!(protocol("FTPS (implicit)", None), Protocol::FtpsImplicit);
        assert_eq!(protocol("FTP over TLS", None), Protocol::Ftps);
        assert_eq!(protocol("", Some(22)), Protocol::Sftp);
        assert_eq!(protocol("3", Some(21)), Protocol::Ftp);
    }
}
