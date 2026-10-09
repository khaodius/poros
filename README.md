<p align="center">
  <img src="public/poros.svg" alt="Poros" width="96" height="96">
</p>

<h1 align="center">Poros</h1>

<p align="center">
  A fast, keyboard-friendly SFTP, FTP and cloud storage client for Linux and Windows.
</p>

<p align="center">
  <a href="https://github.com/khaodius/poros/actions/workflows/ci.yml"><img src="https://github.com/khaodius/poros/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License: Apache-2.0"></a>
</p>

![Poros with a local folder, two server tabs and transfers running in the queue](.github/assets/screenshot.png)

## Features

- **Protocols**: SFTP, FTP, FTPS with explicit or implicit TLS, Google Drive and OneDrive.
- **Server-to-server copies** by dragging files from one server tab onto another. Two FTP servers
  send files straight to each other (FXP) when both allow it.
- **Tabs and docking**: local folders and server sessions are tabs that can be merged, split,
  resized, detached into their own windows and dragged from one window into another.
- **Parallel transfers**: each worker uses its own connection, large files are split across
  idle connections, and the worker count can change while transfers run.
- **File operations**: cut, copy and paste, Move to and Copy to, and dragging onto a folder (hold
  Ctrl to copy) move and copy files within a server or this computer without a transfer.
  Properties changes permissions, owner and group.
- **File compare** of any two files, side by side with changed words highlighted.
- **Folder sync** one way or both ways, by size and time, size alone or contents, with rsync-style
  excludes and a preview of every change before it runs.
- **rsync delta transfers** over Poros's own SSH connection, with nothing to install locally.
- **Safe writes**: files are written under a temporary name and renamed into place once complete,
  every file's size is checked with the server, and an optional setting compares checksums with
  the server's copy.
- **Automatic recovery**: transfers wait out a dropped connection for up to 10 minutes and resume
  where they stopped, server tabs reconnect on their own, and unfinished transfers come back
  paused after a restart.
- **Scheduled tasks** synchronize folders or run a server command once, every few minutes, hours
  or days, or at a time of day on chosen weekdays. Tasks run while Poros is open, and a run missed
  while it was closed happens once at the next start.
- **When the queue finishes**, Poros can close, lock, sleep, hibernate, log off or shut down after a
  countdown, run a local command, show a notification or play a sound.
- **Server commands** from the right-click menu, with the selected files filled in and the output
  streamed into a window.
- **Proxies and jump hosts**: SOCKS5, SOCKS4/4a and HTTP CONNECT proxies, and chains of saved
  connections used as jump hosts, like `ProxyJump` in OpenSSH.
- **Authentication** with OpenSSH, PEM, PKCS#8 and PuTTY `.ppk` (v2, v3) keys, SSH agents,
  Pageant, password and keyboard-interactive login.
- **Host key verification** against your existing `~/.ssh/known_hosts`, which Poros never modifies.
  FTPS certificates the system does not trust get the same approval prompt, by fingerprint.
- **Saved connections**, with passwords and cloud sign-ins stored only on request and only in the
  system keychain.
- **Themes** as shareable JSON files, ten built in, with every color editable in settings.
- **Signed updates**: Poros checks for a new release at start-up and installs it only after
  verifying its signature. Settings > Updates turns the check off.

## Performance

3,600 files totaling 1.4 GiB (3,000 of 16 to 256 KiB, 560 of 1 MiB, 40 of 12 MiB), median of
three runs over a 1 Gbit/s link:

| Client                     | Upload | Download |
| -------------------------- | -----: | -------: |
| Poros, 1 worker            | 20.3 s |   26.2 s |
| Poros, 4 workers (default) | 14.1 s |   15.5 s |
| Poros, 8 workers           | 13.5 s |   13.3 s |
| OpenSSH sftp 9.6           | 17.6 s |   17.9 s |
| rsync 3.2.7                | 13.6 s |   13.7 s |

At 8 workers Poros sustains about 900 Mbit/s, 95% of the link's measured TCP throughput. With
Verify checksums turned on, a similar transfer over a 1 Gbit/s link with 1 ms latency took about
30% longer; the default checks cost nothing measurable.

Setup: Poros 0.2.0 release build and a stock OpenSSH 9.6 server on Ubuntu 24.04, in separate
network namespaces joined by a link capped at 1 Gbit/s (941 Mbit/s measured with iperf3, no added
latency), files on RAM disks, 4-core Xeon at 2.1 GHz. To reproduce, run
`POROS_TEST_SSH_PORT=2222 cargo run --release --example transfer_benchmark` in `src-tauri`
against a server set up as for the end-to-end tests.

On a real connection, gigabit at both ends, Poros moved 3,643 files (1.4 GiB) in 16.6 s, about
720 Mbit/s.

## Install

Download an installer from the [latest release](https://github.com/khaodius/poros/releases/latest):

| Platform | Packages                             |
| -------- | ------------------------------------ |
| Windows  | `-setup.exe` (recommended) or `.msi` |
| Linux    | `.deb`, `.rpm` or `.AppImage`        |

To upgrade 0.2.0, install the newer package over it; settings and saved connections are kept.
Later versions update themselves.
Changes are listed in the [changelog](CHANGELOG.md).

### Verifying a download

Every release includes `SHA256SUMS.txt` and a signed build provenance attestation for each file.

```sh
# Linux, in the download folder
sha256sum --check --ignore-missing SHA256SUMS.txt

# Windows (PowerShell), compare the output with SHA256SUMS.txt
Get-FileHash <file> -Algorithm SHA256

# Provenance, with the GitHub CLI
gh attestation verify <file> --repo khaodius/poros
```

The Windows installers are not code-signed yet, so SmartScreen may warn about an unknown publisher.

## Notes

- **SFTP only**: folder sync, rsync, scheduled tasks, server commands, moving and copying on the
  server, Properties, comparing server files, checksum verification, proxies and jump hosts. Files
  pasted or dragged within an FTP or cloud server are copied through the transfer queue, and FTP
  uploads are written in place rather than under a temporary name.
- **Google Drive and OneDrive** sign in through the system browser. Builds without app credentials
  ask for your own under Settings > Cloud accounts; see
  [Registering the cloud apps](#registering-the-cloud-apps).
- **rsync** runs over SSH only and needs rsync 2.6.4 or newer on the server. rsync daemons
  (`rsync://`, port 873) are not supported. Servers without rsync fall back to SFTP.
- **Data** is stored in `~/.config/io.github.khaodius.poros/` on Linux and
  `%APPDATA%\io.github.khaodius.poros\` on Windows. Themes live in its `themes/` folder,
  unfinished transfers in `transfers.json` and scheduled tasks in `schedules.json`. Neither file
  holds passwords or sign-ins.
- **Wayland**: Poros sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` and `__NV_DISABLE_EXPLICIT_SYNC=1`
  at startup to avoid blank windows on some compositors and NVIDIA drivers. Set either variable
  to override.

## Keyboard shortcuts

| Keys                         | Action                             |
| ---------------------------- | ---------------------------------- |
| Ctrl + L                     | Type a path                        |
| Ctrl + F                     | Filter                             |
| Tab                          | Switch pane                        |
| Backspace, Alt + Up          | Parent folder                      |
| Alt + Left, Alt + Right      | Back, forward                      |
| F2                           | Rename                             |
| Ctrl + X, Ctrl + C, Ctrl + V | Cut, copy, paste                   |
| Alt + Enter                  | Properties                         |
| Alt + Down, Alt + Up         | Next, previous change in a compare |
| F7, Ctrl + Shift + N         | New folder                         |
| F5, Ctrl + R                 | Refresh                            |
| Ctrl + T, Ctrl + W           | New tab, close tab                 |
| Ctrl + ,                     | Settings                           |

## Build from source

Requires [Node.js](https://nodejs.org) 22+, [Rust](https://rustup.rs) 1.88+ and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
git clone https://github.com/khaodius/poros.git
cd poros
npm ci
npm run tauri dev      # run with hot reload
npm run tauri build    # installers in src-tauri/target/release/bundle
```

## Development

CI runs these checks on Linux and Windows:

```sh
npm run lint && npm run format:check && npm run typecheck && npm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

End-to-end tests in `src-tauri/tests/` run against a real SSH server when `POROS_TEST_SSH_PORT`
is set, and against FTP servers when the `POROS_TEST_FTP*` variables listed in `ftp_server.rs` are
set. The CI workflow shows how to start them.

To release, set the new version in `package.json`, `src-tauri/Cargo.toml` and
`src-tauri/tauri.conf.json`, update the lockfiles (`npm install`, then `cargo check` in
`src-tauri`), add a section to [CHANGELOG.md](CHANGELOG.md) and merge to `main`. The release
workflow builds, tags and publishes it, and signs the installers for the updater with the
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets, which
hold the private key matching the public key in `src-tauri/tauri.conf.json`. Without them the
release stops before building. The `POROS_GOOGLE_CLIENT_ID`, `POROS_GOOGLE_CLIENT_SECRET` and
`POROS_MICROSOFT_CLIENT_ID` secrets give release builds their cloud sign-in; without them, users
enter their own.

### Registering the cloud apps

Each provider needs an app registration. Store the values as the repository secrets above, or paste
them under Settings > Cloud accounts.

**Google Drive**

1. In [Google Cloud Console](https://console.cloud.google.com/), create a project and enable the
   Google Drive API under APIs and Services > Library.
2. Under Google Auth Platform, set up the consent screen with audience External. While the app is
   in Testing, add every Google account that will sign in under Test users.
3. Under Clients, create a client of type Desktop app and copy its client ID and client secret.

In Testing, Google expires sign-ins after 7 days. Publishing the app with the full Drive scope
requires Google's verification.

**OneDrive**

1. In the [Microsoft Entra admin center](https://entra.microsoft.com/), open App registrations >
   New registration and choose Accounts in any organizational directory and personal Microsoft
   accounts.
2. Under Authentication, add the Mobile and desktop applications platform with the redirect URI
   `http://localhost`, and set Allow public client flows to Yes.
3. Under API permissions, add the Microsoft Graph delegated permissions `Files.ReadWrite.All` and
   `offline_access`.
4. Copy the Application (client) ID. There is no secret.

## License

[Apache License 2.0](LICENSE)
