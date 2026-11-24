//! Names as the local system shows them. A server may hold names the local system cannot:
//! Windows refuses `< > : " \ | ? *`, control characters, a trailing dot or space and device
//! names such as `CON`, treats `Readme` and `README` as one file, and both systems limit how
//! long a name may be. Every such name is shown under a stand-in that maps back to the one
//! name it came from, so a file is never hidden and never mistaken for another.
//!
//! Characters Windows refuses move into the Unicode private use area at `U+F000` plus the
//! character's code, the convention Windows' own tools for Unix and Mac servers use. Two names
//! that end up equal, exactly or by case, are told apart by a number in brackets, and a name
//! that is too long is cut and given a short hash of the whole name. A folder's names are
//! mapped together, in order, so the result does not depend on the order a server lists them.

use std::collections::{BTreeMap, HashMap};

/// The private use block a refused character is moved into.
const STAND_IN_BASE: u32 = 0xF000;
/// Names of devices Windows reserves, whatever extension follows.
const DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
/// The longest name either system takes, in its own units.
const MAX_NAME_UNITS: usize = 255;
const HASH_DIGITS: usize = 8;

/// What the local system allows in a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameStyle {
    /// Linux and macOS: anything but `/` and NUL, at most 255 bytes, case kept apart.
    Unix,
    /// Windows: reserved characters and device names, at most 255 UTF-16 units, case folded.
    Windows,
}

impl NameStyle {
    pub fn native() -> Self {
        if cfg!(windows) {
            NameStyle::Windows
        } else {
            NameStyle::Unix
        }
    }

    fn folds_case(self) -> bool {
        self == NameStyle::Windows
    }

    fn units(self, name: &str) -> usize {
        match self {
            NameStyle::Unix => name.len(),
            NameStyle::Windows => name.encode_utf16().count(),
        }
    }

    /// The key two names share when the local system cannot tell them apart.
    pub fn key(self, name: &str) -> String {
        if self.folds_case() {
            name.to_lowercase()
        } else {
            name.to_string()
        }
    }
}

fn stand_in(character: char) -> char {
    char::from_u32(STAND_IN_BASE + character as u32).unwrap_or(character)
}

fn refused_by_windows(character: char) -> bool {
    matches!(character, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*') || (character as u32) < 0x20
}

/// The part of a name Windows compares with its device names: up to the first dot, without
/// trailing spaces.
fn device_stem(name: &str) -> &str {
    name.split('.').next().unwrap_or(name).trim_end_matches(' ')
}

fn is_device_name(name: &str) -> bool {
    let stem = device_stem(name);
    DEVICE_NAMES
        .iter()
        .any(|device| device.eq_ignore_ascii_case(stem))
}

/// A remote name with every character the local system refuses replaced by its stand-in.
/// Names that cannot be shown at all (empty, `.`, `..`, or holding `/` or NUL) give `None`.
fn encoded(remote: &str, style: NameStyle) -> Option<String> {
    if remote.is_empty() || remote == "." || remote == ".." || remote.contains(['/', '\0']) {
        return None;
    }
    if style == NameStyle::Unix {
        return Some(remote.to_string());
    }
    let mut local: Vec<char> = remote
        .chars()
        .map(|character| {
            if refused_by_windows(character) {
                stand_in(character)
            } else {
                character
            }
        })
        .collect();
    if let Some(last) = local.last_mut() {
        if matches!(*last, '.' | ' ') {
            *last = stand_in(*last);
        }
    }
    let mut local: String = local.into_iter().collect();
    if is_device_name(&local) {
        // The last letter of the device name itself, so `CON.txt` keeps its extension.
        let stem_length = device_stem(&local).len();
        let last_letter = local[..stem_length]
            .chars()
            .next_back()
            .expect("a device name has letters");
        let start = stem_length - last_letter.len_utf8();
        local.replace_range(start..stem_length, &stand_in(last_letter).to_string());
    }
    Some(local)
}

/// Undoes `encoded` for a name the local system handed over.
fn decoded(local: &str) -> String {
    local
        .chars()
        .map(|character| {
            let code = character as u32;
            if (STAND_IN_BASE..STAND_IN_BASE + 0x80).contains(&code) {
                char::from_u32(code - STAND_IN_BASE).unwrap_or(character)
            } else {
                character
            }
        })
        .collect()
}

/// A 32-bit FNV-1a hash, enough to tell apart the few names of one folder that share a start.
fn short_hash(name: &str) -> String {
    let hash = name.bytes().fold(0x811C_9DC5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    format!("{hash:0width$x}", width = HASH_DIGITS)
}

/// The extension of a name, with its dot, if it is short enough to be worth keeping.
fn kept_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= 16 => &name[dot..],
        _ => "",
    }
}

/// `name` cut to fit `limit` units once `suffix` and its extension are added back.
fn shortened(name: &str, suffix: &str, style: NameStyle, limit: usize) -> String {
    let extension = kept_extension(name);
    let stem = &name[..name.len() - extension.len()];
    let budget = limit.saturating_sub(style.units(suffix) + style.units(extension));
    let mut kept = String::new();
    let mut used = 0;
    for character in stem.chars() {
        let units = match style {
            NameStyle::Unix => character.len_utf8(),
            NameStyle::Windows => character.len_utf16(),
        };
        if used + units > budget {
            break;
        }
        kept.push(character);
        used += units;
    }
    format!("{kept}{suffix}{extension}")
}

/// The name a remote name is shown under when nothing else in its folder is in its way.
pub fn local_name(remote: &str, style: NameStyle) -> Option<String> {
    let local = encoded(remote, style)?;
    if style.units(&local) <= MAX_NAME_UNITS {
        return Some(local);
    }
    let suffix = format!("~{}", short_hash(remote));
    Some(shortened(&local, &suffix, style, MAX_NAME_UNITS))
}

/// The remote name a new file or folder gets when the local system creates `local`.
pub fn remote_name_for_new(local: &str, style: NameStyle) -> String {
    if style == NameStyle::Unix {
        return local.to_string();
    }
    let candidate = decoded(local);
    // Only what `encoded` produces is undone, so a name that merely holds private use
    // characters stays as it is.
    if encoded(&candidate, style).as_deref() == Some(local) {
        candidate
    } else {
        local.to_string()
    }
}

/// The names of one folder, both ways.
#[derive(Debug, Default, Clone)]
pub struct FolderNames {
    /// Local name by remote name.
    local_by_remote: HashMap<String, String>,
    /// Remote name by the key of its local name.
    remote_by_key: HashMap<String, String>,
}

impl FolderNames {
    /// Maps every name of a folder. Names that cannot be shown are left out.
    pub fn of<'a>(remote_names: impl IntoIterator<Item = &'a str>, style: NameStyle) -> Self {
        // In byte order, so the same folder always maps the same way.
        let ordered: BTreeMap<&str, ()> = remote_names.into_iter().map(|name| (name, ())).collect();
        let mut names = Self::default();
        // First the names that keep their plain form, so a stand-in never takes the place of
        // a name that needed none.
        let mut clashing = Vec::new();
        for remote in ordered.keys() {
            let Some(local) = local_name(remote, style) else {
                continue;
            };
            let key = style.key(&local);
            if names.remote_by_key.contains_key(&key) {
                clashing.push((*remote, local));
            } else {
                names.insert(remote, local, key);
            }
        }
        for (remote, local) in clashing {
            let mut number = 2;
            loop {
                let suffix = format!(" ({number})");
                let candidate = shortened(&local, &suffix, style, MAX_NAME_UNITS);
                let key = style.key(&candidate);
                if !names.remote_by_key.contains_key(&key) {
                    names.insert(remote, candidate, key);
                    break;
                }
                number += 1;
            }
        }
        names
    }

    fn insert(&mut self, remote: &str, local: String, key: String) {
        self.remote_by_key.insert(key, remote.to_string());
        self.local_by_remote.insert(remote.to_string(), local);
    }

    pub fn local(&self, remote: &str) -> Option<&str> {
        self.local_by_remote.get(remote).map(String::as_str)
    }

    /// The remote name behind a local one, for a name that exists in the folder.
    pub fn remote(&self, local: &str, style: NameStyle) -> Option<&str> {
        self.remote_by_key
            .get(&style.key(local))
            .map(String::as_str)
    }

    /// Adds a name created through the mount, keeping the stand-in it was created under.
    pub fn add(&mut self, remote: &str, local: &str, style: NameStyle) {
        self.insert(remote, local.to_string(), style.key(local));
    }

    pub fn remove(&mut self, remote: &str, style: NameStyle) {
        if let Some(local) = self.local_by_remote.remove(remote) {
            self.remote_by_key.remove(&style.key(&local));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(remote: &str) -> String {
        local_name(remote, NameStyle::Windows).unwrap()
    }

    #[test]
    fn unix_names_pass_through() {
        assert_eq!(
            local_name("a:b*c?.txt ", NameStyle::Unix).as_deref(),
            Some("a:b*c?.txt ")
        );
        assert_eq!(local_name("CON", NameStyle::Unix).as_deref(), Some("CON"));
        assert_eq!(remote_name_for_new("a:b", NameStyle::Unix), "a:b");
    }

    #[test]
    fn names_that_cannot_be_shown_are_left_out() {
        for name in ["", ".", "..", "a/b", "a\0b"] {
            assert_eq!(local_name(name, NameStyle::Unix), None, "{name:?}");
            assert_eq!(local_name(name, NameStyle::Windows), None, "{name:?}");
        }
    }

    #[test]
    fn windows_reserved_characters_get_stand_ins_and_come_back() {
        let remote = "report: \"final\" <v2>|a\\b?*.txt";
        let local = windows(remote);
        assert!(!local.chars().any(refused_by_windows), "{local:?}");
        assert_eq!(remote_name_for_new(&local, NameStyle::Windows), remote);
        assert_eq!(windows("tab\there"), "tab\u{F009}here");
    }

    #[test]
    fn windows_trailing_dots_and_spaces_survive() {
        assert_eq!(windows("notes."), "notes\u{F02E}");
        assert_eq!(windows("notes "), "notes\u{F020}");
        assert_eq!(windows("a.b"), "a.b");
        for remote in ["notes.", "notes ", "..."] {
            assert_eq!(
                remote_name_for_new(&windows(remote), NameStyle::Windows),
                remote
            );
        }
    }

    #[test]
    fn windows_device_names_are_set_apart() {
        assert_eq!(windows("CON"), "CO\u{F04E}");
        assert_eq!(windows("con.txt"), "co\u{F06E}.txt");
        assert_eq!(windows("LPT1.tar.gz"), "LPT\u{F031}.tar.gz");
        assert_eq!(windows("console"), "console");
        assert_eq!(windows("COM10"), "COM10");
        for remote in ["CON", "con.txt", "nul", "Aux.log", "LPT1.tar.gz"] {
            let local = windows(remote);
            assert!(!is_device_name(&local), "{local:?}");
            assert_eq!(remote_name_for_new(&local, NameStyle::Windows), remote);
        }
    }

    #[test]
    fn a_new_name_with_private_use_characters_of_its_own_is_kept() {
        // `A` is never given a stand-in, so this is not something `encoded` produced.
        let odd = "x\u{F041}y";
        assert_eq!(remote_name_for_new(odd, NameStyle::Windows), odd);
    }

    #[test]
    fn names_differing_by_case_are_told_apart_on_windows_only() {
        let remote = ["README", "Readme", "readme", "other"];
        let unix = FolderNames::of(remote, NameStyle::Unix);
        for name in remote {
            assert_eq!(unix.local(name), Some(name));
        }

        let names = FolderNames::of(remote, NameStyle::Windows);
        assert_eq!(names.local("README"), Some("README"));
        assert_eq!(names.local("Readme"), Some("Readme (2)"));
        assert_eq!(names.local("readme"), Some("readme (3)"));
        assert_eq!(names.remote("readme", NameStyle::Windows), Some("README"));
        assert_eq!(
            names.remote("README (2)", NameStyle::Windows),
            Some("Readme")
        );
        assert_eq!(
            names.remote("readme (3)", NameStyle::Windows),
            Some("readme")
        );
        assert_eq!(names.remote("missing", NameStyle::Windows), None);
    }

    #[test]
    fn the_mapping_does_not_depend_on_listing_order() {
        let forward = FolderNames::of(["a.TXT", "A.txt", "a.txt"], NameStyle::Windows);
        let backward = FolderNames::of(["a.txt", "A.txt", "a.TXT"], NameStyle::Windows);
        for name in ["a.TXT", "A.txt", "a.txt"] {
            assert_eq!(forward.local(name), backward.local(name));
        }
        assert_eq!(forward.local("A.txt"), Some("A.txt"));
        assert_eq!(forward.local("a.TXT"), Some("a (2).TXT"));
    }

    #[test]
    fn a_plain_name_keeps_its_place_over_a_stand_in() {
        // "a (2)" exists for real, so the clash of "A" with "a" moves on to (3).
        let names = FolderNames::of(["A", "a", "a (2)"], NameStyle::Windows);
        assert_eq!(names.local("A"), Some("A"));
        assert_eq!(names.local("a (2)"), Some("a (2)"));
        assert_eq!(names.local("a"), Some("a (3)"));
    }

    #[test]
    fn long_names_are_cut_with_a_hash_and_stay_apart() {
        let first = format!("{}-one.tar.gz", "x".repeat(400));
        let second = format!("{}-two.tar.gz", "x".repeat(400));
        for style in [NameStyle::Unix, NameStyle::Windows] {
            let names = FolderNames::of([first.as_str(), second.as_str()], style);
            let local_first = names.local(&first).unwrap();
            let local_second = names.local(&second).unwrap();
            assert_ne!(local_first, local_second);
            assert!(style.units(local_first) <= MAX_NAME_UNITS);
            assert!(local_first.ends_with(".gz"));
            assert_eq!(names.remote(local_first, style), Some(first.as_str()));
            assert_eq!(names.remote(local_second, style), Some(second.as_str()));
        }
    }

    #[test]
    fn long_names_count_bytes_on_unix_and_utf16_units_on_windows() {
        let name = "é".repeat(200);
        assert_eq!(local_name(&name, NameStyle::Windows).unwrap(), name);
        let cut = local_name(&name, NameStyle::Unix).unwrap();
        assert!(cut.len() <= MAX_NAME_UNITS);
        assert!(cut.contains('~'));
    }

    #[test]
    fn names_created_through_the_mount_are_found_again() {
        let mut names = FolderNames::of(["one"], NameStyle::Windows);
        let local = "draft\u{F03A}v2";
        let remote = remote_name_for_new(local, NameStyle::Windows);
        assert_eq!(remote, "draft:v2");
        names.add(&remote, local, NameStyle::Windows);
        assert_eq!(
            names.remote("DRAFT\u{F03A}V2", NameStyle::Windows),
            Some("draft:v2")
        );
        names.remove(&remote, NameStyle::Windows);
        assert_eq!(names.remote(local, NameStyle::Windows), None);
    }
}
