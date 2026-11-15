//! The programs that extract archives, what each can open, and the command lines that extract
//! with them, the same on a server as on this computer. None overwrites a file already there.

use serde::{Deserialize, Serialize};

use super::formats::ArchiveFormat;
use crate::rsync::shell_quote;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolKind {
    /// 7-Zip (`7z`, `7zz`), or p7zip's full `7z`.
    SevenZip,
    /// 7-Zip's standalone `7za`, which has no RAR or ISO support.
    SevenZipStandalone,
    /// WinRAR's own program, which opens most formats.
    WinRar,
    /// RARLAB's `unrar`, or the `Rar` and `UnRAR` that come with WinRAR.
    UnRar,
    /// The free `unrar-free`, which Debian and Ubuntu also install as `unrar`. It opens fewer
    /// RAR archives than RARLAB's.
    UnrarFree,
    /// The Unarchiver's `unar`.
    Unar,
    /// libarchive's tar: `bsdtar`, the `tar` of Windows 10 and later and of BSD and macOS.
    BsdTar,
    GnuTar,
    Unzip,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

use ArchiveFormat as Format;

const TAR_FORMATS: &[Format] = &[
    Format::Tar,
    Format::TarGzip,
    Format::TarBzip2,
    Format::TarXz,
    Format::TarZstd,
    Format::TarLzma,
];

impl ToolKind {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::SevenZip => "7-Zip",
            Self::SevenZipStandalone => "7-Zip (7za)",
            Self::WinRar => "WinRAR",
            Self::UnRar => "UnRAR",
            Self::UnrarFree => "unrar-free",
            Self::Unar => "unar",
            Self::BsdTar => "bsdtar",
            Self::GnuTar => "GNU tar",
            Self::Unzip => "unzip",
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Zstd => "zstd",
        }
    }

    /// The formats it opens. 7-Zip and 7za open compressed tar files in two steps, piping the
    /// tar archive from one run into another.
    pub fn formats(self) -> &'static [Format] {
        match self {
            Self::SevenZip => &[
                Format::Zip,
                Format::SevenZip,
                Format::Rar,
                Format::Tar,
                Format::TarGzip,
                Format::TarBzip2,
                Format::TarXz,
                Format::TarLzma,
                Format::Gzip,
                Format::Bzip2,
                Format::Xz,
                Format::Lzma,
                Format::Iso,
                Format::Cab,
            ],
            Self::SevenZipStandalone => &[
                Format::Zip,
                Format::SevenZip,
                Format::Tar,
                Format::TarGzip,
                Format::TarBzip2,
                Format::TarXz,
                Format::TarLzma,
                Format::Gzip,
                Format::Bzip2,
                Format::Xz,
                Format::Lzma,
                Format::Cab,
            ],
            Self::WinRar => &[
                Format::Rar,
                Format::Zip,
                Format::SevenZip,
                Format::Tar,
                Format::TarGzip,
                Format::TarBzip2,
                Format::TarXz,
                Format::Gzip,
                Format::Bzip2,
                Format::Xz,
                Format::Iso,
                Format::Cab,
            ],
            Self::UnRar | Self::UnrarFree => &[Format::Rar],
            Self::Unar => &[
                Format::Zip,
                Format::SevenZip,
                Format::Rar,
                Format::Tar,
                Format::TarGzip,
                Format::TarBzip2,
                Format::TarXz,
                Format::Gzip,
                Format::Bzip2,
                Format::Xz,
                Format::Iso,
                Format::Cab,
            ],
            Self::BsdTar => &[
                Format::Zip,
                Format::SevenZip,
                Format::Rar,
                Format::Tar,
                Format::TarGzip,
                Format::TarBzip2,
                Format::TarXz,
                Format::TarZstd,
                Format::TarLzma,
                Format::Iso,
                Format::Cab,
            ],
            Self::GnuTar => TAR_FORMATS,
            Self::Unzip => &[Format::Zip],
            Self::Gzip => &[Format::Gzip],
            Self::Bzip2 => &[Format::Bzip2],
            Self::Xz => &[Format::Xz, Format::Lzma],
            Self::Zstd => &[Format::Zstd],
        }
    }

    pub fn opens(self, format: Format) -> bool {
        self.formats().contains(&format)
    }
}

/// The kinds to try for a format, best first: each format's own tool, then those that open the
/// most of it reliably.
pub fn preference(format: Format) -> &'static [ToolKind] {
    use ToolKind::*;
    match format {
        Format::Zip => &[SevenZip, BsdTar, Unzip, SevenZipStandalone, Unar, WinRar],
        Format::SevenZip => &[SevenZip, SevenZipStandalone, BsdTar, Unar, WinRar],
        Format::Rar => &[UnRar, WinRar, SevenZip, Unar, BsdTar, UnrarFree],
        Format::TarZstd => &[GnuTar, BsdTar],
        Format::Tar | Format::TarGzip | Format::TarBzip2 | Format::TarXz | Format::TarLzma => {
            &[GnuTar, BsdTar, SevenZip, SevenZipStandalone, WinRar, Unar]
        }
        Format::Gzip => &[Gzip, SevenZip, SevenZipStandalone, Unar, WinRar],
        Format::Bzip2 => &[Bzip2, SevenZip, SevenZipStandalone, Unar, WinRar],
        Format::Xz => &[Xz, SevenZip, SevenZipStandalone, Unar, WinRar],
        Format::Zstd => &[Zstd],
        Format::Lzma => &[Xz, SevenZip, SevenZipStandalone],
        Format::Iso => &[SevenZip, BsdTar, Unar, WinRar],
        Format::Cab => &[SevenZip, BsdTar, SevenZipStandalone, Unar, WinRar],
    }
}

/// A program that extracts: its kind, and how to start it (a name on the server's `PATH`, or
/// a full path on this computer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub kind: ToolKind,
    pub program: String,
}

/// A program the user picked, with arguments where `{archive}` and `{folder}` stand for the
/// archive and the folder to extract into.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTool {
    pub program: String,
    pub arguments: String,
}

/// One program run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
}

/// Where the unpacked data ends up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// The program writes into the folder itself.
    IntoFolder,
    /// The last program writes the one file it unpacks to its output, kept as this file.
    Stdout(String),
}

/// What to run: programs whose output feeds the next one's input, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub stages: Vec<Invocation>,
    pub output: Output,
}

/// Where an extraction writes: the folder that receives the files, and for a single
/// compressed file the name it unpacks to there.
pub struct Placement<'a> {
    pub archive: &'a str,
    pub folder: &'a str,
    pub single_file: Option<&'a str>,
    /// The separator a folder ends with to mark it as one, for UnRAR and WinRAR.
    pub separator: char,
}

fn run(program: &str, args: &[&str]) -> Invocation {
    Invocation {
        program: program.to_string(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
    }
}

fn as_folder(folder: &str, separator: char) -> String {
    if folder.ends_with(['/', '\\']) {
        folder.to_string()
    } else {
        format!("{folder}{separator}")
    }
}

/// The command lines that extract `format` with `tool` as `placement` says, if it can.
pub fn plan(tool: &Tool, format: Format, placement: &Placement) -> Option<Plan> {
    if !tool.kind.opens(format) {
        return None;
    }
    let Placement {
        archive,
        folder,
        single_file,
        separator,
    } = *placement;
    let program = tool.program.as_str();
    let into_folder = |invocation: Invocation| Plan {
        stages: vec![invocation],
        output: Output::IntoFolder,
    };
    let to_file = |invocation: Invocation| {
        single_file.map(|file| Plan {
            stages: vec![invocation],
            output: Output::Stdout(file.to_string()),
        })
    };
    let output_switch = format!("-o{folder}");
    match tool.kind {
        ToolKind::SevenZip | ToolKind::SevenZipStandalone => {
            if format.is_single_file() {
                return to_file(run(program, &["x", "-so", "-bd", archive]));
            }
            if format.is_tar() && format != Format::Tar {
                return Some(Plan {
                    stages: vec![
                        run(program, &["x", "-so", "-bd", archive]),
                        run(
                            program,
                            &["x", "-si", "-ttar", "-y", "-aos", "-bd", &output_switch],
                        ),
                    ],
                    output: Output::IntoFolder,
                });
            }
            Some(into_folder(run(
                program,
                &["x", "-y", "-aos", "-bd", &output_switch, archive],
            )))
        }
        ToolKind::WinRar => Some(into_folder(run(
            program,
            &[
                "x",
                "-ibck",
                "-y",
                "-o-",
                "-p-",
                archive,
                &as_folder(folder, separator),
            ],
        ))),
        ToolKind::UnRar => Some(into_folder(run(
            program,
            &[
                "x",
                "-y",
                "-o-",
                "-p-",
                archive,
                &as_folder(folder, separator),
            ],
        ))),
        ToolKind::UnrarFree => Some(into_folder(run(
            program,
            &["-x", archive, &as_folder(folder, separator)],
        ))),
        ToolKind::Unar => Some(into_folder(run(
            program,
            &["-q", "-s", "-D", "-o", folder, archive],
        ))),
        ToolKind::BsdTar => Some(into_folder(run(
            program,
            &["-x", "-k", "-f", archive, "-C", folder],
        ))),
        ToolKind::GnuTar => Some(into_folder(run(
            program,
            &["-x", "--skip-old-files", "-f", archive, "-C", folder],
        ))),
        ToolKind::Unzip => Some(into_folder(run(
            program,
            &["-n", "-q", archive, "-d", folder],
        ))),
        ToolKind::Gzip | ToolKind::Bzip2 => to_file(run(program, &["-dc", archive])),
        ToolKind::Xz if format == Format::Lzma => {
            to_file(run(program, &["--format=lzma", "-dc", archive]))
        }
        ToolKind::Xz => to_file(run(program, &["-dc", archive])),
        ToolKind::Zstd => to_file(run(program, &["-dcq", archive])),
    }
}

/// The command line of a program the user picked, which extracts into the folder.
pub fn custom_plan(tool: &CustomTool, archive: &str, folder: &str) -> Plan {
    let args = split_arguments(&tool.arguments)
        .into_iter()
        .map(|argument| {
            argument
                .replace("{archive}", archive)
                .replace("{folder}", folder)
        })
        .collect();
    Plan {
        stages: vec![Invocation {
            program: tool.program.trim().to_string(),
            args,
        }],
        output: Output::IntoFolder,
    }
}

/// Splits arguments at spaces, keeping what double quotes hold together, as Windows does.
pub fn split_arguments(arguments: &str) -> Vec<String> {
    let mut split = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for character in arguments.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            character if character.is_whitespace() && !quoted => {
                if started {
                    split.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            character => {
                current.push(character);
                started = true;
            }
        }
    }
    if started {
        split.push(current);
    }
    split
}

/// The best of `tools` for `format`, `preferred` first when it opens it.
pub fn choose<'a>(tools: &'a [Tool], format: Format, preferred: Option<&Tool>) -> Option<&'a Tool> {
    if let Some(preferred) = preferred.filter(|tool| tool.kind.opens(format)) {
        if let Some(tool) = tools.iter().find(|tool| *tool == preferred) {
            return Some(tool);
        }
    }
    preference(format)
        .iter()
        .find_map(|kind| tools.iter().find(|tool| tool.kind == *kind))
}

/// A plan as one POSIX shell script for a server.
pub fn shell_script(plan: &Plan) -> String {
    let render = |invocation: &Invocation| {
        std::iter::once(&invocation.program)
            .chain(&invocation.args)
            .map(|word| shell_quote(word))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let pipeline = plan
        .stages
        .iter()
        .map(render)
        .collect::<Vec<_>>()
        .join(" | ");
    match &plan.output {
        Output::IntoFolder => pipeline,
        // `set -o noclobber` keeps a file that appeared in the meantime.
        Output::Stdout(file) => format!("set -C; {pipeline} > {}", shell_quote(file)),
    }
}

/// Programs looked for on servers, in the order they are reported.
pub const SERVER_PROGRAMS: &[&str] = &[
    "7zz",
    "7z",
    "7za",
    "unrar",
    "unrar-free",
    "unar",
    "bsdtar",
    "tar",
    "unzip",
    "gzip",
    "bzip2",
    "xz",
    "zstd",
];

/// The script that lists which of [`SERVER_PROGRAMS`] a server has, which tar its `tar` is, and
/// whether its `unrar` is RARLAB's.
/// It runs under `sh`, whatever the login shell. The last line shows it ran at all, as servers
/// that allow SFTP alone can succeed without running anything.
pub fn server_probe() -> String {
    let script = format!(
        "for program in {}; do command -v \"$program\" >/dev/null 2>&1 && echo \"tool $program\"; \
         done; command -v tar >/dev/null 2>&1 && echo \"tar $(tar --version 2>&1 | head -n 1)\"; \
         command -v unrar >/dev/null 2>&1 && echo \"unrar $(unrar 2>&1 | grep -i -m 1 unrar)\"; \
         echo poros-archive-tools",
        SERVER_PROGRAMS.join(" ")
    );
    format!("sh -c {}", shell_quote(&script))
}

pub const PROBE_DONE: &str = "poros-archive-tools";

/// The tools a server's probe reported; `None` when the probe did not run.
pub fn parse_server_probe(output: &str) -> Option<Vec<Tool>> {
    if !output.lines().any(|line| line.trim() == PROBE_DONE) {
        return None;
    }
    let banner = |program: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(program)?.strip_prefix(' '))
            .unwrap_or("")
    };
    let tar_version = banner("tar");
    let unrar_banner = banner("unrar");
    let mut tools = Vec::new();
    for program in output
        .lines()
        .filter_map(|line| line.strip_prefix("tool "))
        .map(str::trim)
    {
        let kind = match program {
            "7zz" | "7z" => ToolKind::SevenZip,
            "7za" => ToolKind::SevenZipStandalone,
            "unrar" => unrar_kind(unrar_banner),
            "unrar-free" => ToolKind::UnrarFree,
            "unar" => ToolKind::Unar,
            "bsdtar" => ToolKind::BsdTar,
            "tar" => tar_kind(tar_version),
            "unzip" => ToolKind::Unzip,
            "gzip" => ToolKind::Gzip,
            "bzip2" => ToolKind::Bzip2,
            "xz" => ToolKind::Xz,
            "zstd" => ToolKind::Zstd,
            _ => continue,
        };
        // `7zz` is reported before `7z`, and wins.
        if tools.iter().any(|tool: &Tool| tool.kind == kind) {
            continue;
        }
        tools.push(Tool {
            kind,
            program: program.to_string(),
        });
    }
    Some(tools)
}

/// RARLAB's `unrar` calls itself freeware when it starts; `unrar-free` does not.
pub fn unrar_kind(banner: &str) -> ToolKind {
    if banner.to_ascii_lowercase().contains("freeware") {
        ToolKind::UnRar
    } else {
        ToolKind::UnrarFree
    }
}

/// GNU tar names itself in its version; others (libarchive's, BusyBox's) take the `-k` that
/// keeps existing files without failing.
pub fn tar_kind(version: &str) -> ToolKind {
    if version.contains("GNU tar") {
        ToolKind::GnuTar
    } else {
        ToolKind::BsdTar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(kind: ToolKind, program: &str) -> Tool {
        Tool {
            kind,
            program: program.to_string(),
        }
    }

    fn placement<'a>(archive: &'a str, folder: &'a str, single: Option<&'a str>) -> Placement<'a> {
        Placement {
            archive,
            folder,
            single_file: single,
            separator: '/',
        }
    }

    #[test]
    fn picks_each_formats_own_tool_first() {
        let tools = [
            tool(ToolKind::Unzip, "unzip"),
            tool(ToolKind::GnuTar, "tar"),
            tool(ToolKind::SevenZip, "7z"),
            tool(ToolKind::Gzip, "gzip"),
        ];
        let chosen = |format| choose(&tools, format, None).map(|tool| tool.program.as_str());
        assert_eq!(chosen(Format::Zip), Some("7z"));
        assert_eq!(chosen(Format::TarGzip), Some("tar"));
        assert_eq!(chosen(Format::Rar), Some("7z"));
        assert_eq!(chosen(Format::Gzip), Some("gzip"));
        assert_eq!(chosen(Format::Zstd), None);
    }

    #[test]
    fn prefers_the_picked_tool_where_it_opens_the_format() {
        let tools = [
            tool(ToolKind::SevenZip, "C:\\Program Files\\7-Zip\\7z.exe"),
            tool(ToolKind::WinRar, "C:\\Program Files\\WinRAR\\WinRAR.exe"),
            tool(ToolKind::BsdTar, "C:\\Windows\\System32\\tar.exe"),
        ];
        let winrar = tools[1].clone();
        let chosen = |format| choose(&tools, format, Some(&winrar)).map(|tool| tool.kind);
        assert_eq!(chosen(Format::Zip), Some(ToolKind::WinRar));
        assert_eq!(chosen(Format::Rar), Some(ToolKind::WinRar));
        assert_eq!(chosen(Format::TarZstd), Some(ToolKind::BsdTar));
    }

    #[test]
    fn never_overwrites() {
        let args = |kind, program, format, archive, folder| {
            let planned = plan(
                &tool(kind, program),
                format,
                &placement(archive, folder, None),
            );
            planned.unwrap().stages[0].args.clone()
        };
        assert_eq!(
            args(
                ToolKind::SevenZip,
                "7z",
                Format::Zip,
                "/srv/a.zip",
                "/srv/a"
            ),
            ["x", "-y", "-aos", "-bd", "-o/srv/a", "/srv/a.zip"]
        );
        assert_eq!(
            args(ToolKind::UnRar, "unrar", Format::Rar, "/srv/b.rar", "/srv"),
            ["x", "-y", "-o-", "-p-", "/srv/b.rar", "/srv/"]
        );
        assert_eq!(
            args(
                ToolKind::GnuTar,
                "tar",
                Format::TarXz,
                "/srv/c.txz",
                "/srv/c"
            ),
            ["-x", "--skip-old-files", "-f", "/srv/c.txz", "-C", "/srv/c"]
        );
    }

    #[test]
    fn pipes_compressed_tar_files_through_seven_zip_twice() {
        let seven = tool(ToolKind::SevenZip, "7z");
        let piped = plan(&seven, Format::TarGzip, &placement("/a.tgz", "/a", None)).unwrap();
        assert_eq!(piped.stages.len(), 2);
        assert_eq!(piped.stages[1].args[..3], ["x", "-si", "-ttar"]);
        assert_eq!(
            shell_script(&piped),
            "'7z' 'x' '-so' '-bd' '/a.tgz' | '7z' 'x' '-si' '-ttar' '-y' '-aos' '-bd' '-o/a'"
        );
    }

    #[test]
    fn writes_single_files_without_replacing_one() {
        let gzip = tool(ToolKind::Gzip, "gzip");
        let unpacked = plan(
            &gzip,
            Format::Gzip,
            &placement("/d/x.sql.gz", "/d", Some("/d/x.sql")),
        )
        .unwrap();
        assert_eq!(unpacked.output, Output::Stdout("/d/x.sql".into()));
        assert_eq!(
            shell_script(&unpacked),
            "set -C; 'gzip' '-dc' '/d/x.sql.gz' > '/d/x.sql'"
        );
        let xz = tool(ToolKind::Xz, "xz");
        let lzma = plan(
            &xz,
            Format::Lzma,
            &placement("/d/y.lzma", "/d", Some("/d/y")),
        )
        .unwrap();
        assert_eq!(lzma.stages[0].args, ["--format=lzma", "-dc", "/d/y.lzma"]);
    }

    #[test]
    fn refuses_formats_a_tool_cannot_open() {
        let unzip = tool(ToolKind::Unzip, "unzip");
        assert!(plan(&unzip, Format::Rar, &placement("/a.rar", "/", None)).is_none());
        let standalone = tool(ToolKind::SevenZipStandalone, "7za");
        assert!(plan(&standalone, Format::Rar, &placement("/a.rar", "/", None)).is_none());
    }

    #[test]
    fn fills_in_a_custom_command() {
        let custom = CustomTool {
            program: "C:\\Tools\\bz.exe".into(),
            arguments: "x -o:\"{folder}\" -y \"{archive}\"".into(),
        };
        let filled = custom_plan(&custom, "C:\\Users\\me\\a b.zip", "C:\\Users\\me\\a b");
        assert_eq!(
            filled.stages[0].args,
            ["x", "-o:C:\\Users\\me\\a b", "-y", "C:\\Users\\me\\a b.zip"]
        );
    }

    #[test]
    fn reads_what_a_server_has() {
        let output = "tool 7zz\ntool 7z\ntool unzip\ntool tar\ntar tar (GNU tar) 1.34\n\
                      poros-archive-tools\n";
        let tools = parse_server_probe(output).unwrap();
        assert_eq!(
            tools,
            [
                tool(ToolKind::SevenZip, "7zz"),
                tool(ToolKind::Unzip, "unzip"),
                tool(ToolKind::GnuTar, "tar"),
            ]
        );
        let free = "tool unrar\ntool unrar-free\nunrar unrar: Archive not specified\n\
                    poros-archive-tools\n";
        assert_eq!(
            parse_server_probe(free).unwrap(),
            [tool(ToolKind::UnrarFree, "unrar")]
        );
        let rarlab = "tool unrar\nunrar UNRAR 7.01 freeware      Copyright (c) 1993-2024\n\
                      poros-archive-tools\n";
        assert_eq!(parse_server_probe(rarlab).unwrap()[0].kind, ToolKind::UnRar);
        let busybox = "tool tar\ntar tar: unrecognized option\nporos-archive-tools\n";
        assert_eq!(
            parse_server_probe(busybox).unwrap()[0].kind,
            ToolKind::BsdTar
        );
        assert_eq!(
            parse_server_probe("This service allows sftp connections only."),
            None
        );
    }

    #[test]
    fn probes_under_sh() {
        let probe = server_probe();
        assert!(probe.starts_with("sh -c '"));
        assert!(probe.contains("command -v"));
        assert!(probe.ends_with("echo poros-archive-tools'"));
    }
}
