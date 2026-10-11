//! Filter rules as rsync reads them. Rules are checked in order and the first that matches
//! decides; a path no rule matches is included. A rule can apply to the side files are sent
//! from, the side they are sent to, or both: the sending side's rules decide what is copied, the
//! receiving side's what is kept from deletion.
//!
//! Patterns: `*` and `?` stay within one path component, `**` crosses them (`a/**/b` also
//! matches `a/b`), `dir/***` matches a folder and everything in it, and `[...]` matches a set of
//! characters. A leading `/` anchors a pattern to the synchronized folder (or to the folder
//! holding a per-folder rule file); otherwise a pattern without a `/` matches a name at any
//! depth, and one with a `/` matches the end of a path. A trailing `/` matches folders only.
//!
//! Rule lines: `+`/`include`, `-`/`exclude`, `P`/`protect`, `R`/`risk`, `H`/`hide`,
//! `S`/`show`, `!`/`clear`, `.`/`merge FILE` and `:`/`dir-merge FILE`, with rsync's modifiers.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, AppResult};

/// The longest chain of merge files that include each other.
const MAX_MERGE_DEPTH: usize = 8;

/// What `-C` excludes before the user's own lists, as rsync has it.
const CVS_IGNORED: &str = "RCS SCCS CVS CVS.adm RCSLOG cvslog.* tags TAGS .make.state \
    .nse_depinfo *~ #* .#* ,* _$* *$ *.old *.bak *.BAK *.orig *.rej .del-* *.a *.olb *.o *.obj \
    *.so *.exe *.Z *.elc *.ln core .svn/ .git/ .hg/ .bzr/";

/// The per-folder rule file `-F` reads.
pub const RSYNC_FILTER_FILE: &str = ".rsync-filter";
const CVS_IGNORE_FILE: &str = ".cvsignore";

/// Which end of the synchronization a rule is asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Included,
    Excluded {
        /// Set by a perishable rule, which does not keep the folder around it from deletion.
        perishable: bool,
    },
}

impl Verdict {
    pub fn is_included(self) -> bool {
        self == Verdict::Included
    }
}

/// Everything the rules are built from, in the order they are checked.
#[derive(Debug, Clone, Default)]
pub struct FilterSpec {
    /// Rules in rsync's filter syntax, one per entry.
    pub rules: Vec<String>,
    /// Patterns, each an include rule.
    pub includes: Vec<String>,
    /// The contents of include lists read from files, one pattern per line.
    pub include_lists: Vec<String>,
    pub excludes: Vec<String>,
    pub exclude_lists: Vec<String>,
    /// Restricts the synchronization to these paths, as `--files-from` does.
    pub files_from: Option<Vec<String>>,
    /// With `files_from`, copies everything inside the folders listed.
    pub recursive: bool,
    /// Reads `.rsync-filter` in every folder (`-F`).
    pub per_folder_filters: bool,
    /// Also leaves the `.rsync-filter` files themselves out (`-FF`).
    pub exclude_filter_files: bool,
    /// Excludes version-control and build files, and what `.cvsignore` files list (`-C`).
    pub cvs_exclude: bool,
    /// Patterns from `~/.cvsignore` and `CVSIGNORE`, added to the `-C` list.
    pub cvs_ignore: String,
    /// Rules that name no side apply to the sender only, so the receiver deletes what they
    /// exclude (`--delete-excluded`).
    pub delete_excluded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Include,
    Exclude,
}

#[derive(Debug, Clone)]
struct Rule {
    action: Action,
    pattern: Pattern,
    sender: bool,
    receiver: bool,
    perishable: bool,
    /// Matches what the pattern does not.
    negate: bool,
}

impl Rule {
    fn decide(&self, path: &str, is_dir: bool, side: Side) -> Option<Verdict> {
        let applies = match side {
            Side::Sender => self.sender,
            Side::Receiver => self.receiver,
        };
        if !applies || self.pattern.matches(path, is_dir) == self.negate {
            return None;
        }
        Some(match self.action {
            Action::Include => Verdict::Included,
            Action::Exclude => Verdict::Excluded {
                perishable: self.perishable,
            },
        })
    }
}

#[derive(Debug, Clone)]
enum Entry {
    Rule(Rule),
    /// The rules of a per-folder merge file, by index into `Filter::merges`.
    PerFolder(usize),
}

/// How a per-folder merge file is read.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MergeSpec {
    file_name: String,
    /// Its rules also apply in the folders below.
    inherit: bool,
    /// Splits lines into words, each a rule.
    word_split: bool,
    /// Every line is a pattern for this action, without a rule prefix.
    default_action: Option<Action>,
    /// Lines may hold `!` to clear the list, as `.cvsignore` files may.
    allows_clear: bool,
    sides: Sides,
    perishable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sides {
    sender: bool,
    receiver: bool,
    /// The rule named its side, so `--delete-excluded` leaves it alone.
    explicit: bool,
}

impl Sides {
    const BOTH: Sides = Sides {
        sender: true,
        receiver: true,
        explicit: false,
    };
}

/// The rules for a synchronization. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    entries: Arc<[Entry]>,
    merges: Arc<[MergeSpec]>,
    delete_excluded: bool,
}

/// The rules per-folder merge files add in one folder, nearest folder first. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct FolderRules {
    chains: Arc<[Option<Arc<Frame>>]>,
}

#[derive(Debug)]
struct Frame {
    /// The folder holding the merge file, relative to the synchronized folder.
    base: String,
    rules: Vec<Rule>,
    inherit: bool,
    parent: Option<Arc<Frame>>,
}

/// One parsed line.
enum Parsed {
    Rule(Rule),
    Clear,
    Merge { file: String, spec: MergeSpec },
    PerFolder(MergeSpec),
}

impl Filter {
    /// `read_file` reads a file on this computer named by a `merge` rule.
    pub fn build(
        spec: &FilterSpec,
        read_file: &dyn Fn(&str) -> AppResult<String>,
    ) -> AppResult<Self> {
        let mut builder = Builder {
            spec,
            read_file,
            entries: Vec::new(),
            merges: Vec::new(),
        };
        if spec.per_folder_filters {
            builder.add_line(&format!("dir-merge {RSYNC_FILTER_FILE}"), 0)?;
            if spec.exclude_filter_files {
                builder.add_line(&format!("- {RSYNC_FILTER_FILE}"), 0)?;
            }
        }
        for rule in &spec.rules {
            builder.add_line(rule, 0)?;
        }
        for pattern in spec.includes.iter().map(String::as_str).chain(
            spec.include_lists
                .iter()
                .flat_map(|list| list.lines())
                .filter(|line| !is_comment(line)),
        ) {
            builder.add_pattern(Action::Include, pattern)?;
        }
        for pattern in spec.excludes.iter().map(String::as_str).chain(
            spec.exclude_lists
                .iter()
                .flat_map(|list| list.lines())
                .filter(|line| !is_comment(line)),
        ) {
            builder.add_pattern(Action::Exclude, pattern)?;
        }
        if let Some(paths) = &spec.files_from {
            for rule in files_from_rules(paths, spec.recursive) {
                builder.add_line(&rule, 0)?;
            }
        }
        if spec.cvs_exclude {
            builder.add_line("-C", 0)?;
            builder.add_line(":C", 0)?;
        }
        Ok(Self {
            entries: builder.entries.into(),
            merges: builder.merges.into(),
            delete_excluded: spec.delete_excluded,
        })
    }

    /// The names of the per-folder rule files to look for in every folder.
    pub fn merge_file_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .merges
            .iter()
            .map(|merge| merge.file_name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// The rules in `folder`, a child of the folder `parent` holds the rules of; `files` maps
    /// the names of the merge files found in `folder` to their contents.
    pub fn enter(
        &self,
        parent: &FolderRules,
        folder: &str,
        files: &HashMap<String, String>,
    ) -> AppResult<FolderRules> {
        if self.merges.is_empty() {
            return Ok(FolderRules::default());
        }
        let parent_folder = folder.rsplit_once('/').map_or("", |(parent, _)| parent);
        let mut chains = Vec::with_capacity(self.merges.len());
        for (index, merge) in self.merges.iter().enumerate() {
            let mut inherited = parent.chains.get(index).cloned().flatten();
            // Rules a parent folder kept to itself stop there.
            if let Some(head) = inherited
                .as_ref()
                .filter(|head| !head.inherit && head.base == parent_folder && !folder.is_empty())
            {
                inherited = head.parent.clone();
            }
            let chain = match files.get(&merge.file_name) {
                Some(contents) => {
                    let (rules, cleared) =
                        self.per_folder_rules(merge, contents).map_err(|error| {
                            let path = if folder.is_empty() {
                                merge.file_name.clone()
                            } else {
                                format!("{folder}/{}", merge.file_name)
                            };
                            AppError::invalid(format!("{path}: {}", error.message))
                        })?;
                    Some(Arc::new(Frame {
                        base: folder.to_string(),
                        rules,
                        inherit: merge.inherit,
                        parent: if cleared { None } else { inherited },
                    }))
                }
                None => inherited,
            };
            chains.push(chain);
        }
        Ok(FolderRules {
            chains: chains.into(),
        })
    }

    fn per_folder_rules(&self, merge: &MergeSpec, contents: &str) -> AppResult<(Vec<Rule>, bool)> {
        let mut rules = Vec::new();
        let mut cleared = false;
        for parsed in parse_merge_contents(merge, contents)? {
            match parsed {
                Parsed::Rule(rule) => rules.push(self.finish(rule)),
                Parsed::Clear => {
                    rules.clear();
                    cleared = true;
                }
                Parsed::Merge { .. } | Parsed::PerFolder(_) => {
                    return Err(AppError::invalid(
                        "per-folder rule files cannot merge other files",
                    ))
                }
            }
        }
        Ok((rules, cleared))
    }

    fn finish(&self, rule: Rule) -> Rule {
        finish_rule(rule, self.delete_excluded)
    }

    /// Whether `path`, relative to the synchronized folder, is copied (for the sender) or
    /// kept from deletion (for the receiver). `folder` holds the rules of the folder `path` is
    /// in.
    pub fn verdict(&self, path: &str, is_dir: bool, side: Side, folder: &FolderRules) -> Verdict {
        for entry in self.entries.iter() {
            match entry {
                Entry::Rule(rule) => {
                    if let Some(verdict) = rule.decide(path, is_dir, side) {
                        return verdict;
                    }
                }
                Entry::PerFolder(index) => {
                    let mut frame = folder.chains.get(*index).and_then(Option::as_deref);
                    while let Some(current) = frame {
                        let relative = relative_to(path, &current.base);
                        for rule in &current.rules {
                            if let Some(verdict) = rule.decide(relative, is_dir, side) {
                                return verdict;
                            }
                        }
                        frame = current.parent.as_deref();
                    }
                }
            }
        }
        Verdict::Included
    }
}

/// With `--delete-excluded`, a rule that names no side is the sender's alone.
fn finish_rule(mut rule: Rule, delete_excluded: bool) -> Rule {
    if delete_excluded && rule.sender && rule.receiver {
        rule.receiver = false;
    }
    rule
}

struct Builder<'a> {
    spec: &'a FilterSpec,
    read_file: &'a dyn Fn(&str) -> AppResult<String>,
    entries: Vec<Entry>,
    merges: Vec<MergeSpec>,
}

impl Builder<'_> {
    fn add_line(&mut self, line: &str, depth: usize) -> AppResult<()> {
        let parsed = parse_line(line).map_err(|error| {
            AppError::invalid(format!("\"{}\": {}", line.trim(), error.message))
        })?;
        for item in parsed {
            self.add_parsed(item, depth)?;
        }
        Ok(())
    }

    fn add_pattern(&mut self, action: Action, pattern: &str) -> AppResult<()> {
        let trimmed = pattern.trim_end_matches(['\r', '\n']).trim_end();
        if trimmed.trim_start().is_empty() {
            return Ok(());
        }
        let rule = Rule {
            action,
            pattern: Pattern::parse(trimmed)
                .map_err(|error| AppError::invalid(format!("\"{trimmed}\": {}", error.message)))?,
            sender: true,
            receiver: true,
            perishable: false,
            negate: false,
        };
        self.add_parsed(Parsed::Rule(rule), 0)
    }

    fn add_parsed(&mut self, parsed: Parsed, depth: usize) -> AppResult<()> {
        match parsed {
            Parsed::Rule(rule) => {
                let rule = finish_rule(rule, self.spec.delete_excluded);
                self.entries.push(Entry::Rule(rule));
            }
            Parsed::Clear => self.entries.clear(),
            Parsed::Merge { file, spec } => {
                if depth >= MAX_MERGE_DEPTH {
                    return Err(AppError::invalid(format!(
                        "merge files include each other more than {MAX_MERGE_DEPTH} deep"
                    )));
                }
                let contents = (self.read_file)(&file)?;
                for item in parse_merge_contents(&spec, &contents)
                    .map_err(|error| AppError::invalid(format!("{file}: {}", error.message)))?
                {
                    self.add_parsed(item, depth + 1)?;
                }
            }
            Parsed::PerFolder(spec) if spec.file_name.is_empty() => {
                for item in self.cvs_rules(spec.sides)? {
                    self.add_parsed(item, depth)?;
                }
            }
            Parsed::PerFolder(spec) => {
                let index = match self.merges.iter().position(|known| *known == spec) {
                    Some(index) => index,
                    None => {
                        self.merges.push(spec);
                        self.merges.len() - 1
                    }
                };
                self.entries.push(Entry::PerFolder(index));
            }
        }
        Ok(())
    }

    /// The `-C` list as perishable excludes, with the user's own ignore patterns.
    fn cvs_rules(&self, sides: Sides) -> AppResult<Vec<Parsed>> {
        CVS_IGNORED
            .split_whitespace()
            .chain(self.spec.cvs_ignore.split_whitespace())
            .map(|word| {
                Ok(Parsed::Rule(Rule {
                    action: Action::Exclude,
                    pattern: Pattern::parse(word)?,
                    sender: sides.sender,
                    receiver: sides.receiver,
                    perishable: true,
                    negate: false,
                }))
            })
            .collect()
    }
}

fn is_comment(line: &str) -> bool {
    let line = line.trim();
    line.is_empty() || line.starts_with('#') || line.starts_with(';')
}

/// One line of the rule syntax; `-C` can stand for many rules.
fn parse_line(line: &str) -> AppResult<Vec<Parsed>> {
    let line = line.trim_end_matches(['\r', '\n']).trim_end();
    if is_comment(line) {
        return Ok(Vec::new());
    }
    let line = line.trim_start();
    let (kind, modifiers, argument) = split_rule(line)?;
    let parsed = match kind {
        RuleKind::Clear => {
            if !modifiers.is_empty() || !argument.is_empty() {
                return Err(AppError::invalid("clear takes nothing after it"));
            }
            Parsed::Clear
        }
        RuleKind::Merge => {
            if argument.is_empty() {
                return Err(AppError::invalid("merge needs a file"));
            }
            let mut spec = merge_spec(modifiers)?;
            spec.file_name = argument.to_string();
            return Ok(vec![Parsed::Merge {
                file: argument.to_string(),
                spec,
            }]);
        }
        RuleKind::PerFolder => {
            let mut spec = merge_spec(modifiers)?;
            let name = argument.trim_start_matches('/');
            if !name.is_empty() {
                spec.file_name = name.to_string();
            }
            if spec.file_name.is_empty() {
                return Err(AppError::invalid("dir-merge needs a file name"));
            }
            if spec.file_name.contains('/') {
                return Err(AppError::invalid(
                    "a per-folder rule file is a name, not a path",
                ));
            }
            let exclude_self = modifiers.contains('e');
            let name = spec.file_name.clone();
            let mut parsed = Vec::new();
            if exclude_self {
                parsed.push(Parsed::Rule(Rule {
                    action: Action::Exclude,
                    pattern: Pattern::parse(&name)?,
                    sender: true,
                    receiver: true,
                    perishable: false,
                    negate: false,
                }));
            }
            parsed.push(Parsed::PerFolder(spec));
            return Ok(parsed);
        }
        RuleKind::Pattern(action, base_sides) => {
            let mut sides = base_sides;
            let mut perishable = false;
            let mut negate = false;
            let mut absolute = false;
            let mut cvs = false;
            for modifier in modifiers.chars() {
                match modifier {
                    '/' => absolute = true,
                    '!' => negate = true,
                    'C' => cvs = true,
                    's' => sides = side_only(sides, Side::Sender)?,
                    'r' => sides = side_only(sides, Side::Receiver)?,
                    'p' => perishable = true,
                    'x' => {
                        return Err(AppError::invalid(
                            "extended attributes do not travel over SFTP",
                        ))
                    }
                    other => return Err(AppError::invalid(format!("unknown modifier '{other}'"))),
                }
            }
            if cvs {
                if !argument.is_empty() || action != Action::Exclude {
                    return Err(AppError::invalid("-C takes no pattern"));
                }
                // Expanded by the builder, which knows the user's own ignore patterns.
                return Ok(vec![Parsed::PerFolder(MergeSpec {
                    file_name: String::new(),
                    inherit: false,
                    word_split: true,
                    default_action: Some(Action::Exclude),
                    allows_clear: false,
                    sides,
                    perishable: true,
                })]);
            }
            if argument.is_empty() {
                return Err(AppError::invalid("missing a pattern"));
            }
            if absolute {
                return Err(AppError::invalid(
                    "the / modifier (match absolute paths) is not supported; anchor the pattern with a leading / instead",
                ));
            }
            Parsed::Rule(Rule {
                action,
                pattern: Pattern::parse(argument)?,
                sender: sides.sender,
                receiver: sides.receiver,
                perishable,
                negate,
            })
        }
    };
    Ok(vec![parsed])
}

fn side_only(sides: Sides, side: Side) -> AppResult<Sides> {
    if sides.explicit {
        return Err(AppError::invalid(
            "this rule already applies to one side only",
        ));
    }
    Ok(Sides {
        sender: side == Side::Sender,
        receiver: side == Side::Receiver,
        explicit: true,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    Pattern(Action, Sides),
    Clear,
    Merge,
    PerFolder,
}

/// The rule, its modifiers and its pattern or file name.
fn split_rule(line: &str) -> AppResult<(RuleKind, &str, &str)> {
    const LONG_NAMES: [&str; 9] = [
        "dir-merge",
        "include",
        "exclude",
        "protect",
        "risk",
        "hide",
        "show",
        "clear",
        "merge",
    ];
    let long = LONG_NAMES.iter().find(|name| {
        line.strip_prefix(**name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', ',']))
    });
    let (name, rest) = match long {
        Some(name) => (*name, &line[name.len()..]),
        None => {
            let mut characters = line.chars();
            let first = characters.next().unwrap_or(' ');
            let name =
                match first {
                    '+' => "include",
                    '-' => "exclude",
                    'P' => "protect",
                    'R' => "risk",
                    'H' => "hide",
                    'S' => "show",
                    '!' => "clear",
                    '.' => "merge",
                    ':' => "dir-merge",
                    _ => return Err(AppError::invalid(
                        "a rule starts with +, -, P, R, H, S, !, . or : (or include, exclude, ...)",
                    )),
                };
            (name, characters.as_str())
        }
    };
    let both = Sides::BOTH;
    let sender = Sides {
        sender: true,
        receiver: false,
        explicit: true,
    };
    let receiver = Sides {
        sender: false,
        receiver: true,
        explicit: true,
    };
    let kind = match name {
        "include" => RuleKind::Pattern(Action::Include, both),
        "exclude" => RuleKind::Pattern(Action::Exclude, both),
        "protect" => RuleKind::Pattern(Action::Exclude, receiver),
        "risk" => RuleKind::Pattern(Action::Include, receiver),
        "hide" => RuleKind::Pattern(Action::Exclude, sender),
        "show" => RuleKind::Pattern(Action::Include, sender),
        "clear" => RuleKind::Clear,
        "merge" => RuleKind::Merge,
        _ => RuleKind::PerFolder,
    };
    // Modifiers follow the rule directly, or after a comma; a space or `_` ends them.
    let rest = rest.strip_prefix(',').unwrap_or(rest);
    let end = rest.find([' ', '_']).unwrap_or(rest.len());
    let modifiers = &rest[..end];
    let argument = rest.get(end + 1..).unwrap_or("");
    Ok((kind, modifiers, argument))
}

/// How a merge or dir-merge rule's file is read; the `C` modifier names `.cvsignore`.
fn merge_spec(modifiers: &str) -> AppResult<MergeSpec> {
    let mut spec = MergeSpec {
        file_name: String::new(),
        inherit: true,
        word_split: false,
        default_action: None,
        allows_clear: true,
        sides: Sides::BOTH,
        perishable: false,
    };
    for modifier in modifiers.chars() {
        match modifier {
            '-' => spec.default_action = Some(Action::Exclude),
            '+' => spec.default_action = Some(Action::Include),
            'C' => {
                spec.inherit = false;
                spec.word_split = true;
                spec.default_action = Some(Action::Exclude);
                spec.file_name = CVS_IGNORE_FILE.to_string();
            }
            'n' => spec.inherit = false,
            'w' => spec.word_split = true,
            'e' => {}
            's' => spec.sides = side_only(spec.sides, Side::Sender)?,
            'r' => spec.sides = side_only(spec.sides, Side::Receiver)?,
            'p' => spec.perishable = true,
            other => return Err(AppError::invalid(format!("unknown modifier '{other}'"))),
        }
    }
    Ok(spec)
}

/// The rules in a merge file's text, with the merge rule's modifiers applied.
fn parse_merge_contents(spec: &MergeSpec, contents: &str) -> AppResult<Vec<Parsed>> {
    let mut parsed = Vec::new();
    for line in contents.lines() {
        if is_comment(line) {
            continue;
        }
        let items: Vec<&str> = if spec.word_split {
            line.split_whitespace().collect()
        } else {
            vec![line]
        };
        for item in items {
            if spec.allows_clear && item.trim() == "!" {
                parsed.push(Parsed::Clear);
                continue;
            }
            match spec.default_action {
                Some(action) => {
                    let pattern = item.trim_end();
                    parsed.push(Parsed::Rule(Rule {
                        action,
                        pattern: Pattern::parse(pattern).map_err(|error| {
                            AppError::invalid(format!("\"{pattern}\": {}", error.message))
                        })?,
                        sender: spec.sides.sender,
                        receiver: spec.sides.receiver,
                        perishable: spec.perishable,
                        negate: false,
                    }));
                }
                None => {
                    for mut item in parse_line(item).map_err(|error| {
                        AppError::invalid(format!("\"{}\": {}", item.trim(), error.message))
                    })? {
                        if let Parsed::Rule(rule) = &mut item {
                            if spec.sides.explicit {
                                rule.sender = spec.sides.sender;
                                rule.receiver = spec.sides.receiver;
                            }
                            rule.perishable |= spec.perishable;
                        }
                        parsed.push(item);
                    }
                }
            }
        }
    }
    Ok(parsed)
}

/// Rules that let through only the listed paths, the folders leading to them and, when
/// `recursive`, everything inside listed folders.
fn files_from_rules(paths: &[String], recursive: bool) -> Vec<String> {
    let mut rules = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        let path = path.trim_end_matches(['\r', '\n']);
        let path = path.trim_start_matches("./").trim_matches('/');
        if path.is_empty() || path.starts_with('#') || path.starts_with(';') {
            continue;
        }
        let escaped: Vec<String> = path.split('/').map(escape_pattern).collect();
        for depth in 1..escaped.len() {
            let folder = format!("+ /{}/", escaped[..depth].join("/"));
            if seen.insert(folder.clone()) {
                rules.push(folder);
            }
        }
        let full = escaped.join("/");
        let rule = if recursive {
            format!("+ /{full}/***")
        } else {
            format!("+ /{full}")
        };
        if seen.insert(rule.clone()) {
            rules.push(rule.clone());
        }
        // A listed file is not a folder, so `***` alone would miss it.
        if recursive {
            let file = format!("+ /{full}");
            if seen.insert(file.clone()) {
                rules.push(file);
            }
        }
    }
    rules.push("- *".to_string());
    rules
}

fn escape_pattern(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// The part of `path` inside `base`, both relative to the synchronized folder.
fn relative_to<'a>(path: &'a str, base: &str) -> &'a str {
    if base.is_empty() {
        return path;
    }
    path.strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path)
}

#[derive(Debug, Clone)]
struct Pattern {
    /// Without the anchoring `/` and the trailing `/`.
    text: Vec<char>,
    anchored: bool,
    folders_only: bool,
    /// Slashes inside the pattern.
    slashes: usize,
    double_star: bool,
    starts_with_double_star: bool,
    /// Ends in `/***`, which also matches the folder itself.
    folder_and_contents: bool,
}

impl Pattern {
    fn parse(raw: &str) -> AppResult<Self> {
        let folders_only = raw.ends_with('/') && raw.len() > 1;
        let trimmed = if folders_only {
            raw.trim_end_matches('/')
        } else {
            raw
        };
        let anchored = trimmed.starts_with('/');
        let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
        let text: Vec<char> = body.chars().collect();
        if text.is_empty() {
            return Err(AppError::invalid(if anchored {
                "would match everything; use - * or - /* to exclude everything"
            } else {
                "missing a pattern"
            }));
        }
        if !balanced_brackets(&text) {
            return Err(AppError::invalid("has an unclosed ["));
        }
        Ok(Self {
            slashes: text.iter().filter(|character| **character == '/').count(),
            double_star: body.contains("**"),
            starts_with_double_star: body.starts_with("**"),
            folder_and_contents: body.ends_with("/***") || body == "***",
            text,
            anchored,
            folders_only,
        })
    }

    /// Follows rsync's `rule_matches`.
    fn matches(&self, path: &str, is_dir: bool) -> bool {
        if self.folders_only && !is_dir {
            return false;
        }
        let name_only = !self.anchored && self.slashes == 0 && !self.double_star;
        let subject = if name_only {
            path.rsplit('/').next().unwrap_or(path)
        } else {
            path
        };
        let mut text: Vec<char> = Vec::with_capacity(subject.len() + 2);
        if self.starts_with_double_star && !self.anchored {
            text.push('/');
        }
        text.extend(subject.chars());
        if is_dir && self.folder_and_contents {
            text.push('/');
        }
        if self.anchored || name_only || self.starts_with_double_star {
            return glob(&self.text, &text);
        }
        if !self.double_star {
            // The last `slashes + 1` components.
            let mut seen = 0;
            for (index, character) in text.iter().enumerate().rev() {
                if *character == '/' {
                    seen += 1;
                    if seen == self.slashes + 1 {
                        return glob(&self.text, &text[index + 1..]);
                    }
                }
            }
            return glob(&self.text, &text);
        }
        std::iter::once(0)
            .chain(
                text.iter()
                    .enumerate()
                    .filter(|(_, character)| **character == '/')
                    .map(|(index, _)| index + 1),
            )
            .any(|start| glob(&self.text, &text[start..]))
    }
}

fn glob(pattern: &[char], text: &[char]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some('*') if pattern.get(1) == Some(&'*') => {
            let stars = pattern
                .iter()
                .take_while(|character| **character == '*')
                .count();
            let rest = &pattern[stars..];
            // `a/**/b` also matches `a/b`.
            let no_folders = rest.first() == Some(&'/') && glob(&rest[1..], text);
            no_folders || (0..=text.len()).any(|skip| glob(rest, &text[skip..]))
        }
        Some('*') => {
            let rest = &pattern[1..];
            let component = text
                .iter()
                .position(|&character| character == '/')
                .unwrap_or(text.len());
            (0..=component).any(|skip| glob(rest, &text[skip..]))
        }
        Some('?') => {
            text.first().is_some_and(|&character| character != '/')
                && glob(&pattern[1..], &text[1..])
        }
        Some('[') => {
            let Some((set, rest)) = character_set(&pattern[1..]) else {
                return false;
            };
            text.first()
                .is_some_and(|&character| character != '/' && set.contains(character))
                && glob(rest, &text[1..])
        }
        Some('\\') if pattern.len() > 1 => {
            text.first() == Some(&pattern[1]) && glob(&pattern[2..], &text[1..])
        }
        Some(&literal) => text.first() == Some(&literal) && glob(&pattern[1..], &text[1..]),
    }
}

struct CharacterSet<'a> {
    negated: bool,
    items: &'a [char],
}

impl CharacterSet<'_> {
    fn contains(&self, character: char) -> bool {
        let mut found = false;
        let mut index = 0;
        while index < self.items.len() {
            if self.items[index] == '[' && self.items.get(index + 1) == Some(&':') {
                if let Some(length) = self.items[index + 2..]
                    .windows(2)
                    .position(|pair| pair == [':', ']'])
                {
                    let class: String = self.items[index + 2..index + 2 + length].iter().collect();
                    found |= in_class(&class, character);
                    index += length + 4;
                    continue;
                }
            }
            let start = self.items[index];
            if index + 2 < self.items.len() && self.items[index + 1] == '-' {
                found |= (start..=self.items[index + 2]).contains(&character);
                index += 3;
            } else {
                found |= start == character;
                index += 1;
            }
        }
        found != self.negated
    }
}

fn in_class(class: &str, character: char) -> bool {
    match class {
        "alnum" => character.is_alphanumeric(),
        "alpha" => character.is_alphabetic(),
        "blank" => character == ' ' || character == '\t',
        "cntrl" => character.is_control(),
        "digit" => character.is_ascii_digit(),
        "graph" => character.is_ascii_graphic(),
        "lower" => character.is_lowercase(),
        "print" => character.is_ascii_graphic() || character == ' ',
        "punct" => character.is_ascii_punctuation(),
        "space" => character.is_whitespace(),
        "upper" => character.is_uppercase(),
        "xdigit" => character.is_ascii_hexdigit(),
        _ => false,
    }
}

/// The set after a `[`, and the pattern after its `]`.
fn character_set(pattern: &[char]) -> Option<(CharacterSet<'_>, &[char])> {
    let negated = matches!(pattern.first(), Some('!' | '^'));
    let start = usize::from(negated);
    // A `]` right after the opening bracket is a member, not the end; `[:class:]` is skipped.
    let mut index = if pattern.get(start) == Some(&']') {
        start + 1
    } else {
        start
    };
    while index < pattern.len() {
        if pattern[index] == '[' && pattern.get(index + 1) == Some(&':') {
            if let Some(length) = pattern[index + 2..]
                .windows(2)
                .position(|pair| pair == [':', ']'])
            {
                index += length + 4;
                continue;
            }
        }
        if pattern[index] == ']' {
            return Some((
                CharacterSet {
                    negated,
                    items: &pattern[start..index],
                },
                &pattern[index + 1..],
            ));
        }
        index += 1;
    }
    None
}

fn balanced_brackets(pattern: &[char]) -> bool {
    let mut index = 0;
    while index < pattern.len() {
        match pattern[index] {
            '\\' => index += 2,
            '[' => match character_set(&pattern[index + 1..]) {
                Some((_, rest)) => index = pattern.len() - rest.len(),
                None => return false,
            },
            _ => index += 1,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_files(name: &str) -> AppResult<String> {
        Err(AppError::invalid(format!("no file {name}")))
    }

    fn build(spec: FilterSpec) -> Filter {
        Filter::build(&spec, &no_files).unwrap()
    }

    fn rules(lines: &[&str]) -> Filter {
        build(FilterSpec {
            rules: lines.iter().map(|line| line.to_string()).collect(),
            ..FilterSpec::default()
        })
    }

    fn excludes(patterns: &[&str]) -> Filter {
        build(FilterSpec {
            excludes: patterns.iter().map(|pattern| pattern.to_string()).collect(),
            ..FilterSpec::default()
        })
    }

    fn sent(filter: &Filter, path: &str, is_dir: bool) -> bool {
        filter
            .verdict(path, is_dir, Side::Sender, &FolderRules::default())
            .is_included()
    }

    fn kept(filter: &Filter, path: &str, is_dir: bool) -> bool {
        !filter
            .verdict(path, is_dir, Side::Receiver, &FolderRules::default())
            .is_included()
    }

    #[test]
    fn plain_names_match_at_any_depth() {
        let filter = excludes(&["node_modules", "*.log"]);
        assert!(!sent(&filter, "node_modules", true));
        assert!(!sent(&filter, "web/node_modules", true));
        assert!(!sent(&filter, "logs/today.log", false));
        assert!(sent(&filter, "node_modules_backup", true));
        assert!(sent(&filter, "today.log.gz", false));
    }

    #[test]
    fn anchored_and_folder_patterns() {
        let filter = excludes(&["/build", "cache/", "docs/*.tmp"]);
        assert!(!sent(&filter, "build", true));
        assert!(sent(&filter, "src/build", true));
        assert!(!sent(&filter, "cache", true));
        assert!(sent(&filter, "cache", false));
        assert!(!sent(&filter, "docs/a.tmp", false));
        assert!(!sent(&filter, "site/docs/a.tmp", false));
        assert!(sent(&filter, "docs/sub/a.tmp", false));
    }

    #[test]
    fn wildcards_and_sets() {
        let filter = excludes(&[
            "/a/**/z",
            "file?.[ch]",
            "[!.]*.bak",
            "v[[:digit:]]",
            "# a comment",
            "",
        ]);
        assert!(!sent(&filter, "a/z", false));
        assert!(!sent(&filter, "a/b/c/z", false));
        assert!(!sent(&filter, "file1.c", false));
        assert!(!sent(&filter, "deep/fileX.h", false));
        assert!(sent(&filter, "file10.c", false));
        assert!(!sent(&filter, "old.bak", false));
        assert!(sent(&filter, ".old.bak", false));
        assert!(!sent(&filter, "v7", false));
        assert!(sent(&filter, "vx", false));
    }

    #[test]
    fn first_matching_rule_wins() {
        // Only text files, in any folder.
        let filter = rules(&["+ */", "+ *.txt", "- *"]);
        assert!(sent(&filter, "docs", true));
        assert!(sent(&filter, "docs/readme.txt", false));
        assert!(!sent(&filter, "docs/logo.png", false));

        let filter = build(FilterSpec {
            includes: vec!["keep.log".into()],
            excludes: vec!["*.log".into()],
            ..FilterSpec::default()
        });
        assert!(sent(&filter, "keep.log", false));
        assert!(!sent(&filter, "other.log", false));
    }

    #[test]
    fn triple_star_matches_a_folder_and_its_contents() {
        let filter = rules(&["+ /site/***", "- *"]);
        assert!(sent(&filter, "site", true));
        assert!(sent(&filter, "site/css/main.css", false));
        assert!(!sent(&filter, "other", true));
        assert!(!sent(&filter, "sitemap.xml", false));
    }

    #[test]
    fn double_star_prefix_and_infix_slashes() {
        let filter = excludes(&["**/tmp/*.o", "lib/*.a"]);
        assert!(!sent(&filter, "tmp/x.o", false));
        assert!(!sent(&filter, "a/b/tmp/x.o", false));
        assert!(!sent(&filter, "src/lib/z.a", false));
        assert!(sent(&filter, "lib/deep/z.a", false));
    }

    #[test]
    fn sides_decide_what_is_sent_and_what_is_kept() {
        let filter = rules(&["P /uploads/", "H secret.txt", "- *.cache"]);
        // Protected on the receiver only.
        assert!(sent(&filter, "uploads", true));
        assert!(kept(&filter, "uploads", true));
        // Hidden from the sender only, so the receiver may delete it.
        assert!(!sent(&filter, "secret.txt", false));
        assert!(!kept(&filter, "secret.txt", false));
        // Both sides.
        assert!(!sent(&filter, "a.cache", false));
        assert!(kept(&filter, "a.cache", false));

        // With --delete-excluded, rules without a side become the sender's.
        let filter = build(FilterSpec {
            rules: vec!["P /uploads/".into()],
            excludes: vec!["*.cache".into()],
            delete_excluded: true,
            ..FilterSpec::default()
        });
        assert!(!sent(&filter, "a.cache", false));
        assert!(!kept(&filter, "a.cache", false));
        assert!(kept(&filter, "uploads", true));
    }

    #[test]
    fn modifiers() {
        let filter = rules(&["-! */", "-sp *.tmp"]);
        // Negated: excludes everything that is not a folder.
        assert!(sent(&filter, "dir", true));
        assert!(!sent(&filter, "file", false));

        let filter = rules(&["exclude,s *.tmp", "-p *.o"]);
        assert!(!sent(&filter, "a.tmp", false));
        assert!(!kept(&filter, "a.tmp", false));
        assert_eq!(
            filter.verdict("x.o", false, Side::Receiver, &FolderRules::default()),
            Verdict::Excluded { perishable: true }
        );
    }

    #[test]
    fn clear_drops_the_rules_before_it() {
        let filter = rules(&["- *.log", "!", "- *.tmp"]);
        assert!(sent(&filter, "a.log", false));
        assert!(!sent(&filter, "a.tmp", false));
    }

    #[test]
    fn merge_files_are_read_in_place() {
        let read = |name: &str| -> AppResult<String> {
            match name {
                "/home/me/rules" => Ok("- *.bak\n# comment\n+ keep.tmp\n".into()),
                "/home/me/list" => Ok("*.tmp\n".into()),
                other => no_files(other),
            }
        };
        let filter = Filter::build(
            &FilterSpec {
                rules: vec![". /home/me/rules".into(), "merge,- /home/me/list".into()],
                ..FilterSpec::default()
            },
            &read,
        )
        .unwrap();
        assert!(!sent(&filter, "a.bak", false));
        assert!(sent(&filter, "keep.tmp", false));
        assert!(!sent(&filter, "other.tmp", false));
        assert!(Filter::build(
            &FilterSpec {
                rules: vec![". /missing".into()],
                ..FilterSpec::default()
            },
            &read,
        )
        .is_err());
    }

    #[test]
    fn per_folder_rule_files_apply_below_their_folder() {
        let filter = build(FilterSpec {
            per_folder_filters: true,
            exclude_filter_files: true,
            excludes: vec!["*.o".into()],
            ..FilterSpec::default()
        });
        assert_eq!(filter.merge_file_names(), [".rsync-filter"]);
        let root = filter
            .enter(
                &FolderRules::default(),
                "",
                &HashMap::from([(".rsync-filter".into(), "- *.tmp\n".into())]),
            )
            .unwrap();
        let src = filter
            .enter(
                &root,
                "src",
                &HashMap::from([(".rsync-filter".into(), "+ keep.tmp\n- /generated/\n".into())]),
            )
            .unwrap();
        let deep = filter.enter(&src, "src/deep", &HashMap::new()).unwrap();
        let verdict = |path: &str, is_dir: bool, rules: &FolderRules| {
            filter
                .verdict(path, is_dir, Side::Sender, rules)
                .is_included()
        };
        assert!(!verdict(".rsync-filter", false, &root));
        assert!(!verdict("a.tmp", false, &root));
        assert!(!verdict("src/a.tmp", false, &src));
        assert!(verdict("src/keep.tmp", false, &src));
        assert!(verdict("src/deep/keep.tmp", false, &deep));
        // Anchored to the folder holding the file.
        assert!(!verdict("src/generated", true, &src));
        assert!(verdict("generated", true, &root));
        assert!(!verdict("src/deep/x.o", false, &deep));
    }

    #[test]
    fn rules_kept_to_one_folder_stop_there() {
        let filter = rules(&[":n- .ignore"]);
        let root = filter
            .enter(
                &FolderRules::default(),
                "",
                &HashMap::from([(".ignore".into(), "*.tmp".into())]),
            )
            .unwrap();
        let child = filter.enter(&root, "child", &HashMap::new()).unwrap();
        let sent_in = |path: &str, rules: &FolderRules| {
            filter
                .verdict(path, false, Side::Sender, rules)
                .is_included()
        };
        assert!(!sent_in("a.tmp", &root));
        assert!(sent_in("child/a.tmp", &child));
    }

    #[test]
    fn cvs_exclude_skips_build_and_version_control_files() {
        let filter = build(FilterSpec {
            cvs_exclude: true,
            cvs_ignore: "*.swp".into(),
            ..FilterSpec::default()
        });
        assert!(!sent(&filter, ".git", true));
        assert!(!sent(&filter, "main.o", false));
        assert!(!sent(&filter, "notes.txt~", false));
        assert!(!sent(&filter, ".x.swp", false));
        assert!(sent(&filter, "main.c", false));
        assert_eq!(filter.merge_file_names(), [".cvsignore"]);
        let root = filter
            .enter(
                &FolderRules::default(),
                "",
                &HashMap::from([(".cvsignore".into(), "dist *.map".into())]),
            )
            .unwrap();
        assert!(!filter
            .verdict("dist", true, Side::Sender, &root)
            .is_included());
        assert!(!filter
            .verdict("app.js.map", false, Side::Sender, &root)
            .is_included());
        assert_eq!(
            filter.verdict("main.o", false, Side::Receiver, &root),
            Verdict::Excluded { perishable: true }
        );
    }

    #[test]
    fn files_from_lets_through_only_the_listed_paths() {
        let filter = build(FilterSpec {
            files_from: Some(vec![
                "./docs/guide.md".into(),
                "assets/".into(),
                "# comment".into(),
            ]),
            recursive: true,
            ..FilterSpec::default()
        });
        assert!(sent(&filter, "docs", true));
        assert!(sent(&filter, "docs/guide.md", false));
        assert!(!sent(&filter, "docs/other.md", false));
        assert!(sent(&filter, "assets", true));
        assert!(sent(&filter, "assets/img/logo.png", false));
        assert!(!sent(&filter, "readme.md", false));
        // Kept from deletion outside the list too.
        assert!(kept(&filter, "readme.md", false));
    }

    #[test]
    fn invalid_rules_are_refused() {
        for line in [
            "/",
            "file[",
            "x pattern",
            "-x *.o",
            "- ",
            "merge",
            ":",
            "!x",
        ] {
            let result = Filter::build(
                &FilterSpec {
                    rules: vec![line.to_string()],
                    ..FilterSpec::default()
                },
                &no_files,
            );
            assert!(result.is_err(), "{line} should be refused");
        }
        assert!(Filter::build(
            &FilterSpec {
                excludes: vec!["/".into()],
                ..FilterSpec::default()
            },
            &no_files
        )
        .is_err());
    }
}
