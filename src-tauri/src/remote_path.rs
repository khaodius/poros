//! Remote paths are always `/`-separated, so `std::path` must not be used for them.

pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub fn parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(separator) => Some(trimmed[..separator].to_string()),
        None => None,
    }
}

pub fn file_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins() {
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/home/u", "f.txt"), "/home/u/f.txt");
        assert_eq!(join("/home/u/", "f.txt"), "/home/u/f.txt");
    }

    #[test]
    fn parents() {
        assert_eq!(parent("/"), None);
        assert_eq!(parent("/etc"), Some("/".into()));
        assert_eq!(parent("/home/u"), Some("/home".into()));
        assert_eq!(parent("/home/u/"), Some("/home".into()));
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("/home/u/f.txt"), "f.txt");
        assert_eq!(file_name("/home/u/"), "u");
        assert_eq!(file_name("rel"), "rel");
    }
}
