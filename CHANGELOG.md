# Changelog

Installers for every version are attached to its
[release](https://github.com/khaodius/poros/releases), with SHA-256 checksums.

## 0.8.0 - 2026-11-24

- **Poros in German, Spanish, French, Japanese and Simplified Chinese.** Settings > Interface >
  Language offers Deutsch, English, Español, Français, 日本語 and 简体中文, each named in its own
  language, and System, which follows the computer's language and falls back to English for one
  Poros does not ship. The whole window switches at once, open dialogs and the transfer queue
  included, without a restart, and the page a cloud sign-in leaves in the browser speaks the same
  language. Dates, times and numbers are written for the computer's region in the interface's
  language, so an en-GB computer keeps the formats it had. Japanese and Chinese use the system's own
  fonts for their characters. Messages from the backend, the log and `poros-cli` stay in English,
  key names such as Ctrl and Shift are not translated, and the date and time fields in the scheduler
  and the speed limit rules follow the operating system's format.
- **Folders opened before show at once.** A server folder opened earlier shows its last listing the
  moment you open it again, while Poros lists it once more behind it. The refresh button spins and a
  thin line runs under the path bar until the fresh listing takes its place, keeping the scroll
  position, the cursor and the selection. Every change Poros makes on a server updates that folder
  in every pane and window. A folder that could not be refreshed stays on show, dimmed, with the
  reason and Try again. Listings are kept in memory only, for up to 300 folders, and F5 always asks
  the server. When the cursor rests, the parent folder and the folder under the cursor are listed
  ahead, except on cloud storage unless you turn that on. Settings > Interface > Server folders has
  the switches.
- **Preview the file under the cursor.** The Preview button on a pane's toolbar, or F3, opens a
  panel beside the file list, or under it in a narrow pane. It shows pictures (PNG, JPEG, GIF, WebP,
  BMP, ICO, AVIF and SVG, whose scripts never run) with their dimensions, the start of text and
  source files highlighted as in the editor, a count, a size and thumbnails for folders and
  selections, and the first bytes in hex for anything else. A file is read only once the cursor
  rests on it, and only as far as needed. Pictures too large to decode safely ask first or are left
  out, and files on cloud storage wait for Show preview unless Settings > Interface > Preview says
  otherwise. On FTP a preview waits briefly for the one connection and gives way to transfers.
  Ctrl+Shift+P moves the keyboard into the panel. Audio, video and PDF files get the details view.
- **Search a folder and every folder inside it.** The search button on a pane's toolbar,
  Ctrl+Shift+F or Search in this folder finds items by name (text, `*` and `?` patterns or regular
  expressions, with Match case), kind, size, date and depth, on every kind of connection, inside
  vaults and on this computer. Results stream into the pane with a Folder column and act like files
  in a folder: open, transfer, drag, rename and delete them, or use Go to folder. On SFTP servers
  with GNU `find` the search runs on the server, S3 and B2 list a whole tree in one paged call, and
  elsewhere Poros reads folders several at a time; on Google Drive, OneDrive and Box that is one
  folder per request, so large trees take a while. Searches stop at 10,000 results, and
  `poros-cli find` does the same from the command line.
- **Compare two panes' folders.** Compare folders, on a pane's toolbar, by Ctrl+Shift+D or in the
  pane's menu, marks the rows of two panes as newer, older, only here, different or identical, each
  with its own tint, stripe and symbol, and counts them in a bar over each pane. It works between
  this computer and any server, between two servers, inside vaults and across windows, with folder
  sync's rules for times and sizes. Compare contents of subfolders walks both trees in the
  background and marks the folders whose contents differ. Options hide identical items, compare by
  size alone, ignore time zone offsets of whole hours, include hidden files and browse both panes
  together. Select newer and only here, Select different and Synchronize these folders act on the
  result; the last opens folder sync for a local and an SFTP folder only, with the rules in
  Settings > Sync. File compare's menu entries now read Compare files.
- **Rename many items at once.** With several items selected, F2 or Rename opens Rename N items:
  find and replace with text or regular expressions, a template built from the name, extension,
  number, date, time and parent folder, a counter, letter case, extension and tidying rules, applied
  in order, with a live preview of every new name. Names that would clash, are already taken, are
  too long, or hold characters or reserved names the server or this computer refuses are flagged on
  their row, and Rename stays off until none are left. The renames run in a safe order as one
  operation that can be cancelled, and a swap cut short by a failure is undone. Each item takes its
  own rename, two round trips on a server, and on an FTP server that ignores case, a clash of case
  alone is refused only when it runs.
- **Open files in any program.** Open, in every pane's menu, opens a file in the program set up for
  its type, or else the system's default program, and Open with lists those programs, the default
  program, the built-in editor and Choose a program. Ctrl+Enter opens a file the same way. A file on
  a server, on any protocol and in vaults, is downloaded to a private working copy, and each save
  the program makes is uploaded through the transfer queue. If the server's copy changed meanwhile,
  Poros asks first, with Keep both, Overwrite, Don't upload and Compare. The status bar lists the
  files being edited and their uploads, and closing a server tab, locking a vault or quitting asks
  first while saves have not reached their server. Settings > Interface > Open with holds the
  programs and the file patterns each opens, Upload each save without asking, and how long copies
  are kept after the program closes; double-clicking a file can also open it in its program or ask.
  A file of a type that runs, such as a script, an installer or a document with macros, judged by
  its name and its contents, is never handed to the system's default program, only to a program you
  choose or set up for its type. On Windows each working copy carries the mark of a download from
  the internet, so Office opens documents from a server in Protected View.
- **Mount a server or vault as a drive on Linux.** Mount as drive..., in a server pane's menu on
  empty space and in its tab's menu, shows the connection, or an unlocked vault on it, as a folder
  on this computer, `~/Poros/<connection>` unless you choose another, so any program can open and
  save its files. Reads come from a cache filled by ranged reads. Saves are kept on this computer
  and uploaded and checked when the program closes the file, and on SFTP and FTP the upload is
  written beside the file and moved over it, so a save cut short leaves the server's file whole. A
  save that cannot reach the server stays on this computer and is offered again. Mounted drives show
  in the status bar with Open, Retry, Unmount and Force unmount, and closing the tab, disconnecting,
  locking the vault or quitting asks first while programs have files open or uploads are pending.
  `poros-cli mount MOUNTPOINT [PATH]` mounts from the command line until Ctrl+C. Drives need
  `/dev/fuse` and `fusermount3`, and Poros explains how to install the latter when it is missing.
- **Share links to files on cloud storage.** Share link..., in the menu of a file on S3 or Backblaze
  B2, or of a file or folder on Google Drive, OneDrive or Box, makes a link that opens it for people
  without an account. The dialog offers what the service allows (view, edit or download, who can
  open it, how long it works, a password) and says in one sentence what the link allows and until
  when, with Copy and Open in browser. S3 links are signed for up to seven days. B2 gives the plain
  address of a file in a public bucket, and in a private one a download authorization that also
  opens files whose names start with the file's name, as the dialog says. Drive, OneDrive and Box
  links last until removed, and the item's existing links are listed with Stop sharing beside each.
  Several items get one link each. S3 and B2 links cannot be taken back before they end, and shared
  drives on Google Drive are not listed. `poros-cli share` does the same from the command line, and
  the log never records a link or its password.
- **Older versions and deleted files on S3 and B2.** Versions..., in the menu of a file in an S3
  bucket that keeps versions or on Backblaze B2, lists every version and delete marker. Restore
  copies a version on top, so the current one stays in the history, Download saves it under its name
  with its date added, Open shows it in the editor, read only, Compare sets it beside the current
  file, and a version or a delete marker can be deleted for good after a question. Show deleted
  files, in a pane's menu inside a bucket, shows deleted files and folders in place, struck through,
  to restore or delete for good, a folder with everything deleted under it. `poros-cli versions` and
  `ls --deleted` do the same from the command line. Showing deleted files reads every version under
  the folder, which can be slow in buckets with long histories.
- **Speed limits by time of day.** Settings > Transfers > Speed limit schedule holds a list of
  rules, each for some weekdays between two times, with an upload and a download limit, Unlimited or
  Pause. Where two rules overlap the later one wins, and a week view shows what they add up to.
  While a rule applies, the status bar shows the limits and until when, and Ignore schedule, there
  or in the queue's toolbar, sets the schedule aside until its next change. Running transfers follow
  each change without restarting. Pause holds that direction's transfers as queued, with "Waiting
  for the speed limit schedule until 11:30" on their rows, and they continue from where they
  stopped. Folder sync, scheduled tasks and `poros-cli` keep to the schedule too, except for a
  direction given `--limit-up` or `--limit-down`.
- **Scheduled transfers.** Transfer files is a third kind of scheduled task, beside synchronizing
  and running a command: an upload, a download or a copy between two saved connections, on any
  protocol, of a list of files and folders. Each run can go into a folder of its own, such as
  `backup-{date}`, and the task says what to do with a file that is already there. Schedule this...
  and Schedule these... in the queue's menu, and Schedule upload... and Schedule download... in a
  pane's menu, fill a new task from transfers you ran or the files you selected. Runs go through the
  transfer queue, with its workers, speed limits, integrity checks and retries, and their transfers
  carry the task's name. Files inside a vault cannot be scheduled, since a task cannot unlock the
  vault.
- **Scheduled tasks from the command line.** `poros-cli schedule` lists the tasks saved in Poros,
  runs them by name, runs every task that is due (`run --due`, for cron or a systemd timer), keeps
  running them as they fall due (`daemon`), and turns them on and off. The app and the command line
  never run one task twice at once: a task still running is skipped, with exit status 13 on the
  command line. Each run is on record from its start, so a run stopped by a crash or a kill shows as
  failed rather than keeping the result of the run before. `poros-cli help schedule` covers cron,
  systemd and the keychain on machines without a screen. The daemon reads the settings once, when it
  starts.
- **More progress bar looks.** Settings > Appearance > Progress bars adds eight fills (candy
  stripes, chevrons, aurora, neon, wave, line, dots and blocks), five motions (scan, ripple, comet,
  sparkle and a breathing glow), four kinds of ends (round, soft, square and slanted, in place of
  the Rounded ends switch) and ten presets shown as live cards. Work of unknown length, such as an
  extraction, shows a bar that roams instead of an empty one, playing the chosen motion as it goes.
- **English reads a little differently.** Numbers in messages are grouped, as in "1,234 items", a
  unit after 1 is singular, as in "every 1 minute", and a label inside a sentence keeps the capitals
  of an abbreviation, as in "smaller than 1 MB" where 0.7.0 wrote "1 mb". A share link's end is one
  date in the region's format: "until May 14 at 18:30" on en-US and "until 14 May at 18:30" on
  en-GB.
- **Typed rsync rules keep their order.** Filter rules typed as rsync arguments, in Settings > Sync,
  saved with a site or typed for one run, are checked in the order given and the first match
  decides, as rsync does, before the dialog's Include and Exclude fields. So a site saved with
  `--exclude='*.log'` now leaves `keep.log` out even when Include names it, where 0.7.0 copied it,
  and with `--delete-excluded` deletes it from the target. This can change which files a saved task
  picks. Typed rules saved by 0.7.0 now show in the Filter rules field as `- x` and `+ x`, `-F`
  counts across the defaults, the site and the dialog as on one command line, and `a/**/b` needs at
  least one folder between `a` and `b`. `poros-cli sync` checks a connection's saved arguments
  before its `--include` and `--exclude` flags.
- **Server commands get the selected names as arguments.** The placeholders `{path}`, `{name}`,
  `{paths}` and `{names}` now become `"${1}"`, `"${2}"` and so on, and the names are sent apart from
  the command, so no shell reads a name as part of it. A command that uses selected names runs under
  `sh` when the account's login shell is not a POSIX shell, such as fish, csh, nushell, xonsh or
  PowerShell, so there it must be written in `sh` syntax: `for f in "$@"`, not `for f in $argv`.
  Commands without names still run in the login shell, and `{folder}` alone stands for `"$PWD"`,
  which nushell does not expand (write `$env.PWD` there). For a command Poros cannot read for sure,
  one with a here-document, `$'...'` or an unclosed quote, the dialog and the saved commands warn
  that placeholders stay one word only outside quotes: one inside double quotes is split into words
  and expanded as a pattern, but never run.
- **Operations on a server go through `sh`.** Delete, copy, move across disks, owner and group
  changes, extraction, checksums and rsync now ask the login shell for `sh -s` alone and send their
  work to it. Accounts limited to fixed commands, such as an `rrsync` forced command or `rssh`,
  therefore no longer get rsync: transfers to them send whole files over SFTP, and the log says so.
  A server without `sh`, such as Windows OpenSSH with cmd.exe or PowerShell, deletes, copies and
  changes permissions over SFTP, and server commands from the app, which run in the pane's folder,
  fail there with the shell's own error. Servers such as SFTPGo, which run a hashing program without
  a shell, keep whole-file checks for names made of plain characters, and other names are read back.
  Text a login script prints no longer fails rsync.
- **`poros-cli` exits differently in a few cases.** SIGTERM, as `systemctl stop` or a shutdown
  sends, now stops every command the way Ctrl+C does and exits 130 instead of 143. `mv` or `cp` of
  one item that cannot be placed exits with that error's own status, such as 8 for a rename the
  server refuses, instead of 1 with a list of failures. An empty path, as from an unset variable, no
  longer stands for the connection's folder: `rm`, `mv`, `cp`, `get`, `put` and the other commands
  that act on a path refuse it with exit 2. `rm` and `mv` refuse the home folder and the folder the
  connection starts in however they are spelled, and `rm` refuses a pattern at the top of the server
  such as `/*`, so `rm -rf "$UNSET/*"` touches nothing. `verify` exits 10 when part of a tree could
  not be read and names what it did not compare. `--conflict fail` never gives an item another name,
  replacing a file with itself exits 2, two sources with one name exit 9 before anything moves, and
  a source named twice is placed once. `--host-key` is refused with exit 2 on a connection whose
  server key Poros cannot check, such as plain FTP, B2, Google Drive, OneDrive or Box.
- **Deleting an S3 bucket in the app takes its history with it.** When the service refuses because
  older versions or delete markers remain, Poros removes them first, and the delete dialog says so
  beforehand: "Every older version and deleted file kept in the bucket goes too." `poros-cli rm`
  still refuses such a bucket unless given `--all-versions`.
- **Tabs with unsaved work ask before they close.** Closing a server tab of any kind, or locking a
  vault, while a file only it can save has unsaved changes now shows the unsaved-changes question,
  and Close other tabs asks about each tab in turn and stops at the first one you keep. Closing the
  last editor or terminal tab on a session with no file tab disconnects that session. Files dropped
  on a folder that cannot be listed to check for a vault are refused with a notice.
- **Security: server file names never reach a login shell that is not POSIX.** Poros quoted file
  names for a POSIX shell and put them on the SSH command line, which the server hands to the
  account's login shell. A login shell such as fish, csh, nushell, xonsh or PowerShell reads those
  quotes its own way, so a file someone else named on the server could break out of them and run a
  command as you when you deleted, copied, moved, extracted, hashed, synchronized or rsynced it.
  With fish, a file named `x\';touch poros-pwned;echo \'` ran `touch`. Now no name, path or pattern
  goes on that line, for every operation Poros runs on a server, the drive's upload checks and
  `poros-cli checksum` included.
- **Security: host keys and certificates are checked for the server that shows them.** Approving a
  new jump host's key also let that jump host answer for the server behind it with its own key,
  which Remember then saved as the server's. An approval now holds for the host and port it was
  asked about alone, and the same goes for FTPS, WebDAV and S3 certificates. A key marked `@revoked`
  in any known_hosts file is refused, and is never asked about or saved, where before the mark
  counted only above every line trusting the key in the same file. `poros-cli --host-key` now pins
  the server for the whole run: any other key or certificate is refused, even one the system trusts,
  so a stale pin for a renewed certificate exits 5 or 6.
- **Security: cloud names and server answers.** An S3 or B2 key such as `reports/../x`, or a Google
  Drive item named `..`, was listed as a folder whose delete or rename acted on the folder above it,
  and on S3 and B2 on every object in the bucket and the bucket itself. Names that are not file
  names are now left out of listings on every kind of server, and delete and rename refuse any path
  with a `.` or `..` part. A typed `https://`, `davs://` or `webdavs://` address is always reached
  over HTTPS, where with the type left on WebDAV (HTTP) it went in the clear, password included. A
  WebDAV or S3 server that sends an answer without end is cut off at 64 MiB instead of filling
  memory, and an S3 or B2 service that lists keys outside a folder can no longer make renaming or
  deleting that folder reach them.
- **Security: hostile key files, import files and FTP servers.** A private key file with extreme
  unlock settings could freeze Poros for minutes or make it try to allocate a petabyte. Key files
  are now capped at 1 MiB and their unlock costs checked before anything is derived, far above what
  PuTTYgen, ssh-keygen and openssl produce. Import files nested too deeply, OpenSSH `Include` lines
  that fan out to thousands of files, and files larger than the system says, such as `/dev/zero`,
  are capped too. An FTP server's passive reply is followed into this computer or the local network
  only when the server itself is there, so a server can no longer point a data connection at
  `127.0.0.1` or another machine on your network; on a private network whose server names another
  internal host, such as a load balancer, Poros connects to the server's own address and names the
  other one if that fails.
- **Fixed: interrupted transfers continue where they stopped.** The check of the part of an SFTP
  file already transferred hashed nothing, so every paused or reconnected transfer began again at
  the first byte. FTP servers that forget where to resume on any other command began again too,
  since Poros told them before opening the data connection rather than right before the transfer.
- **Fixed: transfers clean up after themselves.** The temporary file of a transfer that failed an
  integrity check is deleted when the transfer leaves the queue unfinished, and a download that
  fails its published checksum is no longer left under its real name. Removing several unfinished
  uploads deletes every one of their temporary files on the server, where only the first one or two
  went. A large upload to a slow server no longer fails at its final flush, which has its own limit
  of 10 minutes. Files download from folders that can be entered but not listed, and pausing a
  folder while it is listed pauses what it holds.
- **Fixed: the queue and its saved files.** A transfer could fail the moment it started with
  "Session is no longer open" when it was queued as the idle queue tidied up. Two saves of the
  queue, or of another file Poros keeps, made at the same moment no longer damage or roll back the
  saved file, and two host keys approved at once are both kept. Files queued by scheduled tasks
  start on their own, where with Start transfers automatically off they waited for a Start nobody
  pressed; transfers queued by hand still wait. A synchronization counts only the files it copied,
  raising or removing a speed limit takes effect at once, on Windows two names that differ only by a
  trailing dot or space no longer write one file at once, and SSH connections Poros closed on
  purpose are no longer reported as lost.
- **Fixed: FTP connections stay in step.** A connection that a failed or cancelled request left one
  reply behind could read that reply for the next request, showing a file as a folder or with the
  wrong size; it is now replaced. A server-to-server copy whose target never connects to the source
  is relayed through this computer instead of waiting for good, and the address the source names is
  kept for the target, as two servers in one private network need.
- **Fixed: large uploads to S3 and B2 survive a busy moment.** The start and the completion of an
  upload in parts were sent once, so a server error, a dropped connection or an error S3 reports
  inside a 200 answer failed the whole upload after its last part. Both are now retried like the
  parts, up to five times. A completion retried after its answer was lost is confirmed from the
  stored object's size, time and part count before it counts as done, and one that cannot be
  confirmed fails saying the file may already be there. Google Drive, OneDrive and Box upload
  sessions are opened again after a server error, and two sessions on one Box account no longer sign
  each other out about an hour later.
- **Fixed: lists, renames and dialogs while things change underneath.** A row being renamed no
  longer scrolls back into view or selects its name again on every reload, so typing is not mixed
  with the old name. Shift+click counts from the cursor when the row clicked before is gone, F2 and
  a slow click rename only the selected rows on show, and quick changes to a setting, such as
  Ctrl+wheel steps, apply in order. A dialog that finishes after it was dismissed closes only
  itself, never another one, and an import dialog closed during its scan discards what it read,
  passwords included.
- **Fixed: tabs, editors and terminals across reconnects.** After a reconnect, an upload could go to
  a hidden tab or into a locked vault, and files edited on FTP, WebDAV and cloud servers or in
  vaults could not be saved. Editors and terminals now follow their server to the new session, and
  save only through a session to the same account, route and host key, keeping the changes and
  saying so when there is none. A reconnect finished after its tab closed disconnects again, and a
  terminal resized while its shell opened takes the new size.
- **Fixed: sync, terminals, commands and vaults.** Remove source files deleted sources that had
  changed since the comparison or while rsync read them; they now stay, with a note. A backup folder
  inside the target is kept from deletion whatever the rules say. A terminal on the browsing
  connection ends its shell on the server when it closes, a vault rename cut off part way undoes
  itself instead of hiding the entry, and a command cut off by a lost connection reads "ended
  without an exit status" rather than finished.
- **Fixed: the command line keeps files on edge cases.** `mv` and `cp --conflict replace` keep the
  old file until the new one is in place, and moving a folder onto a file exits 9 and keeps the
  file. Two items placed with one name no longer take the same numbered name and replace each other.
  `rm -f` with a pattern that matches nothing skips only that pattern, and a folder cannot be copied
  into itself through a link.
- **Mount as a drive is Linux only in this release.** Windows says it is not available yet. On SFTP
  and FTP a drive saves a file by moving the upload over it, so the file's owner becomes the login
  user, hard links to it break, its ACLs are lost, the folder needs room for a second copy, and a
  file the login may not write is still replaced when its folder may be written. Requests that
  change files on a drive have no time limit, so none is ever made twice: a program waiting on a
  server that stops answering waits until it answers, the connection is found dead, or the drive is
  unmounted by force. Vault drives over SFTP and FTP still write in place, changing part of a large
  file fetches and uploads all of it, changes made elsewhere show on the drive within 10 seconds,
  links inside vaults cannot be opened, and mounts are not remembered.
- **Other limits.** Transfers held by the speed limit schedule when Poros closes come back Paused,
  like every unfinished transfer, and wait for Start. A scheduled run reloads only the panes opened
  through its saved connection, and a run that falls due while the computer sleeps runs once on
  wake. A copy of a vault file open in another program is unencrypted, and after a crash one holding
  a save not uploaded stays on this computer until you upload or discard it. A program that hands
  its file to a window already open and exits counts as closed, so if its first save comes after the
  time copies are kept, 10 minutes unless set otherwise, that save is not uploaded. A WebDAV folder
  of about 150,000 items or more, which arrives in one answer, now fails with "The server sent an
  answer larger than 64 MiB". Closing an SFTP upload waits up to 10 minutes for a slow server, so a
  connection that dies at that moment is noticed late. Backblaze B2, Google Drive, OneDrive and Box
  are built against their documentation and stub servers, without live accounts. In German, a label
  that is not a noun keeps its capital inside a sentence.
- **Going back to 0.7.0.** Settings > Updates > Install another version still offers it. 0.7.0 sets
  aside a `schedules.json` that holds a transfer task as not valid, keeping it as
  `schedules.json.corrupt-<time>` and starting with no scheduled tasks, so delete transfer tasks
  before going back to keep the others. It shows Slanted and Soft progress bar ends as Square and
  Round, and the new fills and motions as Solid and none. The first settings change it saves drops
  the settings new in 0.8.0, such as the language, the speed limit schedule and the programs under
  Open with, and a download of an older S3 or B2 version still in the queue fetches the current file
  instead.

## 0.7.0 - 2026-11-23

- **Encrypted vaults on any server.** A folder on a server can now hold a
  [Cryptomator](https://cryptomator.org) vault: its file names, folder names and contents are
  encrypted before they leave this computer, and only its password opens it. New vault, on a
  server pane's toolbar and in its right-click menu, makes one. Opening a vault's folder shows a
  card asking for the password where the files would be, and once unlocked the same pane lists
  the decrypted files, marked Encrypted, in a tab named after the vault. Lock on the toolbar
  forgets the keys again, as does closing the server's tab. The password is never saved.
- **The same vaults as the Cryptomator apps.** Poros reads and writes vault format 8, which
  Cryptomator 1.6 and later use on Windows, macOS, Linux, Android and iOS, with both of its
  ciphers, AES-GCM and the older AES-CTR with HMAC. A vault made in Poros opens there and a vault
  made there opens in Poros, long names, Unicode names and passwords included. The tests hold
  Poros against Cryptomator's own libraries in both directions.
- **Vaults in every transfer.** Uploads, downloads and copies between servers go through an
  unlocked vault like any other folder, each worker encrypting and decrypting on its own
  connection, so they run in parallel. The editor opens and saves files in a vault, and files
  can be made, renamed and deleted there. Every chunk of a file is authenticated as it is read,
  and with the integrity checks on, uploads into a vault on SFTP, FTP and WebDAV servers are
  read back, decrypted and compared with their source. Files dropped or pasted onto a locked
  vault's folder are refused, so nothing lands unencrypted among its files.
- **What vaults leave out.** An interrupted upload into a vault starts over rather than
  resuming, transfers for a vault are not kept across a restart, and symbolic links in a vault
  are listed but not followed. Folder sync, Properties, comparing, extracting on the server and
  the terminal work on a server's own files, and `poros-cli` does not open vaults. Vaults older
  than format 8 are upgraded by opening them once in Cryptomator.

## 0.6.5 - 2026-11-18

- **Tabs and windows come back after a restart.** Remember tabs and layout only brought back local
  tabs, at the folder each first opened in, and the main window's arrangement of panes. Now every
  window reopens as it was: server tabs reconnect and open at the folder they showed, terminals
  start a new shell on their server, editors reopen their files as last saved, and local panes
  open at the folder they were in. Windows torn out of the main one open again where they were,
  at the size they were, and the main window opens where it was, maximized if it was. A window
  closed by itself is not reopened; only the windows open when Poros closes are.
- **Reconnecting is shown in the tab.** A server tab says it is reconnecting until its server
  answers. A server that is down shows why, with Try again. One whose password is not kept asks
  you to sign in, with the connection filled in, and tabs that shared a session before, such as a
  server and its terminal, reconnect together. Settings > Interface > Reconnect to servers on start
  turns the automatic reconnection off, so reopened server tabs wait for Connect.
- **A folder that is gone opens the usual one instead.** A pane reopened at a folder deleted or
  unplugged since opens at its usual first folder rather than an error.

## 0.6.4 - 2026-11-17

- **Extract here does what Extract to a folder does.** In a local pane on Windows, Extract here
  could seem to do nothing. Both choices ran the same program, but Extract here unpacked straight
  into the archive's folder and quietly left out anything whose name was already there. An
  archive with the folder or files it holds already beside it, as there are after extracting it
  to a folder or when it was zipped from that folder, added nothing that could be seen, and what
  did come out was not selected, so it was lost among the folder's other files. Extract here now
  unpacks into a new folder beside the archive, as Extract to a folder does, then moves what came
  out next to the archive. Anything whose name is taken gets a numbered one, such as `site (2)`
  or `notes (2).txt`, so nothing is replaced or mixed into what is there. What came out is
  selected in every pane showing the folder, with its date group opened if it was collapsed.
  Servers work the same way, and a failed or cancelled extraction leaves nothing behind.

## 0.6.3 - 2026-11-17

- **Flying files and their rows arrive together.** With flying files on, a finished file's row
  showed in the pane it went to as soon as the pane reloaded, half a second or more before its
  icon got there. The row now stays hidden until the icon touches down in it, then fades in
  under it as the icon settles. Rows already on show, such as that of a file being replaced, are
  left as they are, and a row never stays hidden for more than a few seconds, should its file
  not fly. A row that moves while its file is on the way, as it does when files listed above it
  arrive or the list is scrolled or sorted, is followed, so the icon still lands on it.

## 0.6.2 - 2026-11-16

- **Files dragged out of the window and back show where they will land.** On Windows, files from
  a server dragged past the window's edge and back over Poros lost the pane's drop highlight and
  the name of what was being dragged beside the pointer, though letting go still dropped them
  where they belonged. Windows tells the page nothing of a drag back over it unless the drag
  carries files already on this computer, so Poros now follows the pointer itself: the pane or
  folder under it lights up, the dragged files and what a drop would do follow it again, holding
  Ctrl shows a copy, and the pointer is the ordinary one. The files also fly from where they were
  let go of rather than from where they left the window.

## 0.6.1 - 2026-11-15

- **Extract archives from the right-click menu.** Extract here, or Extract to a folder named after
  the archive, on ZIP, 7z, RAR, tar, tar.gz, tar.bz2, tar.xz, tar.zst, tar.lzma, gzip, bzip2, xz,
  zstd, LZMA, ISO and CAB files, one or several at once. Archives are recognized by their first
  bytes as well as their names. Nothing already there is replaced: a folder that is taken gets a
  numbered name, Extract here leaves existing files alone, and a single compressed file such as
  `dump.sql.gz` unpacks beside itself under a free name. Extraction shows in the operations tray
  and can be cancelled, and a damaged archive leaves no empty folder behind and says what the
  program said.
- **On a server, with the server's own programs.** Archives on an SFTP server are extracted on the
  server, with nothing downloaded. Poros looks once per connection for 7-Zip, unrar, unar,
  bsdtar, tar, unzip, gzip, bzip2, xz and zstd, and runs the one that opens each format best.
  Servers that allow SFTP alone run no programs, and neither do FTP and cloud servers; download
  an archive from one of those to extract it on this computer.
- **On this computer, with the program you choose.** Settings > Archives lists the extractors
  installed on Windows and Linux, found on the PATH and where 7-Zip, WinRAR and PeaZip install
  themselves, with the formats each opens. Leave it to pick the best one for each format, choose
  one to use for every format it opens, or point it at another program with its own arguments,
  where `{archive}` and `{folder}` stand for the archive and the folder it goes into.
- **Flying files fly again.** They stayed still for anyone whose system asks for less motion,
  which Windows does whenever its animation effects are off, so turning them on did nothing on
  many Windows installs. The switch in Settings > Appearance now decides, and the Play button
  beside it always plays.
- **Each file lands where it went.** Files fly one by one as the queue finishes each, out of the
  pane they came from and onto the row they land on, or onto the folder row they went into. When
  many finish at once, those headed for the same row go together as a pile with a count. Moves
  and copies within a server or this computer land on their rows too. Flights use the system's
  own file icons, at a size that stays sharp, and the app's icons where the system has none.

## 0.6.0 - 2026-11-12

- **Poros on the command line.** `poros-cli` browses, transfers, verifies and synchronizes files
  without a window, for scripts, CI jobs and AI agents: `ls`, `stat`, `cat`, `write`, `get`,
  `put`, `mkdir`, `rm`, `mv`, `cp`, `chmod`, `chown`, `du`, `checksum`, `verify`, `sync` and
  `exec`. It works with every protocol the app does, through the same transfer queue, parallel
  workers, integrity checks and rsync-style synchronization, and takes connections saved in
  Poros by name, or any address such as `sftp://user@host`. The README's
  [Command line](README.md#command-line) section has the details.
- **It never prompts.** Passwords, keys and passphrases come from flags or environment variables,
  saved connections, the system keychain or the SSH agent. A host key seen for the first time
  stops the run with its fingerprint until `--host-key` or `--trust-new-host` accepts it, and a
  changed key is never accepted without its fingerprint.
- **Made for programs.** `--json` prints one JSON document for every result and error, exit
  statuses say what happened (not found, sign-in refused, contents differ and so on), and `batch`
  runs many commands over one connection, one JSON line in and one out each. `exec` exits with
  the status of the command it ran on the server.
- **A download of its own for servers.** Each release now carries `poros-cli` for Linux and
  Windows on its own, built without the app, so it runs on servers and in containers that have
  no desktop. On Linux it needs nothing but the C library.

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
