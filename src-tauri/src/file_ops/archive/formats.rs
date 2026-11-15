//! Telling archives apart: by name, and by their first bytes, which win when the two disagree.

use serde::{Deserialize, Serialize};

/// How much of a file to read to know what it is: a tar header ends at 512 bytes.
pub const HEAD_BYTES: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
    Rar,
    Tar,
    TarGzip,
    TarBzip2,
    TarXz,
    TarZstd,
    TarLzma,
    /// One compressed file, not an archive of several.
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lzma,
    Iso,
    Cab,
}

/// What a single stream is compressed with, for archives that compress a tar file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Lzma,
}

/// Name endings, each before any it ends with, so `.tar.gz` wins over `.gz`.
const ENDINGS: &[(&str, ArchiveFormat)] = &[
    (".tar.gz", ArchiveFormat::TarGzip),
    (".tar.bz2", ArchiveFormat::TarBzip2),
    (".tar.zstd", ArchiveFormat::TarZstd),
    (".tar.lzma", ArchiveFormat::TarLzma),
    (".tar.xz", ArchiveFormat::TarXz),
    (".tar.zst", ArchiveFormat::TarZstd),
    (".nupkg", ArchiveFormat::Zip),
    (".tbz2", ArchiveFormat::TarBzip2),
    (".tzst", ArchiveFormat::TarZstd),
    (".lzma", ArchiveFormat::Lzma),
    (".zstd", ArchiveFormat::Zstd),
    (".tgz", ArchiveFormat::TarGzip),
    (".tbz", ArchiveFormat::TarBzip2),
    (".txz", ArchiveFormat::TarXz),
    (".tlz", ArchiveFormat::TarLzma),
    (".tar", ArchiveFormat::Tar),
    (".zip", ArchiveFormat::Zip),
    (".jar", ArchiveFormat::Zip),
    (".war", ArchiveFormat::Zip),
    (".ear", ArchiveFormat::Zip),
    (".apk", ArchiveFormat::Zip),
    (".whl", ArchiveFormat::Zip),
    (".rar", ArchiveFormat::Rar),
    (".bz2", ArchiveFormat::Bzip2),
    (".zst", ArchiveFormat::Zstd),
    (".iso", ArchiveFormat::Iso),
    (".cab", ArchiveFormat::Cab),
    (".7z", ArchiveFormat::SevenZip),
    (".gz", ArchiveFormat::Gzip),
    (".xz", ArchiveFormat::Xz),
];

impl ArchiveFormat {
    /// The format a name says, from its ending.
    pub fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        ENDINGS
            .iter()
            .find(|(ending, _)| lower.len() > ending.len() && lower.ends_with(ending))
            .map(|&(_, format)| format)
    }

    /// The format a file is, from its first bytes and its name. The bytes decide what kind of
    /// archive it is; the name, whether a compressed file holds a tar archive.
    pub fn detect(name: &str, head: &[u8]) -> Option<Self> {
        let named = Self::from_name(name);
        let Some(sniffed) = sniff(head) else {
            return named;
        };
        match sniffed {
            Sniffed::Archive(format) => Some(format),
            Sniffed::Compressed(compression) => Some(match named {
                Some(format) if format.compression() == Some(compression) => format,
                Some(format) if format.is_tar() => tar_with(compression),
                _ => single(compression),
            }),
        }
    }

    pub fn is_tar(self) -> bool {
        matches!(
            self,
            Self::Tar
                | Self::TarGzip
                | Self::TarBzip2
                | Self::TarXz
                | Self::TarZstd
                | Self::TarLzma
        )
    }

    /// One compressed file, which unpacks to a single file rather than a folder of them.
    pub fn is_single_file(self) -> bool {
        matches!(
            self,
            Self::Gzip | Self::Bzip2 | Self::Xz | Self::Zstd | Self::Lzma
        )
    }

    fn compression(self) -> Option<Compression> {
        match self {
            Self::TarGzip | Self::Gzip => Some(Compression::Gzip),
            Self::TarBzip2 | Self::Bzip2 => Some(Compression::Bzip2),
            Self::TarXz | Self::Xz => Some(Compression::Xz),
            Self::TarZstd | Self::Zstd => Some(Compression::Zstd),
            Self::TarLzma | Self::Lzma => Some(Compression::Lzma),
            _ => None,
        }
    }

    /// How people name it, for messages.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::SevenZip => "7z",
            Self::Rar => "RAR",
            Self::Tar => "tar",
            Self::TarGzip => "tar.gz",
            Self::TarBzip2 => "tar.bz2",
            Self::TarXz => "tar.xz",
            Self::TarZstd => "tar.zst",
            Self::TarLzma => "tar.lzma",
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz => "xz",
            Self::Zstd => "Zstandard",
            Self::Lzma => "LZMA",
            Self::Iso => "ISO",
            Self::Cab => "CAB",
        }
    }
}

fn tar_with(compression: Compression) -> ArchiveFormat {
    match compression {
        Compression::Gzip => ArchiveFormat::TarGzip,
        Compression::Bzip2 => ArchiveFormat::TarBzip2,
        Compression::Xz => ArchiveFormat::TarXz,
        Compression::Zstd => ArchiveFormat::TarZstd,
        Compression::Lzma => ArchiveFormat::TarLzma,
    }
}

fn single(compression: Compression) -> ArchiveFormat {
    match compression {
        Compression::Gzip => ArchiveFormat::Gzip,
        Compression::Bzip2 => ArchiveFormat::Bzip2,
        Compression::Xz => ArchiveFormat::Xz,
        Compression::Zstd => ArchiveFormat::Zstd,
        Compression::Lzma => ArchiveFormat::Lzma,
    }
}

enum Sniffed {
    Archive(ArchiveFormat),
    Compressed(Compression),
}

fn sniff(head: &[u8]) -> Option<Sniffed> {
    const TAR_MAGIC_OFFSET: usize = 257;
    let starts = |magic: &[u8]| head.starts_with(magic);
    if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") || starts(b"PK\x07\x08") {
        return Some(Sniffed::Archive(ArchiveFormat::Zip));
    }
    if starts(b"7z\xBC\xAF\x27\x1C") {
        return Some(Sniffed::Archive(ArchiveFormat::SevenZip));
    }
    if starts(b"Rar!\x1A\x07") {
        return Some(Sniffed::Archive(ArchiveFormat::Rar));
    }
    if starts(b"MSCF") {
        return Some(Sniffed::Archive(ArchiveFormat::Cab));
    }
    if starts(b"\x1F\x8B") {
        return Some(Sniffed::Compressed(Compression::Gzip));
    }
    if starts(b"BZh") {
        return Some(Sniffed::Compressed(Compression::Bzip2));
    }
    if starts(b"\xFD7zXZ\x00") {
        return Some(Sniffed::Compressed(Compression::Xz));
    }
    if starts(b"\x28\xB5\x2F\xFD") {
        return Some(Sniffed::Compressed(Compression::Zstd));
    }
    if head
        .get(TAR_MAGIC_OFFSET..TAR_MAGIC_OFFSET + 5)
        .is_some_and(|magic| magic == b"ustar")
    {
        return Some(Sniffed::Archive(ArchiveFormat::Tar));
    }
    None
}

/// The name of what a file unpacks to: `site.tar.gz` -> `site`, `notes.txt.gz` -> `notes.txt`,
/// `photos.part1.rar` -> `photos`. Never empty.
pub fn unpacked_name(name: &str, format: ArchiveFormat) -> String {
    let lower = name.to_ascii_lowercase();
    let ending = ENDINGS
        .iter()
        .filter(|(_, named)| *named == format || ArchiveFormat::from_name(name) == Some(*named))
        .map(|(ending, _)| *ending)
        .find(|ending| lower.len() > ending.len() && lower.ends_with(ending));
    let stem = match ending {
        Some(ending) => &name[..name.len() - ending.len()],
        None => name,
    };
    let stem = strip_volume(stem);
    let stem = stem.trim_end_matches(['.', ' ']);
    if stem.is_empty() || stem == name {
        if format.is_single_file() {
            format!("{name}.out")
        } else {
            format!("{name} files")
        }
    } else {
        stem.to_string()
    }
}

/// `photos.part1` -> `photos`, the first volume of a split RAR archive.
fn strip_volume(stem: &str) -> &str {
    let lower = stem.to_ascii_lowercase();
    match lower.rfind(".part") {
        Some(dot)
            if dot > 0
                && lower.len() > dot + 5
                && lower[dot + 5..].bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            &stem[..dot]
        }
        _ => stem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_head() -> Vec<u8> {
        let mut head = vec![0u8; 512];
        head[257..262].copy_from_slice(b"ustar");
        head
    }

    #[test]
    fn reads_names_longest_ending_first() {
        assert_eq!(
            ArchiveFormat::from_name("site.tar.gz"),
            Some(ArchiveFormat::TarGzip)
        );
        assert_eq!(
            ArchiveFormat::from_name("SITE.TGZ"),
            Some(ArchiveFormat::TarGzip)
        );
        assert_eq!(
            ArchiveFormat::from_name("notes.txt.gz"),
            Some(ArchiveFormat::Gzip)
        );
        assert_eq!(
            ArchiveFormat::from_name("backup.tar.zst"),
            Some(ArchiveFormat::TarZstd)
        );
        assert_eq!(
            ArchiveFormat::from_name("game.7z"),
            Some(ArchiveFormat::SevenZip)
        );
        assert_eq!(
            ArchiveFormat::from_name("app.jar"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(ArchiveFormat::from_name("readme.md"), None);
        assert_eq!(ArchiveFormat::from_name(".zip"), None);
    }

    #[test]
    fn trusts_the_first_bytes_over_the_name() {
        assert_eq!(
            ArchiveFormat::detect("download.bin", b"PK\x03\x04rest"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::detect("really-a-rar.zip", b"Rar!\x1A\x07\x01\x00"),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::detect("plain.tar.gz", &tar_head()),
            Some(ArchiveFormat::Tar)
        );
        assert_eq!(
            ArchiveFormat::detect("unknown", b"7z\xBC\xAF\x27\x1C"),
            Some(ArchiveFormat::SevenZip)
        );
    }

    #[test]
    fn keeps_tar_from_the_name_for_compressed_files() {
        assert_eq!(
            ArchiveFormat::detect("site.tar.gz", b"\x1F\x8B\x08"),
            Some(ArchiveFormat::TarGzip)
        );
        assert_eq!(
            ArchiveFormat::detect("site.tar.gz", b"\xFD7zXZ\x00"),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::detect("dump.sql.gz", b"\x1F\x8B\x08"),
            Some(ArchiveFormat::Gzip)
        );
        assert_eq!(
            ArchiveFormat::detect("dump.sql.xz", b"BZh9"),
            Some(ArchiveFormat::Bzip2)
        );
        assert_eq!(
            ArchiveFormat::detect("disk.iso", b"\0\0\0\0"),
            Some(ArchiveFormat::Iso)
        );
        assert_eq!(ArchiveFormat::detect("readme", b"hello"), None);
    }

    #[test]
    fn names_what_comes_out() {
        assert_eq!(unpacked_name("site.tar.gz", ArchiveFormat::TarGzip), "site");
        assert_eq!(unpacked_name("Site.TGZ", ArchiveFormat::TarGzip), "Site");
        assert_eq!(
            unpacked_name("dump.sql.gz", ArchiveFormat::Gzip),
            "dump.sql"
        );
        assert_eq!(
            unpacked_name("photos.part1.rar", ArchiveFormat::Rar),
            "photos"
        );
        assert_eq!(unpacked_name("v1.2.zip", ArchiveFormat::Zip), "v1.2");
        assert_eq!(unpacked_name("plain.tar.gz", ArchiveFormat::Tar), "plain");
        assert_eq!(
            unpacked_name("download", ArchiveFormat::Zip),
            "download files"
        );
        assert_eq!(unpacked_name("blob", ArchiveFormat::Gzip), "blob.out");
    }
}
