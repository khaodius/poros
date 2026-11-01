//! WinSCP sites, from `WinSCP.ini` or an export of its registry key.

use super::registry::{self, Section};
use super::{percent_decode, Candidate, Parsed};
use crate::cloud::s3::Addressing;
use crate::connections::AuthType;
use crate::protocol::Protocol;

pub const REGISTRY_KEY: &str = "HKEY_CURRENT_USER\\Software\\Martin Prikryl\\WinSCP 2\\Sessions";

/// WinSCP's reversible scrambling of saved passwords, used unless a master password is set.
const PASSWORD_MAGIC: u8 = 0xA3;
const PASSWORD_FLAG: u8 = 0xFF;

pub fn parse(text: &str) -> Parsed {
    let sections = if registry::is_export(text) {
        registry::parse_export(text)
    } else {
        registry::parse_ini(text)
    };
    let master_password = sections.iter().any(|section| {
        section.name.ends_with("Configuration\\Security")
            && section.number("UseMasterPassword") == Some(1)
    });
    let mut parsed = Parsed::default();
    for section in &sections {
        let Some((_, path)) = section.name.split_once("Sessions\\") else {
            continue;
        };
        let mut names: Vec<String> = path.split('/').map(percent_decode).collect();
        let name = names.pop().unwrap_or_default();
        if names.is_empty() && name == "Default Settings" {
            continue;
        }
        session(section, &name, names, master_password, &mut parsed);
    }
    parsed
}

fn session(
    section: &Section,
    name: &str,
    folder: Vec<String>,
    master_password: bool,
    parsed: &mut Parsed,
) {
    let value = |key: &str| section.text(key).map(percent_decode).unwrap_or_default();
    let tls = section.number("Ftps").unwrap_or(0);
    let (protocol, scp) = match section.number("FSProtocol").unwrap_or(1) {
        0 => (Protocol::Sftp, true),
        1 | 2 => (Protocol::Sftp, false),
        5 => match tls {
            0 => (Protocol::Ftp, false),
            1 => (Protocol::FtpsImplicit, false),
            _ => (Protocol::Ftps, false),
        },
        6 if tls == 0 => (Protocol::WebDav, false),
        6 => (Protocol::WebDavs, false),
        7 => (Protocol::S3, false),
        _ => {
            parsed.skip(name, "Poros does not connect with this protocol");
            return;
        }
    };
    let host_name = value("HostName");
    let username = value("UserName");
    let host = match protocol {
        Protocol::S3 if tls == 0 => format!(
            "http://{}",
            if host_name.is_empty() {
                crate::cloud::s3::AWS_ENDPOINT
            } else {
                &host_name
            }
        ),
        _ => host_name.clone(),
    };
    if host.is_empty() {
        parsed.skip(name, "It has no server address");
        return;
    }
    let port = section
        .number("PortNumber")
        .and_then(|port| u16::try_from(port).ok());
    let mut candidate = Candidate::new(protocol, name, &host, port, &username);
    candidate.folder = folder;
    if scp {
        candidate.note("WinSCP used SCP for this site; Poros connects over SFTP.");
    }
    candidate.use_key_file(&value("PublicKeyFile"));
    candidate.set_remote_path(&value("RemoteDirectory"));
    candidate.connection.ftp_active = protocol.is_ftp() && section.number("FtpPasvMode") == Some(0);
    if protocol == Protocol::S3 {
        candidate.connection.s3.region = value("S3DefaultRegion");
        if section.number("S3UrlStyle") == Some(1) {
            candidate.connection.s3.addressing = Addressing::Path;
        }
    }
    if let Some(scrambled) = section.text("Password") {
        if master_password {
            parsed
                .warn("WinSCP's master password protects the passwords, so they were left behind.");
        } else {
            candidate.set_secret(unscramble_password(
                scrambled,
                &format!("{username}{host_name}"),
            ));
        }
    }

    let index = parsed.candidates.len();
    parsed.candidates.push(candidate);
    if protocol == Protocol::Sftp && section.number("Tunnel") == Some(1) {
        tunnel(section, index, parsed);
    }
}

/// The SSH server WinSCP tunnels through, as a jump host.
fn tunnel(section: &Section, index: usize, parsed: &mut Parsed) {
    let value = |key: &str| section.text(key).map(percent_decode).unwrap_or_default();
    let host = value("TunnelHostName");
    if host.is_empty() {
        return;
    }
    let port = section
        .number("TunnelPortNumber")
        .and_then(|port| u16::try_from(port).ok());
    let username = value("TunnelUserName");
    let mut jump = Candidate::new(Protocol::Sftp, "", &host, port, &username);
    jump.connection.name = format!("{} (tunnel)", jump.connection.name);
    jump.folder = parsed.candidates[index].folder.clone();
    jump.use_key_file(&value("TunnelPublicKeyFile"));
    if jump.connection.auth_type == AuthType::Password {
        jump.note("WinSCP keeps the tunnel's password apart; type it when you connect.");
    }
    let jump = parsed.jump_host(jump);
    parsed.link_jump(index, jump);
}

/// Undoes WinSCP's scrambling. `key` is the user name and host name the site was saved with;
/// newer versions put it in front of the password.
fn unscramble_password(scrambled: &str, key: &str) -> Option<String> {
    let digits: Vec<u32> = scrambled
        .chars()
        .map(|digit| digit.to_digit(16))
        .collect::<Option<_>>()?;
    let mut bytes = digits
        .chunks_exact(2)
        .map(|pair| !(((pair[0] << 4) | pair[1]) as u8 ^ PASSWORD_MAGIC));
    let flag = bytes.next()?;
    let length = if flag == PASSWORD_FLAG {
        bytes.next()?;
        bytes.next()?
    } else {
        flag
    };
    let padding = bytes.next()?;
    for _ in 0..padding {
        bytes.next()?;
    }
    let password: Vec<u8> = (0..length).map(|_| bytes.next()).collect::<Option<_>>()?;
    let password = if flag == PASSWORD_FLAG {
        password.strip_prefix(key.as_bytes())?.to_vec()
    } else {
        password
    };
    String::from_utf8(password).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WinSCP's scrambling, with fixed padding instead of random bytes.
    fn scramble(password: &str, key: &str) -> String {
        let scramble_byte = |byte: u8| format!("{:02X}", !byte ^ PASSWORD_MAGIC);
        let plain = format!("{key}{password}");
        let mut scrambled = String::new();
        scrambled += &scramble_byte(PASSWORD_FLAG);
        scrambled += &scramble_byte(0);
        scrambled += &scramble_byte(plain.len() as u8);
        scrambled += &scramble_byte(3);
        for padding in [7, 8, 9] {
            scrambled += &scramble_byte(padding);
        }
        for byte in plain.bytes() {
            scrambled += &scramble_byte(byte);
        }
        scrambled += &scramble_byte(42);
        scrambled
    }

    #[test]
    fn unscrambles_passwords() {
        let scrambled = scramble("hunter2", "aliceexample.com");
        assert_eq!(
            unscramble_password(&scrambled, "aliceexample.com").as_deref(),
            Some("hunter2")
        );
        assert_eq!(unscramble_password(&scrambled, "eveexample.com"), None);
        assert_eq!(unscramble_password("zz", ""), None);
        // Before WinSCP 4 the password had no key in front.
        let old = format!(
            "{:02X}{:02X}{:02X}{:02X}",
            !2u8 ^ PASSWORD_MAGIC,
            !0u8 ^ PASSWORD_MAGIC,
            !b'o' ^ PASSWORD_MAGIC,
            !b'k' ^ PASSWORD_MAGIC
        );
        assert_eq!(unscramble_password(&old, "any").as_deref(), Some("ok"));
    }

    #[test]
    fn reads_ini_sessions_with_folders_tunnels_and_passwords() {
        let password = scramble("s3cret", "aliceexample.com");
        let ini = format!(
            "[Configuration\\Interface]\r\nFoo=1\r\n\
             [Sessions\\Default%20Settings]\r\nHostName=ignored\r\n\
             [Sessions\\Work/My%20Site]\r\nHostName=example.com\r\nUserName=alice\r\n\
             PortNumber=2222\r\nFSProtocol=2\r\nPublicKeyFile=C:%5Ckeys%5Cid.ppk\r\n\
             RemoteDirectory=/var/www\r\nPassword={password}\r\nTunnel=1\r\n\
             TunnelHostName=bastion.example.com\r\nTunnelUserName=jump\r\n\
             [Sessions\\ftp]\r\nHostName=ftp.example.com\r\nFSProtocol=5\r\nFtps=3\r\n\
             FtpPasvMode=0\r\n\
             [Sessions\\scp]\r\nHostName=old.example.com\r\nFSProtocol=0\r\n\
             [Sessions\\dav]\r\nHostName=cloud.example.com\r\nFSProtocol=6\r\nFtps=1\r\n\
             RemoteDirectory=/remote.php/webdav/\r\n\
             [Sessions\\minio]\r\nHostName=minio.lan\r\nPortNumber=9000\r\nFSProtocol=7\r\n\
             S3UrlStyle=1\r\nS3DefaultRegion=eu-west-1\r\n\
             [Sessions\\nohost]\r\nUserName=x\r\n"
        );
        let parsed = parse(&ini);
        let sites = &parsed.candidates;
        assert_eq!(sites.len(), 6);

        let site = &sites[0];
        assert_eq!(site.folder, vec!["Work"]);
        assert_eq!(site.connection.name, "My Site");
        assert_eq!(
            (site.connection.host.as_str(), site.connection.port),
            ("example.com", 2222)
        );
        assert_eq!(site.connection.auth_type, AuthType::PublicKey);
        assert_eq!(
            site.connection.key_path.as_deref(),
            Some("C:\\keys\\id.ppk")
        );
        assert_eq!(site.connection.remote_path.as_deref(), Some("/var/www"));
        assert_eq!(site.secret.as_deref(), Some("s3cret"));
        let tunnel = &sites[site.jump.unwrap()];
        assert_eq!(tunnel.connection.host, "bastion.example.com");
        assert_eq!(tunnel.connection.username, "jump");
        assert_eq!(tunnel.connection.name, "bastion.example.com (tunnel)");

        let ftp = &sites[2].connection;
        assert_eq!((ftp.protocol, ftp.port), (Protocol::Ftps, 21));
        assert!(ftp.ftp_active);
        assert_eq!(sites[3].notes.len(), 1);
        assert_eq!(
            sites[4].connection.host,
            "cloud.example.com/remote.php/webdav"
        );
        assert_eq!(sites[4].connection.protocol, Protocol::WebDavs);
        let minio = &sites[5].connection;
        assert_eq!(minio.host, "http://minio.lan");
        assert_eq!(minio.port, 9000);
        assert_eq!(minio.s3.addressing, Addressing::Path);
        assert_eq!(minio.s3.region, "eu-west-1");
        assert_eq!(parsed.skipped.len(), 1);
    }

    #[test]
    fn reads_registry_exports_and_respects_the_master_password() {
        let reg = format!(
            "Windows Registry Editor Version 5.00\r\n\r\n\
             [{REGISTRY_KEY}\\Prod]\r\n\"HostName\"=\"prod.example.com\"\r\n\
             \"UserName\"=\"root\"\r\n\"PortNumber\"=dword:00000016\r\n\
             \"Password\"=\"{}\"\r\n\r\n\
             [HKEY_CURRENT_USER\\Software\\Martin Prikryl\\WinSCP 2\\Configuration\\Security]\r\n\
             \"UseMasterPassword\"=dword:00000001\r\n",
            scramble("x", "rootprod.example.com")
        );
        let parsed = parse(&reg);
        assert_eq!(parsed.candidates.len(), 1);
        assert_eq!(parsed.candidates[0].connection.name, "Prod");
        assert_eq!(parsed.candidates[0].secret, None);
        assert_eq!(parsed.warnings.len(), 1);
    }
}
