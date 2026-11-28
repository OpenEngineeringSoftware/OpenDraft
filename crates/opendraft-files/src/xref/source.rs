//! Reading a drawing to use as an external reference's source.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use opendraft_core::{Document, XrefStamp};

use crate::format_for;

use super::error::XrefLoadError;
use super::path::stamp;

/// What one source may bring. Far above a real drawing; a hostile file cannot
/// make the snapshot unbounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XrefLimits {
    pub max_model_entities: u64,
    pub max_local_blocks: u64,
    pub max_layers: u64,
    pub max_text_styles: u64,
    pub max_linetypes: u64,
}

impl Default for XrefLimits {
    fn default() -> Self {
        Self {
            max_model_entities: 250_000,
            max_local_blocks: 10_000,
            max_layers: 10_000,
            max_text_styles: 2_000,
            max_linetypes: 1_000,
        }
    }
}

/// A source drawing read into memory, with its file's stamp.
pub struct XrefSource {
    pub document: Document,
    pub path: PathBuf,
    pub stamp: Option<XrefStamp>,
    /// What the format itself left out (unsupported objects, approximated
    /// curves), worded for the user and carried into the plan's report.
    pub warnings: Vec<String>,
}

/// Read `path` as a drawing to snapshot. The whole file is read, so a file
/// over its format's limit is refused first; a slow import notices
/// `cancelled`.
pub fn read_source(
    path: &Path,
    cancelled: &AtomicBool,
    limits: &XrefLimits,
) -> Result<XrefSource, XrefLoadError> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(XrefLoadError::Cancelled);
    }
    let Some(format) = format_for(path) else {
        return Err(XrefLoadError::UnsupportedSource(
            path.to_string_lossy().into_owned(),
        ));
    };
    let length = std::fs::metadata(path)?.len();
    format.check_size(length)?;
    // The stamp is taken before the read: a file that changes while it is
    // read then looks changed, never current.
    let stamp = stamp(path);
    let bytes = std::fs::read(path)?;
    let loaded = format.read_cancellable(&bytes, cancelled)?;
    loaded
        .document
        .validate()
        .map_err(|e| XrefLoadError::InvalidSource(e.to_string()))?;

    let model = loaded
        .document
        .objects()
        .iter()
        .filter(|o| o.layout.is_none())
        .count() as u64;
    if model > limits.max_model_entities {
        return Err(XrefLoadError::TooMany {
            what: "objects in its model space",
            limit: limits.max_model_entities,
        });
    }
    if loaded.document.layers().len() as u64 > limits.max_layers {
        return Err(XrefLoadError::TooMany {
            what: "layers",
            limit: limits.max_layers,
        });
    }
    if loaded.document.text_styles().len() as u64 > limits.max_text_styles {
        return Err(XrefLoadError::TooMany {
            what: "text styles",
            limit: limits.max_text_styles,
        });
    }
    if loaded.document.linetypes().records().len() as u64 > limits.max_linetypes {
        return Err(XrefLoadError::TooMany {
            what: "linetypes",
            limit: limits.max_linetypes,
        });
    }

    Ok(XrefSource {
        document: loaded.document,
        path: path.to_owned(),
        stamp,
        warnings: loaded.warnings,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn a_missing_source_is_an_io_error() {
        let limits = XrefLimits::default();
        let error = read_source(
            Path::new("no such xref source.dwg"),
            &AtomicBool::new(false),
            &limits,
        );
        assert!(matches!(error, Err(XrefLoadError::Io(_))));
    }

    #[test]
    fn a_known_extension_with_a_bad_file_names_the_source() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("od-xref-bad-{}.odraft", std::process::id()));
        std::fs::write(&path, b"not a drawing").unwrap();
        let error = read_source(&path, &AtomicBool::new(false), &XrefLimits::default());
        assert!(matches!(error, Err(XrefLoadError::File(_))));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_unknown_extension_is_not_a_source() {
        let error = read_source(
            Path::new("notes.txt"),
            &AtomicBool::new(false),
            &XrefLimits::default(),
        );
        assert!(matches!(error, Err(XrefLoadError::UnsupportedSource(_))));
    }
}
