//! The name a server file's working copy gets on this computer. A server can call a file
//! anything, so the name is made safe for the local file system before anything is written
//! under it, and stays as close to the original as that allows: the program opening the copy
//! goes by its name and extension.

/// Longer names are cut, keeping the extension. Well inside what every file system takes,
/// with room left for the temporary files programs write beside what they save.
const MAX_NAME_BYTES: usize = 180;
/// An extension longer than this is taken for part of the name when cutting it.
const MAX_KEPT_EXTENSION_BYTES: usize = 24;
const FALLBACK_NAME: &str = "file";

/// Names Windows keeps for devices, with any extension and in any case.
const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The working copy's file name for the server file called `name`.
pub fn working_copy_name(name: &str) -> String {
    sanitize(name, cfg!(windows))
}

/// One plain path component for `name`. Path separators of either kind, control characters
/// and characters that reorder text or cannot be seen become `_`, so the name cannot leave its
/// folder or show an extension it does not have. `for_windows` also replaces what Windows
/// refuses in names and steps around its device names.
pub fn sanitize(name: &str, for_windows: bool) -> String {
    let mut cleaned: String = name
        .chars()
        .map(|character| {
            if is_unsafe(character) || (for_windows && r#"<>:"|?*"#.contains(character)) {
                '_'
            } else {
                character
            }
        })
        .collect();
    if for_windows {
        // Windows drops trailing dots and spaces, so two names would be one file.
        let kept = cleaned.trim_end_matches(['.', ' ']).len();
        cleaned.truncate(kept);
    }
    let cleaned = cleaned.trim();
    let mut cleaned = match cleaned {
        "" | "." | ".." => FALLBACK_NAME.to_string(),
        _ => shorten(cleaned),
    };
    if for_windows && is_reserved_device_name(&cleaned) {
        cleaned.insert(0, '_');
    }
    cleaned
}

fn is_unsafe(character: char) -> bool {
    matches!(character, '/' | '\\')
        || character.is_control()
        // Marks that reorder or hide text, which can make `gpj.exe` read as `exe.jpg`.
        || matches!(
            character,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
        )
}

fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    RESERVED_DEVICE_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

/// Cuts a name that is too long from the end of its stem, so the extension survives.
fn shorten(name: &str) -> String {
    if name.len() <= MAX_NAME_BYTES {
        return name.to_string();
    }
    let extension = name
        .rfind('.')
        .filter(|&dot| dot > 0 && name.len() - dot <= MAX_KEPT_EXTENSION_BYTES)
        .map_or("", |dot| &name[dot..]);
    let stem = &name[..name.len() - extension.len()];
    let mut end = MAX_NAME_BYTES - extension.len();
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{extension}", stem[..end].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_names_stay_as_they_are() {
        for name in [
            "report.xlsx",
            "My photo (final) v2.psd",
            ".bashrc",
            "archive.tar.gz",
            "caf\u{e9} \u{65e5}\u{672c}.txt",
            "-rf",
        ] {
            assert_eq!(sanitize(name, false), name);
            assert_eq!(sanitize(name, true), name);
        }
    }

    #[test]
    fn names_cannot_leave_their_folder() {
        assert_eq!(sanitize("../../etc/passwd", false), ".._.._etc_passwd");
        assert_eq!(sanitize("..\\..\\boot.ini", false), ".._.._boot.ini");
        assert_eq!(sanitize("/absolute", false), "_absolute");
        assert_eq!(sanitize("C:\\Windows\\win.ini", true), "C__Windows_win.ini");
        assert_eq!(sanitize("..", false), "file");
        assert_eq!(sanitize(".", true), "file");
        assert_eq!(sanitize("", false), "file");
        assert_eq!(sanitize("   ", false), "file");
        for name in ["../x", "a/b", "a\\b", "..", "/"] {
            let safe = sanitize(name, cfg!(windows));
            let mut components = std::path::Path::new(&safe).components();
            assert!(matches!(
                (components.next(), components.next()),
                (Some(std::path::Component::Normal(_)), None)
            ));
        }
    }

    #[test]
    fn control_and_reordering_characters_are_replaced() {
        assert_eq!(sanitize("a\nb\tc\0d.txt", false), "a_b_c_d.txt");
        assert_eq!(sanitize("line\u{7f}.txt", false), "line_.txt");
        // Right-to-left override: shown as `invoice_exe.pdf`'s mirror without the mark.
        assert_eq!(sanitize("invoice\u{202e}fdp.exe", false), "invoice_fdp.exe");
        assert_eq!(sanitize("zero\u{200b}width.txt", true), "zero_width.txt");
    }

    #[test]
    fn windows_names_avoid_what_windows_refuses() {
        assert_eq!(sanitize("what?.txt", true), "what_.txt");
        assert_eq!(sanitize("a<b>c:d\"e|f*.log", true), "a_b_c_d_e_f_.log");
        assert_eq!(sanitize("what?.txt", false), "what?.txt");
        assert_eq!(sanitize("stream.txt:hidden", true), "stream.txt_hidden");
        assert_eq!(sanitize("trailing. . ", true), "trailing");
        assert_eq!(sanitize("... ", true), "file");
        assert_eq!(sanitize("CON", true), "_CON");
        assert_eq!(sanitize("nul.txt", true), "_nul.txt");
        assert_eq!(sanitize("Com1.tar.gz", true), "_Com1.tar.gz");
        assert_eq!(sanitize("LPT9", true), "_LPT9");
        assert_eq!(sanitize("console.log", true), "console.log");
        assert_eq!(sanitize("COM10", true), "COM10");
        assert_eq!(sanitize("CON", false), "CON");
    }

    #[test]
    fn long_names_are_cut_and_keep_their_extension() {
        let long = format!("{}.xlsx", "r".repeat(400));
        let cut = sanitize(&long, false);
        assert_eq!(cut.len(), MAX_NAME_BYTES);
        assert!(cut.ends_with(".xlsx"));

        let wide = format!("{}.txt", "\u{65e5}".repeat(200));
        let cut = sanitize(&wide, true);
        assert!(cut.len() <= MAX_NAME_BYTES);
        assert!(cut.ends_with(".txt"));
        assert!(cut.starts_with('\u{65e5}'));

        let no_extension = "n".repeat(300);
        assert_eq!(sanitize(&no_extension, false).len(), MAX_NAME_BYTES);
        let long_extension = format!("a.{}", "e".repeat(300));
        assert_eq!(sanitize(&long_extension, false).len(), MAX_NAME_BYTES);
    }
}
