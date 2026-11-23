//! Tells files that a system would run from files it would show. Opening a file from a server
//! with the system's default program must never start it, so a file of a type that runs is
//! only ever handed to an editor, and only once the user has said so.

use std::io::Read;
use std::path::Path;

/// Extensions a system runs, installs or follows when asked to open the file.
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    // Windows programs, installers and what the shell starts like one.
    "exe",
    "com",
    "scr",
    "pif",
    "cpl",
    "msi",
    "msp",
    "mst",
    "msc",
    "msix",
    "msixbundle",
    "appx",
    "appxbundle",
    "appinstaller",
    "application",
    "appref-ms",
    "gadget",
    "dll",
    "sys",
    "ocx",
    "drv",
    // Scripts Windows runs.
    "bat",
    "cmd",
    "ps1",
    "ps1xml",
    "ps2",
    "psc1",
    "psm1",
    "psd1",
    "vb",
    "vbs",
    "vbe",
    "js",
    "jse",
    "ws",
    "wsf",
    "wsc",
    "wsh",
    "hta",
    "reg",
    "inf",
    "ins",
    "isp",
    "sct",
    "shb",
    "shs",
    // Shortcuts and files that lead somewhere else.
    "lnk",
    "url",
    "scf",
    "library-ms",
    "search-ms",
    "settingcontent-ms",
    "website",
    "desktop",
    "webloc",
    "inetloc",
    // Unix shells and scripts.
    "sh",
    "bash",
    "zsh",
    "csh",
    "ksh",
    "fish",
    "run",
    "bin",
    "out",
    "elf",
    "appimage",
    "command",
    "tool",
    "app",
    "action",
    "workflow",
    // Programs other runtimes start.
    "jar",
    "jnlp",
    "py",
    "pyw",
];

/// How much of a file is read to see whether it is a program.
const CONTENT_PROBE_BYTES: usize = 512;

/// Whether a file called `name` is of a type a system runs rather than shows. Decided by its
/// last extension in any case, after the trailing dots and spaces Windows ignores.
pub fn is_executable_name(name: &str) -> bool {
    let name = name.trim_end_matches(['.', ' ']);
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() {
        return false;
    }
    let extension = extension.to_lowercase();
    EXECUTABLE_EXTENSIONS.contains(&extension.as_str())
}

/// Whether a file's first bytes are those of a program or a script whatever it is called: a
/// script's `#!` line, a Windows, Linux or macOS program, or a desktop entry.
pub fn is_executable_content(start: &[u8]) -> bool {
    const PROGRAM_MARKS: [&[u8]; 7] = [
        b"#!",
        b"MZ",
        b"\x7fELF",
        b"\xfe\xed\xfa\xce",
        b"\xfe\xed\xfa\xcf",
        b"\xce\xfa\xed\xfe",
        b"\xcf\xfa\xed\xfe",
    ];
    let start = start.strip_prefix(b"\xef\xbb\xbf").unwrap_or(start);
    if PROGRAM_MARKS.iter().any(|mark| start.starts_with(mark)) {
        return true;
    }
    let text = start.trim_ascii_start();
    text.starts_with(b"[Desktop Entry]") || text.starts_with(b"[InternetShortcut]")
}

/// Whether the file at `path` is a program by its content. A file that cannot be read is not
/// taken for one: nothing can run it either.
pub fn has_executable_content(path: &Path) -> bool {
    let mut start = Vec::with_capacity(CONTENT_PROBE_BYTES);
    let read = std::fs::File::open(path).and_then(|file| {
        file.take(CONTENT_PROBE_BYTES as u64)
            .read_to_end(&mut start)
    });
    read.is_ok() && is_executable_content(&start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_scripts_and_shortcuts_are_executable_by_name() {
        for name in [
            "setup.exe",
            "SETUP.EXE",
            "run.bat",
            "build.cmd",
            "deploy.ps1",
            "install.sh",
            "app.desktop",
            "Documents.lnk",
            "tool.jar",
            "product.msi",
            "page.hta",
            "macro.vbs",
            "script.py",
            "Program.AppImage",
            "link.url",
            "invoice.pdf.exe",
            "trailing.exe.",
            "trailing.cmd  ",
            "photo.jpg .scr",
        ] {
            assert!(is_executable_name(name), "{name}");
        }
    }

    #[test]
    fn documents_are_not_executable_by_name() {
        for name in [
            "report.xlsx",
            "notes.txt",
            "photo.psd",
            "server.log",
            "archive.tar.gz",
            "exe",
            "sh",
            ".sh",
            ".bashrc",
            "Makefile",
            "exe.txt",
            "setup.exe.txt",
            "data.json",
            "",
            "...",
        ] {
            assert!(!is_executable_name(name), "{name}");
        }
    }

    #[test]
    fn programs_are_known_by_their_first_bytes() {
        assert!(is_executable_content(b"#!/bin/sh\nrm -rf ~\n"));
        assert!(is_executable_content(
            b"\xef\xbb\xbf#!/usr/bin/env python3\n"
        ));
        assert!(is_executable_content(b"MZ\x90\x00\x03"));
        assert!(is_executable_content(b"\x7fELF\x02\x01\x01"));
        assert!(is_executable_content(b"\xcf\xfa\xed\xfe\x07\x00"));
        assert!(is_executable_content(
            b"\n  [Desktop Entry]\nExec=sh -c 'curl x | sh'\n"
        ));
        assert!(is_executable_content(
            b"[InternetShortcut]\r\nURL=file:///c:/x.exe"
        ));

        assert!(!is_executable_content(b""));
        assert!(!is_executable_content(b"# a comment, not a shebang\n"));
        assert!(!is_executable_content(b"Monthly report\n"));
        assert!(!is_executable_content(b"PK\x03\x04"));
        assert!(!is_executable_content(b"%PDF-1.7"));
        assert!(!is_executable_content(b"[Settings]\nname=value\n"));
    }

    #[test]
    fn a_script_without_a_telling_name_is_found_by_reading_it() {
        let folder = tempfile::tempdir().unwrap();
        let script = folder.path().join("notes.txt");
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        let text = folder.path().join("readme.txt");
        std::fs::write(&text, "plain words\n").unwrap();
        assert!(has_executable_content(&script));
        assert!(!has_executable_content(&text));
        assert!(!has_executable_content(&folder.path().join("missing")));
    }
}
