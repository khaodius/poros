//! INI files and Windows registry exports (`.reg`), which WinSCP, PuTTY and MobaXterm keep
//! their sites in.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    /// Registry numbers are in decimal, like INI values.
    pub values: Vec<(String, String)>,
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    /// The value, trimmed, when it is not empty.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn number(&self, key: &str) -> Option<i64> {
        self.get(key)?.trim().parse().ok()
    }
}

pub fn is_export(text: &str) -> bool {
    let first_line = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .lines()
        .next()
        .unwrap_or_default();
    first_line.starts_with("Windows Registry Editor") || first_line.starts_with("REGEDIT4")
}

pub fn parse_ini(text: &str) -> Vec<Section> {
    let mut sections = vec![Section::default()];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = section_name(line) {
            sections.push(Section {
                name: name.to_string(),
                values: Vec::new(),
            });
        } else if let Some((key, value)) = line.split_once('=') {
            last(&mut sections)
                .values
                .push((key.trim().to_string(), value.to_string()));
        }
    }
    sections.retain(|section| !section.values.is_empty() || !section.name.is_empty());
    sections
}

/// String and number values; binary and other kinds are left out.
pub fn parse_export(text: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = section_name(line) {
            sections.push(Section {
                name: name.to_string(),
                values: Vec::new(),
            });
            continue;
        }
        let Some(section) = sections.last_mut() else {
            continue;
        };
        let Some((name, rest)) = quoted(line) else {
            continue;
        };
        let Some(data) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let data = data.trim();
        let value = if data.starts_with('"') {
            quoted(data).map(|(value, _)| value)
        } else if let Some(hex) = data.strip_prefix("dword:") {
            u32::from_str_radix(hex.trim(), 16)
                .ok()
                .map(|number| number.to_string())
        } else {
            None
        };
        if let Some(value) = value {
            section.values.push((name, value));
        }
    }
    sections
}

fn section_name(line: &str) -> Option<&str> {
    line.strip_prefix('[')?.strip_suffix(']')
}

fn last(sections: &mut [Section]) -> &mut Section {
    sections.last_mut().expect("parsing starts with a section")
}

/// The text of a `"..."` string with `\\` and `\"` escapes at the start of `text`, and what
/// follows it.
fn quoted(text: &str) -> Option<(String, &str)> {
    let rest = text.strip_prefix('"')?;
    let mut value = String::new();
    let mut characters = rest.char_indices();
    while let Some((index, character)) = characters.next() {
        match character {
            '\\' => {
                if let Some((_, escaped)) = characters.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some((value, &rest[index + 1..])),
            character => value.push(character),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_registry_exports() {
        let sections = parse_export(
            "Windows Registry Editor Version 5.00\r\n\r\n\
             [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\My%20Server]\r\n\
             \"HostName\"=\"alice@example.com\"\r\n\
             \"PortNumber\"=dword:00000016\r\n\
             \"PublicKeyFile\"=\"C:\\\\keys\\\\a \\\"b\\\".ppk\"\r\n\
             \"Colour0\"=hex:01,02,\\\r\n  03\r\n\
             @=\"default\"\r\n",
        );
        assert!(is_export("\u{feff}Windows Registry Editor Version 5.00\n"));
        assert_eq!(sections.len(), 1);
        let session = &sections[0];
        assert!(session.name.ends_with("\\Sessions\\My%20Server"));
        assert_eq!(session.get("hostname"), Some("alice@example.com"));
        assert_eq!(session.number("PortNumber"), Some(22));
        assert_eq!(session.get("PublicKeyFile"), Some("C:\\keys\\a \"b\".ppk"));
        assert_eq!(session.get("Colour0"), None);
    }

    #[test]
    fn reads_ini_files() {
        let sections = parse_ini("; comment\n[Sessions\\a]\nHostName=x=y\n\n[Empty]\n");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].text("HostName"), Some("x=y"));
        assert_eq!(sections[1].name, "Empty");
    }
}
