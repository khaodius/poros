use std::path::PathBuf;

use clap::builder::FalseyValueParser;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

use super::exit;
use crate::settings::MAX_WORKERS;

const CONNECTION: &str = "Connection";
const SIGN_IN: &str = "Sign-in";
const HOST_KEYS: &str = "Host keys";
const OUTPUT: &str = "Output";
const TRANSFERS: &str = "Transfers";

const ABOUT: &str = "Poros without its window: browse, transfer, verify and synchronize files on \
SFTP, FTP, FTPS, WebDAV, S3, Backblaze B2 and cloud drives, from scripts and agents.";

const AFTER_HELP: &str = "\
Connections:
  -c takes a connection saved in Poros (its name or id), user@host[:port], or an address:
  sftp://, ftp://, ftps:// (implicit TLS), ftpes:// (explicit TLS), dav://, davs://,
  s3://ACCESS_KEY_ID@endpoint/bucket and b2://KEY_ID@. Paths without a leading / start in the
  connection's folder; ~ is the home folder.

Sign-in never prompts. Passwords come from --password-env, --password-file, --password-stdin,
POROS_PASSWORD or the system keychain (for saved connections); key passphrases likewise from
POROS_PASSPHRASE. Without any, SFTP tries the SSH agent and then ~/.ssh/id_ed25519, id_ecdsa
and id_rsa.

Scheduled tasks made in Poros run from here too: see poros-cli schedule --help for cron and
systemd timers.";

const SCHEDULE_HELP: &str = "\
Tasks are made and edited in Poros, under Scheduler; poros-cli lists them, runs them and turns
them on and off, on a computer where Poros is not open as well as beside it. A task never runs
twice at once: Poros, a daemon and any number of runs from cron may be alive together, and a
task one of them is running is skipped by the others. Results are written to the task, where
Poros shows them.

Run what is due from cron, every five minutes:
  */5 * * * * poros-cli schedule run --due --quiet

Or from a systemd timer, as a user service:
  ~/.config/systemd/user/poros-schedule.service
    [Service]
    Type=oneshot
    ExecStart=/usr/bin/poros-cli schedule run --due
  ~/.config/systemd/user/poros-schedule.timer
    [Timer]
    OnCalendar=*:0/5
    Persistent=true
    [Install]
    WantedBy=timers.target
  systemctl --user enable --now poros-schedule.timer

Or keep a daemon running, as a service of Type=simple with ExecStart=/usr/bin/poros-cli
schedule daemon. It notices tasks edited in Poros, a wake from sleep and a clock that was set.

Ctrl+C or SIGTERM, as systemctl stop and a shutdown send, stops a run or the daemon, and each
run it stops is written to the task as stopped. A run whose process is killed outright leaves
the same result.

A run more than --missed-after minutes late, because nothing was there to start it, counts as
missed: it happens once, late, when the task makes up for missed runs, and is passed over
otherwise.

Passwords come from the system keychain. Where there is none, as on a server without a desktop,
connections that sign in with a key or the SSH agent work as they are; for one that needs a
password, name it with --connection and give the password with POROS_PASSWORD, --password-env
or --password-file (and a key passphrase likewise). Nothing given is sent to another server.
--host-key likewise needs --connection and holds only that connection's server to the key;
--trust-new-host accepts the new keys of other servers and jump hosts, never in place of it.

Exit status: 0 when every task run succeeded, otherwise the status of the first task that did
not succeed: 13 for one already running elsewhere, 10 when some of its files failed, 1 for a
command that failed, 3 to 6 when it could not connect or sign in, 130 when it was stopped.";

#[derive(Debug, Parser)]
#[command(
    name = "poros-cli",
    version,
    about = ABOUT,
    after_help = AFTER_HELP,
    after_long_help = format!("{AFTER_HELP}\n\n{}", exit::TABLE),
    max_term_width = 100,
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOptions,
    #[command(subcommand)]
    pub command: Command,
}

/// One line of `batch`: a command and its own arguments, without the connection options.
#[derive(Debug, Parser)]
#[command(name = "poros-cli", no_binary_name = true, disable_version_flag = true)]
pub struct BatchLine {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Args)]
pub struct GlobalOptions {
    /// The server: a saved connection's name or id, user@host[:port], or an address such as
    /// sftp://user@host:2222/srv
    #[arg(short = 'c', long, env = "POROS_CONNECTION", global = true, help_heading = CONNECTION)]
    pub connection: Option<String>,

    /// Seconds to wait for the server to answer
    #[arg(long, value_name = "SECONDS", global = true, help_heading = CONNECTION)]
    pub timeout: Option<u64>,

    /// Tunnel through this saved connection, as OpenSSH's ProxyJump does
    #[arg(short = 'J', long, value_name = "CONNECTION", global = true, help_heading = CONNECTION)]
    pub jump: Option<String>,

    /// Connect directly, without the proxy from Poros's settings
    #[arg(long, global = true, help_heading = CONNECTION)]
    pub no_proxy: bool,

    /// FTP data connections come from the server (active mode)
    #[arg(long, global = true, help_heading = CONNECTION)]
    pub ftp_active: bool,

    /// S3 region; by default the endpoint's, or us-east-1
    #[arg(long, value_name = "REGION", global = true, help_heading = CONNECTION)]
    pub s3_region: Option<String>,

    /// Put the S3 bucket in the request path, as MinIO and most self-hosted servers expect
    #[arg(long, global = true, help_heading = CONNECTION)]
    pub s3_path_style: bool,

    /// Sign in as this user instead of the saved or given one
    #[arg(short = 'u', long, env = "POROS_USER", global = true, help_heading = SIGN_IN)]
    pub user: Option<String>,

    /// Read the password from this environment variable
    #[arg(long, value_name = "VARIABLE", global = true, help_heading = SIGN_IN)]
    pub password_env: Option<String>,

    /// Read the password from the first line of this file
    #[arg(long, value_name = "FILE", global = true, help_heading = SIGN_IN)]
    pub password_file: Option<PathBuf>,

    /// Read the password from the first line of standard input
    #[arg(long, global = true, help_heading = SIGN_IN)]
    pub password_stdin: bool,

    /// Sign in with this private key: OpenSSH, PEM, PKCS#8 or PuTTY .ppk
    #[arg(short = 'i', long, value_name = "FILE", env = "POROS_IDENTITY", global = true, help_heading = SIGN_IN)]
    pub identity: Option<PathBuf>,

    /// Read the key's passphrase from this environment variable
    #[arg(long, value_name = "VARIABLE", global = true, help_heading = SIGN_IN)]
    pub passphrase_env: Option<String>,

    /// Read the key's passphrase from the first line of this file
    #[arg(long, value_name = "FILE", global = true, help_heading = SIGN_IN)]
    pub passphrase_file: Option<PathBuf>,

    /// Sign in with the keys of the running SSH agent, or Pageant on Windows
    #[arg(long, global = true, help_heading = SIGN_IN)]
    pub agent: bool,

    /// Never read passwords saved in the system keychain
    #[arg(long, env = "POROS_NO_KEYCHAIN", value_parser = FalseyValueParser::new(), global = true, help_heading = SIGN_IN)]
    pub no_keychain: bool,

    /// Accept the server only with this host key or TLS certificate fingerprint (SHA256:...),
    /// even one that is not known or has changed
    #[arg(long, value_name = "FINGERPRINT", env = "POROS_HOST_KEY", global = true, help_heading = HOST_KEYS)]
    pub host_key: Option<String>,

    /// Accept a server whose key is not known yet. A changed key is still refused.
    #[arg(long, env = "POROS_TRUST_NEW_HOST", value_parser = FalseyValueParser::new(), global = true, help_heading = HOST_KEYS)]
    pub trust_new_host: bool,

    /// Remember the key accepted by --host-key or --trust-new-host, for Poros and later runs
    #[arg(long, global = true, help_heading = HOST_KEYS)]
    pub save_host_key: bool,

    /// Print the result as one JSON document on standard output, errors included
    #[arg(long, env = "POROS_JSON", value_parser = FalseyValueParser::new(), global = true, help_heading = OUTPUT)]
    pub json: bool,

    /// How to report progress on standard error
    #[arg(long, value_enum, default_value_t = ProgressMode::Auto, global = true, help_heading = OUTPUT)]
    pub progress: ProgressMode,

    /// Log more to standard error; repeat for more detail
    #[arg(short, long, action = ArgAction::Count, global = true, help_heading = OUTPUT)]
    pub verbose: u8,

    /// Log nothing but errors, and no progress
    #[arg(short, long, global = true, help_heading = OUTPUT)]
    pub quiet: bool,

    /// The folder with Poros's settings, saved connections and trusted host keys
    #[arg(long, value_name = "FOLDER", env = "POROS_CONFIG_DIR", global = true, help_heading = OUTPUT)]
    pub config_dir: Option<PathBuf>,

    /// Start from the built-in defaults instead of the transfer and connection settings saved in
    /// Poros
    #[arg(long, global = true, help_heading = OUTPUT)]
    pub default_settings: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProgressMode {
    /// A line that updates in place when standard error is a terminal, nothing otherwise
    Auto,
    None,
    /// Text lines, one every few seconds when standard error is not a terminal
    Text,
    /// One JSON object per line
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Connect, sign in, and show the session
    Info,
    /// List the connections saved in Poros
    Connections,
    /// List folders
    #[command(visible_alias = "list")]
    Ls(ListArgs),
    /// Show the size, time and permissions of paths
    Stat(PathsArgs),
    /// Print files to standard output
    Cat(CatArgs),
    /// Write standard input to a file on the server
    Write(WriteArgs),
    /// Download files and folders: get SOURCE... DESTINATION
    #[command(visible_alias = "download")]
    Get(TransferArgs),
    /// Upload files and folders: put SOURCE... DESTINATION
    #[command(visible_alias = "upload")]
    Put(TransferArgs),
    /// Create folders
    Mkdir(MakeDirArgs),
    /// Delete files and folders
    #[command(visible_alias = "delete")]
    Rm(RemoveArgs),
    /// Move or rename on the server: mv SOURCE... DESTINATION
    Mv(PlaceArgs),
    /// Copy on the server: cp SOURCE... DESTINATION
    Cp(PlaceArgs),
    /// Change permissions, as octal (644) or symbols (u+x,go-w)
    Chmod(ChmodArgs),
    /// Change the owner and group: OWNER, OWNER:GROUP or :GROUP
    Chown(ChownArgs),
    /// Count the files, folders and bytes under paths
    Du(PathsArgs),
    /// Search a folder and everything below it by name, size and date: find NAME [FOLDER]
    #[command(visible_alias = "search")]
    Find(FindArgs),
    /// Hash files on the server
    Checksum(ChecksumArgs),
    /// Check that a local and a server copy hold the same contents
    Verify(VerifyArgs),
    /// Make a local and a server folder match, the way rsync does
    Sync(SyncArgs),
    /// Make links that open files for people without an account, on S3, Backblaze B2,
    /// Google Drive, OneDrive and Box
    Share(ShareArgs),
    /// Run a command on the server through its shell, exiting with its status
    Exec(ExecArgs),
    /// Run commands read from standard input over one connection: one JSON array of
    /// arguments per line, or an object {"id", "args", "input"}, answered with one JSON line
    /// each
    Batch,
    /// List the scheduled tasks saved in Poros, run them now or when they are due, or keep
    /// running them as they fall due
    #[command(after_long_help = SCHEDULE_HELP)]
    Schedule(ScheduleArgs),
}

impl Command {
    /// Lists saved connections or reads the command stream, so needs no connection of its own.
    /// Scheduled tasks open the connections they are saved with.
    pub fn needs_connection(&self) -> bool {
        !matches!(self, Self::Connections | Self::Schedule(_))
    }
}

#[derive(Debug, Args)]
pub struct ScheduleArgs {
    #[command(subcommand)]
    pub command: ScheduleCommand,
}

#[derive(Debug, Subcommand)]
pub enum ScheduleCommand {
    /// List the tasks with their next and last run
    #[command(visible_alias = "ls")]
    List,
    /// Run tasks now, by name or id, or with --due every enabled task whose time has come
    Run(ScheduleRunArgs),
    /// Stay running and run tasks as they fall due, until interrupted
    Daemon,
    /// Turn tasks on, so they run on their schedule
    Enable(ScheduleTasksArgs),
    /// Turn tasks off
    Disable(ScheduleTasksArgs),
}

#[derive(Debug, Args)]
pub struct ScheduleRunArgs {
    /// The tasks, by name or id
    #[arg(
        value_name = "TASK",
        required_unless_present = "due",
        conflicts_with = "due"
    )]
    pub tasks: Vec<String>,
    /// Run every enabled task whose time has come, once, then exit: the mode for cron and
    /// systemd timers
    #[arg(long)]
    pub due: bool,
    /// With --due, a run later than this counts as missed rather than due
    #[arg(long, value_name = "MINUTES", default_value_t = 60)]
    pub missed_after: u32,
}

#[derive(Debug, Args)]
pub struct ScheduleTasksArgs {
    /// The tasks, by name or id
    #[arg(required = true, value_name = "TASK")]
    pub tasks: Vec<String>,
}

#[derive(Debug, Args)]
pub struct PathsArgs {
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// The folder; the connection's own by default
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,
    /// Show permissions, owner, size and time
    #[arg(short, long)]
    pub long: bool,
    /// Show hidden entries
    #[arg(short, long)]
    pub all: bool,
    /// Go into folders
    #[arg(short = 'R', long)]
    pub recursive: bool,
    /// How many levels a recursive listing goes down
    #[arg(long, value_name = "LEVELS", requires = "recursive")]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Args)]
pub struct FindArgs {
    /// What names have to look like: text found anywhere in a name, or a pattern with * and ?
    /// for the whole name. "" finds every name
    #[arg(value_name = "NAME")]
    pub name: String,
    /// Where to search; the connection's own folder by default
    #[arg(value_name = "FOLDER")]
    pub folder: Option<String>,
    /// NAME is a regular expression, found anywhere in a name unless anchored with ^ and $
    #[arg(short = 'E', long)]
    pub regex: bool,
    /// Tell upper case from lower case
    #[arg(short = 's', long)]
    pub case_sensitive: bool,
    /// Find only files or only folders
    #[arg(short = 't', long = "type", value_enum, value_name = "KIND")]
    pub kind: Option<FindKind>,
    /// At least this large: bytes, or with a unit as in 500K, 100M, 2G
    #[arg(long, value_name = "SIZE")]
    pub min_size: Option<String>,
    /// At most this large
    #[arg(long, value_name = "SIZE")]
    pub max_size: Option<String>,
    /// Changed at this time or later: 2026-11-01, 2026-11-01T08:30:00Z, or an age as in 30m,
    /// 12h, 7d
    #[arg(long, value_name = "WHEN")]
    pub newer: Option<String>,
    /// Changed at this time or earlier
    #[arg(long, value_name = "WHEN")]
    pub older: Option<String>,
    /// How many levels down to look; 1 is the folder alone
    #[arg(long, value_name = "LEVELS", value_parser = clap::value_parser!(u32).range(1..))]
    pub max_depth: Option<u32>,
    /// Search hidden folders too
    #[arg(short = 'a', long)]
    pub hidden: bool,
    /// Search the folders links lead to; a link back up is not entered twice
    #[arg(short = 'L', long)]
    pub follow: bool,
    /// Stop after this many results
    #[arg(long, value_name = "COUNT", default_value_t = crate::search::DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..=crate::search::MAX_LIMIT as i64))]
    pub limit: u32,
    /// Show permissions, owner, size and time
    #[arg(short, long)]
    pub long: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FindKind {
    #[value(alias = "f")]
    File,
    #[value(alias = "d", alias = "dir")]
    Folder,
}

#[derive(Debug, Args)]
pub struct CatArgs {
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    /// Start this many bytes into each file
    #[arg(long, value_name = "BYTES", default_value_t = 0)]
    pub offset: u64,
    /// Read at most this many bytes of each file
    #[arg(long, value_name = "BYTES")]
    pub length: Option<u64>,
}

#[derive(Debug, Args)]
pub struct WriteArgs {
    #[arg(value_name = "PATH")]
    pub path: String,
    /// Permissions for the file, in octal
    #[arg(long, value_name = "MODE")]
    pub mode: Option<String>,
    /// Refuse to replace a file that exists
    #[arg(short, long)]
    pub no_clobber: bool,
    /// Create the folders above the file when they are missing
    #[arg(short, long)]
    pub parents: bool,
}

#[derive(Debug, Args)]
pub struct TransferArgs {
    /// The files and folders to copy, then where to put them
    #[arg(required = true, num_args = 2.., value_name = "PATH")]
    pub paths: Vec<String>,
    /// Create the destination folder when it is missing
    #[arg(short, long)]
    pub parents: bool,
    #[command(flatten)]
    pub options: TransferOptions,
}

#[derive(Debug, Clone, Args)]
pub struct TransferOptions {
    /// Files moved at once, each on its own connection
    #[arg(short, long, value_name = "COUNT", value_parser = clap::value_parser!(u32).range(1..=MAX_WORKERS as i64), help_heading = TRANSFERS)]
    pub workers: Option<u32>,
    /// What to do with a file already at the destination
    #[arg(long, value_enum, default_value_t = Exists::Overwrite, help_heading = TRANSFERS)]
    pub exists: Exists,
    /// Every integrity check on, whatever the settings say: SHA-256 of both copies, reading
    /// back from servers that cannot hash, checked resumes, flushing, published checksums and
    /// signatures, and times and permissions read back
    #[arg(long, overrides_with = "no_verify", help_heading = TRANSFERS)]
    pub verify: bool,
    /// Check sizes only: no checksums, no reading back
    #[arg(long, help_heading = TRANSFERS)]
    pub no_verify: bool,
    /// Never read a file back to check it on servers that cannot hash it, which doubles the
    /// traffic with them
    #[arg(long, help_heading = TRANSFERS)]
    pub no_read_back: bool,
    /// Count a file done without waiting for it to be written to disk
    #[arg(long, help_heading = TRANSFERS)]
    pub no_flush: bool,
    /// Ignore checksum files and signatures published beside downloads
    #[arg(long, help_heading = TRANSFERS)]
    pub no_check_published: bool,
    /// A minisign public key that published .minisig signatures are checked against, besides
    /// the ones in the settings; may be repeated
    #[arg(long, value_name = "KEY", help_heading = TRANSFERS)]
    pub minisign_key: Vec<String>,
    /// Write straight to the destination name instead of a temporary one renamed at the end
    #[arg(long, help_heading = TRANSFERS)]
    pub no_temp_files: bool,
    /// Never split a large file between workers
    #[arg(long, help_heading = TRANSFERS)]
    pub no_segments: bool,
    /// Never send only the changed parts of a file through rsync
    #[arg(long, help_heading = TRANSFERS)]
    pub no_delta: bool,
    /// Upload speed limit in KiB per second
    #[arg(long, value_name = "KIB", help_heading = TRANSFERS)]
    pub limit_up: Option<u32>,
    /// Download speed limit in KiB per second
    #[arg(long, value_name = "KIB", help_heading = TRANSFERS)]
    pub limit_down: Option<u32>,
    /// Attempts after a failed file
    #[arg(long, value_name = "COUNT", help_heading = TRANSFERS)]
    pub retries: Option<u32>,
    /// Keep the permissions of each file
    #[arg(long, help_heading = TRANSFERS)]
    pub preserve_permissions: bool,
    /// Leave modification times as the copy sets them
    #[arg(long, help_heading = TRANSFERS)]
    pub no_preserve_times: bool,
    /// Run workers as channels of one connection, for servers that refuse more
    #[arg(long, help_heading = TRANSFERS)]
    pub single_connection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Exists {
    Overwrite,
    Skip,
    /// Replace only when the source is newer
    Newer,
    /// Replace only when size or time differ
    Different,
    /// Continue a shorter file from where it ends
    Resume,
    /// Keep both, numbering the new one
    Rename,
}

#[derive(Debug, Args)]
pub struct MakeDirArgs {
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    /// Create missing folders above, and accept folders that exist
    #[arg(short, long)]
    pub parents: bool,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    /// Delete folders with everything in them
    #[arg(short, long)]
    pub recursive: bool,
    /// Pass over paths that do not exist
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PlaceArgs {
    /// The items, then the folder or new name they go to
    #[arg(required = true, num_args = 2.., value_name = "PATH")]
    pub paths: Vec<String>,
    /// What to do when the destination already has an item of the same name
    #[arg(long, value_enum, default_value_t = Conflict::Fail)]
    pub conflict: Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Conflict {
    /// Stop without changing anything
    Fail,
    /// Replace files and merge folders
    Replace,
    Skip,
    /// Give the new item a free name such as "name (2).txt"
    KeepBoth,
}

#[derive(Debug, Args)]
pub struct ChmodArgs {
    #[arg(value_name = "MODE")]
    pub mode: String,
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    /// Also change everything inside folders
    #[arg(short = 'R', long)]
    pub recursive: bool,
}

#[derive(Debug, Args)]
pub struct ChownArgs {
    #[arg(value_name = "OWNER[:GROUP]")]
    pub owner: String,
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    /// Also change everything inside folders
    #[arg(short = 'R', long)]
    pub recursive: bool,
}

#[derive(Debug, Args)]
pub struct ChecksumArgs {
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    #[arg(short, long, value_enum, default_value_t = HashAlgorithm::Sha256)]
    pub algorithm: HashAlgorithm,
    /// Hash the files as they download instead of with the server's command
    #[arg(long)]
    pub stream: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
    Sha1,
    Md5,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    /// A local file or folder
    #[arg(value_name = "LOCAL")]
    pub local: String,
    /// The server copy of the same file or folder
    #[arg(value_name = "REMOTE")]
    pub remote: String,
    /// Leave out paths that match, as rsync's --exclude
    #[arg(long, value_name = "PATTERN")]
    pub exclude: Vec<String>,
    /// More rsync options that decide what is compared, such as "--exclude-from=list"
    #[arg(long, value_name = "OPTIONS", allow_hyphen_values = true)]
    pub rsync: Option<String>,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// The folder on this computer; it comes first whatever the direction
    #[arg(value_name = "LOCAL")]
    pub local: String,
    /// The folder on the server
    #[arg(value_name = "REMOTE")]
    pub remote: String,
    /// up makes the server folder match the local one, down the other way round; both copies
    /// new files each way and lets the newer copy win
    #[arg(short, long, value_enum)]
    pub direction: Direction,
    /// Delete what the source does not have
    #[arg(long)]
    pub delete: bool,
    /// Leave out paths that match
    #[arg(long, value_name = "PATTERN")]
    pub exclude: Vec<String>,
    /// Keep paths that match, even when an --exclude would leave them out. Rules in the
    /// connection's saved rsync arguments are checked first, as rsync checks rules in order
    #[arg(long, value_name = "PATTERN")]
    pub include: Vec<String>,
    /// Compare contents instead of size and time
    #[arg(long)]
    pub checksum: bool,
    /// Compare sizes only
    #[arg(long)]
    pub size_only: bool,
    /// Show what would change without changing anything
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    /// What to do with files changed on both sides (with --direction both)
    #[arg(long, value_enum, default_value_t = SyncConflict::Skip)]
    pub conflict: SyncConflict,
    /// Delete even when the source folder is empty, which usually means the wrong folder
    #[arg(long)]
    pub allow_empty_source: bool,
    /// List every change in the text output, as rsync's --itemize-changes
    #[arg(long)]
    pub itemize: bool,
    /// rsync options, such as "-a --delete-after --exclude=*.tmp"
    #[arg(long, value_name = "OPTIONS", allow_hyphen_values = true)]
    pub rsync: Option<String>,
    /// rsync options after --, taken as rsync takes them
    #[arg(last = true, value_name = "RSYNC OPTIONS")]
    pub rsync_arguments: Vec<String>,
    #[command(flatten)]
    pub options: TransferOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Direction {
    Up,
    Down,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SyncConflict {
    /// Leave them and report them
    Skip,
    /// Upload the local copy
    Local,
    /// Download the server copy
    Remote,
}

#[derive(Debug, Args)]
pub struct ShareArgs {
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
    /// How long the link works: seconds, or a number with s, m, h or d, such as 90m or 7d.
    /// S3 and Backblaze B2 links always end, after 1h unless told otherwise and within 7d
    #[arg(short, long, value_name = "LIFETIME", value_parser = parse_lifetime)]
    pub expires: Option<u64>,
    /// What the link lets its holder do
    #[arg(short, long, value_enum, default_value_t = ShareKind::View)]
    pub kind: ShareKind,
    /// Who the link works for, where the service has a choice
    #[arg(long, value_enum, default_value_t = ShareAudience::Anyone)]
    pub audience: ShareAudience,
    /// Remove the links the paths have instead of making one, where the service keeps them
    #[arg(long, conflicts_with_all = ["expires", "kind", "audience"])]
    pub revoke: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ShareKind {
    /// Open the item in the browser
    View,
    /// Let the holder change the item
    Edit,
    /// Save the file under its name
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ShareAudience {
    /// Whoever holds the link
    Anyone,
    /// People signed in to the account's organization
    Organization,
    /// People the item is already shared with
    Invited,
}

/// A lifetime in seconds: `3600`, `90s`, `45m`, `12h` or `7d`.
fn parse_lifetime(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let (digits, unit) = match text.find(|character: char| !character.is_ascii_digit()) {
        Some(split) => text.split_at(split),
        None => (text, "s"),
    };
    let seconds_per_unit: u64 = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => {
            return Err(format!(
                "{text} is not a lifetime such as 3600, 90m, 12h or 7d"
            ))
        }
    };
    digits
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(seconds_per_unit))
        .ok_or_else(|| format!("{text} is not a lifetime such as 3600, 90m, 12h or 7d"))
}

#[derive(Debug, Args)]
pub struct ExecArgs {
    /// Run in this folder on the server
    #[arg(long, value_name = "FOLDER")]
    pub cwd: Option<String>,
    /// The command and its arguments, joined with spaces as ssh does
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "COMMAND"
    )]
    pub command: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn arguments_are_consistent() {
        Cli::command().debug_assert();
        BatchLine::command().debug_assert();
    }

    #[test]
    fn share_reads_lifetimes_and_kinds() {
        assert_eq!(parse_lifetime("3600"), Ok(3600));
        assert_eq!(parse_lifetime("90s"), Ok(90));
        assert_eq!(parse_lifetime("45m"), Ok(2700));
        assert_eq!(parse_lifetime("1h"), Ok(3600));
        assert_eq!(parse_lifetime("7d"), Ok(604_800));
        assert_eq!(parse_lifetime("0"), Ok(0));
        for refused in ["", "h", "1w", "-5m", "1.5h", "1 h", "99999999999999999999d"] {
            assert!(parse_lifetime(refused).is_err(), "{refused}");
        }
        let cli = Cli::try_parse_from([
            "poros-cli",
            "share",
            "/bucket/a.txt",
            "b.txt",
            "--expires",
            "2h",
            "--kind",
            "download",
            "-c",
            "files",
        ])
        .unwrap();
        let Command::Share(share) = cli.command else {
            panic!("not share");
        };
        assert_eq!(share.paths, ["/bucket/a.txt", "b.txt"]);
        assert_eq!(share.expires, Some(7200));
        assert_eq!(share.kind, ShareKind::Download);
        assert_eq!(share.audience, ShareAudience::Anyone);
        assert!(!share.revoke);
        let defaults = Cli::try_parse_from(["poros-cli", "share", "a.txt"]).unwrap();
        let Command::Share(share) = defaults.command else {
            panic!("not share");
        };
        assert_eq!((share.expires, share.kind), (None, ShareKind::View));
        assert!(Cli::try_parse_from(["poros-cli", "share"]).is_err());
        assert!(Cli::try_parse_from([
            "poros-cli",
            "share",
            "a.txt",
            "--revoke",
            "--expires",
            "1h"
        ])
        .is_err());
    }

    #[test]
    fn connection_options_go_anywhere_on_the_line() {
        let cli = Cli::try_parse_from([
            "poros-cli",
            "ls",
            "-l",
            "/srv",
            "-c",
            "prod",
            "--json",
            "--trust-new-host",
        ])
        .unwrap();
        assert_eq!(cli.global.connection.as_deref(), Some("prod"));
        assert!(cli.global.json && cli.global.trust_new_host);
        let Command::Ls(list) = cli.command else {
            panic!("not ls");
        };
        assert!(list.long);
        assert_eq!(list.paths, ["/srv"]);
    }

    #[test]
    fn sync_takes_rsync_options_after_two_dashes() {
        let cli = Cli::try_parse_from([
            "poros-cli",
            "sync",
            "-d",
            "up",
            "site",
            "/var/www",
            "-w",
            "8",
            "--",
            "-a",
            "--exclude=*.tmp",
        ])
        .unwrap();
        let Command::Sync(sync) = cli.command else {
            panic!("not sync");
        };
        assert_eq!(sync.direction, Direction::Up);
        assert_eq!(sync.options.workers, Some(8));
        assert_eq!(sync.rsync_arguments, ["-a", "--exclude=*.tmp"]);
    }

    #[test]
    fn exec_keeps_the_command_options_for_the_command() {
        let cli = Cli::try_parse_from(["poros-cli", "exec", "ls", "-la", "/tmp"]).unwrap();
        let Command::Exec(exec) = cli.command else {
            panic!("not exec");
        };
        assert_eq!(exec.command, ["ls", "-la", "/tmp"]);
    }

    #[test]
    fn workers_are_limited() {
        assert!(Cli::try_parse_from(["poros-cli", "get", "-w", "99", "a", "b"]).is_err());
        assert!(Cli::try_parse_from(["poros-cli", "get", "a"]).is_err());
    }

    #[test]
    fn schedule_runs_named_tasks_or_the_due_ones() {
        let cli =
            Cli::try_parse_from(["poros-cli", "schedule", "run", "Nightly", "abc-123"]).unwrap();
        let Command::Schedule(ScheduleArgs {
            command: ScheduleCommand::Run(run),
        }) = cli.command
        else {
            panic!("not schedule run");
        };
        assert_eq!(run.tasks, ["Nightly", "abc-123"]);
        assert!(!run.due);
        assert_eq!(run.missed_after, 60);

        let due = Cli::try_parse_from([
            "poros-cli",
            "schedule",
            "run",
            "--due",
            "--missed-after",
            "20",
        ]);
        assert!(due.is_ok());
        assert!(Cli::try_parse_from(["poros-cli", "schedule", "run"]).is_err());
        assert!(Cli::try_parse_from(["poros-cli", "schedule", "run", "--due", "Nightly"]).is_err());
        assert!(Cli::try_parse_from(["poros-cli", "schedule", "enable"]).is_err());
        assert!(!Cli::try_parse_from(["poros-cli", "schedule", "daemon"])
            .unwrap()
            .command
            .needs_connection());
    }

    #[test]
    fn batch_lines_take_no_connection_options() {
        assert!(BatchLine::try_parse_from(["ls", "/srv"]).is_ok());
        assert!(BatchLine::try_parse_from(["ls", "-c", "prod"]).is_err());
    }
}
