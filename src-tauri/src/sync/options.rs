//! rsync's options as a synchronization carries them out, and rsync's command-line syntax for
//! them: extra arguments typed as rsync takes them are read into the same options, and the
//! options are shown back as the rsync command they amount to.
//!
//! Options that move data, decide what is compared, copied or deleted, or set attributes are
//! carried out. Ones about rsync's own process or connection are accepted and noted. A few
//! cannot work over SFTP and are refused, so a sync never quietly does less than asked.

use serde::{Deserialize, Serialize};

use super::chmod::Chmod;
use super::filter::{Filter, FilterSpec};
use super::plan::{CompareMode, SyncDirection};
use crate::error::AppResult;

pub const DEFAULT_TIME_TOLERANCE_SECS: u32 = 2;
pub const MAX_TIME_TOLERANCE_SECS: u32 = 24 * 60 * 60;
/// rsync's own limit for protocol 30 and later.
pub const MAX_BLOCK_SIZE: u64 = 128 * 1024;
/// What `--backup` adds to names when no backup folder is given.
pub const DEFAULT_BACKUP_SUFFIX: &str = "~";
const MAX_TEXT_CHARS: usize = 4096;

/// When files the source does not have are deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeleteTiming {
    Before,
    /// rsync's default. Folders are compared as a whole, so this deletes before copying.
    #[default]
    During,
    /// Found while comparing, deleted once copying is done.
    Delay,
    After,
}

impl DeleteTiming {
    pub fn after_copying(self) -> bool {
        matches!(self, Self::Delay | Self::After)
    }
}

/// What happens to symbolic links on the sending side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkMode {
    /// Copies what a link points to (`-L`).
    #[default]
    Follow,
    /// Recreates the link itself (`-l`).
    Preserve,
    /// Leaves links out, as rsync does with neither.
    Skip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppendMode {
    #[default]
    Off,
    /// Sends only what a shorter target lacks, trusting the part it has (`--append`).
    Append,
    /// The same, then checks the whole file and copies it again if it differs.
    Verify,
}

/// Reading `.rsync-filter` files in each folder (`-F`, and `-FF` to leave them out too).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterFiles {
    #[default]
    Off,
    Read,
    ReadAndExclude,
}

/// Everything a synchronization can be told, grouped as rsync's manual groups it. Field names
/// before `existing_only` are the ones synchronizations were saved with before these options,
/// so saved tasks and requests read as they did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SyncOptions {
    pub compare: CompareMode,
    /// `--modify-window`.
    pub time_tolerance_secs: u32,
    /// `-u`.
    pub skip_newer_on_target: bool,
    pub ignore_existing: bool,
    /// `--delete`.
    pub delete_extraneous: bool,
    pub excludes: Vec<String>,

    /// `--existing`: updates files the target has, creating nothing new.
    pub existing_only: bool,
    /// Bytes.
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,

    pub recursive: bool,
    /// `-d`: without recursion, still creates the folders at the top.
    pub dirs: bool,
    pub mkpath: bool,
    pub prune_empty_dirs: bool,
    pub whole_file: bool,
    pub inplace: bool,
    pub append: AppendMode,
    /// Bytes per rsync block.
    pub block_size: Option<u64>,
    pub remove_source_files: bool,
    pub fsync: bool,
    /// The command that starts rsync on the server; empty uses the site's or the default.
    pub rsync_path: String,

    pub delete_timing: DeleteTiming,
    pub delete_excluded: bool,
    pub max_delete: Option<u64>,
    pub force: bool,
    pub ignore_errors: bool,

    /// Rules in rsync's filter syntax (`--filter`).
    pub filter_rules: Vec<String>,
    pub includes: Vec<String>,
    /// Files on this computer, relative ones to the local folder.
    pub include_from: Vec<String>,
    pub exclude_from: Vec<String>,
    pub files_from: String,
    pub from0: bool,
    pub filter_files: FilterFiles,
    pub cvs_exclude: bool,

    pub times: bool,
    pub omit_dir_times: bool,
    pub perms: bool,
    pub executability: bool,
    pub chmod: String,
    pub owner: bool,
    pub group: bool,
    /// `USER:GROUP`, either part optional.
    pub chown: String,
    pub numeric_ids: bool,

    pub links: LinkMode,
    pub safe_links: bool,
    pub copy_unsafe_links: bool,
    pub copy_dirlinks: bool,
    pub keep_dirlinks: bool,

    /// KiB per second; zero is unlimited.
    pub bwlimit_kib: u64,
    pub compress: bool,

    pub backup: bool,
    pub backup_dir: String,
    /// Empty means rsync's default: `~` without a backup folder, nothing with one.
    pub backup_suffix: String,

    pub dry_run: bool,
    pub itemize: bool,
    pub verbose: bool,
    pub stats: bool,
    /// A file on this computer each run is written to.
    pub log_file: String,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            compare: CompareMode::SizeAndTime,
            time_tolerance_secs: DEFAULT_TIME_TOLERANCE_SECS,
            skip_newer_on_target: false,
            ignore_existing: false,
            delete_extraneous: false,
            excludes: Vec::new(),
            existing_only: false,
            min_size: None,
            max_size: None,
            recursive: true,
            dirs: false,
            mkpath: false,
            prune_empty_dirs: false,
            whole_file: false,
            inplace: false,
            append: AppendMode::Off,
            block_size: None,
            remove_source_files: false,
            fsync: false,
            rsync_path: String::new(),
            delete_timing: DeleteTiming::During,
            delete_excluded: false,
            max_delete: None,
            force: false,
            ignore_errors: false,
            filter_rules: Vec::new(),
            includes: Vec::new(),
            include_from: Vec::new(),
            exclude_from: Vec::new(),
            files_from: String::new(),
            from0: false,
            filter_files: FilterFiles::Off,
            cvs_exclude: false,
            times: true,
            omit_dir_times: false,
            perms: false,
            executability: false,
            chmod: String::new(),
            owner: false,
            group: false,
            chown: String::new(),
            numeric_ids: false,
            links: LinkMode::Follow,
            safe_links: false,
            copy_unsafe_links: false,
            copy_dirlinks: false,
            keep_dirlinks: false,
            bwlimit_kib: 0,
            compress: false,
            backup: false,
            backup_dir: String::new(),
            backup_suffix: String::new(),
            dry_run: false,
            itemize: false,
            verbose: false,
            stats: false,
            log_file: String::new(),
        }
    }
}

impl SyncOptions {
    /// What `--backup` adds to names.
    pub fn backup_suffix(&self) -> &str {
        if !self.backup_suffix.is_empty() {
            &self.backup_suffix
        } else if self.backup_dir.is_empty() {
            DEFAULT_BACKUP_SUFFIX
        } else {
            ""
        }
    }

    /// Deletes extraneous files, which a two-way synchronization never does.
    pub fn deletes(&self, direction: SyncDirection) -> bool {
        (self.delete_extraneous || self.delete_excluded) && direction != SyncDirection::Both
    }

    pub fn time_tolerance(&self) -> i64 {
        i64::from(self.time_tolerance_secs.min(MAX_TIME_TOLERANCE_SECS))
    }

    pub fn chmod_rules(&self) -> Chmod {
        Chmod::parse(&self.chmod).unwrap_or_default()
    }

    /// The filter rules, reading `merge` files and the include, exclude and files-from lists
    /// with `read_file`.
    pub fn filter_spec(
        &self,
        read_file: &dyn Fn(&str) -> AppResult<String>,
    ) -> AppResult<FilterSpec> {
        let read_lists = |paths: &[String]| -> AppResult<Vec<String>> {
            paths.iter().map(|path| read_file(path)).collect()
        };
        let files_from = if self.files_from.trim().is_empty() {
            None
        } else {
            let contents = read_file(self.files_from.trim())?;
            let separator = if self.from0 { '\0' } else { '\n' };
            Some(contents.split(separator).map(str::to_string).collect())
        };
        Ok(FilterSpec {
            rules: self.filter_rules.clone(),
            includes: self.includes.clone(),
            include_lists: read_lists(&self.include_from)?,
            excludes: self.excludes.clone(),
            exclude_lists: read_lists(&self.exclude_from)?,
            files_from,
            recursive: self.recursive,
            per_folder_filters: self.filter_files != FilterFiles::Off,
            exclude_filter_files: self.filter_files == FilterFiles::ReadAndExclude,
            cvs_exclude: self.cvs_exclude,
            cvs_ignore: std::env::var("CVSIGNORE").unwrap_or_default(),
            delete_excluded: self.delete_excluded,
        })
    }
}

/// Options checked, with what was noted about them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionCheck {
    pub options: SyncOptions,
    /// The options as rsync arguments.
    pub arguments: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Default)]
struct Notes {
    warnings: Vec<String>,
    errors: Vec<String>,
}

/// Applies each layer of rsync arguments over `options` in turn, then checks the result.
pub fn check(options: SyncOptions, layers: &[&str], direction: SyncDirection) -> OptionCheck {
    let mut options = options;
    let mut notes = Notes::default();
    for layer in layers {
        apply_arguments(&mut options, layer, &mut notes);
    }
    validate(&options, direction, &mut notes);
    OptionCheck {
        arguments: command_line(&render(&options)),
        options,
        warnings: notes.warnings,
        errors: notes.errors,
    }
}

fn validate(options: &SyncOptions, direction: SyncDirection, notes: &mut Notes) {
    let texts = [
        ("--chmod", &options.chmod),
        ("--chown", &options.chown),
        ("--rsync-path", &options.rsync_path),
        ("--files-from", &options.files_from),
        ("--backup-dir", &options.backup_dir),
        ("--suffix", &options.backup_suffix),
        ("--log-file", &options.log_file),
    ];
    for (name, text) in texts {
        if text.chars().count() > MAX_TEXT_CHARS {
            notes
                .errors
                .push(format!("{name}: at most {MAX_TEXT_CHARS} characters"));
        }
    }
    if let Err(error) = Chmod::parse(&options.chmod) {
        notes.errors.push(format!("--chmod: {}", error.message));
    }
    if let Err(message) = parse_chown(&options.chown) {
        notes.errors.push(format!("--chown: {message}"));
    }
    if options.time_tolerance_secs > MAX_TIME_TOLERANCE_SECS {
        notes.errors.push(format!(
            "--modify-window: at most {MAX_TIME_TOLERANCE_SECS} seconds"
        ));
    }
    if options
        .block_size
        .is_some_and(|size| size == 0 || size > MAX_BLOCK_SIZE)
    {
        notes.errors.push(format!(
            "--block-size: between 1 and {MAX_BLOCK_SIZE} bytes"
        ));
    }
    if options.rsync_path.contains(['\n', '\r']) {
        notes
            .errors
            .push("--rsync-path: the command must be on one line".into());
    }
    let spec = FilterSpec {
        rules: options.filter_rules.clone(),
        includes: options.includes.clone(),
        excludes: options.excludes.clone(),
        ..FilterSpec::default()
    };
    // Merge files are read when comparing; here only the rules themselves are checked.
    if let Err(error) = Filter::build(&spec, &|_| Ok(String::new())) {
        notes.errors.push(error.message);
    }
    if !options.log_file.is_empty() && !std::path::Path::new(&options.log_file).is_absolute() {
        notes
            .errors
            .push("--log-file: give the file's full path".into());
    }
    if direction == SyncDirection::Both {
        if options.remove_source_files {
            notes
                .errors
                .push("--remove-source-files: a two-way synchronization has no one source".into());
        }
        if options.delete_extraneous || options.delete_excluded {
            notes
                .warnings
                .push("Deleting is off when synchronizing both ways".into());
        }
    }
    if (options.delete_extraneous || options.delete_excluded) && !options.recursive && !options.dirs
    {
        notes
            .errors
            .push("--delete does not work without -r or -d".into());
    }
    if options.backup && options.inplace {
        notes.warnings.push(
            "--inplace with --backup writes a temporary file, so the old version can be kept"
                .into(),
        );
    }
    if cfg!(windows)
        && direction == SyncDirection::Download
        && (options.owner || options.group || !options.chown.trim().is_empty())
    {
        notes
            .warnings
            .push("Windows files have no Unix owner or group to set".into());
    }
    if options.existing_only && options.ignore_existing {
        notes
            .warnings
            .push("--existing with --ignore-existing copies nothing; only deletions happen".into());
    }
}

/// `USER:GROUP` with either part optional, as names or numbers.
pub fn parse_chown(text: &str) -> Result<(Option<String>, Option<String>), String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok((None, None));
    }
    let (user, group) = match text.split_once(':') {
        Some((user, group)) => (user, group),
        None => (text, ""),
    };
    let check = |part: &str| -> Result<Option<String>, String> {
        if part.is_empty() {
            return Ok(None);
        }
        let valid = part.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '$')
        }) && !part.starts_with('-');
        if valid {
            Ok(Some(part.to_string()))
        } else {
            Err(format!("\"{part}\" is not a user or group name or number"))
        }
    };
    let parsed = (check(user)?, check(group)?);
    if parsed == (None, None) {
        return Err("give a user, a group or both, as USER:GROUP".into());
    }
    Ok(parsed)
}

/// A size with rsync's suffixes: `K`, `M`, `G`, `T` and `P` count in 1024s (`KiB` too),
/// `KB` and the like in 1000s, and a trailing `+1` or `-1` adjusts by one byte.
pub fn parse_size(text: &str, default_unit: char) -> Result<u64, String> {
    let text = text.trim();
    let number_end = text
        .find(|character: char| !(character.is_ascii_digit() || character == '.'))
        .unwrap_or(text.len());
    let (number, mut rest) = text.split_at(number_end);
    let number: f64 = number
        .parse()
        .map_err(|_| format!("\"{text}\" is not a size"))?;
    let mut adjust = 0i64;
    if let Some(stripped) = rest.strip_suffix("+1") {
        adjust = 1;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_suffix("-1") {
        adjust = -1;
        rest = stripped;
    }
    let mut units = rest.chars();
    let unit = units.next().unwrap_or(default_unit).to_ascii_lowercase();
    let base: f64 = match units.as_str().to_ascii_lowercase().as_str() {
        "" | "ib" => 1024.0,
        "b" if unit != 'b' => 1000.0,
        _ => return Err(format!("\"{text}\" has an unknown unit")),
    };
    let power = match unit {
        'b' => 0,
        'k' => 1,
        'm' => 2,
        'g' => 3,
        't' => 4,
        'p' => 5,
        _ => return Err(format!("\"{text}\" has an unknown unit")),
    };
    let bytes = number * base.powi(power);
    if !bytes.is_finite() || bytes >= u64::MAX as f64 {
        return Err(format!("\"{text}\" is too large"));
    }
    Ok((bytes as i64 + adjust).max(0) as u64)
}

type SwitchFn = fn(&mut SyncOptions, bool);
type SetterFn = fn(&mut SyncOptions, &str) -> Result<(), String>;

enum Spec {
    /// On or off; `--no-NAME` turns it off.
    Switch(SwitchFn),
    Value(SetterFn),
    /// Accepted; it changes nothing worth a word, such as rsync's own output.
    Quiet {
        takes_value: bool,
    },
    /// Accepted, though Poros does not do what it asks.
    Ignored {
        takes_value: bool,
        reason: &'static str,
    },
    /// Refused: a synchronization would do something else than asked.
    Refused {
        takes_value: bool,
        reason: &'static str,
    },
}

const NOT_OVER_SFTP: &str = "not carried over SFTP";
const OWN_CONNECTION: &str = "Poros connects with the site's own SSH settings";

fn long_option(name: &str) -> Option<Spec> {
    use Spec::{Ignored, Quiet, Refused, Switch, Value};
    let ignored = |reason| Ignored {
        takes_value: false,
        reason,
    };
    let ignored_value = |reason| Ignored {
        takes_value: true,
        reason,
    };
    let refused = |reason| Refused {
        takes_value: false,
        reason,
    };
    let refused_value = |reason| Refused {
        takes_value: true,
        reason,
    };
    Some(match name {
        // Transfer
        "archive" => Switch(|options, on| {
            if on {
                options.recursive = true;
                options.links = LinkMode::Preserve;
                options.perms = true;
                options.times = true;
                options.group = true;
                options.owner = true;
            }
        }),
        "recursive" => Switch(|options, on| options.recursive = on),
        "dirs" => Switch(|options, on| options.dirs = on),
        "mkpath" => Switch(|options, on| options.mkpath = on),
        "prune-empty-dirs" => Switch(|options, on| options.prune_empty_dirs = on),
        "whole-file" => Switch(|options, on| options.whole_file = on),
        "inplace" => Switch(|options, on| options.inplace = on),
        "append" => Switch(|options, on| {
            options.append = if on {
                AppendMode::Append
            } else {
                AppendMode::Off
            }
        }),
        "append-verify" => Switch(|options, on| {
            options.append = if on {
                AppendMode::Verify
            } else {
                AppendMode::Off
            }
        }),
        "block-size" => Value(|options, value| {
            let size = parse_size(value, 'b')?;
            if size == 0 || size > MAX_BLOCK_SIZE {
                return Err(format!("between 1 and {MAX_BLOCK_SIZE} bytes"));
            }
            options.block_size = Some(size);
            Ok(())
        }),
        "remove-source-files" | "remove-sent-files" => {
            Switch(|options, on| options.remove_source_files = on)
        }
        "fsync" => Switch(|options, on| options.fsync = on),
        "rsync-path" => Value(|options, value| {
            options.rsync_path = value.trim().to_string();
            Ok(())
        }),
        "partial" | "progress" | "implied-dirs" => Quiet { takes_value: false },
        "partial-dir" => ignored_value("an interrupted file resumes from a hidden file beside it"),
        "temp-dir" => ignored_value("files are written under a temporary name beside the target"),
        "delay-updates" => ignored("each file is moved into place as soon as it is complete"),
        "sparse" | "preallocate" => ignored("files are written out in full"),
        "one-file-system" => ignored("folders on other file systems are synchronized too"),
        "fuzzy" => ignored("changes are worked out from the target's own copy only"),
        "relative" => refused("the folders to synchronize are the ones chosen in the dialog"),
        "compare-dest" | "copy-dest" | "link-dest" => {
            refused_value("reference folders on the receiving side are not supported")
        }
        "write-batch" | "only-write-batch" | "read-batch" => {
            refused_value("batch files are not supported")
        }
        "stop-after" | "time-limit" | "stop-at" => {
            refused_value("a synchronization cannot be stopped at a set time")
        }
        // Comparison
        "checksum" => Switch(|options, on| set_compare(options, CompareMode::Checksum, on)),
        "size-only" => Switch(|options, on| set_compare(options, CompareMode::SizeOnly, on)),
        "ignore-times" => Switch(|options, on| set_compare(options, CompareMode::Always, on)),
        "modify-window" => Value(|options, value| {
            let seconds: u32 = value
                .trim()
                .parse()
                .map_err(|_| "a whole number of seconds".to_string())?;
            if seconds > MAX_TIME_TOLERANCE_SECS {
                return Err(format!("at most {MAX_TIME_TOLERANCE_SECS} seconds"));
            }
            options.time_tolerance_secs = seconds;
            Ok(())
        }),
        "update" => Switch(|options, on| options.skip_newer_on_target = on),
        "existing" | "ignore-non-existing" => Switch(|options, on| options.existing_only = on),
        "ignore-existing" => Switch(|options, on| options.ignore_existing = on),
        "max-size" => Value(|options, value| {
            options.max_size = Some(parse_size(value, 'b')?);
            Ok(())
        }),
        "min-size" => Value(|options, value| {
            options.min_size = Some(parse_size(value, 'b')?);
            Ok(())
        }),
        "checksum-choice" | "cc" => ignored_value("Poros compares contents with MD5"),
        "checksum-seed" => Quiet { takes_value: true },
        // Deletion
        "delete" => Switch(|options, on| options.delete_extraneous = on),
        "del" | "delete-during" => {
            Switch(|options, on| set_delete(options, DeleteTiming::During, on))
        }
        "delete-before" => Switch(|options, on| set_delete(options, DeleteTiming::Before, on)),
        "delete-delay" => Switch(|options, on| set_delete(options, DeleteTiming::Delay, on)),
        "delete-after" => Switch(|options, on| set_delete(options, DeleteTiming::After, on)),
        "delete-excluded" => Switch(|options, on| {
            options.delete_excluded = on;
            options.delete_extraneous |= on;
        }),
        "max-delete" => Value(|options, value| {
            options.max_delete = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| "a number of files".to_string())?,
            );
            Ok(())
        }),
        "force" => Switch(|options, on| options.force = on),
        "ignore-errors" => Switch(|options, on| options.ignore_errors = on),
        "ignore-missing-args" => Quiet { takes_value: false },
        "delete-missing-args" => ignored("listed paths that are missing are left alone"),
        // Filters
        "filter" => Value(|options, value| {
            options.filter_rules.push(value.to_string());
            Ok(())
        }),
        "exclude" => Value(|options, value| {
            options.excludes.push(value.to_string());
            Ok(())
        }),
        "include" => Value(|options, value| {
            options.includes.push(value.to_string());
            Ok(())
        }),
        "exclude-from" => Value(|options, value| {
            options.exclude_from.push(non_empty(value)?);
            Ok(())
        }),
        "include-from" => Value(|options, value| {
            options.include_from.push(non_empty(value)?);
            Ok(())
        }),
        "files-from" => Value(|options, value| {
            options.files_from = non_empty(value)?;
            Ok(())
        }),
        "from0" => Switch(|options, on| options.from0 = on),
        "cvs-exclude" => Switch(|options, on| options.cvs_exclude = on),
        // Permissions, ownership and times
        "perms" => Switch(|options, on| options.perms = on),
        "executability" => Switch(|options, on| options.executability = on),
        "chmod" => Value(|options, value| {
            Chmod::parse(value).map_err(|error| error.message)?;
            if options.chmod.is_empty() {
                options.chmod = value.to_string();
            } else {
                options.chmod = format!("{},{value}", options.chmod);
            }
            Ok(())
        }),
        "owner" => Switch(|options, on| options.owner = on),
        "group" => Switch(|options, on| options.group = on),
        "chown" => Value(|options, value| {
            parse_chown(value)?;
            options.chown = value.trim().to_string();
            Ok(())
        }),
        "numeric-ids" => Switch(|options, on| options.numeric_ids = on),
        "usermap" | "groupmap" => refused_value("set the owner with --chown instead"),
        "times" => Switch(|options, on| options.times = on),
        "omit-dir-times" => Switch(|options, on| options.omit_dir_times = on),
        "omit-link-times" => Quiet { takes_value: false },
        "atimes" | "crtimes" | "open-noatime" => {
            ignored("access and creation times are kept as the target sets them")
        }
        "acls" | "xattrs" => ignored(NOT_OVER_SFTP),
        "hard-links" => ignored("hard-linked files are copied as separate files"),
        "devices" | "specials" | "D" | "copy-devices" | "write-devices" => {
            ignored("devices and special files cannot be created over SFTP and are passed over")
        }
        "super" | "fake-super" => ignored("Poros writes as the signed-in user"),
        "copy-as" => ignored_value("Poros writes as the signed-in user"),
        // Links
        "links" => Switch(|options, on| {
            if on {
                options.links = LinkMode::Preserve;
            } else if options.links == LinkMode::Preserve {
                options.links = LinkMode::Skip;
            }
        }),
        "copy-links" => Switch(|options, on| {
            if on {
                options.links = LinkMode::Follow;
            } else if options.links == LinkMode::Follow {
                options.links = LinkMode::Skip;
            }
        }),
        "safe-links" => Switch(|options, on| options.safe_links = on),
        "copy-unsafe-links" => Switch(|options, on| options.copy_unsafe_links = on),
        "copy-dirlinks" => Switch(|options, on| options.copy_dirlinks = on),
        "keep-dirlinks" => Switch(|options, on| options.keep_dirlinks = on),
        "munge-links" => refused("links are created as they are"),
        // Bandwidth and compression
        "bwlimit" => Value(|options, value| {
            let bytes = parse_size(value, 'k')?;
            options.bwlimit_kib = bytes.div_ceil(1024);
            Ok(())
        }),
        "compress" => Switch(|options, on| options.compress = on),
        "compress-level" | "zl" => ignored_value("SSH compresses at a level of its own"),
        "compress-choice" | "zc" => ignored_value("SSH compresses with zlib"),
        "skip-compress" => ignored_value("SSH compresses everything on the connection"),
        "old-compress" | "new-compress" => Quiet { takes_value: false },
        // Backup
        "backup" => Switch(|options, on| options.backup = on),
        "backup-dir" => Value(|options, value| {
            options.backup_dir = non_empty(value)?;
            options.backup = true;
            Ok(())
        }),
        "suffix" => Value(|options, value| {
            if value.contains('/') {
                return Err("a suffix cannot hold /".into());
            }
            options.backup_suffix = value.to_string();
            Ok(())
        }),
        // Output
        "dry-run" => Switch(|options, on| options.dry_run = on),
        "itemize-changes" => Switch(|options, on| options.itemize = on),
        "verbose" => Switch(|options, on| options.verbose = on),
        "stats" => Switch(|options, on| options.stats = on),
        "log-file" => Value(|options, value| {
            options.log_file = non_empty(value)?;
            Ok(())
        }),
        "log-file-format" | "out-format" => {
            ignored_value("changes are written in rsync's itemized form")
        }
        "list-only" => ignored("use a dry run (-n) to see what would change"),
        "quiet" | "human-readable" | "8-bit-output" | "no-motd" | "msgs2stderr" | "blocking-io"
        | "secluded-args" | "protect-args" | "old-args" | "trust-sender" | "P" => {
            Quiet { takes_value: false }
        }
        "info" | "debug" | "stderr" | "outbuf" | "max-alloc" => Quiet { takes_value: true },
        // Connection
        "rsh" | "port" | "address" | "sockopts" | "early-input" | "protocol" => {
            ignored_value(OWN_CONNECTION)
        }
        "remote-option" => ignored_value(OWN_CONNECTION),
        "ipv4" | "ipv6" => ignored(OWN_CONNECTION),
        "timeout" | "contimeout" => ignored_value("set timeouts in Settings > Connection"),
        "password-file" => ignored_value("rsync daemons are not supported"),
        "iconv" => ignored_value("names are sent as UTF-8"),
        "server" | "sender" | "daemon" | "no-detach" => refused("rsync runs this for itself"),
        "config" | "dparam" => refused_value("rsync daemons are not supported"),
        "version" | "help" => refused("there is nothing to show here"),
        _ => return None,
    })
}

fn short_option(character: char) -> Option<&'static str> {
    Some(match character {
        'a' => "archive",
        'b' => "backup",
        'c' => "checksum",
        'd' => "dirs",
        'e' => "rsh",
        'f' => "filter",
        'g' => "group",
        'h' => "human-readable",
        'i' => "itemize-changes",
        'k' => "copy-dirlinks",
        'l' => "links",
        'm' => "prune-empty-dirs",
        'n' => "dry-run",
        'o' => "owner",
        'p' => "perms",
        'q' => "quiet",
        'r' => "recursive",
        's' => "secluded-args",
        't' => "times",
        'u' => "update",
        'v' => "verbose",
        'x' => "one-file-system",
        'y' => "fuzzy",
        'z' => "compress",
        'A' => "acls",
        'B' => "block-size",
        'C' => "cvs-exclude",
        'D' => "D",
        'E' => "executability",
        'F' => "F",
        'H' => "hard-links",
        'I' => "ignore-times",
        'J' => "omit-link-times",
        'K' => "keep-dirlinks",
        'L' => "copy-links",
        'M' => "remote-option",
        'N' => "crtimes",
        'O' => "omit-dir-times",
        'P' => "P",
        'R' => "relative",
        'S' => "sparse",
        'T' => "temp-dir",
        'U' => "atimes",
        'V' => "version",
        'W' => "whole-file",
        'X' => "xattrs",
        '0' => "from0",
        '4' => "ipv4",
        '6' => "ipv6",
        '8' => "8-bit-output",
        '@' => "modify-window",
        _ => return None,
    })
}

fn set_compare(options: &mut SyncOptions, mode: CompareMode, on: bool) {
    if on {
        options.compare = mode;
    } else if options.compare == mode {
        options.compare = CompareMode::SizeAndTime;
    }
}

fn set_delete(options: &mut SyncOptions, timing: DeleteTiming, on: bool) {
    options.delete_extraneous = on;
    if on {
        options.delete_timing = timing;
    }
}

fn non_empty(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err("needs a value".into())
    } else {
        Ok(value.to_string())
    }
}

/// Reads rsync arguments into `options`.
fn apply_arguments(options: &mut SyncOptions, text: &str, notes: &mut Notes) {
    let words = match split_words(text) {
        Ok(words) => words,
        Err(message) => {
            notes.errors.push(message);
            return;
        }
    };
    let mut words = words.into_iter().peekable();
    if words
        .peek()
        .is_some_and(|first| first == "rsync" || first.ends_with("/rsync"))
    {
        words.next();
    }
    let mut filter_files = 0;
    let mut positional = Vec::new();
    while let Some(word) = words.next() {
        if word == "--" {
            positional.extend(words.by_ref());
            break;
        }
        if let Some(long) = word.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            let shown = format!("--{name}");
            match long_option(name) {
                Some(spec) => run(options, spec, &shown, true, inline, &mut words, notes),
                None => match name.strip_prefix("no-") {
                    Some(negated) => {
                        let negated = match negated.chars().collect::<Vec<_>>()[..] {
                            [single] => short_option(single).unwrap_or(negated),
                            _ => negated,
                        };
                        match long_option(negated) {
                            Some(Spec::Switch(switch)) if inline.is_none() => {
                                switch(options, false);
                            }
                            Some(Spec::Quiet { .. } | Spec::Ignored { .. }) => {}
                            _ => notes.errors.push(format!("{shown}: not an rsync option")),
                        }
                    }
                    None => notes.errors.push(format!("{shown}: not an rsync option")),
                },
            }
            continue;
        }
        let Some(cluster) = word.strip_prefix('-').filter(|cluster| !cluster.is_empty()) else {
            positional.push(word);
            continue;
        };
        for (index, character) in cluster.char_indices() {
            let shown = format!("-{character}");
            if character == 'F' {
                filter_files += 1;
                continue;
            }
            let Some(spec) = short_option(character).and_then(long_option) else {
                notes.errors.push(format!("{shown}: not an rsync option"));
                continue;
            };
            let takes_value = match &spec {
                Spec::Value(_) => true,
                Spec::Quiet { takes_value }
                | Spec::Ignored { takes_value, .. }
                | Spec::Refused { takes_value, .. } => *takes_value,
                Spec::Switch(_) => false,
            };
            if takes_value {
                let rest = &cluster[index + character.len_utf8()..];
                let inline = (!rest.is_empty()).then(|| rest.to_string());
                run(options, spec, &shown, false, inline, &mut words, notes);
                break;
            }
            run(options, spec, &shown, false, None, &mut words, notes);
        }
    }
    if filter_files > 0 {
        options.filter_files = if filter_files == 1 {
            FilterFiles::Read
        } else {
            FilterFiles::ReadAndExclude
        };
    }
    if !positional.is_empty() {
        notes.warnings.push(format!(
            "Left out {}: the folders to synchronize are the ones chosen in the dialog",
            positional.join(" ")
        ));
    }
}

fn run(
    options: &mut SyncOptions,
    spec: Spec,
    shown: &str,
    long: bool,
    inline: Option<String>,
    words: &mut std::iter::Peekable<std::vec::IntoIter<String>>,
    notes: &mut Notes,
) {
    let mut value = |inline: Option<String>| -> Option<String> {
        inline.or_else(|| {
            let next = words.next();
            if next.is_none() {
                notes.errors.push(format!("{shown}: needs a value"));
            }
            next
        })
    };
    match spec {
        Spec::Switch(switch) => {
            if long && inline.is_some() {
                notes.errors.push(format!("{shown}: takes no value"));
            } else {
                switch(options, true);
            }
        }
        Spec::Value(set) => {
            if let Some(text) = value(inline) {
                if let Err(message) = set(options, &text) {
                    notes.errors.push(format!("{shown}: {message}"));
                }
            }
        }
        Spec::Quiet { takes_value } => {
            if takes_value {
                value(inline);
            }
        }
        Spec::Ignored {
            takes_value,
            reason,
        } => {
            if takes_value {
                value(inline);
            }
            notes.warnings.push(format!("{shown} is ignored: {reason}"));
        }
        Spec::Refused {
            takes_value,
            reason,
        } => {
            if takes_value {
                value(inline);
            }
            notes
                .errors
                .push(format!("{shown} is not supported: {reason}"));
        }
    }
}

/// Splits arguments as a shell would: quotes group words, and a backslash escapes a space, a
/// quote or another backslash. Other backslashes stay, so Windows paths need no doubling.
fn split_words(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\'' => {
                in_word = true;
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err("An argument is missing its closing '".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match characters.next() {
                        Some('"') => break,
                        Some('\\') if matches!(characters.peek(), Some('"' | '\\')) => {
                            word.extend(characters.next());
                        }
                        Some(inner) => word.push(inner),
                        None => return Err("An argument is missing its closing \"".into()),
                    }
                }
            }
            '\\' if characters
                .peek()
                .is_some_and(|next| next.is_whitespace() || matches!(next, '\'' | '"' | '\\')) =>
            {
                in_word = true;
                word.extend(characters.next());
            }
            character if character.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            character => {
                in_word = true;
                word.push(character);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// The options as rsync arguments, leaving out what rsync does by default.
pub fn render(options: &SyncOptions) -> Vec<String> {
    let mut short = String::new();
    let archive = options.recursive
        && options.links == LinkMode::Preserve
        && options.perms
        && options.times
        && options.group
        && options.owner;
    if archive {
        short.push('a');
    } else {
        for (on, letter) in [
            (options.recursive, 'r'),
            (options.links == LinkMode::Preserve, 'l'),
            (options.perms, 'p'),
            (options.times, 't'),
            (options.group, 'g'),
            (options.owner, 'o'),
        ] {
            if on {
                short.push(letter);
            }
        }
    }
    for (on, letter) in [
        (options.dirs && !options.recursive, 'd'),
        (options.links == LinkMode::Follow, 'L'),
        (options.skip_newer_on_target, 'u'),
        (options.compare == CompareMode::Checksum, 'c'),
        (options.compare == CompareMode::Always, 'I'),
        (options.executability && !options.perms, 'E'),
        (options.omit_dir_times, 'O'),
        (options.whole_file, 'W'),
        (options.prune_empty_dirs, 'm'),
        (options.copy_dirlinks, 'k'),
        (options.keep_dirlinks, 'K'),
        (options.compress, 'z'),
        (options.backup, 'b'),
        (options.cvs_exclude, 'C'),
        (options.dry_run, 'n'),
        (options.itemize, 'i'),
        (options.verbose, 'v'),
    ] {
        if on {
            short.push(letter);
        }
    }
    match options.filter_files {
        FilterFiles::Off => {}
        FilterFiles::Read => short.push('F'),
        FilterFiles::ReadAndExclude => short.push_str("FF"),
    }
    let mut words = Vec::new();
    if !short.is_empty() {
        words.push(format!("-{short}"));
    }
    let mut flag = |on: bool, name: &str| {
        if on {
            words.push(format!("--{name}"));
        }
    };
    flag(options.compare == CompareMode::SizeOnly, "size-only");
    flag(options.existing_only, "existing");
    flag(options.ignore_existing, "ignore-existing");
    flag(options.inplace, "inplace");
    flag(options.append == AppendMode::Append, "append");
    flag(options.append == AppendMode::Verify, "append-verify");
    flag(options.mkpath, "mkpath");
    flag(options.remove_source_files, "remove-source-files");
    flag(options.fsync, "fsync");
    if options.delete_excluded {
        words.push("--delete-excluded".into());
    } else if options.delete_extraneous && options.delete_timing == DeleteTiming::During {
        words.push("--delete".into());
    }
    if options.delete_extraneous || options.delete_excluded {
        match options.delete_timing {
            DeleteTiming::During => {}
            DeleteTiming::Before => words.push("--delete-before".into()),
            DeleteTiming::Delay => words.push("--delete-delay".into()),
            DeleteTiming::After => words.push("--delete-after".into()),
        }
    }
    let mut flag = |on: bool, name: &str| {
        if on {
            words.push(format!("--{name}"));
        }
    };
    flag(options.force, "force");
    flag(options.ignore_errors, "ignore-errors");
    flag(options.numeric_ids, "numeric-ids");
    flag(
        options.links == LinkMode::Preserve && options.safe_links,
        "safe-links",
    );
    flag(options.copy_unsafe_links, "copy-unsafe-links");
    flag(options.from0, "from0");
    flag(options.stats, "stats");
    let mut value = |name: &str, value: String| {
        words.push(format!("--{name}={value}"));
    };
    if options.time_tolerance_secs != 0 {
        value("modify-window", options.time_tolerance_secs.to_string());
    }
    if let Some(size) = options.min_size {
        value("min-size", size.to_string());
    }
    if let Some(size) = options.max_size {
        value("max-size", size.to_string());
    }
    if let Some(limit) = options.max_delete {
        value("max-delete", limit.to_string());
    }
    if let Some(size) = options.block_size {
        value("block-size", size.to_string());
    }
    if options.bwlimit_kib > 0 {
        value("bwlimit", options.bwlimit_kib.to_string());
    }
    for (text, name) in [
        (&options.chmod, "chmod"),
        (&options.chown, "chown"),
        (&options.backup_dir, "backup-dir"),
        (&options.backup_suffix, "suffix"),
        (&options.rsync_path, "rsync-path"),
        (&options.files_from, "files-from"),
        (&options.log_file, "log-file"),
    ] {
        if !text.is_empty() {
            value(name, text.clone());
        }
    }
    for (list, name) in [
        (&options.filter_rules, "filter"),
        (&options.includes, "include"),
        (&options.excludes, "exclude"),
        (&options.include_from, "include-from"),
        (&options.exclude_from, "exclude-from"),
    ] {
        for item in list.iter().filter(|item| !item.trim().is_empty()) {
            value(name, item.clone());
        }
    }
    words
}

/// Arguments joined into one line, quoted where a shell would need it.
pub fn command_line(words: &[String]) -> String {
    words
        .iter()
        .map(|word| quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '-' | '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | ','
                )
        });
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> (SyncOptions, Notes) {
        let mut options = SyncOptions::default();
        let mut notes = Notes::default();
        apply_arguments(&mut options, text, &mut notes);
        (options, notes)
    }

    #[test]
    fn archive_and_short_clusters() {
        let (options, notes) = parsed("rsync -avzh --delete");
        assert!(notes.errors.is_empty() && notes.warnings.is_empty());
        assert!(options.recursive && options.perms && options.times);
        assert!(options.owner && options.group && options.compress && options.verbose);
        assert_eq!(options.links, LinkMode::Preserve);
        assert!(options.delete_extraneous);
        assert_eq!(options.delete_timing, DeleteTiming::During);

        let (options, _) = parsed("-a --no-perms --no-o --no-g -B 8192 -f'- *.o' -FF");
        assert!(!options.perms && !options.owner && !options.group);
        assert_eq!(options.block_size, Some(8192));
        assert_eq!(options.filter_rules, ["- *.o"]);
        assert_eq!(options.filter_files, FilterFiles::ReadAndExclude);
        let (options, _) = parsed("-B4096 -F");
        assert_eq!(options.block_size, Some(4096));
        assert_eq!(options.filter_files, FilterFiles::Read);
    }

    #[test]
    fn values_inline_or_following() {
        let (options, notes) = parsed(
            "--exclude=*.log --exclude 'my folder/' --include \"keep me\" --chmod=D755 \
             --chmod F644 --chown=www-data:www-data --bwlimit=1.5m --max-size=10M \
             --min-size 1k --modify-window=0 --max-delete=5 --delete-after --backup-dir=old \
             --suffix=.bak --log-file=/tmp/sync.log --files-from=list.txt --from0",
        );
        assert_eq!(notes.errors, Vec::<String>::new());
        assert_eq!(options.excludes, ["*.log", "my folder/"]);
        assert_eq!(options.includes, ["keep me"]);
        assert_eq!(options.chmod, "D755,F644");
        assert_eq!(options.chown, "www-data:www-data");
        assert_eq!(options.bwlimit_kib, 1536);
        assert_eq!(options.max_size, Some(10 * 1024 * 1024));
        assert_eq!(options.min_size, Some(1024));
        assert_eq!(options.time_tolerance_secs, 0);
        assert_eq!(options.max_delete, Some(5));
        assert!(options.delete_extraneous);
        assert_eq!(options.delete_timing, DeleteTiming::After);
        assert!(options.backup);
        assert_eq!(options.backup_dir, "old");
        assert_eq!(options.backup_suffix(), ".bak");
        assert_eq!(options.log_file, "/tmp/sync.log");
        assert_eq!(options.files_from, "list.txt");
        assert!(options.from0);
    }

    #[test]
    fn unsupported_and_ignored_options_are_reported() {
        let (_, notes) = parsed("-H -A --timeout=30 --progress -P --info=progress2 src/ host:dst/");
        assert!(notes.errors.is_empty());
        assert_eq!(notes.warnings.len(), 4);
        assert!(notes.warnings[0].starts_with("-H is ignored"));
        assert!(notes.warnings[2].starts_with("--timeout is ignored"));
        assert!(notes.warnings[3].contains("src/ host:dst/"));

        let (_, notes) = parsed("--link-dest=/old -R --frobnicate -Q --chmod=u+q --delete=yes");
        assert_eq!(notes.errors.len(), 6);
        assert!(notes.errors[0].starts_with("--link-dest is not supported"));
        assert!(notes.errors[2].starts_with("--frobnicate: not an rsync option"));
        assert!(notes.errors[5].contains("takes no value"));

        let (_, notes) = parsed("--exclude 'unclosed");
        assert_eq!(notes.errors.len(), 1);
        let (_, notes) = parsed("--exclude");
        assert_eq!(notes.errors, ["--exclude: needs a value"]);
    }

    #[test]
    fn negations_and_comparisons() {
        let (options, _) = parsed("-c --no-c");
        assert_eq!(options.compare, CompareMode::SizeAndTime);
        let (options, _) = parsed("--size-only");
        assert_eq!(options.compare, CompareMode::SizeOnly);
        let (options, _) = parsed("-I --no-L");
        assert_eq!(options.compare, CompareMode::Always);
        assert_eq!(options.links, LinkMode::Skip);
        let (options, _) = parsed("--no-times --no-recursive -d --no-implied-dirs");
        assert!(!options.times && !options.recursive && options.dirs);
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("100", 'b'), Ok(100));
        assert_eq!(parse_size("1k", 'b'), Ok(1024));
        assert_eq!(parse_size("1KB", 'b'), Ok(1000));
        assert_eq!(parse_size("1KiB", 'b'), Ok(1024));
        assert_eq!(parse_size("2.5M", 'b'), Ok(2_621_440));
        assert_eq!(parse_size("1g-1", 'b'), Ok(1_073_741_823));
        assert_eq!(parse_size("1g+1", 'b'), Ok(1_073_741_825));
        assert_eq!(parse_size("300", 'k'), Ok(307_200));
        assert!(parse_size("ten", 'b').is_err());
        assert!(parse_size("5q", 'b').is_err());
    }

    #[test]
    fn rendered_arguments_read_back_the_same() {
        let options = SyncOptions {
            compare: CompareMode::Checksum,
            time_tolerance_secs: 3,
            skip_newer_on_target: true,
            delete_extraneous: true,
            excludes: vec!["*.tmp".into(), "my dir/".into()],
            existing_only: true,
            min_size: Some(10),
            max_size: Some(1 << 20),
            dirs: true,
            recursive: false,
            mkpath: true,
            prune_empty_dirs: true,
            whole_file: true,
            inplace: true,
            append: AppendMode::Verify,
            block_size: Some(2048),
            remove_source_files: true,
            fsync: true,
            rsync_path: "sudo rsync".into(),
            delete_timing: DeleteTiming::Delay,
            delete_excluded: true,
            max_delete: Some(10),
            force: true,
            ignore_errors: true,
            filter_rules: vec!["P /keep/".into()],
            includes: vec!["important.tmp".into()],
            include_from: vec!["/lists/in".into()],
            exclude_from: vec!["C:\\lists\\out.txt".into()],
            files_from: "/lists/files".into(),
            from0: true,
            filter_files: FilterFiles::ReadAndExclude,
            cvs_exclude: true,
            perms: false,
            executability: true,
            chmod: "D755,F644".into(),
            owner: true,
            group: false,
            chown: "1000:1000".into(),
            numeric_ids: true,
            links: LinkMode::Preserve,
            safe_links: true,
            copy_unsafe_links: true,
            copy_dirlinks: true,
            keep_dirlinks: true,
            bwlimit_kib: 500,
            compress: true,
            backup: true,
            backup_dir: "backups".into(),
            backup_suffix: ".old".into(),
            dry_run: true,
            itemize: true,
            verbose: true,
            stats: true,
            log_file: "/var/log/poros sync.log".into(),
            ..SyncOptions::default()
        };
        let line = command_line(&render(&options));
        let mut read_back = SyncOptions {
            recursive: false,
            times: false,
            time_tolerance_secs: 0,
            links: LinkMode::Skip,
            ..SyncOptions::default()
        };
        let mut notes = Notes::default();
        apply_arguments(&mut read_back, &line, &mut notes);
        assert_eq!(notes.errors, Vec::<String>::new(), "{line}");
        assert_eq!(read_back, options, "{line}");

        let defaults = command_line(&render(&SyncOptions::default()));
        assert_eq!(defaults, "-rtL --modify-window=2");
    }

    #[test]
    fn checking_layers_and_mistakes() {
        let checked = check(
            SyncOptions::default(),
            &["-z --chmod=F644", "--no-z --exclude=.git/"],
            SyncDirection::Upload,
        );
        assert!(checked.errors.is_empty());
        assert!(!checked.options.compress);
        assert_eq!(checked.options.excludes, [".git/"]);
        assert_eq!(
            checked.arguments,
            "-rtL --modify-window=2 --chmod=F644 --exclude=.git/"
        );

        let checked = check(
            SyncOptions {
                chmod: "D9".into(),
                chown: "a b".into(),
                filter_rules: vec!["x nonsense".into()],
                remove_source_files: true,
                log_file: "relative.log".into(),
                ..SyncOptions::default()
            },
            &[],
            SyncDirection::Both,
        );
        assert_eq!(checked.errors.len(), 5, "{:?}", checked.errors);

        let checked = check(
            SyncOptions::default(),
            &["--delete --no-r"],
            SyncDirection::Upload,
        );
        assert_eq!(checked.errors, ["--delete does not work without -r or -d"]);
        let checked = check(
            SyncOptions::default(),
            &["--delete --no-r -d"],
            SyncDirection::Upload,
        );
        assert!(checked.errors.is_empty());
    }

    #[test]
    fn chown_forms() {
        assert_eq!(
            parse_chown("www:staff"),
            Ok((Some("www".into()), Some("staff".into())))
        );
        assert_eq!(parse_chown(":1000"), Ok((None, Some("1000".into()))));
        assert_eq!(parse_chown("root"), Ok((Some("root".into()), None)));
        assert!(parse_chown(":").is_err());
        assert!(parse_chown("-x").is_err());
    }
}
