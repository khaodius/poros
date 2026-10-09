# Changelog

Installers for every version are attached to its
[release](https://github.com/khaodius/poros/releases), with SHA-256 checksums.

## 0.3.1 - 2026-10-09

- **Dragging a tab out of a window** works across the whole desktop. Once the tab leaves the
  window, its pane there fades out instead of sliding along the window's edge, and the
  preview follows the pointer right up to the edges of the screen. The new window opens
  exactly where the preview was, on whichever monitor it is, including monitors scaled
  differently from the window the tab came from.

## 0.3.0 - 2026-10-09

- **FTP, FTPS, Google Drive and OneDrive** alongside SFTP. Drop files from one server tab onto
  another to copy between servers; two FTP servers send the file straight to each other (FXP)
  when both allow it.
- **Built-in editor and SSH terminal**: edit local and server files in a tab, keeping their
  encoding and line endings, with a warning when a server file changed since it was opened.
  Any server tab can open a shell in a tab of its own.
- **File operations on the server**: cut, copy and paste, Move to and Copy to, Properties for
  permissions, owner, group and folder sizes, and a side-by-side compare of any two files,
  local or on a server.
- **Automation**: scheduled folder synchronizations and server commands, actions for when the
  queue finishes (notify, play a sound, sleep, shut down or run a command), saved server
  commands, SOCKS and HTTP proxies, and jump hosts.
- **Safer transfers**: files are written under a temporary name and renamed into place once
  complete, resumes check the data already there, checksums can be compared after each file,
  and transfers wait out dropped connections and carry on. Unfinished transfers come back
  paused at the next start.
- **In-app updates**: Poros checks for a new release at start-up or from Settings, and installs
  it only after verifying its signature against the update key built into the app.
- **Tabs and windows**: drag a tab from one Poros window into another. A tab dragged out over
  the desktop shows a small preview of the window it will become, and the window opens exactly
  there, kept on screen. The editor and terminal open in a pane of their own beside the pane
  they came from, and the other panes make room.
- **Rounded window corners** on Windows 11, following the corner roundness setting.

Copies installed from the 0.2.0 release have no updater, so install 0.3.0 over them once; later
versions arrive through the in-app updater. Settings and saved connections are kept.

## 0.2.0 - 2026-10-08

The first published release of Poros, a dual-pane SFTP and rsync client for Linux and Windows.

- **Tabs and docking**: local folders and server sessions are tabs. Drag a tab onto another
  pane to merge it in or split the pane, resize with the dividers, or pull a tab out into a
  window of its own.
- **Transfer queue** with parallel workers, each on its own SSH connection. Large files are
  split across idle connections, and the worker count can change while transfers run.
- **Folder synchronization** one way or both ways, by size and time, size alone or contents,
  with rsync-style excludes and a preview of every upload, download, deletion and conflict
  before anything changes.
- **rsync delta transfers** over Poros's own SSH connection, so only the changed parts of a file
  cross the network and nothing needs installing locally, Windows included. Servers without
  rsync get the whole file over SFTP.
- **Saved connections**. Passwords and passphrases are kept only when you ask, and then only in
  the system keychain.
- **Every common key format**: OpenSSH, PEM, PKCS#8 and PuTTY `.ppk` versions 2 and 3, plus
  SSH agents and Pageant, password and keyboard-interactive login.
- **Host key checks** against your existing `known_hosts`, which Poros never writes to.
- **Themes** as shareable JSON files, ten built in, with a color picker for every part of the
  interface, and settings for fonts, text size, corner roundness and row height.
- **Windows setup** that upgrades an older Poros in place, keeping settings and saved
  connections.
