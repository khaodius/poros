//! What a search looks for: checked once when the search starts, then asked about every entry.

use serde::Deserialize;

use super::pattern::NamePattern;
use super::SearchRequest;
use crate::error::{AppError, AppResult};
use crate::model::FileEntry;

pub const DEFAULT_LIMIT: u32 = 10_000;
pub const MAX_LIMIT: u32 = 100_000;
/// Folders nested deeper than this below the start are not entered, whatever was asked.
pub const MAX_DEPTH: u32 = 200;
const MAX_PATH_LENGTH: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KindFilter {
    #[default]
    Any,
    /// Everything that is not a folder: files, links to files, broken links.
    Files,
    /// Folders and links to folders.
    Folders,
}

#[derive(Debug, Clone)]
pub struct Criteria {
    pub name: NamePattern,
    pub kind: KindFilter,
    /// In bytes, both ends included. Folders have no size, so a size limit leaves them out.
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// In seconds since the Unix epoch, both ends included.
    pub modified_after: Option<i64>,
    pub modified_before: Option<i64>,
    /// 1 looks only at what is directly in the start folder.
    pub max_depth: Option<u32>,
    /// Whether hidden folders are entered. A hidden item in a folder that is searched is
    /// found either way.
    pub enter_hidden: bool,
    pub follow_links: bool,
    pub limit: usize,
}

impl Criteria {
    pub fn from_request(request: &SearchRequest) -> AppResult<Self> {
        if request.folder.is_empty() {
            return Err(AppError::invalid("Choose a folder to search in"));
        }
        if request.folder.len() > MAX_PATH_LENGTH || request.folder.contains('\0') {
            return Err(AppError::invalid("That folder cannot be searched"));
        }
        if let (Some(least), Some(most)) = (request.min_size, request.max_size) {
            if least > most {
                return Err(AppError::invalid(
                    "The smallest size is larger than the largest size",
                ));
            }
        }
        if let (Some(after), Some(before)) = (request.modified_after, request.modified_before) {
            if after > before {
                return Err(AppError::invalid(
                    "The earliest date is later than the latest date",
                ));
            }
        }
        if request.max_depth == Some(0) {
            return Err(AppError::invalid("The depth has to be at least 1"));
        }
        let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
        if limit == 0 || limit > MAX_LIMIT {
            return Err(AppError::invalid(format!(
                "The number of results has to be between 1 and {MAX_LIMIT}"
            )));
        }
        Ok(Self {
            name: NamePattern::new(
                &request.pattern,
                request.pattern_kind,
                request.case_sensitive,
            )?,
            kind: request.kind,
            min_size: request.min_size,
            max_size: request.max_size,
            modified_after: request.modified_after,
            modified_before: request.modified_before,
            max_depth: request.max_depth,
            enter_hidden: request.hidden,
            follow_links: request.follow_links,
            limit: limit as usize,
        })
    }

    /// The deepest level looked at, counting what is directly in the start folder as 1.
    pub fn depth_limit(&self) -> u32 {
        self.max_depth.unwrap_or(MAX_DEPTH).min(MAX_DEPTH)
    }

    pub fn matches(&self, entry: &FileEntry) -> bool {
        self.matches_kind(entry)
            && self.name.matches(&entry.name)
            && self.matches_size(entry)
            && self.matches_date(entry)
    }

    /// The name and kind alone, which never change when a link is looked up.
    pub fn matches_name_and_kind(&self, entry: &FileEntry) -> bool {
        self.matches_kind(entry) && self.name.matches(&entry.name)
    }

    fn matches_kind(&self, entry: &FileEntry) -> bool {
        match self.kind {
            KindFilter::Any => true,
            KindFilter::Files => !entry.is_dir_like(),
            KindFilter::Folders => entry.is_dir_like(),
        }
    }

    fn matches_size(&self, entry: &FileEntry) -> bool {
        if self.min_size.is_none() && self.max_size.is_none() {
            return true;
        }
        !entry.is_dir_like()
            && self.min_size.is_none_or(|least| entry.size >= least)
            && self.max_size.is_none_or(|most| entry.size <= most)
    }

    fn matches_date(&self, entry: &FileEntry) -> bool {
        if self.modified_after.is_none() && self.modified_before.is_none() {
            return true;
        }
        entry.modified.is_some_and(|modified| {
            self.modified_after.is_none_or(|after| modified >= after)
                && self.modified_before.is_none_or(|before| modified <= before)
        })
    }

    pub fn limits_size_or_date(&self) -> bool {
        self.min_size.is_some()
            || self.max_size.is_some()
            || self.modified_after.is_some()
            || self.modified_before.is_some()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::file_ops::Location;
    use crate::model::{EntryKind, LinkTarget};

    pub fn request(pattern: &str) -> SearchRequest {
        SearchRequest {
            search_id: "search-1".to_string(),
            location: Location::Local,
            folder: "/srv".to_string(),
            pattern: pattern.to_string(),
            pattern_kind: Default::default(),
            case_sensitive: false,
            kind: KindFilter::Any,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            max_depth: None,
            hidden: false,
            follow_links: false,
            limit: None,
        }
    }

    pub fn entry(name: &str, kind: EntryKind, size: u64, modified: Option<i64>) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: format!("/srv/{name}"),
            kind,
            link_target: None,
            size,
            modified,
            permissions: None,
            owner: None,
            group: None,
            hidden: name.starts_with('.'),
        }
    }

    fn link(name: &str, target: LinkTarget) -> FileEntry {
        FileEntry {
            link_target: Some(target),
            ..entry(name, EntryKind::Symlink, 12, Some(1_000))
        }
    }

    #[test]
    fn defaults_find_everything_up_to_the_default_limit() {
        let criteria = Criteria::from_request(&request("")).unwrap();
        assert_eq!(criteria.limit, DEFAULT_LIMIT as usize);
        assert_eq!(criteria.depth_limit(), MAX_DEPTH);
        assert!(criteria.matches(&entry("a.txt", EntryKind::File, 0, None)));
        assert!(criteria.matches(&entry("folder", EntryKind::Dir, 0, None)));
    }

    #[test]
    fn kinds_count_links_by_what_they_lead_to() {
        let files = Criteria::from_request(&SearchRequest {
            kind: KindFilter::Files,
            ..request("")
        })
        .unwrap();
        let folders = Criteria::from_request(&SearchRequest {
            kind: KindFilter::Folders,
            ..request("")
        })
        .unwrap();
        let file = entry("a.txt", EntryKind::File, 1, None);
        let folder = entry("folder", EntryKind::Dir, 0, None);
        assert!(files.matches(&file) && !folders.matches(&file));
        assert!(!files.matches(&folder) && folders.matches(&folder));
        assert!(folders.matches(&link("to-folder", LinkTarget::Dir)));
        assert!(!files.matches(&link("to-folder", LinkTarget::Dir)));
        assert!(files.matches(&link("to-file", LinkTarget::File)));
        assert!(files.matches(&link("dangling", LinkTarget::Broken)));
        assert!(files.matches(&entry("socket", EntryKind::Other, 0, None)));
    }

    #[test]
    fn sizes_include_both_ends_and_leave_folders_out() {
        let criteria = Criteria::from_request(&SearchRequest {
            min_size: Some(100),
            max_size: Some(200),
            ..request("")
        })
        .unwrap();
        let sized = |size| entry("file", EntryKind::File, size, None);
        assert!(!criteria.matches(&sized(99)));
        assert!(criteria.matches(&sized(100)));
        assert!(criteria.matches(&sized(200)));
        assert!(!criteria.matches(&sized(201)));
        assert!(!criteria.matches(&entry("folder", EntryKind::Dir, 150, None)));
        assert!(!criteria.matches(&link("to-folder", LinkTarget::Dir)));

        let at_least = Criteria::from_request(&SearchRequest {
            min_size: Some(1),
            ..request("")
        })
        .unwrap();
        assert!(!at_least.matches(&sized(0)));
        assert!(at_least.matches(&sized(u64::MAX)));
    }

    #[test]
    fn dates_include_both_ends_and_need_a_date() {
        let criteria = Criteria::from_request(&SearchRequest {
            modified_after: Some(1_000),
            modified_before: Some(2_000),
            ..request("")
        })
        .unwrap();
        let dated = |modified| entry("file", EntryKind::File, 1, modified);
        assert!(!criteria.matches(&dated(Some(999))));
        assert!(criteria.matches(&dated(Some(1_000))));
        assert!(criteria.matches(&dated(Some(2_000))));
        assert!(!criteria.matches(&dated(Some(2_001))));
        assert!(!criteria.matches(&dated(None)));
        assert!(criteria.matches(&entry("folder", EntryKind::Dir, 0, Some(1_500))));
    }

    #[test]
    fn depth_is_capped() {
        let depth = |max_depth| {
            Criteria::from_request(&SearchRequest {
                max_depth,
                ..request("")
            })
            .map(|criteria| criteria.depth_limit())
        };
        assert_eq!(depth(Some(3)).unwrap(), 3);
        assert_eq!(depth(Some(u32::MAX)).unwrap(), MAX_DEPTH);
        assert!(depth(Some(0)).is_err());
    }

    #[test]
    fn requests_that_make_no_sense_are_refused() {
        let refused = |request: SearchRequest| Criteria::from_request(&request).is_err();
        assert!(refused(SearchRequest {
            folder: String::new(),
            ..request("")
        }));
        assert!(refused(SearchRequest {
            folder: "/a\0b".to_string(),
            ..request("")
        }));
        assert!(refused(SearchRequest {
            min_size: Some(2),
            max_size: Some(1),
            ..request("")
        }));
        assert!(refused(SearchRequest {
            modified_after: Some(2),
            modified_before: Some(1),
            ..request("")
        }));
        assert!(refused(SearchRequest {
            limit: Some(0),
            ..request("")
        }));
        assert!(refused(SearchRequest {
            limit: Some(MAX_LIMIT + 1),
            ..request("")
        }));
        assert!(refused(SearchRequest {
            pattern_kind: crate::search::PatternKind::Regex,
            ..request("[")
        }));
    }
}
