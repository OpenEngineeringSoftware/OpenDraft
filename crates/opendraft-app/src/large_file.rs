//! A warning before a large drawing is opened. While OpenDraft is early in
//! development big files are slow and memory hungry, so the user is told and
//! chooses whether to go on (issue #118); nothing is refused.

use std::path::{Path, PathBuf};

use crate::files::display_name;
use crate::state::AppState;
use opendraft_commands::Wording;
use opendraft_ui::key;

/// Files bigger than this are asked about before they are read.
pub(crate) const WARN_BYTES: u64 = 20 * 1024 * 1024;

/// A size in megabytes for a message, to one decimal place.
pub(crate) fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// A file waiting for the user to say whether to open it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LargeOpen {
    pub path: PathBuf,
    pub bytes: u64,
}

impl LargeOpen {
    pub fn name(&self) -> String {
        display_name(&self.path)
    }
}

impl AppState {
    /// Whether `path` must be asked about first; if so it is queued for the
    /// question. A file whose size cannot be read is not: the open reports why.
    pub(crate) fn hold_if_large(&mut self, path: &Path) -> bool {
        let Ok(bytes) = std::fs::metadata(path).map(|m| m.len()) else {
            return false;
        };
        if bytes <= WARN_BYTES {
            return false;
        }
        if !self.large_opens.iter().any(|l| l.path == path) {
            self.large_opens.push(LargeOpen {
                path: path.to_owned(),
                bytes,
            });
        }
        true
    }

    /// The file the large-file question is about.
    pub fn large_open(&self) -> Option<&LargeOpen> {
        self.large_opens.first()
    }

    /// Apply the answer to the large-file question: open the file anyway, or
    /// leave it. The next file waiting, if any, is asked about after.
    pub fn answer_large_open(&mut self, open: bool) {
        if self.large_opens.is_empty() {
            return;
        }
        let held = self.large_opens.remove(0);
        if open {
            self.begin_open(&held.path);
        } else {
            self.set_message(Wording::new(
                key!("Did not open %{name}"),
                &[("name", &held.name())],
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::{Script, ScriptedPicker};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("opendraft_large_{name}_{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn state() -> AppState {
        let mut state = AppState::new();
        state.picker = Box::new(ScriptedPicker(Script::default()));
        state
    }

    /// A sparse file of `bytes`: its size is all the question looks at.
    fn sized(dir: &TempDir, name: &str, bytes: u64) -> PathBuf {
        let path = dir.0.join(name);
        std::fs::File::create(&path)
            .unwrap()
            .set_len(bytes)
            .unwrap();
        path
    }

    #[test]
    fn a_small_file_opens_without_asking() {
        let dir = TempDir::new("small");
        let path = sized(&dir, "plan.dxf", WARN_BYTES);
        let mut state = state();
        assert!(!state.hold_if_large(&path));
        assert!(state.large_open().is_none());
    }

    #[test]
    fn a_large_file_is_asked_about_and_can_be_left() {
        let dir = TempDir::new("leave");
        let path = sized(&dir, "site.dwg", WARN_BYTES + 1);
        let mut state = state();
        assert!(state.open_path(&path));
        assert!(
            !state.message.contains("open"),
            "nothing is read before the answer"
        );
        let held = state.large_open().unwrap();
        assert_eq!((held.name().as_str(), held.bytes), ("site", WARN_BYTES + 1));
        state.answer_large_open(false);
        assert!(state.large_open().is_none());
        assert_eq!(state.message, "Did not open site");
    }

    #[test]
    fn opening_anyway_reads_the_file_and_asks_about_the_next() {
        let dir = TempDir::new("anyway");
        let first = sized(&dir, "a.dwg", WARN_BYTES + 1);
        let second = sized(&dir, "b.dwg", WARN_BYTES + 1);
        let mut state = state();
        state.open_paths(&[first.clone(), second.clone(), first.clone()]);
        assert_eq!(state.large_opens.len(), 2, "each file is asked about once");
        state.answer_large_open(true);
        assert_eq!(state.large_open().map(|l| l.path.clone()), Some(second));
        // The zero-filled file is not a DWG: the read was tried and failed.
        assert!(
            state.message.starts_with("Could not open"),
            "{}",
            state.message
        );
    }

    #[test]
    fn sizes_read_in_megabytes() {
        assert_eq!(megabytes(WARN_BYTES), "20.0 MB");
        assert_eq!(megabytes(35 * 1024 * 1024 + 300 * 1024), "35.3 MB");
    }
}
