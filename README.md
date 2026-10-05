<p align="center">
  <img src="public/poros.svg" alt="Poros" width="96" height="96">
</p>

<h1 align="center">Poros</h1>

<p align="center">
  A fast, keyboard-friendly SFTP client for Linux and Windows.
</p>

<p align="center">
  <a href="https://github.com/khaodius/poros/actions/workflows/ci.yml"><img src="https://github.com/khaodius/poros/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License: Apache-2.0"></a>
</p>

It is a native desktop app built with
[Tauri](https://tauri.app): the interface is TypeScript and React, and everything that touches
the network or the disk is Rust.

## Features

- **Dual-pane browsing** of local and remote folders with breadcrumbs, a typed path bar,
  history, sorting, filtering and hidden file toggling. Lists are virtualized, so large folders
  stay responsive.
- **Every common key format**, including the ones other clients choke on:
  - OpenSSH, PEM (PKCS#1) and PKCS#8 private keys, encrypted or not.
  - PuTTY `.ppk` files, version 2 and 3, encrypted (Argon2id, Argon2i, Argon2d) or not, with
    RSA, Ed25519 and ECDSA keys.
  - Files mangled by Windows editors or mail clients: byte order marks, CRLF line endings,
    stray blank lines and stripped comment spaces are repaired before parsing.
  - A clear "wrong passphrase" message instead of a generic failure.
- **SSH agents**: `SSH_AUTH_SOCK` on Linux, and the OpenSSH Authentication Agent service or
  Pageant on Windows.
- **Password and keyboard-interactive** authentication.
- **Host key verification** that reads your existing `~/.ssh/known_hosts` (hashed entries,
  wildcards, `@revoked` markers and non-standard ports included) and asks before trusting a new
  or changed key. Poros never writes to your OpenSSH files.
- **File management** on both sides: new folder, rename, recursive delete with confirmation,
  copy path.
- **Symlink aware**: links to folders can be opened, broken links are marked.
- **Activity log** with server banners and every operation Poros performs.
- **Light and dark themes** that follow your system.
- **Native on Linux and Windows**, including Wayland sessions, Windows drive letters and UNC
  paths.

### Roadmap

- Transfer queue with parallel workers, pause, resume and retry.
- Folder sync built on rsync.
- Settings for appearance and behavior.
- Site manager with saved connections.

## Install

Prebuilt installers are attached to each [release](https://github.com/khaodius/poros/releases)
once one is published:

| Platform | Packages                    |
| -------- | --------------------------- |
| Windows  | `.msi`, `-setup.exe`        |
| Linux    | `.deb`, `.rpm`, `.AppImage` |

Every CI run also uploads installers as build artifacts.

## Build from source

You need [Node.js](https://nodejs.org) 22 or newer and [Rust](https://rustup.rs) 1.88 or newer.

### Linux

Install the WebKitGTK development packages first.

Debian and Ubuntu:

```sh
sudo apt install build-essential curl wget file libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libssl-dev libxdo-dev
```

Fedora:

```sh
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel
sudo dnf group install c-development
```

Arch Linux:

```sh
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
```

### Windows

Install the [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
with the "Desktop development with C++" workload. WebView2 ships with Windows 10 and 11.

### Run and package

```sh
git clone https://github.com/khaodius/poros.git
cd poros
npm ci
npm run tauri dev      # run with hot reload
npm run tauri build    # installers in src-tauri/target/release/bundle
```

## Usage

Type a host into the bar at the top and press Enter. It accepts `host`, `user@host`,
`user@host:port` and `sftp://user@host:port/path`. The button beside **Connect** opens the full
connection dialog for key files and SSH agents.

The first time you connect to a server, Poros shows its key fingerprint. Choose **Trust and
connect** to remember it, or **Connect once** to trust it for this session only.

### Keyboard

| Keys                                    | Action                          |
| --------------------------------------- | ------------------------------- |
| Up, Down, Page Up, Page Down, Home, End | Move                            |
| Shift + movement                        | Extend the selection            |
| Ctrl + click, Shift + click             | Toggle, select a range          |
| Ctrl + A                                | Select all                      |
| Enter                                   | Open folder                     |
| Backspace, Alt + Up                     | Parent folder                   |
| Alt + Left, Alt + Right                 | Back, forward                   |
| F5, Ctrl + R                            | Refresh                         |
| F2                                      | Rename                          |
| F7, Ctrl + Shift + N                    | New folder                      |
| Delete                                  | Delete                          |
| Ctrl + L                                | Type a path                     |
| Ctrl + F                                | Filter                          |
| Tab                                     | Switch between local and remote |
| Letters                                 | Jump to a matching name         |
| Escape                                  | Clear the selection             |

### Where Poros keeps its data

Trusted host keys go to `known_hosts` in the app's config folder:

- Linux: `~/.config/io.github.khaodius.poros/`
- Windows: `%APPDATA%\io.github.khaodius.poros\`

### Wayland

Poros sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` and `__NV_DISABLE_EXPLICIT_SYNC=1` at startup,
which avoids blank windows and protocol errors on some Wayland compositors and NVIDIA drivers.
Set either variable yourself to override it.

## Development

```sh
npm run lint && npm run format:check && npm run typecheck && npm test
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs all of the above on Linux and Windows and builds installers for both.

### End-to-end tests

`src-tauri/tests/sftp_server.rs` connects to a real OpenSSH server and is skipped unless
`POROS_TEST_SSH_PORT` is set. It expects a server on `127.0.0.1` that accepts the user
`POROS_TEST_SSH_USER` (default `poros`) with the password `POROS_TEST_SSH_PASSWORD` (default
`poros-pass`) and every key in `src-tauri/tests/fixtures/keys/*.pub`. A throwaway server on
Linux:

```sh
sudo useradd --create-home poros && echo 'poros:poros-pass' | sudo chpasswd
sudo -u poros mkdir -p /home/poros/.ssh /home/poros/data
cat src-tauri/tests/fixtures/keys/*.pub | sudo -u poros tee /home/poros/.ssh/authorized_keys >/dev/null
sudo chmod 600 /home/poros/.ssh/authorized_keys

mkdir -p /tmp/poros-sshd
sudo ssh-keygen -q -t ed25519 -N '' -f /tmp/poros-sshd/host_ed25519
cat > /tmp/poros-sshd/sshd_config <<EOF
Port 2222
ListenAddress 127.0.0.1
HostKey /tmp/poros-sshd/host_ed25519
PidFile /tmp/poros-sshd/sshd.pid
PasswordAuthentication yes
UsePAM no
Subsystem sftp internal-sftp
EOF
sudo mkdir -p /run/sshd && sudo /usr/sbin/sshd -f /tmp/poros-sshd/sshd_config

cd src-tauri && POROS_TEST_SSH_PORT=2222 cargo test
```

Test keys are encrypted with the passphrase `poros-test`.

## License

[Apache License 2.0](LICENSE)
