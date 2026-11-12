# Changelog

Installers for every version are attached to its
[release](https://github.com/khaodius/poros/releases), with SHA-256 checksums.

## 0.5.7 - 2026-11-12

- **Every transfer is checked, by default.** The checks under Settings > Transfers > Integrity
  are all on, including on installs updated from an earlier version, where Verify checksums and
  Flush to disk come on once and stay as you set them after that. Each can be turned off. On
  3,600 mostly small files over a link with 1 ms latency, they add 24% to an upload and 31% to a
  download, and 68 to 89% on a server that has every file read back; large files cost less, as
  most of the cost is a round trip per file. The README's
  [Integrity checks](README.md#integrity-checks) section has the measurements.
- **Checksums are SHA-256, part by part.** A finished file is compared with its source by
  SHA-256, computed on SFTP servers by their own `sha256sum`, or `sha512sum` where that is all
  there is. SHA-1 and MD5, which can be forged, are no longer used. Where the server has GNU
  `split`, files of 4 MiB and more are hashed in up to 64 parts, so a file that arrives damaged
  has only the parts that differ sent again, up to half the file; beyond that it starts over.
  The checksum commands run through one shell each worker keeps open on the server, so a file's
  check costs a single round trip rather than a new channel and login shell. rsync results, which
  rsync itself checks with MD5, are compared by SHA-256 too, and fall back to copying the whole
  file if they differ.
- **Servers that cannot hash a file have it read back.** On SFTP servers without a checksum
  command, such as ones that allow SFTP alone, and on FTP and WebDAV servers, each finished file
  is read back and hashed here. This doubles the traffic with those servers, and can be turned
  off on its own with Read files back when the server cannot hash them.
- **Resumed files are checked before they continue.** The part already transferred is compared
  with the source part by part, and the transfer continues from the first part that differs,
  where before only the last 64 KiB were compared and a mismatch started it over. A file that
  failed a check starts over in full on its next attempt, where before a resume could take a
  damaged copy of the same size as already there.
- **A source that changes while it is sent starts over.** Local files are compared to the
  nanosecond, and on Linux by inode and status change time too, before and after they are read
  and hashed, so an edit that puts the modification time back is still caught.
- **Files are on disk before they count as done.** Flush to disk is on: the server stores each
  upload, and this computer each download and the folder entry its rename makes, before the file
  is finished.
- **Times and permissions are read back.** Once a file is in place, its size, time and
  permissions are read again, and any the server or file system did not keep are noted in the
  log.
- **Downloads are checked against what their publisher signed.** A download is compared with
  `SHA256SUMS`, `SHA512SUMS`, `name.sha256` and similar files beside it on the server, with
  minisign `.minisig` signatures by keys you add under Trusted minisign keys, and through `gpg`,
  where it is installed, with `.asc` and `.sig` signatures by keys in your keyring. A file that
  fails is not saved and is not retried; one with nothing published beside it downloads as
  before.

## 0.5.6 - 2026-11-12

- **Progress bars your way.** Settings > Appearance > Progress bars picks how bars are filled
  (solid, gradient, striped, segmented or glowing), their color (the theme's, the theme's upload
  and download colors, or one of your own), how thick the transfer list's bars are, rounded or
  square ends, and what moves on a bar while its work runs: stripes, a shimmer or a pulse, which
  hold still when the system asks for less motion. Each transfer can show its percent, what has
  arrived of its size, the time left or nothing, beside the bar or on it, where the text changes
  color as the fill passes under it. Finished, paused and failed transfers keep their green, gray
  and red unless you turn that off. A preview shows every change as you make it, and the look
  applies to the other progress bars in Poros too.
- **Flying files, for fun.** With Settings > Appearance > Flying files on, files you upload,
  download, move or copy hop out of their rows as a pile of their own icons, arc across the window
  and land in a heap where they go, which lights up as they arrive: the folder they were dropped
  on, the pane, or its tab when the pane is not on show. Folders spill a few files on the way, a
  count rides on piles of more than one, and at most 24 icons fly however many files there are.
  It is off unless you turn it on, a button beside the switch plays it first, and it stays still
  when the system asks for less motion.

## 0.5.5 - 2026-11-10

- **Finished transfers always show as finished.** In a large batch the queue could leave a file
  shown as transferring after it had arrived, when the window took in an update older than what it
  already showed, or missed one. Updates are now numbered: the window ignores ones it already has,
  reads the whole queue again when one goes missing, and reads it once more each time the queue
  settles.
- **Transfers wait for a server that stops answering, then carry on.** A connection dropping in
  the middle of an SFTP transfer was reported as an ordinary error, so the transfer spent its few
  quick retries and failed while the server was still away. It now counts as a lost connection:
  the server's transfers wait for it, without using up their retries, for as long as Settings >
  Connection > Keep trying for allows. Transfers that failed because the server stayed away longer
  than that go back in the queue on their own once Poros reaches the server again, as when its tab
  reconnects or another transfer to it gets through.
- **The drop highlight comes back with a pane dragged back into the window.** With a game or
  another program open that takes the pointer, a pane dragged past the window's edge and back lost
  its drop highlight, which then stayed where the pointer came back in, if it showed at all. The
  drag now follows the pointer itself until the window hears from it again, and letting go over
  the dragged pane drops it there.
- **More of a window's top left moves it.** The margin around the panes, including the strip
  between the top bar and the tabs, and the space beside the tabs move the window as the top bar
  does, so a window just torn out can be taken up again without aiming for the top bar. Double
  clicking beside the tabs still opens a new tab. A torn-out window opens with the pointer halfway
  down its top bar, on the Poros name.

## 0.5.4 - 2026-11-01

- **Drag files out to Explorer, GNOME Files and Dolphin.** Files and folders dragged from a tab
  past the edge of the window now drop into Windows Explorer, the desktop, GNOME Files, Dolphin
  or any other program that takes files. Files on a server download ahead of the rest of the queue
  while you drag. Explorer copies them in once they arrive, with its own progress window. GNOME
  Files and other Linux file managers wait for the download and then copy them, and Dolphin and
  the KDE desktop tell Poros the folder they were dropped in, so they download straight there.
  Letting go back over Poros drops them as if they had never left the window.

## 0.5.3 - 2026-11-01

- **Import connections from other apps.** Import in the Connect dialog and on the new tab page
  copies the saved sites of FileZilla, WinSCP, SmartFTP, Cyberduck and Mountain Duck, PuTTY and
  KiTTY, MobaXterm, the OpenSSH config (`~/.ssh/config`, with its `Include` files) and the server
  bookmarks of GNOME Files and KDE Dolphin. Poros finds them where each app keeps them, including
  the Windows registry, or reads a file you choose and works out which app wrote it. Every site is
  listed first with how it signs in, so you pick which to import; sites already saved are left
  unticked. Folders can go in front of the names, jump hosts and tunnels (`ProxyJump`, WinSCP
  tunnels, MobaXterm gateways) become saved jump hosts, and the passwords FileZilla and WinSCP
  keep go straight to the system keychain unless you untick that. Sites Poros cannot connect to,
  such as Telnet sessions, are listed with the reason.

## 0.5.2 - 2026-11-01

- **Local files are grouped by date, as in Windows Explorer.** The local list puts its files under
  Today, Yesterday, Earlier this week, Last week, Earlier this month, Last month, Earlier this
  year and A long time ago, going by the calendar and the week your language starts on. Within
  each group the files keep the order you sort by, folders first if you keep them there. Sorting
  by Modified oldest first turns the groups around too. Click a group's arrow to collapse or
  expand it, or its heading to select everything in it; Ctrl or Shift adds to the selection.
  Group by in the right-click menu turns grouping off or on for the list and collapses or
  expands every group at once. Settings > Appearance > Grouping has a switch for local lists and
  one for server lists, which stay ungrouped unless you turn it on.

## 0.5.1 - 2026-10-21

- **Click a selected name again to rename it.** As in Windows Explorer, clicking the name of the
  only selected file once more, after a pause, turns the name into a field to type the new one
  in, with the name before the extension selected. Enter or clicking elsewhere renames, Esc
  leaves the name as it was. A double click still opens the file, and dragging or pressing a key
  before the field appears leaves the name alone. F2 and Rename in the menu now rename in the row
  too, instead of in a dialog.
- **Deleting on an SFTP server is faster and shows its progress.** Poros asks the server to run
  `rm` when its shell allows it, and otherwise deletes over four connections at once, so a folder
  with thousands of files goes in seconds rather than minutes. While it runs, the operations panel
  shows how many entries are gone out of how many there are, the path being deleted and the
  latest entry that could not be deleted with the server's reason, and Cancel stops it. Entries
  that could not be deleted are listed in the log.
- **A dragged pane stays above other programs.** The pane being dragged could be drawn behind
  other programs' windows and the taskbar, so it seemed to disappear while dragging. It is now
  put back on top every time it shows and as it moves.

## 0.5.0 - 2026-10-18

- **Copy from programs that take over the mouse.** Text selected in a terminal program that draws
  its own selection and copies through the terminal, such as tmux in mouse mode or an AI coding
  assistant, now reaches the clipboard. These programs send the text as an OSC 52 sequence,
  which the terminal used to ignore. A server can put text on the clipboard this way but never
  read it. Hold Shift while dragging to select with the terminal itself instead.
- **The terminal cursor fades.** It fades out and back in rather than snapping on and off, a
  little slower than before. With reduced motion turned on in the system it blinks as before.
- **Follow finished transfers.** Follow in the Completed list keeps each transfer in view as it
  finishes, until you turn it off.
- **Drag to select transfers.** Dragging across the Queue, Failed or Completed list selects every
  transfer from the one you pressed on to the one under the pointer. Holding the pointer near or
  past the list's top or bottom edge scrolls it, and so does the mouse wheel. Hold Ctrl or Shift
  to add to the selection, and press Esc to cancel.

## 0.4.9 - 2026-10-13

- **A dragged pane stays on top of the drop highlight.** The highlight of where the pane will
  land is drawn under the pane rather than over it, so it no longer tints the pane or cuts across
  it, and where the pane reaches past the window's edge the two windows no longer draw parts of
  it a few pixels apart. Split down, Merge as tab and the rest now show in the pane's own title
  bar.
- **Replacing files asks at once.** Dropping files on a server that already has them shows the
  File already exists prompt as soon as the transfer reaches them. It used to wait until the
  window was clicked again when Windows had not handed the page its focus, which could happen
  while dragging. The prompt now shows in whichever Poros window was used last.

## 0.4.8 - 2026-10-12

- **Ctrl + Shift + click adds a range.** It selects every row from the one last clicked to the
  one you click and keeps the rows already selected, as in Windows Explorer, so Ctrl + click one
  file and Ctrl + Shift + click another to fill in the rows between them.
- **Resize the file list's columns** by dragging the line before a heading. The name column gives
  or takes the room, so the line follows the pointer. Double-click the line to fit the column to
  what it shows. Widths are remembered, and Reset column widths in a heading's right-click menu
  puts them back.
- **System file icons.** Settings > Appearance can show the icons Windows Explorer uses for each
  type of file, or your Linux desktop's icon theme, in place of Poros's own, for files on servers
  too.
- **Smooth scrolling.** Settings > Appearance can make the mouse wheel glide file lists, the log
  and other panels rather than jump. Touchpads scroll as before.
- **Copy the whole path.** The path bar keeps room after the last folder however long the path, so
  a click there turns it into a field with the whole path selected. A path too long to show keeps
  its end in view and fades out at its start.
- **Merging a pane shows where its tab goes.** Dragged onto another pane's tab bar, the pane
  shrinks away into a tab that opens between the tabs where it will land, and the pane it joins
  lights up with Merge as tab. Dropping a pane on the middle of another opens its tab at the end
  of that pane's tab bar the same way.

## 0.4.7 - 2026-10-12

- **A dragged pane drops where the pointer is.** Over a pane, the outer third on each side splits
  it that way and the middle merges the tab in, measured at the pointer rather than at the middle
  of the lifted pane. Dragging a pane low over another one no longer shows nothing, then Split
  down, then nothing again as the pointer rises a little. The marker also holds still while the
  pointer wavers on the line between two zones.
- **Right-clicking during a drag cancels it**, as Escape does and as Windows' own drag and drop
  does, and opens no menu. With the pointer over another program's window, the drag used to
  stall for a few seconds instead. The same goes for the box that selects files.
- **The box that selects files keeps up with the pointer.** It is drawn as the pointer moves
  instead of a frame later, and selecting rows redraws only the rows that change.

## 0.4.6 - 2026-10-11

- **Updates download in the background.** Choosing Update, whether Poros offered it at start-up
  or you found it in Settings, closes the dialog, and a notice in the bottom-right corner shows
  the download while you keep working. Once the update is ready, the notice offers Restart now
  or Later. Later leaves it for the next time you open Poros; on Windows it installs as you close
  Poros. Errors show in the same notice, with Try again.
- The status bar and Settings > Updates show how far the update has got. Clicking the status bar
  brings back a hidden notice.
- **Install another version.** Settings > Updates lists the releases from 0.4.5 on, so you can go
  back to an earlier one, and installs the one you pick the same way as an update, signature check
  included. An .rpm install lists only newer releases, since rpm does not install an older
  version over a newer one.

## 0.4.5 - 2026-10-11

- **Drag a box to select files.** Pressing beside a file's name, or in the empty space below the
  rows, and dragging draws a box that selects every row it touches. Near the top or bottom of the
  list it scrolls, faster the further the pointer goes. Hold Ctrl or Shift to add the rows to the
  selection, or press Escape to put the selection back. Dragging a file by its name, or any
  selected row, still moves the files.

## 0.4.4 - 2026-10-11

- **A dragged pane past the window's edge is one pane again.** On Windows the part drawn over
  the desktop sat several pixels off the pane, so its edges, title and drop marker showed
  twice. It now lands exactly on the pane, and is drawn the same way, rounded corners, edge and
  shadow included, so nothing changes where the window's edge cuts across it. A tab dropped on
  the desktop also opens exactly where its preview was.
- **Taking a screenshot while dragging** no longer leaves the drag running after it, with
  everything slow until Escape. When another program takes the mouse, such as Windows' screen
  snipping, the drag ends as with Escape. The same goes for resizing panes and the queue.

## 0.4.3 - 2026-10-11

- **Thinner scrollbars** that lie on the border of the pane or panel they scroll. Each end of the
  thumb curves into that border, so it reads as part of it. The thumb widens under the pointer,
  and the terminal's scrollbar matches.
- **The mouse's back and forward buttons** go back and forward in the pane under the pointer.
- **Back and Forward remember the cursor**: returning to a folder puts the cursor where it was, or
  on the folder you came out of, and scrolls it into view, as going up now does too. Pressing Back
  several times quickly steps back that many folders.
- **Keyboard**: Space selects or deselects the cursor's row, Insert does and moves down, Ctrl+Up
  and Ctrl+Down move without selecting, Alt+Home goes to the home folder, Ctrl+H shows hidden
  files, the Menu key or Shift+F10 opens the menu for the selection, and Ctrl+Tab or
  Ctrl+PageDown and Ctrl+PageUp switch tabs. Leaving the path bar or the filter with Enter or
  Escape gives the keyboard back to the file list.
- A new, renamed or pasted item is selected, and after a delete the cursor moves to the next row.
- F5 and Ctrl+R refresh the active pane from anywhere outside the terminal and the editor,
  instead of reloading the window.
- The tab bar scrolls with the wheel when its tabs do not fit, and double-clicking its empty part
  opens a tab.
- Notices stay while the pointer is on them, and a repeated notice no longer stacks.
- Fixed: the queue's toolbar could remove a transfer that had finished while it was selected, and
  the terminal showed a black strip below its last row.

## 0.4.2 - 2026-10-11

- **New file** shows an example name in faint text, such as `example.lua`, `example.cpp` or
  `example.json`, with a different kind of file each time it opens. It disappears as you type.

## 0.4.1 - 2026-10-11

- **rsync's options for folder synchronization**, grouped as rsync's manual groups them: which
  files are copied, transfer behaviour, deletion, filter rules, times, permissions and
  ownership, symbolic links, bandwidth and compression, backups, and dry runs with reporting.
  Each option shows the flag it stands for, and a raw arguments box takes anything typed as on
  rsync's command line, such as `-av --delete --exclude=.git/`. The command line the options
  amount to is shown as you change them, with any mistakes.
- **Defaults, sites and single runs**: Settings > Sync holds the defaults, a saved SFTP
  connection can add rsync arguments of its own and its own rsync command, and Synchronize
  folders and scheduled tasks can change any option for one synchronization.
- **The preview shows more**: rsync's itemized change codes, notes on the options, deletions
  held back by `--max-delete`, protected and size-limited files, and a dry run that changes
  nothing and writes what would happen to the log.
- Synchronizations now keep times and permissions as their own `-t` and `-p` options say,
  starting from the transfer settings' choices.

## 0.4.0 - 2026-10-11

- **WebDAV**, over HTTP or HTTPS, with Basic or Digest sign-in. Quick connect takes `dav://`,
  `davs://`, `http://` and `https://` addresses. Certificates the system does not trust get the
  same approval prompt as FTPS, by fingerprint.
- **Amazon S3 and S3-compatible services** such as Wasabi, Cloudflare R2, MinIO and Backblaze
  B2's S3 endpoint. Leave the endpoint empty for Amazon S3 or enter any other, with its region
  and bucket addressing. Buckets show as folders at the top level, and a key restricted to one
  bucket opens in it.
- **Backblaze B2** over its native API, with an application key ID and application key. Large
  files go up in parts, deleting hides a file so its earlier versions stay, and a key
  restricted to one bucket opens in it.
- **Box**, signing in through the browser like Google Drive and OneDrive. Register a Box app
  under Settings > Cloud accounts; the steps are listed there and in the README.
- Every new kind of connection browses, uploads and downloads with parallel parts for large
  files, renames, deletes, creates folders and creates new files without replacing one already
  there.

## 0.3.5 - 2026-10-11

- **A dragged pane reaches past the window's edge.** Dragging a tab toward the edge of its
  window, the lifted pane carries on over the desktop beside it instead of being cut off at the
  edge, showing its own content, and the marker of where it will land shows over it on both
  sides of the edge.

## 0.3.4 - 2026-10-11

- **New file** next to New folder, in each pane's toolbar and right-click menu, or with
  Shift+F4. It creates an empty file on this computer or on any server, SFTP, FTP, FTPS, Google
  Drive or OneDrive, and never replaces a file already there.

## 0.3.3 - 2026-10-11

- **Dragging a tab out of a window** is one smooth motion. Near the window's edge the pane
  turns into the window it will open as, and the preview on the desktop is that window at its
  real size instead of a small copy. When you drop the tab, the preview stays until the new
  window has drawn in its place, so nothing jumps in size or blinks on the way.

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
