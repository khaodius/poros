//! The OpenSSH client config (`~/.ssh/config`): every `Host` alias without wildcards becomes a
//! site, with the settings OpenSSH would use for it, `Include` files and `ProxyJump` chains
//! included.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use super::{parse_port, split_user_host, Candidate, Input, Parsed};
use crate::connections::AuthType;
use crate::protocol::Protocol;

/// How deep `Include` files may nest, as in OpenSSH.
const MAX_INCLUDE_DEPTH: usize = 16;

pub fn looks_like_config(text: &str) -> bool {
    text.lines().any(|line| {
        let words = words(line);
        matches!(words.first(), Some(keyword) if keyword.eq_ignore_ascii_case("host")
            || keyword.eq_ignore_ascii_case("match"))
            && words.len() > 1
    })
}

/// The settings that follow a `Host` or `Match` line.
#[derive(Debug, Default)]
struct Block {
    /// `None` for the lines before the first `Host`, which apply to every host; empty for a
    /// `Match`, whose conditions are not evaluated here.
    patterns: Option<Vec<String>>,
    options: Vec<(String, Vec<String>)>,
}

impl Block {
    fn applies_to(&self, alias: &str) -> bool {
        let Some(patterns) = &self.patterns else {
            return true;
        };
        let mut matched = false;
        for pattern in patterns {
            if let Some(negated) = pattern.strip_prefix('!') {
                if wildcard_match(negated, alias) {
                    return false;
                }
            } else if wildcard_match(pattern, alias) {
                matched = true;
            }
        }
        matched
    }
}

struct Config {
    blocks: Vec<Block>,
    base: Option<PathBuf>,
    warnings: Vec<String>,
}

impl Config {
    fn read(&mut self, text: &str, depth: usize) {
        for line in text.lines() {
            let words = words(line);
            let Some((keyword, arguments)) = words.split_first() else {
                continue;
            };
            let keyword = keyword.to_ascii_lowercase();
            match keyword.as_str() {
                "host" => self.blocks.push(Block {
                    patterns: Some(arguments.to_vec()),
                    options: Vec::new(),
                }),
                "match" => {
                    let all = arguments.len() == 1 && arguments[0].eq_ignore_ascii_case("all");
                    self.blocks.push(Block {
                        patterns: if all { None } else { Some(Vec::new()) },
                        options: Vec::new(),
                    });
                }
                "include" => {
                    let patterns = self.blocks.last().and_then(|block| block.patterns.clone());
                    for argument in arguments {
                        self.include(argument, depth);
                    }
                    // Lines after the Include belong to the block it was in again.
                    self.blocks.push(Block {
                        patterns,
                        options: Vec::new(),
                    });
                }
                _ => {
                    if let Some(block) = self.blocks.last_mut() {
                        block.options.push((keyword, arguments.to_vec()));
                    }
                }
            }
        }
    }

    fn include(&mut self, pattern: &str, depth: usize) {
        if depth >= MAX_INCLUDE_DEPTH {
            self.warnings
                .push("Include files nest too deeply; the deepest were left out.".to_string());
            return;
        }
        let expanded = expand_home(pattern);
        let path = match (&self.base, Path::new(&expanded).is_absolute()) {
            (Some(base), false) => base.join(&expanded),
            _ => PathBuf::from(&expanded),
        };
        for file in glob(&path) {
            match std::fs::read(&file) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    self.read(&text, depth + 1);
                }
                Err(error) => self
                    .warnings
                    .push(format!("{} could not be read: {error}", file.display())),
            }
        }
    }

    /// The settings OpenSSH uses for `alias`: the first value of each, in file order.
    fn settings_for(&self, alias: &str) -> HashMap<String, Vec<String>> {
        let mut settings: HashMap<String, Vec<String>> = HashMap::new();
        for block in self.blocks.iter().filter(|block| block.applies_to(alias)) {
            for (keyword, arguments) in &block.options {
                settings
                    .entry(keyword.clone())
                    .or_insert_with(|| arguments.clone());
            }
        }
        settings
    }

    fn aliases(&self) -> Vec<String> {
        let mut aliases: Vec<String> = Vec::new();
        for block in &self.blocks {
            for pattern in block.patterns.iter().flatten() {
                let concrete = !pattern.contains(['*', '?', '!']);
                if concrete
                    && !aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(pattern))
                {
                    aliases.push(pattern.clone());
                }
            }
        }
        aliases
    }

    /// A site for `alias`, with `user` and `port` in place of the configured ones when given,
    /// and its `ProxyJump` setting.
    fn candidate(
        &self,
        name: &str,
        alias: &str,
        user: Option<&str>,
        port: Option<u16>,
    ) -> (Candidate, Option<String>) {
        let settings = self.settings_for(alias);
        let first = |keyword: &str| {
            settings
                .get(keyword)
                .and_then(|arguments| arguments.first())
                .map(String::as_str)
        };
        let host = first("hostname")
            .map(|host| expand_tokens(host, alias, "", alias))
            .unwrap_or_else(|| alias.to_string());
        let username = user
            .map(str::to_string)
            .or_else(|| first("user").map(str::to_string))
            .unwrap_or_else(local_user);
        let port = port.or_else(|| first("port").and_then(parse_port));
        let mut candidate = Candidate::new(Protocol::Sftp, name, &host, port, &username);
        match first("identityfile") {
            Some(path) if !path.eq_ignore_ascii_case("none") => {
                candidate.use_key_file(&expand_tokens(path, &host, &username, alias));
            }
            _ => candidate.connection.auth_type = AuthType::Agent,
        }
        if first("proxycommand").is_some_and(|command| !command.eq_ignore_ascii_case("none")) {
            candidate
                .note("OpenSSH reaches this host with a ProxyCommand, which Poros does not run.");
        }
        let jump = first("proxyjump")
            .filter(|jump| !jump.eq_ignore_ascii_case("none"))
            .map(str::to_string);
        (candidate, jump)
    }
}

pub fn parse(input: &Input) -> Parsed {
    let mut config = Config {
        blocks: vec![Block::default()],
        base: input
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf),
        warnings: Vec::new(),
    };
    config.read(&input.text, 0);
    let mut parsed = Parsed::default();
    for warning in std::mem::take(&mut config.warnings) {
        parsed.warn(warning);
    }

    let mut pending = VecDeque::new();
    let mut alias_index: HashMap<String, usize> = HashMap::new();
    for alias in config.aliases() {
        let (candidate, jump) = config.candidate(&alias, &alias, None, None);
        alias_index.insert(alias.to_ascii_lowercase(), parsed.candidates.len());
        if let Some(jump) = jump {
            pending.push_back((parsed.candidates.len(), jump));
        }
        parsed.candidates.push(candidate);
    }

    while let Some((index, chain)) = pending.pop_front() {
        let mut previous: Option<usize> = None;
        for hop in chain
            .split(',')
            .map(str::trim)
            .filter(|hop| !hop.is_empty())
        {
            let (user, host, port) = parse_hop(hop);
            let hop_index = match (previous, alias_index.get(&host.to_ascii_lowercase())) {
                // The first hop is reached as OpenSSH would reach it on its own.
                (None, Some(&alias)) if user.is_none() && port.is_none() => alias,
                _ => {
                    let name = match previous {
                        Some(previous) => {
                            format!("{hop} via {}", parsed.candidates[previous].connection.name)
                        }
                        None => hop.to_string(),
                    };
                    let (mut candidate, own_jump) = config.candidate(&name, host, user, port);
                    candidate.jump = previous;
                    let before = parsed.candidates.len();
                    let added = parsed.jump_host(candidate);
                    if added == before && previous.is_none() {
                        if let Some(own_jump) = own_jump {
                            pending.push_back((added, own_jump));
                        }
                    }
                    added
                }
            };
            previous = Some(hop_index);
        }
        if let Some(jump) = previous {
            if !parsed.link_jump(index, jump) {
                parsed.candidates[index]
                    .note("Its ProxyJump goes round in a circle, so it connects directly.");
            }
        }
    }
    parsed
}

/// `[user@]host[:port]`, or the same as an `ssh://` address.
fn parse_hop(hop: &str) -> (Option<&str>, &str, Option<u16>) {
    let hop = hop.strip_prefix("ssh://").unwrap_or(hop);
    let (user, address) = split_user_host(hop);
    let user = (!user.is_empty()).then_some(user);
    if let Some(rest) = address.strip_prefix('[') {
        if let Some((host, after)) = rest.split_once(']') {
            return (user, host, after.strip_prefix(':').and_then(parse_port));
        }
    }
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (user, host, parse_port(port)),
        _ => (user, address, None),
    }
}

/// The words of a config line: the keyword (which `=` may follow) and its arguments, with
/// quotes grouping words and `#` starting a comment.
fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quoted = false;
    for character in line.trim().chars() {
        match character {
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            '#' if !quoted && !in_word => break,
            // `=` may separate the keyword from its arguments, with or without spaces.
            character
                if !quoted
                    && (character.is_whitespace()
                        || (character == '='
                            && (words.is_empty() || (words.len() == 1 && !in_word)))) =>
            {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            character => {
                current.push(character);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

/// `*` and `?` wildcards, ignoring case, as OpenSSH matches host names.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pattern_index, mut text_index) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while text_index < text.len() {
        match pattern.get(pattern_index) {
            Some('*') => {
                backtrack = Some((pattern_index, text_index));
                pattern_index += 1;
            }
            Some(&character) if character == '?' || character == text[text_index] => {
                pattern_index += 1;
                text_index += 1;
            }
            _ => match backtrack {
                Some((star, matched)) => {
                    pattern_index = star + 1;
                    text_index = matched + 1;
                    backtrack = Some((star, matched + 1));
                }
                None => return false,
            },
        }
    }
    pattern[pattern_index..]
        .iter()
        .all(|character| *character == '*')
}

/// The files `path` names, with `*` and `?` in its last part matched against a folder.
fn glob(path: &Path) -> Vec<PathBuf> {
    let file_pattern = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !file_pattern.contains(['*', '?']) {
        return if path.is_file() {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let Some(folder) = path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| wildcard_match(&file_pattern, &entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

/// OpenSSH's `%` tokens in host names and key paths: `%h` host, `%r` remote user, `%n` the
/// alias as typed, `%u` local user, `%d` home folder, `%%` a percent sign.
fn expand_tokens(text: &str, host: &str, remote_user: &str, alias: &str) -> String {
    let mut expanded = String::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            expanded.push(character);
            continue;
        }
        match characters.next() {
            Some('h') => expanded.push_str(host),
            Some('r') => expanded.push_str(remote_user),
            Some('n') => expanded.push_str(alias),
            Some('u') => expanded.push_str(&local_user()),
            Some('d') => expanded.push_str(
                &dirs::home_dir()
                    .map(|home| home.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            Some('%') => expanded.push('%'),
            Some(other) => {
                expanded.push('%');
                expanded.push(other);
            }
            None => expanded.push('%'),
        }
    }
    expand_home(&expanded)
}

/// OpenSSH logs in as the local user when the config names none.
fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(text: &str) -> Parsed {
        parse(&Input::from_text("config", text))
    }

    #[test]
    fn splits_lines_into_words() {
        assert_eq!(words("  Host web db  # comment"), vec!["Host", "web", "db"]);
        assert_eq!(words("Port=2222"), vec!["Port", "2222"]);
        assert_eq!(words("Port = 2222"), vec!["Port", "2222"]);
        assert_eq!(
            words("IdentityFile \"~/my keys/id\""),
            vec!["IdentityFile", "~/my keys/id"]
        );
        assert!(words("# only a comment").is_empty());
    }

    #[test]
    fn matches_wildcards() {
        assert!(wildcard_match("*.example.com", "WEB.example.com"));
        assert!(wildcard_match("db?", "db1"));
        assert!(!wildcard_match("db?", "db12"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("a*b*c", "aXXbYY"));
    }

    #[test]
    fn applies_settings_the_way_openssh_does() {
        let parsed = parse_text(
            "User fallback\n\
             Host web web-alias\n  HostName %h.example.com\n  Port 2222\n  IdentityFile /keys/web\n\
             Host db\n  HostName 10.0.0.5\n  User postgres\n\
             Host *.example.com !secret.example.com\n  User admin\n  Port 2200\n\
             Host *\n  User ignored\n  IdentityFile none\n\
             Host secret.example.com\n  ProxyCommand nc %h %p\n",
        );
        let names: Vec<&str> = parsed
            .candidates
            .iter()
            .map(|site| site.connection.name.as_str())
            .collect();
        assert_eq!(names, vec!["web", "web-alias", "db", "secret.example.com"]);
        let web = &parsed.candidates[0].connection;
        assert_eq!(
            (web.host.as_str(), web.port, web.username.as_str()),
            ("web.example.com", 2222, "fallback")
        );
        assert_eq!(web.auth_type, AuthType::PublicKey);
        assert_eq!(web.key_path.as_deref(), Some("/keys/web"));
        assert_eq!(
            parsed.candidates[1].connection.host,
            "web-alias.example.com"
        );
        let db = &parsed.candidates[2].connection;
        assert_eq!((db.host.as_str(), db.port), ("10.0.0.5", 22));
        assert_eq!(db.username, "fallback");
        assert_eq!(db.auth_type, AuthType::Agent);
        let secret = &parsed.candidates[3];
        assert_eq!(secret.connection.port, 22);
        assert_eq!(secret.notes.len(), 1);
    }

    #[test]
    fn links_proxy_jump_chains() {
        let parsed = parse_text(
            "Host bastion\n  HostName bastion.example.com\n  User jump\n\
             Host app\n  HostName 10.1.0.2\n  User deploy\n  ProxyJump bastion\n\
             Host deep\n  HostName 10.2.0.2\n  User deploy\n  ProxyJump ops@gate.example.com:2222,bastion\n\
             Host *.example.com\n  User fromwildcard\n\
             Host loop-a\n  ProxyJump loop-b\n\
             Host loop-b\n  ProxyJump loop-a\n",
        );
        let find = |name: &str| {
            parsed
                .candidates
                .iter()
                .position(|site| site.connection.name == name)
                .unwrap()
        };
        let bastion = find("bastion");
        assert_eq!(parsed.candidates[find("app")].jump, Some(bastion));
        assert_eq!(parsed.candidates[bastion].jump, None);

        let deep_jump = parsed.candidates[find("deep")].jump.unwrap();
        assert_ne!(deep_jump, bastion);
        let second_bastion = &parsed.candidates[deep_jump];
        assert_eq!(second_bastion.connection.host, "bastion.example.com");
        assert_eq!(
            second_bastion.connection.name,
            "bastion via ops@gate.example.com:2222"
        );
        let gate = &parsed.candidates[second_bastion.jump.unwrap()];
        assert_eq!(
            (
                gate.connection.host.as_str(),
                gate.connection.port,
                gate.connection.username.as_str()
            ),
            ("gate.example.com", 2222, "ops")
        );

        let loop_a = &parsed.candidates[find("loop-a")];
        let loop_b = &parsed.candidates[find("loop-b")];
        assert!(loop_a.jump.is_none() || loop_b.jump.is_none());
    }

    #[test]
    fn follows_include_files() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("config.d")).unwrap();
        std::fs::write(
            folder.path().join("config.d/work.conf"),
            "Host work\n  HostName work.example.com\n",
        )
        .unwrap();
        std::fs::write(folder.path().join("config.d/skip.txt"), "Host nope\n").unwrap();
        let config = folder.path().join("config");
        std::fs::write(
            &config,
            "Include config.d/*.conf\nHost home\n  HostName home.lan\n",
        )
        .unwrap();
        let mut input = Input::from_text("config", std::fs::read_to_string(&config).unwrap());
        input.path = Some(config);
        let parsed = parse(&input);
        let names: Vec<&str> = parsed
            .candidates
            .iter()
            .map(|site| site.connection.name.as_str())
            .collect();
        assert_eq!(names, vec!["work", "home"]);
    }

    #[test]
    fn reads_jump_addresses() {
        assert_eq!(parse_hop("host"), (None, "host", None));
        assert_eq!(parse_hop("u@host:2222"), (Some("u"), "host", Some(2222)));
        assert_eq!(parse_hop("ssh://u@[::1]:22"), (Some("u"), "::1", Some(22)));
        assert_eq!(parse_hop("fe80::1"), (None, "fe80::1", None));
    }
}
