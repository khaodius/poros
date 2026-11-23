//! What a name has to look like to be found: plain text found anywhere in the name, a wildcard
//! pattern (`*` and `?`) for the whole name, or a regular expression.

use regex::{Regex, RegexBuilder};
use serde::Deserialize;

use crate::error::{AppError, AppResult};

const MAX_PATTERN_LENGTH: usize = 1024;
/// Keeps a hostile regular expression from taking much memory once compiled.
const REGEX_SIZE_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatternKind {
    /// A wildcard pattern when it holds `*` or `?`, otherwise text found anywhere in the name.
    #[default]
    Text,
    Regex,
}

#[derive(Debug, Clone)]
pub struct NamePattern {
    /// `None` matches every name.
    matcher: Option<Regex>,
    /// What `find -name` is given; see [`NamePattern::find_pattern`].
    find_pattern: Option<String>,
    case_sensitive: bool,
}

impl NamePattern {
    pub fn new(pattern: &str, kind: PatternKind, case_sensitive: bool) -> AppResult<Self> {
        if pattern.len() > MAX_PATTERN_LENGTH {
            return Err(AppError::invalid("The name to search for is too long"));
        }
        if pattern.contains('\0') {
            return Err(AppError::invalid("A name cannot hold a NUL character"));
        }
        if pattern.is_empty() {
            return Ok(Self {
                matcher: None,
                find_pattern: None,
                case_sensitive,
            });
        }
        let (expression, find_pattern) = match kind {
            PatternKind::Regex => (pattern.to_string(), None),
            PatternKind::Text => {
                let wildcards = has_wildcards(pattern);
                let tokens = tokens(pattern);
                (
                    text_expression(&tokens, wildcards, case_sensitive),
                    Some(find_pattern(&tokens, wildcards, case_sensitive)),
                )
            }
        };
        let matcher = RegexBuilder::new(&expression)
            // Text patterns spell out both cases themselves, so that names match here exactly
            // as far as the server's `find` can tell apart.
            .case_insensitive(kind == PatternKind::Regex && !case_sensitive)
            .dot_matches_new_line(true)
            .size_limit(REGEX_SIZE_LIMIT)
            .build()
            .map_err(|error| AppError::invalid(regex_problem(&error)))?;
        Ok(Self {
            matcher: Some(matcher),
            find_pattern,
            case_sensitive,
        })
    }

    pub fn matches(&self, name: &str) -> bool {
        self.matcher
            .as_ref()
            .is_none_or(|matcher| matcher.is_match(name))
    }

    /// A pattern for `find -name` (or `-iname`) in the C locale that matches every name this
    /// pattern matches, and possibly more: `find` narrows down what the server sends and
    /// [`NamePattern::matches`] decides. `None` when `find` cannot narrow anything down.
    pub fn find_pattern(&self) -> Option<&str> {
        self.find_pattern.as_deref()
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    /// `*`: any run of characters, also none.
    Any,
    /// `?`: exactly one character.
    One,
    Literal(char),
}

fn has_wildcards(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

fn tokens(pattern: &str) -> Vec<Token> {
    let wildcards = has_wildcards(pattern);
    pattern
        .chars()
        .map(|character| match character {
            '*' if wildcards => Token::Any,
            '?' if wildcards => Token::One,
            character => Token::Literal(character),
        })
        .collect()
}

fn text_expression(tokens: &[Token], whole_name: bool, case_sensitive: bool) -> String {
    let mut expression = String::new();
    if whole_name {
        expression.push('^');
    }
    for token in tokens {
        match token {
            Token::Any => expression.push_str(".*"),
            Token::One => expression.push('.'),
            Token::Literal(character) => {
                push_literal(&mut expression, *character, case_sensitive);
            }
        }
    }
    if whole_name {
        expression.push('$');
    }
    expression
}

fn push_literal(expression: &mut String, character: char, case_sensitive: bool) {
    let mut buffer = [0; 4];
    let escaped = regex::escape(character.encode_utf8(&mut buffer));
    match other_case(character).filter(|_| !case_sensitive) {
        Some(other) => {
            expression.push_str("(?:");
            expression.push_str(&escaped);
            expression.push('|');
            expression.push_str(&regex::escape(other.encode_utf8(&mut buffer)));
            expression.push(')');
        }
        None => expression.push_str(&escaped),
    }
}

/// The same letter in the other case, for letters whose other case is one character.
fn other_case(character: char) -> Option<char> {
    fn single(cases: &mut dyn Iterator<Item = char>) -> Option<char> {
        let first = cases.next()?;
        cases.next().is_none().then_some(first)
    }
    [
        single(&mut character.to_lowercase()),
        single(&mut character.to_uppercase()),
    ]
    .into_iter()
    .flatten()
    .find(|other| *other != character)
}

/// `find` runs in the C locale, where a character outside ASCII is several bytes and letters
/// outside ASCII have no other case, so those become "one or more bytes".
fn find_pattern(tokens: &[Token], whole_name: bool, case_sensitive: bool) -> String {
    const SOME_BYTES: &str = "?*";
    let mut pattern = String::new();
    if !whole_name {
        pattern.push('*');
    }
    for token in tokens {
        match token {
            Token::Any => pattern.push('*'),
            Token::One => pattern.push_str(SOME_BYTES),
            Token::Literal(character) if !character.is_ascii() && !case_sensitive => {
                pattern.push_str(SOME_BYTES);
            }
            Token::Literal(character) => {
                if matches!(character, '\\' | '*' | '?' | '[') {
                    pattern.push('\\');
                }
                pattern.push(*character);
            }
        }
    }
    if !whole_name {
        pattern.push('*');
    }
    pattern
}

fn regex_problem(error: &regex::Error) -> String {
    match error {
        regex::Error::CompiledTooBig(_) => "That regular expression is too large".to_string(),
        // The last line says what is wrong; the lines before it draw the expression.
        other => {
            let text = other.to_string();
            let reason = text
                .lines()
                .last()
                .unwrap_or_default()
                .trim()
                .trim_start_matches("error: ");
            format!("That is not a valid regular expression: {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(pattern: &str, case_sensitive: bool) -> NamePattern {
        NamePattern::new(pattern, PatternKind::Text, case_sensitive).unwrap()
    }

    #[test]
    fn an_empty_pattern_matches_every_name() {
        let pattern = text("", false);
        assert!(pattern.matches("anything"));
        assert!(pattern.matches(""));
        assert_eq!(pattern.find_pattern(), None);
    }

    #[test]
    fn plain_text_is_found_anywhere_in_the_name() {
        let pattern = text("port", false);
        assert!(pattern.matches("report.pdf"));
        assert!(pattern.matches("Portfolio"));
        assert!(!pattern.matches("notes.txt"));
        assert_eq!(pattern.find_pattern(), Some("*port*"));
    }

    #[test]
    fn wildcards_match_the_whole_name() {
        let pattern = text("*.log", false);
        assert!(pattern.matches("server.log"));
        assert!(pattern.matches("SERVER.LOG"));
        assert!(pattern.matches(".log"));
        assert!(!pattern.matches("server.log.1"));
        assert!(!pattern.matches("catalog"));

        let pattern = text("report-??.pdf", false);
        assert!(pattern.matches("report-01.pdf"));
        assert!(!pattern.matches("report-1.pdf"));
        assert!(!pattern.matches("report-001.pdf"));
    }

    #[test]
    fn case_matters_only_when_asked() {
        assert!(text("readme", false).matches("README.md"));
        assert!(!text("readme", true).matches("README.md"));
        assert!(text("README", true).matches("README.md"));
        assert!(text("*.JPG", false).matches("photo.jpg"));
        assert!(!text("*.JPG", true).matches("photo.jpg"));
    }

    #[test]
    fn letters_outside_ascii_fold_case_too() {
        assert!(text("ÉTÉ", false).matches("été 2024"));
        assert!(text("größe", false).matches("GRÖßE.txt"));
        assert!(!text("ÉTÉ", true).matches("été 2024"));
    }

    #[test]
    fn everything_but_the_wildcards_is_literal() {
        let pattern = text("a.b", false);
        assert!(pattern.matches("xa.by"));
        assert!(!pattern.matches("xaXby"));

        let pattern = text("report[1](final)+$", true);
        assert!(pattern.matches("old report[1](final)+$.txt"));
        assert!(!pattern.matches("report1final"));

        let pattern = text("[draft]*", true);
        assert!(pattern.matches("[draft] plan.txt"));
        assert!(!pattern.matches("d plan.txt"));
    }

    #[test]
    fn names_with_line_breaks_still_match() {
        assert!(text("*.log", false).matches("two\nlines.log"));
        assert!(text("two?lines*", false).matches("two\nlines.log"));
        assert!(text("lines", false).matches("two\nlines.log"));
    }

    #[test]
    fn regular_expressions_search_within_the_name() {
        let pattern = NamePattern::new(r"^IMG_\d{4}\.(jpe?g|png)$", PatternKind::Regex, true);
        let pattern = pattern.unwrap();
        assert!(pattern.matches("IMG_0042.jpeg"));
        assert!(!pattern.matches("img_0042.jpeg"));
        assert!(!pattern.matches("IMG_42.png"));
        assert_eq!(pattern.find_pattern(), None);

        let pattern = NamePattern::new(r"\.bak$", PatternKind::Regex, false).unwrap();
        assert!(pattern.matches("old.BAK"));
        assert!(!pattern.matches("old.bak.txt"));
    }

    #[test]
    fn a_broken_regular_expression_is_explained() {
        let error = NamePattern::new("(unclosed", PatternKind::Regex, false).unwrap_err();
        assert_eq!(error.kind, crate::error::ErrorKind::InvalidInput);
        assert!(error
            .message
            .starts_with("That is not a valid regular expression"));
        assert!(!error.message.contains('\n'));
    }

    #[test]
    fn patterns_that_cannot_be_names_are_refused() {
        assert!(NamePattern::new("a\0b", PatternKind::Text, false).is_err());
        let long = "a".repeat(MAX_PATTERN_LENGTH + 1);
        assert!(NamePattern::new(&long, PatternKind::Text, false).is_err());
    }

    #[test]
    fn find_gets_literals_escaped() {
        assert_eq!(text("a*b?c", true).find_pattern(), Some("a*b?*c"));
        assert_eq!(text(r"[x]\y", true).find_pattern(), Some(r"*\[x]\\y*"));
        assert_eq!(text("-name", true).find_pattern(), Some("*-name*"));
        assert_eq!(text("it's", true).find_pattern(), Some("*it's*"));
    }

    #[test]
    fn find_gets_a_wider_pattern_for_letters_it_cannot_fold() {
        assert_eq!(text("été", true).find_pattern(), Some("*été*"));
        assert_eq!(text("été", false).find_pattern(), Some("*?*t?**"));
        assert_eq!(text("*.Ärger", false).find_pattern(), Some("*.?*rger"));
    }

    /// What `fnmatch` does in the C locale without flags, enough to check the wider patterns.
    fn fnmatch(pattern: &[u8], name: &[u8], fold: bool) -> bool {
        match pattern.split_first() {
            None => name.is_empty(),
            Some((b'*', rest)) => (0..=name.len()).any(|skip| fnmatch(rest, &name[skip..], fold)),
            Some((b'?', rest)) => !name.is_empty() && fnmatch(rest, &name[1..], fold),
            Some((b'\\', rest)) if !rest.is_empty() => {
                name.first() == rest.first() && fnmatch(&rest[1..], &name[1..], fold)
            }
            Some((byte, rest)) => {
                let same = |other: &u8| other == byte || (fold && other.eq_ignore_ascii_case(byte));
                name.first().is_some_and(same) && fnmatch(rest, &name[1..], fold)
            }
        }
    }

    #[test]
    fn whatever_matches_here_also_matches_on_the_server() {
        let patterns = [
            "report",
            "*.log",
            "été",
            "Ärger*",
            "?",
            "??.txt",
            "a?c",
            "[x]",
            r"back\slash",
            "ß",
            "İ",
            "k",
            "naïve*file?",
            "*",
            "日本",
            "?本*",
        ];
        let names = [
            "report.pdf",
            "REPORT",
            "server.log",
            "x.LOG",
            "ÉTÉ.txt",
            "été",
            "ärger.doc",
            "ÄRGER",
            "é",
            "a",
            "ab.txt",
            "éé.txt",
            "abc",
            "aéc",
            "[x]",
            "[X] y",
            r"back\slash",
            "ß",
            "SS",
            "i",
            "İ",
            "K",
            "k",
            "NAÏVE-file1",
            "日本語",
            "two\nlines.log",
            ".hidden",
            "-rf",
            "it's",
            "",
        ];
        for pattern in patterns {
            for case_sensitive in [true, false] {
                let compiled = text(pattern, case_sensitive);
                let on_server = compiled.find_pattern().unwrap();
                for name in names {
                    if compiled.matches(name) {
                        assert!(
                            fnmatch(on_server.as_bytes(), name.as_bytes(), !case_sensitive),
                            "{pattern:?} (case sensitive: {case_sensitive}) matches {name:?} \
                             here but {on_server:?} does not match it on the server"
                        );
                    }
                }
            }
        }
    }
}
