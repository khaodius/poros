//! `--chmod` rules as rsync reads them: comma-separated items, each for folders (`D`), files
//! (`F`) or both, either an octal mode or chmod's symbolic form (`u+rwx`, `go-w`, `a+X`).

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Applies {
    Both,
    Folders,
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Octal(u32),
    Symbolic {
        /// Permission bits the `ugoa` letters cover.
        who: u32,
        operator: char,
        bits: u32,
        /// `X`: execute for folders, and for files that some class can already execute.
        conditional_execute: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rule {
    applies: Applies,
    change: Change,
}

/// Parsed `--chmod` rules, applied in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chmod {
    rules: Vec<Rule>,
}

const USER: u32 = 0o4700;
const GROUP: u32 = 0o2070;
const OTHER: u32 = 0o1007;

impl Chmod {
    pub fn parse(text: &str) -> AppResult<Self> {
        let mut rules = Vec::new();
        for item in text
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let (applies, rest) = match item.as_bytes()[0] {
                b'D' => (Applies::Folders, &item[1..]),
                b'F' => (Applies::Files, &item[1..]),
                _ => (Applies::Both, item),
            };
            for change in parse_item(rest)
                .map_err(|message| AppError::invalid(format!("\"{item}\": {message}")))?
            {
                rules.push(Rule { applies, change });
            }
        }
        Ok(Self { rules })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// `mode` with the rules applied, for a folder or a file.
    pub fn apply(&self, mode: u32, is_dir: bool) -> u32 {
        let mut mode = mode & 0o7777;
        for rule in &self.rules {
            let applies = match rule.applies {
                Applies::Both => true,
                Applies::Folders => is_dir,
                Applies::Files => !is_dir,
            };
            if !applies {
                continue;
            }
            mode = match rule.change {
                Change::Octal(octal) => octal,
                Change::Symbolic {
                    who,
                    operator,
                    bits,
                    conditional_execute,
                } => {
                    let mut bits = bits;
                    if conditional_execute && (is_dir || mode & 0o111 != 0) {
                        bits |= 0o111;
                    }
                    let bits = bits & who;
                    match operator {
                        '+' => mode | bits,
                        '-' => mode & !bits,
                        // `=` leaves the set-id and sticky bits of the classes it names only
                        // when they are given again.
                        _ => (mode & !who) | bits,
                    }
                }
            };
        }
        mode
    }
}

fn parse_item(item: &str) -> Result<Vec<Change>, String> {
    if item.is_empty() {
        return Err("missing a mode".into());
    }
    if item.chars().all(|character| character.is_digit(8)) {
        let octal = u32::from_str_radix(item, 8).map_err(|_| "is not an octal mode")?;
        if octal > 0o7777 {
            return Err("is larger than 7777".into());
        }
        return Ok(vec![Change::Octal(octal)]);
    }
    let mut characters = item.chars().peekable();
    let mut who = 0;
    while let Some(&character) = characters.peek() {
        who |= match character {
            'u' => USER,
            'g' => GROUP,
            'o' => OTHER,
            'a' => USER | GROUP | OTHER,
            _ => break,
        };
        characters.next();
    }
    if who == 0 {
        who = USER | GROUP | OTHER;
    }
    let mut changes = Vec::new();
    while let Some(operator) = characters.next() {
        if !matches!(operator, '+' | '-' | '=') {
            return Err(format!("expected +, - or = where '{operator}' is"));
        }
        let mut bits = 0;
        let mut conditional_execute = false;
        while let Some(&character) = characters.peek() {
            bits |= match character {
                'r' => 0o444,
                'w' => 0o222,
                'x' => 0o111,
                'X' => {
                    conditional_execute = true;
                    0
                }
                's' => 0o6000,
                't' => 0o1000,
                '+' | '-' | '=' => break,
                other => return Err(format!("'{other}' is not one of r, w, x, X, s or t")),
            };
            characters.next();
        }
        changes.push(Change::Symbolic {
            who,
            operator,
            bits,
            conditional_execute,
        });
    }
    if changes.is_empty() {
        return Err("missing +, - or =".into());
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octal_and_symbolic_rules_for_files_and_folders() {
        let chmod = Chmod::parse("D755,F644").unwrap();
        assert_eq!(chmod.apply(0o700, true), 0o755);
        assert_eq!(chmod.apply(0o777, false), 0o644);

        let chmod = Chmod::parse("go-w,u+x").unwrap();
        assert_eq!(chmod.apply(0o666, false), 0o744);

        let chmod = Chmod::parse("a+X").unwrap();
        assert_eq!(chmod.apply(0o644, true), 0o755);
        assert_eq!(chmod.apply(0o644, false), 0o644);
        assert_eq!(chmod.apply(0o744, false), 0o755);

        let chmod = Chmod::parse("Fu=rw,g=r,o=").unwrap();
        assert_eq!(chmod.apply(0o777, false), 0o640);
        assert_eq!(chmod.apply(0o777, true), 0o740);

        let chmod = Chmod::parse("g+s,+t").unwrap();
        assert_eq!(chmod.apply(0o755, true), 0o3755);

        let chmod = Chmod::parse("u+r-w").unwrap();
        assert_eq!(chmod.apply(0o200, false), 0o400);
    }

    #[test]
    fn mistakes_are_named() {
        for text in ["F", "u", "u+q", "8", "17777", "x+r"] {
            assert!(Chmod::parse(text).is_err(), "{text} should be refused");
        }
        assert!(Chmod::parse("").unwrap().is_empty());
    }
}
