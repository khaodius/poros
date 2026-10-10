# Changelog

Installers for every version are attached to its
[release](https://github.com/khaodius/poros/releases), with SHA-256 checksums.

## 0.3.2 - 2026-10-10

Fixes from a code review, each with a test of the failure it fixes.

- **Interrupted transfers keep your files.** Pausing a transfer as its last bytes arrive keeps
  it paused until the file is renamed into place, instead of showing it done with the data left
  in a hidden temporary file. Two transfers to the same file take turns instead of writing into
  one temporary file. On servers without OpenSSH's rename extension, overwriting a file moves
  the old copy aside until the new one is in place, so a dropped connection never leaves
  nothing behind.
- **Synchronization leaves alone what it cannot read.** A folder either side could not list is
  no longer treated as missing, so it is neither deleted nor copied over. A scheduled
  synchronization never deletes files to mirror an empty folder, such as an unplugged drive,
  and reports why it held back.
- **Moving a folder to another disk** moves it one item at a time, locally and on servers that
  do not allow commands, so anything that fails to copy stays where it was instead of being
  deleted with the original.
- **Two crashes fixed**: long terminal output mixing plain text with accented letters or emoji,
  and a malformed checksum reply from a server during an rsync upload.
- **Security**: a known server that shows a different kind of host key gets the changed-key
  warning instead of the new-server prompt. File names from a server that would land outside
  the chosen folder on Windows, such as `C:x.dll`, are refused. Names that look like options,
  such as `-rf`, get `./` in front in server commands. A saved password is sent only to the
  host, port, user and sign-in method it was saved with, so editing a site's host asks for it
  again.
- **Slow and stalled connections**: SFTP and cloud transfers fail only when no data has moved
  for the timeout, not when a slow link takes longer than that. An FTP server that stops
  sending mid-transfer or mid-listing is given up on instead of hanging the connection.
- **Opening Poros a second time** brings the open window forward instead of starting a second
  copy that would run every scheduled task twice.
- **Damaged settings, schedules or queue files** are kept as `<name>.corrupt-<time>` and
  reported at start-up, instead of being silently replaced.
- **Keyboard and mouse**: opening a folder while it refreshes always opens it, arrow keys work
  again after a dialog or menu closes, and Ctrl+W and Ctrl+T do nothing while you type in a
  field or a dialog is open.

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
