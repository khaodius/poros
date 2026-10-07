//! Exclude patterns in the style of rsync. `*` and `?` stay within one path component, `**`
//! crosses them (`a/**/b` also matches `a/b`), and `[...]` matches a set of characters. A leading `/` anchors a pattern to
//! the synchronized folder; otherwise it matches the end of a path, so a plain name matches at
//! any depth. A trailing `/` matches folders only.

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default)]
pub struct Filter {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
struct Rule {
    pattern: Vec<char>,
    anchored: bool,
    folders_only: bool,
}

impl Filter {
    /// One pattern per entry; blank entries and `#` comments are ignored.
    pub fn new(patterns: &[String]) -> AppResult<Self> {
        let mut rules = Vec::new();
        for line in patterns {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let folders_only = line.ends_with('/');
            let trimmed = line.trim_end_matches('/');
            let anchored = trimmed.starts_with('/');
            let pattern: Vec<char> = trimmed.trim_start_matches('/').chars().collect();
            if pattern.is_empty() {
                return Err(AppError::invalid(format!(
                    "\"{line}\" would exclude everything"
                )));
            }
            if !balanced_brackets(&pattern) {
                return Err(AppError::invalid(format!("\"{line}\" has an unclosed [")));
            }
            rules.push(Rule {
                pattern,
                anchored,
                folders_only,
            });
        }
        Ok(Self { rules })
    }

    /// `path` is relative to the synchronized folder, with `/` between components.
    pub fn excludes(&self, path: &str, is_dir: bool) -> bool {
        let path: Vec<char> = path.chars().collect();
        self.rules
            .iter()
            .any(|rule| (is_dir || !rule.folders_only) && rule.matches(&path))
    }
}

impl Rule {
    fn matches(&self, path: &[char]) -> bool {
        if self.anchored {
            return glob(&self.pattern, path);
        }
        // Unanchored patterns match any tail of the path that starts at a component.
        std::iter::once(0)
            .chain(
                path.iter()
                    .enumerate()
                    .filter(|(_, character)| **character == '/')
                    .map(|(index, _)| index + 1),
            )
            .any(|start| glob(&self.pattern, &path[start..]))
    }
}

fn glob(pattern: &[char], text: &[char]) -> bool {
    match pattern.first() {
        None => text.is_empty(),
        Some('*') if pattern.get(1) == Some(&'*') => {
            let rest = &pattern[2..];
            // `a/**/b` also matches `a/b`.
            let no_folders = rest.first() == Some(&'/') && glob(&rest[1..], text);
            no_folders || (0..=text.len()).any(|skip| glob(rest, &text[skip..]))
        }
        Some('*') => {
            let rest = &pattern[1..];
            let component = text.iter().position(|&c| c == '/').unwrap_or(text.len());
            (0..=component).any(|skip| glob(rest, &text[skip..]))
        }
        Some('?') => text.first().is_some_and(|&c| c != '/') && glob(&pattern[1..], &text[1..]),
        Some('[') => {
            let Some((set, rest)) = character_set(&pattern[1..]) else {
                return false;
            };
            text.first().is_some_and(|&c| c != '/' && set.contains(c)) && glob(rest, &text[1..])
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

/// The set after a `[`, and the pattern after its `]`.
fn character_set(pattern: &[char]) -> Option<(CharacterSet<'_>, &[char])> {
    let negated = matches!(pattern.first(), Some('!' | '^'));
    let start = usize::from(negated);
    // A `]` right after the opening bracket is a member, not the end.
    let end = pattern
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, character)| **character == ']')
        .map(|(index, _)| index)?;
    Some((
        CharacterSet {
            negated,
            items: &pattern[start..end],
        },
        &pattern[end + 1..],
    ))
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

    fn filter(patterns: &[&str]) -> Filter {
        let patterns: Vec<String> = patterns.iter().map(|pattern| pattern.to_string()).collect();
        Filter::new(&patterns).unwrap()
    }

    #[test]
    fn plain_names_match_at_any_depth() {
        let rules = filter(&["node_modules", "*.log"]);
        assert!(rules.excludes("node_modules", true));
        assert!(rules.excludes("web/node_modules", true));
        assert!(rules.excludes("logs/today.log", false));
        assert!(!rules.excludes("node_modules_backup", true));
        assert!(!rules.excludes("today.log.gz", false));
    }

    #[test]
    fn anchored_and_folder_patterns() {
        let rules = filter(&["/build", "cache/", "docs/*.tmp"]);
        assert!(rules.excludes("build", true));
        assert!(!rules.excludes("src/build", true));
        assert!(rules.excludes("cache", true));
        assert!(!rules.excludes("cache", false));
        assert!(rules.excludes("docs/a.tmp", false));
        assert!(rules.excludes("site/docs/a.tmp", false));
        assert!(!rules.excludes("docs/sub/a.tmp", false));
    }

    #[test]
    fn wildcards_and_sets() {
        let rules = filter(&["/a/**/z", "file?.[ch]", "[!.]*.bak", "# a comment", ""]);
        assert!(rules.excludes("a/z", false));
        assert!(rules.excludes("a/b/c/z", false));
        assert!(rules.excludes("file1.c", false));
        assert!(rules.excludes("deep/fileX.h", false));
        assert!(!rules.excludes("file10.c", false));
        assert!(rules.excludes("old.bak", false));
        assert!(!rules.excludes(".old.bak", false));
    }

    #[test]
    fn invalid_patterns_are_refused() {
        assert!(Filter::new(&["/".to_string()]).is_err());
        assert!(Filter::new(&["file[".to_string()]).is_err());
    }
}
