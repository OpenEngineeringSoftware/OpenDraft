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

/// What a load left out that the drawing will be missing: the nested
/// references it could not read. The rest of its report stays in the record.
pub(crate) fn missing_content(notes: &[String]) -> Vec<String> {
    notes
        .iter()
        .filter(|note| note.starts_with("Nested external reference") && note.contains("not loaded"))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_nested_references_that_were_left_out_are_reported() {
        let notes = [
            "Loaded nested external reference \"a\" from /x/a.dwg.".to_owned(),
            "Nested external reference \"b\" was not loaded: the file was not found.".to_owned(),
            "Dimension ties, leader annotations and hatch sources were cleared.".to_owned(),
        ];
        assert_eq!(
            missing_content(&notes),
            ["Nested external reference \"b\" was not loaded: the file was not found."]
        );
    }
}
