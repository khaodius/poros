//! End-to-end tests against a real SSH server. Skipped unless `POROS_TEST_SSH_PORT` is set.
//!
//! The server must listen on 127.0.0.1 and accept, for `POROS_TEST_SSH_USER`:
//! the password in `POROS_TEST_SSH_PASSWORD`, and every `tests/fixtures/keys/*.pub` key.
//! The README describes a throwaway `sshd` setup.

use std::path::PathBuf;

use poros_lib::error::ErrorKind;
use poros_lib::events::Events;
use poros_lib::model::{EntryKind, LinkTarget};
use poros_lib::session::SessionManager;
use poros_lib::ssh::{AuthMethod, ConnectProfile, HostKeyApproval};

struct Server {
    port: u16,
    user: String,
    password: String,
}

fn server() -> Option<Server> {
    let port = std::env::var("POROS_TEST_SSH_PORT").ok()?.parse().ok()?;
    Some(Server {
        port,
        user: std::env::var("POROS_TEST_SSH_USER").unwrap_or_else(|_| "poros".into()),
        password: std::env::var("POROS_TEST_SSH_PASSWORD").unwrap_or_else(|_| "poros-pass".into()),
    })
}

fn profile(server: &Server, auth: AuthMethod) -> ConnectProfile {
    ConnectProfile {
        host: "127.0.0.1".into(),
        port: server.port,
        username: server.user.clone(),
        auth,
        initial_path: None,
        timeout_secs: Some(10),
        keepalive_secs: None,
    }
}

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/keys")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// A manager whose app known_hosts already trusts the test server.
async fn trusted_manager(server: &Server) -> (tempfile::TempDir, SessionManager) {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(temp_dir.path().join("known_hosts"), Events::default());
    let password = AuthMethod::Password {
        password: server.password.clone(),
    };
    let error = manager
        .connect(profile(server, password.clone()), None)
        .await
        .err();
    if let Some(error) = error {
        let host_key = error
            .host_key
            .expect("first connect should ask about the host key");
        let info = manager
            .connect(
                profile(server, password),
                Some(HostKeyApproval {
                    fingerprint: host_key.fingerprint,
                    remember: true,
                }),
            )
            .await
            .unwrap();
        manager.disconnect(&info.id).await.unwrap();
    }
    (temp_dir, manager)
}

#[tokio::test]
async fn host_key_prompt_then_remembered() {
    let Some(server) = server() else { return };
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(temp_dir.path().join("known_hosts"), Events::default());
    let auth = AuthMethod::Password {
        password: server.password.clone(),
    };

    let error = manager
        .connect(profile(&server, auth.clone()), None)
        .await
        .unwrap_err();
    // The server may already be in ~/.ssh/known_hosts on a dev machine.
    if error.kind == ErrorKind::HostKeyUnknown {
        let host_key = error.host_key.unwrap();
        assert!(host_key.fingerprint.starts_with("SHA256:"));

        let wrong = HostKeyApproval {
            fingerprint: "SHA256:not-it".into(),
            remember: true,
        };
        let error = manager
            .connect(profile(&server, auth.clone()), Some(wrong))
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::HostKeyUnknown);

        let ok = HostKeyApproval {
            fingerprint: host_key.fingerprint,
            remember: true,
        };
        let info = manager
            .connect(profile(&server, auth.clone()), Some(ok))
            .await
            .unwrap();
        manager.disconnect(&info.id).await.unwrap();
    }

    let info = manager.connect(profile(&server, auth), None).await.unwrap();
    manager.disconnect(&info.id).await.unwrap();
}

#[tokio::test]
async fn password_auth_and_wrong_password() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let error = manager
        .connect(
            profile(
                &server,
                AuthMethod::Password {
                    password: "wrong".into(),
                },
            ),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::AuthFailed, "{}", error.message);
}

#[tokio::test]
async fn every_key_format_authenticates() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let mut files: Vec<(String, Option<String>)> = Vec::new();
    for name in ["rsa", "ed25519", "ecdsa256", "ecdsa384", "ecdsa521"] {
        for version in ["v2", "v3"] {
            files.push((format!("{name}-{version}-plain.ppk"), None));
            files.push((
                format!("{name}-{version}-enc.ppk"),
                Some("poros-test".into()),
            ));
        }
    }
    files.push((
        "ed25519-v3-argon2i-enc.ppk".into(),
        Some("poros-test".into()),
    ));
    files.push((
        "ed25519-v3-argon2d-enc.ppk".into(),
        Some("poros-test".into()),
    ));
    files.push(("nocomment-v3-plain.ppk".into(), None));
    files.push(("openssh-ed25519".into(), None));
    files.push(("openssh-ed25519-enc".into(), Some("poros-test".into())));
    files.push(("pem-rsa".into(), None));
    files.push(("pem-rsa-enc".into(), Some("poros-test".into())));
    files.push(("pkcs8-ecdsa".into(), None));

    for (file, passphrase) in files {
        let auth = AuthMethod::PublicKey {
            key_path: fixture(&file),
            passphrase,
        };
        let info = manager
            .connect(profile(&server, auth), None)
            .await
            .unwrap_or_else(|error| panic!("{file}: {}", error.message));
        manager.disconnect(&info.id).await.unwrap();
    }
}

#[tokio::test]
async fn browse_and_manage_files() {
    let Some(server) = server() else { return };
    let (_dir, manager) = trusted_manager(&server).await;
    let info = manager
        .connect(
            profile(
                &server,
                AuthMethod::Password {
                    password: server.password.clone(),
                },
            ),
            None,
        )
        .await
        .unwrap();
    let session = manager.get(&info.id).await.unwrap();
    let fs = &session.fs;

    let scratch = format!("poros-test-{}", std::process::id());
    let root = fs.make_dir("~", &scratch).await.unwrap();
    let sub = fs.make_dir(&root, "sub").await.unwrap();
    fs.make_dir(&sub, "deeper").await.unwrap();
    let dup = fs.make_dir(&root, "sub").await.unwrap_err();
    assert_eq!(dup.kind, ErrorKind::AlreadyExists);

    let listing = fs.list_dir(&root).await.unwrap();
    assert_eq!(listing.path, root);
    assert_eq!(listing.parent.as_deref(), Some(info.home.as_str()));
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].kind, EntryKind::Dir);

    let renamed = fs.rename(&sub, "renamed").await.unwrap();
    assert!(renamed.ends_with("/renamed"));

    // Symlinks are resolved so the UI can navigate into linked folders.
    let home = fs.list_dir("~").await.unwrap();
    if let Some(link) = home.entries.iter().find(|entry| entry.name == "data-link") {
        assert_eq!(link.kind, EntryKind::Symlink);
        assert_eq!(link.link_target, Some(LinkTarget::Dir));
    }
    if let Some(link) = home
        .entries
        .iter()
        .find(|entry| entry.name == "broken-link")
    {
        assert_eq!(link.link_target, Some(LinkTarget::Broken));
    }

    fs.delete(std::slice::from_ref(&root)).await.unwrap();
    let error = fs.list_dir(&root).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::NotFound);

    manager.disconnect(&info.id).await.unwrap();
    let error = manager.get(&info.id).await.err().unwrap();
    assert_eq!(error.kind, ErrorKind::SessionNotFound);
}
