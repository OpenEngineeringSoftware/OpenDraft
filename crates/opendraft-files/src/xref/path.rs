//! The saved path's form, resolving it against the host drawing's folder, and
//! the source file's stamp.
//!
//! The string is stored exactly as the file spells it. Only [`resolve`] turns
//! it into a path to read, and only when the user attaches or reloads.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use opendraft_core::{path_problem, XrefPathType, XrefProblem, XrefStamp};

/// Why a saved path cannot be turned into a file to read.
#[derive(Debug, PartialEq)]
pub enum XrefPathError {
    Problem(XrefProblem),
    /// A relative path in a drawing that has never been saved.
    NoHostFolder,
}

impl fmt::Display for XrefPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Problem(problem) => write!(f, "{problem}"),
            Self::NoHostFolder => write!(
                f,
                "save the drawing first, or attach the source by its full path"
            ),
        }
    }
}

impl std::error::Error for XrefPathError {}

/// The form a saved path has, from how it is spelled.
pub fn classify(saved: &str) -> XrefPathType {
    let saved = saved.trim();
    if saved.is_empty() {
        return XrefPathType::NoPath;
    }
    if Path::new(saved).is_absolute() {
        return XrefPathType::Full;
    }
    if saved.contains(['/', '\\']) {
        XrefPathType::Relative
    } else {
        // A bare file name, read beside the host drawing.
        XrefPathType::NoPath
    }
}

/// The file a saved path names. A relative path or a bare file name is looked
/// for beside the host drawing (`host_dir`); a full path is used as it is.
/// Nothing is canonicalised: `..` is left to the operating system.
pub fn resolve(saved: &str, host_dir: Option<&Path>) -> Result<PathBuf, XrefPathError> {
    if let Some(problem) = path_problem(saved) {
        return Err(XrefPathError::Problem(problem));
    }
    let raw = PathBuf::from(saved.trim());
    let full = if raw.is_absolute() {
        raw
    } else {
        host_dir.ok_or(XrefPathError::NoHostFolder)?.join(raw)
    };
    // A drawing moved from Windows: its backslashes are literal on Unix, so
    // try the same path with separators the platform understands.
    #[cfg(unix)]
    {
        if !full.exists() && saved.contains('\\') {
            let mut alternate = PathBuf::new();
            for part in full.to_string_lossy().split('\\') {
                alternate.push(part);
            }
            if alternate.exists() {
                return Ok(alternate);
            }
        }
    }
    Ok(full)
}

/// How `source` is written in the host drawing with `host_dir`, for a chosen
/// form. Returns the string, the form actually used (relative falls back to
/// full across drives), and a note when it did.
pub fn format_saved_path(
    source: &Path,
    host_dir: Option<&Path>,
    kind: XrefPathType,
) -> (String, XrefPathType, Option<String>) {
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string_lossy().into_owned());
    let full = source.to_string_lossy().into_owned();
    match kind {
        XrefPathType::Full => (full, XrefPathType::Full, None),
        XrefPathType::NoPath => (name, XrefPathType::NoPath, None),
        XrefPathType::Relative => {
            let Some(host) = host_dir else {
                return (
                    full,
                    XrefPathType::Full,
                    Some("the drawing is not saved, so the path is absolute".to_owned()),
                );
            };
            match relative(source, host) {
                Some(path) => (path, XrefPathType::Relative, None),
                None => (
                    full,
                    XrefPathType::Full,
                    Some(
                        "the source and the drawing share no folder, so the path is absolute"
                            .to_owned(),
                    ),
                ),
            }
        }
    }
}

/// `source` relative to `host`, or `None` when they share no root.
fn relative(source: &Path, host: &Path) -> Option<String> {
    // The leading drive and root: sharing only these is not sharing a folder.
    let anchors = host
        .components()
        .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
        .count();
    let host = keyed(host);
    let source = keyed(source);
    let shared = host
        .iter()
        .zip(source.iter())
        .take_while(|((a, _), (b, _))| a == b)
        .count();
    // `../work/base.dwg` from `/sheets` would reach the file, but only by
    // way of the root, so the path is kept absolute.
    if shared <= anchors {
        return None;
    }
    // Different volumes or roots never share a relative path.
    if !host
        .first()
        .zip(source.first())
        .is_some_and(|(a, b)| a.0 == b.0)
    {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in shared..host.len() {
        parts.push("..".to_owned());
    }
    parts.extend(source[shared..].iter().map(|(_, text)| text.clone()));
    if parts.is_empty() {
        return Some(".".to_owned());
    }
    Some(parts.join(std::path::MAIN_SEPARATOR_STR))
}

/// A path's components as (comparison key, spelling). `.` is dropped; case
/// does not matter on Windows.
fn keyed(path: &Path) -> Vec<(String, String)> {
    path.components()
        .filter_map(|component| {
            let text = match component {
                Component::CurDir => return None,
                Component::ParentDir => "..".to_owned(),
                Component::RootDir => std::path::MAIN_SEPARATOR.to_string(),
                Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
                Component::Normal(name) => name.to_string_lossy().into_owned(),
            };
            let key = if cfg!(windows) {
                text.to_lowercase()
            } else {
                text.clone()
            };
            Some((key, text))
        })
        .collect()
}

/// The source file's size and modified time, when the platform tells us.
pub fn stamp(path: &Path) -> Option<XrefStamp> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs() as i64);
    Some(XrefStamp {
        length: metadata.len(),
        modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_are_read_from_the_spelling() {
        assert_eq!(classify(""), XrefPathType::NoPath);
        assert_eq!(classify("base.dwg"), XrefPathType::NoPath);
        assert_eq!(classify("..\\base.dwg"), XrefPathType::Relative);
        assert_eq!(classify("../base.dwg"), XrefPathType::Relative);
        assert_eq!(
            classify(if cfg!(windows) {
                "C:\\work\\base.dwg"
            } else {
                "/work/base.dwg"
            }),
            XrefPathType::Full
        );
    }

    #[test]
    fn a_relative_or_bare_path_is_looked_for_beside_the_host() {
        let host = Path::new(if cfg!(windows) { "C:\\work" } else { "/work" });
        assert_eq!(
            resolve("..\\sub\\base.dwg", Some(host)).unwrap(),
            host.join("..\\sub\\base.dwg")
        );
        assert_eq!(
            resolve("base.dwg", Some(host)).unwrap(),
            host.join("base.dwg")
        );
        assert_eq!(resolve("base.dwg", None), Err(XrefPathError::NoHostFolder));
    }

    #[test]
    fn bad_paths_are_refused_before_anything_is_read() {
        assert!(matches!(
            resolve("", None),
            Err(XrefPathError::Problem(XrefProblem::EmptyPath))
        ));
        assert!(matches!(
            resolve("a\u{1}b", None),
            Err(XrefPathError::Problem(XrefProblem::PathHasNul))
        ));
        assert!(matches!(
            resolve("https://example.com/base.dwg", None),
            Err(XrefPathError::Problem(XrefProblem::UnsupportedPath(_)))
        ));
    }

    #[test]
    fn relative_paths_walk_up_to_the_shared_folder() {
        if cfg!(windows) {
            let (saved, kind, note) = format_saved_path(
                Path::new("C:\\work\\parts\\base.dwg"),
                Some(Path::new("C:\\work\\sheets")),
                XrefPathType::Relative,
            );
            assert_eq!(saved, "..\\parts\\base.dwg");
            assert_eq!(kind, XrefPathType::Relative);
            assert!(note.is_none());
        } else {
            let (saved, kind, note) = format_saved_path(
                Path::new("/work/parts/base.dwg"),
                Some(Path::new("/work/sheets")),
                XrefPathType::Relative,
            );
            assert_eq!(saved, "../parts/base.dwg");
            assert_eq!(kind, XrefPathType::Relative);
            assert!(note.is_none());
        }
    }

    #[test]
    fn a_path_across_roots_falls_back_to_full_with_a_note() {
        let (source, host) = if cfg!(windows) {
            (Path::new("C:\\work\\base.dwg"), Path::new("D:\\sheets"))
        } else {
            (Path::new("/work/base.dwg"), Path::new("/sheets"))
        };
        let (saved, kind, note) = format_saved_path(source, Some(host), XrefPathType::Relative);
        assert_eq!(saved, source.to_string_lossy());
        assert_eq!(kind, XrefPathType::Full);
        assert!(note.is_some());
    }

    #[test]
    fn an_unsaved_drawing_cannot_hold_a_relative_path() {
        let (saved, kind, note) =
            format_saved_path(Path::new("/work/base.dwg"), None, XrefPathType::Relative);
        assert_eq!(saved, "/work/base.dwg");
        assert_eq!(kind, XrefPathType::Full);
        assert!(note.is_some());
    }

    #[test]
    fn a_no_path_form_keeps_only_the_file_name() {
        let (saved, kind, note) = format_saved_path(
            Path::new("/work/parts/base.dwg"),
            Some(Path::new("/work")),
            XrefPathType::NoPath,
        );
        assert_eq!(saved, "base.dwg");
        assert_eq!(kind, XrefPathType::NoPath);
        assert!(note.is_none());
    }

    #[test]
    fn a_missing_file_has_no_stamp_and_a_present_one_does() {
        assert_eq!(stamp(Path::new("this file is not here.dwg")), None);
        let path = std::env::temp_dir().join(format!("od-xref-stamp-{}.tmp", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let stamped = stamp(&path).unwrap();
        assert_eq!(stamped.length, 3);
        let _ = std::fs::remove_file(path);
    }
}
