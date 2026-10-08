//! Directory listings as FTP servers send them: machine-readable MLSD facts (RFC 3659) when the
//! server offers them, otherwise `ls -l` style or DOS style LIST output.

use crate::model::EntryKind;
use crate::protocol::RemoteStat;
use crate::timestamp::{month_number, parse_ftp_time, unix_seconds, year_of};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<i64>,
    pub permissions: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

impl RawEntry {
    pub fn stat(&self) -> RemoteStat {
        RemoteStat {
            is_dir: self.kind == EntryKind::Dir,
            size: self.size,
            modified: self.modified,
            permissions: self.permissions,
        }
    }
}

pub fn parse_mlsd(text: &str) -> Vec<RawEntry> {
    text.lines()
        .filter_map(parse_mlsd_line)
        .filter(|entry| entry.name != "." && entry.name != "..")
        .collect()
}

/// `type=file;size=12;modify=20240101120000;UNIX.mode=0644; name`. The current and parent
/// folder entries (`cdir`, `pdir`) are left out.
pub fn parse_mlsd_line(line: &str) -> Option<RawEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (facts, name) = line.split_once(' ')?;
    if name.is_empty() {
        return None;
    }
    let mut entry = RawEntry {
        name: name.to_string(),
        kind: EntryKind::Other,
        size: 0,
        modified: None,
        permissions: None,
        owner: None,
        group: None,
    };
    let mut owner_id = None;
    let mut group_id = None;
    for fact in facts.split(';').filter(|fact| !fact.is_empty()) {
        let Some((key, value)) = fact.split_once('=') else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "type" => {
                let value = value.to_ascii_lowercase();
                entry.kind = match value.as_str() {
                    "file" => EntryKind::File,
                    "dir" => EntryKind::Dir,
                    "cdir" | "pdir" => return None,
                    other if other.starts_with("os.unix=slink") || other == "os.unix=symlink" => {
                        EntryKind::Symlink
                    }
                    _ => EntryKind::Other,
                };
            }
            "size" => entry.size = value.parse().unwrap_or(0),
            "modify" => entry.modified = parse_ftp_time(value),
            "unix.mode" => entry.permissions = u32::from_str_radix(value, 8).ok(),
            "unix.ownername" => entry.owner = Some(value.to_string()),
            "unix.groupname" => entry.group = Some(value.to_string()),
            "unix.owner" | "unix.uid" => owner_id = Some(value.to_string()),
            "unix.group" | "unix.gid" => group_id = Some(value.to_string()),
            _ => {}
        }
    }
    entry.owner = entry.owner.or(owner_id);
    entry.group = entry.group.or(group_id);
    if entry.kind == EntryKind::Dir {
        entry.size = 0;
    }
    entry.permissions = entry.permissions.map(|mode| mode & 0o7777);
    Some(entry)
}

/// `now` decides the year of recent entries, which `ls -l` lists with a time instead.
pub fn parse_list(text: &str, now: i64) -> Vec<RawEntry> {
    text.lines()
        .filter_map(|line| parse_unix_line(line, now).or_else(|| parse_dos_line(line)))
        .filter(|entry| entry.name != "." && entry.name != "..")
        .collect()
}

/// Each whitespace-separated field with the byte offset just past its end.
fn fields(line: &str) -> Vec<(&str, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, character) in line.char_indices() {
        match (character.is_whitespace(), start) {
            (true, Some(begin)) => {
                result.push((&line[begin..index], index));
                start = None;
            }
            (false, None) => start = Some(index),
            _ => {}
        }
    }
    if let Some(begin) = start {
        result.push((&line[begin..], line.len()));
    }
    result
}

/// `drwxr-xr-x 2 owner group 4096 Jan  1 12:34 name`, with or without the group, with a year
/// instead of a time for older entries, or with ISO dates.
fn parse_unix_line(line: &str, now: i64) -> Option<RawEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    let fields = fields(line);
    let (mode_text, _) = *fields.first()?;
    let type_character = mode_text.chars().next()?;
    if mode_text.len() < 10 || !"-dlbcps".contains(type_character) {
        return None;
    }
    let kind = match type_character {
        'd' => EntryKind::Dir,
        '-' => EntryKind::File,
        'l' => EntryKind::Symlink,
        _ => EntryKind::Other,
    };

    let (date_index, modified, name_start) =
        (3..fields.len().saturating_sub(1)).find_map(|index| {
            let (first, _) = fields[index];
            let (second, second_end) = fields[index + 1];
            if let Some(modified) = iso_date(first, second) {
                return Some((index, modified, second_end));
            }
            let (third, third_end) = *fields.get(index + 2)?;
            month_date(first, second, third, now).map(|modified| (index, modified, third_end))
        })?;
    let size_index = date_index.checked_sub(1)?;
    let size: u64 = fields[size_index].0.parse().ok()?;
    let name = line.get(name_start + 1..)?;
    let name = match kind {
        EntryKind::Symlink => name.split(" -> ").next().unwrap_or(name),
        _ => name,
    };
    if name.is_empty() {
        return None;
    }
    let owner = (size_index > 2).then(|| fields[2].0.to_string());
    let group = (size_index > 3).then(|| fields[3].0.to_string());
    Some(RawEntry {
        name: name.to_string(),
        kind,
        size: if kind == EntryKind::Dir { 0 } else { size },
        modified: Some(modified),
        permissions: mode_from_symbolic(mode_text),
        owner,
        group,
    })
}

fn iso_date(date: &str, time: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if year.len() != 4 || parts.next().is_some() {
        return None;
    }
    let (hour, minute) = time.split_once(':')?;
    let minute = minute.get(..2)?;
    unix_seconds(
        year.parse().ok()?,
        month.parse().ok()?,
        day.parse().ok()?,
        hour.parse().ok()?,
        minute.parse().ok()?,
        0,
    )
}

fn month_date(month: &str, day: &str, time_or_year: &str, now: i64) -> Option<i64> {
    if month.len() != 3 {
        return None;
    }
    let month = month_number(month)?;
    let day: u32 = day.parse().ok()?;
    if let Some((hour, minute)) = time_or_year.split_once(':') {
        let (hour, minute): (u32, u32) = (hour.parse().ok()?, minute.parse().ok()?);
        let this_year = year_of(now);
        let candidate = unix_seconds(this_year, month, day, hour, minute, 0)?;
        // `ls` shows a time only for the last six months, so a date ahead of now is last year.
        if candidate > now + 2 * 86_400 {
            unix_seconds(this_year - 1, month, day, hour, minute, 0)
        } else {
            Some(candidate)
        }
    } else {
        let year: i64 = time_or_year.parse().ok()?;
        if !(1900..=9999).contains(&year) {
            return None;
        }
        unix_seconds(year, month, day, 0, 0, 0)
    }
}

/// `rwxr-sr-t` style permissions, including set-id and sticky bits.
fn mode_from_symbolic(text: &str) -> Option<u32> {
    let bits: Vec<char> = text.chars().skip(1).take(9).collect();
    if bits.len() != 9 {
        return None;
    }
    let mut mode = 0;
    for (index, character) in bits.iter().enumerate() {
        let position = 8 - index as u32;
        let set = match (index % 3, character) {
            (_, '-') => false,
            (0, 'r') | (1, 'w') | (2, 'x' | 's' | 't') => true,
            (2, 'S' | 'T') => false,
            _ => return None,
        };
        if set {
            mode |= 1 << position;
        }
        let special = match (index, character) {
            (2, 's' | 'S') => 0o4000,
            (5, 's' | 'S') => 0o2000,
            (8, 't' | 'T') => 0o1000,
            _ => 0,
        };
        mode |= special;
    }
    Some(mode)
}

/// `01-15-24  03:49PM       <DIR>          name` or with a size in place of `<DIR>`, as IIS
/// and other Windows servers list.
fn parse_dos_line(line: &str) -> Option<RawEntry> {
    let line = line.trim_end_matches(['\r', '\n']);
    let fields = fields(line);
    let (date, _) = *fields.first()?;
    let (time, _) = *fields.get(1)?;
    let (size_or_dir, size_end) = *fields.get(2)?;
    let mut date_parts = date.split(['-', '/']);
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let year_text = date_parts.next()?;
    let year: i64 = year_text.parse().ok()?;
    let year = match year_text.len() {
        2 if year < 70 => 2000 + year,
        2 => 1900 + year,
        4 => year,
        _ => return None,
    };
    let upper = time.to_ascii_uppercase();
    let (clock, afternoon) = match (upper.strip_suffix("PM"), upper.strip_suffix("AM")) {
        (Some(clock), _) => (clock.to_string(), Some(true)),
        (_, Some(clock)) => (clock.to_string(), Some(false)),
        _ => (upper.clone(), None),
    };
    let (hour, minute) = clock.split_once(':')?;
    let mut hour: u32 = hour.parse().ok()?;
    let minute: u32 = minute.get(..2)?.parse().ok()?;
    match afternoon {
        Some(true) if hour < 12 => hour += 12,
        Some(false) if hour == 12 => hour = 0,
        _ => {}
    }
    let modified = unix_seconds(year, month, day, hour, minute, 0)?;
    let (kind, size) = if size_or_dir.eq_ignore_ascii_case("<DIR>") {
        (EntryKind::Dir, 0)
    } else {
        (EntryKind::File, size_or_dir.replace(',', "").parse().ok()?)
    };
    let name = line.get(size_end..)?.trim_start();
    if name.is_empty() {
        return None;
    }
    Some(RawEntry {
        name: name.to_string(),
        kind,
        size,
        modified: Some(modified),
        permissions: None,
        owner: None,
        group: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2024-10-08 12:00:00 UTC.
    const NOW: i64 = 1_728_388_800;

    #[test]
    fn parses_mlsd_facts() {
        let text = "type=cdir;modify=20241008120000; .\r\n\
                    type=pdir;modify=20241008120000; ..\r\n\
                    type=dir;sizd=4096;modify=20241001093000;UNIX.mode=0755;UNIX.uid=1000;UNIX.gid=1000; www\r\n\
                    type=file;size=1234;modify=20240101000000.500;UNIX.mode=0644;UNIX.ownername=alice;UNIX.groupname=staff; my file.txt\r\n\
                    type=OS.unix=slink:/srv;modify=20240101000000; srv-link\r\n";
        let entries = parse_mlsd(text);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "www");
        assert_eq!(entries[0].kind, EntryKind::Dir);
        assert_eq!(entries[0].size, 0);
        assert_eq!(entries[0].permissions, Some(0o755));
        assert_eq!(entries[0].owner.as_deref(), Some("1000"));
        assert_eq!(entries[1].name, "my file.txt");
        assert_eq!(entries[1].size, 1234);
        assert_eq!(entries[1].modified, Some(1_704_067_200));
        assert_eq!(entries[1].owner.as_deref(), Some("alice"));
        assert_eq!(entries[1].group.as_deref(), Some("staff"));
        assert_eq!(entries[2].kind, EntryKind::Symlink);
    }

    #[test]
    fn parses_unix_listings() {
        let text = "total 12\r\n\
                    drwxr-xr-x    2 1000     1000         4096 Oct 08 09:30 docs\r\n\
                    -rw-r--r--    1 ftp      ftp      12345678 Jan 01  2023  spaced name.bin\r\n\
                    lrwxrwxrwx    1 0        0               7 Oct 08 09:30 link -> target\r\n\
                    -rwsr-xr-t    1 owner           42 Dec 31 23:00 nogroup\r\n\
                    -rw-r--r--    1 alice    staff          10 2024-03-05 14:07 iso.txt\r\n";
        let entries = parse_list(text, NOW);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].name, "docs");
        assert_eq!(entries[0].kind, EntryKind::Dir);
        assert_eq!(entries[0].permissions, Some(0o755));
        assert_eq!(entries[0].modified, unix_seconds(2024, 10, 8, 9, 30, 0));
        assert_eq!(entries[1].name, " spaced name.bin");
        assert_eq!(entries[1].size, 12_345_678);
        assert_eq!(entries[1].modified, unix_seconds(2023, 1, 1, 0, 0, 0));
        assert_eq!(entries[1].owner.as_deref(), Some("ftp"));
        assert_eq!(entries[2].name, "link");
        assert_eq!(entries[2].kind, EntryKind::Symlink);
        assert_eq!(entries[3].name, "nogroup");
        assert_eq!(entries[3].group, None);
        assert_eq!(entries[3].permissions, Some(0o4755 | 0o1000));
        // December 31 would be ahead of October 8, so it is last year's.
        assert_eq!(entries[3].modified, unix_seconds(2023, 12, 31, 23, 0, 0));
        assert_eq!(entries[4].name, "iso.txt");
        assert_eq!(entries[4].modified, unix_seconds(2024, 3, 5, 14, 7, 0));
    }

    #[test]
    fn parses_dos_listings() {
        let text = "10-08-24  03:49PM       <DIR>          My Folder\r\n\
                    01-15-2023  09:05AM            1,234 report.pdf\r\n";
        let entries = parse_list(text, NOW);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "My Folder");
        assert_eq!(entries[0].kind, EntryKind::Dir);
        assert_eq!(entries[0].modified, unix_seconds(2024, 10, 8, 15, 49, 0));
        assert_eq!(entries[1].size, 1234);
        assert_eq!(entries[1].modified, unix_seconds(2023, 1, 15, 9, 5, 0));
    }

    #[test]
    fn ignores_noise() {
        assert!(parse_list("total 0\r\n\r\nsomething else\r\n", NOW).is_empty());
        assert_eq!(parse_mlsd_line("no-facts"), None);
    }
}
