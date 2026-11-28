//! What `-XREF` and `XATTACH` check about a source file before they ask where
//! to put it.

use std::path::Path;

use opendraft_core::path_problem;
use opendraft_ui::key;

/// The name a source file suggests for its reference: the stem, or "xref".
pub(crate) fn stem_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "xref".to_owned())
}

/// Why the source cannot be attached, found before the user is asked where
/// to put it: a bad path, a drawing never saved (so a relative path means
/// nothing) or a file that is not there.
pub(crate) fn source_problem(source_path: &str, host_dir: Option<&str>) -> Option<String> {
    if let Some(problem) = path_problem(source_path) {
        return Some(problem.to_string());
    }
    match opendraft_files::resolve(source_path.trim(), host_dir.map(Path::new)) {
        Ok(path) if path.is_file() => None,
        Ok(path) => Some(format!(
            "{} {}",
            key!("The file was not found:"),
            path.display()
        )),
        Err(why) => Some(why.to_string()),
    }
}
