//! Names for copies that must not take an existing name, and for files written beside the one
//! they replace.

/// Free names to try for a copy of `name`: `report (2).pdf`, `report (3).pdf` and so on. A
/// name that is already numbered counts on from its number. Folders and dotfiles have no
/// extension, and `.tar.gz` style extensions stay together.
pub fn numbered(name: &str, is_dir: bool) -> impl Iterator<Item = String> + '_ {
    let split = if is_dir { None } else { extension_start(name) };
    let (stem, extension) = split.map_or((name, ""), |dot| name.split_at(dot));
    let (base, last) = strip_number(stem);
    (last + 1..).map(move |number| format!("{base} ({number}){extension}"))
}

fn extension_start(name: &str) -> Option<usize> {
    let dot = name.rfind('.').filter(|&dot| dot > 0)?;
    let stem = &name[..dot];
    match stem.rfind('.') {
        Some(inner) if inner > 0 && stem[inner..].eq_ignore_ascii_case(".tar") => Some(inner),
        _ => Some(dot),
    }
}

/// `report (4)` is `report` and 4; anything else is itself and 1.
fn strip_number(stem: &str) -> (&str, u32) {
    let numbered = stem
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
        .and_then(|(base, digits)| {
            let number = digits.parse::<u32>().ok()?;
            (!base.is_empty() && number >= 2 && !digits.starts_with('0')).then_some((base, number))
        });
    numbered.unwrap_or((stem, 1))
}

/// A hidden name in the same folder for writing a file that then replaces `name`.
pub fn temporary(name: &str) -> String {
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];
    format!(".{name}.poros-{tag}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(name: &str, is_dir: bool) -> String {
        numbered(name, is_dir).next().unwrap()
    }

    #[test]
    fn numbers_before_the_extension() {
        assert_eq!(first("report.pdf", false), "report (2).pdf");
        assert_eq!(first("archive.tar.gz", false), "archive (2).tar.gz");
        assert_eq!(first("Makefile", false), "Makefile (2)");
        assert_eq!(first(".bashrc", false), ".bashrc (2)");
        assert_eq!(first("photos.2024", true), "photos.2024 (2)");
    }

    #[test]
    fn counts_on_from_an_existing_number() {
        assert_eq!(first("report (2).pdf", false), "report (3).pdf");
        assert_eq!(first("notes (9)", true), "notes (10)");
        assert_eq!(first("(2)", true), "(2) (2)");
        assert_eq!(first("v (01)", true), "v (01) (2)");
        let names: Vec<String> = numbered("a.txt", false).take(3).collect();
        assert_eq!(names, ["a (2).txt", "a (3).txt", "a (4).txt"]);
    }

    #[test]
    fn temporary_names_are_hidden_and_distinct() {
        let first = temporary("site.conf");
        assert!(first.starts_with(".site.conf.poros-"));
        assert_ne!(first, temporary("site.conf"));
    }
}
