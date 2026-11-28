//! Clip boundaries (`SPATIAL_FILTER`, XCLIP) are not preserved, so import
//! says so instead of letting the drawing look complete.

/// What to tell the user about `count` clip boundaries that were dropped.
pub(super) fn warning(count: usize) -> Option<String> {
    (count > 0).then(|| {
        format!(
            "{} not preserved: clipped references were imported unclipped. \
             Keep the original file until clipping is built.",
            crate::plural(count as u64, "clip boundary")
        )
    })
}
