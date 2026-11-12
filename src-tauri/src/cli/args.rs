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
and id_rsa.";

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
    /// Hash files on the server
    Checksum(ChecksumArgs),
    /// Check that a local and a server copy hold the same contents
    Verify(VerifyArgs),
    /// Make a local and a server folder match, the way rsync does
    Sync(SyncArgs),
    /// Run a command on the server through its shell, exiting with its status
    Exec(ExecArgs),
    /// Run commands read from standard input over one connection: one JSON array of
    /// arguments per line, or an object {"id", "args", "input"}, answered with one JSON line
    /// each
    Batch,
}

impl Command {
    /// Lists saved connections or reads the command stream, so needs no connection of its own.
    pub fn needs_connection(&self) -> bool {
        !matches!(self, Self::Connections)
    }
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
    /// Keep paths that match, even when an exclude would leave them out
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
    fn batch_lines_take_no_connection_options() {
        assert!(BatchLine::try_parse_from(["ls", "/srv"]).is_ok());
        assert!(BatchLine::try_parse_from(["ls", "-c", "prod"]).is_err());
    }
}
