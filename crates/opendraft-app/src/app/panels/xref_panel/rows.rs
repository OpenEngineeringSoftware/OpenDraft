//! What the External References palette lists: one row for each reference of
//! the drawing, and below it one for each reference its source attaches.
//!
//! Status is derived from the record, the inserts and the source file's
//! metadata; it is built once per drawing revision (see the panel) so a frame
//! does not stat every file.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;

use opendraft_core::{BlockId, Document, XrefDef, XrefKind};
use opendraft_files::resolve;
use opendraft_ui::key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Health {
    Loaded,
    Unloaded,
    NotLoaded,
    Unreferenced,
    NotFound,
    ShowingSaved,
    Changed,
    Unresolved,
}

impl Health {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Loaded => key!("Loaded"),
            Self::Unloaded => key!("Unloaded"),
            Self::NotLoaded => key!("Not loaded"),
            Self::Unreferenced => key!("Unreferenced"),
            Self::NotFound => key!("Not Found"),
            Self::ShowingSaved => key!("Not Found — showing saved copy"),
            Self::Changed => key!("Changed on disk"),
            Self::Unresolved => key!("Unresolved"),
        }
    }

    /// Whether the source cannot be shown as it is on disk.
    pub(super) fn is_problem(self) -> bool {
        !matches!(self, Self::Loaded | Self::Unreferenced)
    }
}

/// A row's identity: the reference in the drawing, and which of the ones
/// inside its source it is (`None` for the reference itself).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct RowKey {
    pub block: BlockId,
    pub nested: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct Row {
    pub key: RowKey,
    /// Position in the list of the row whose source attaches this one.
    pub parent: Option<usize>,
    pub depth: usize,
    pub name: String,
    pub health: Health,
    pub size: Option<u64>,
    pub kind: XrefKind,
    pub date: Option<i64>,
    /// The path as it is saved.
    pub path: String,
    /// The file found at that path; empty when there is none.
    pub found: String,
}

impl Row {
    pub(super) fn is_nested(&self) -> bool {
        self.key.nested.is_some()
    }
}

/// The columns, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Column {
    Name,
    Status,
    Size,
    Type,
    Date,
    Path,
}

impl Column {
    pub(super) const ALL: [Column; 6] = [
        Column::Name,
        Column::Status,
        Column::Size,
        Column::Type,
        Column::Date,
        Column::Path,
    ];

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Name => key!("Reference Name"),
            Self::Status => key!("Status"),
            Self::Size => key!("Size"),
            Self::Type => key!("Type"),
            Self::Date => key!("Date"),
            Self::Path => key!("Saved Path"),
        }
    }
}

/// Which column the rows are ordered by, and which way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Sort {
    pub column: Column,
    pub ascending: bool,
}

impl Sort {
    /// A click on a column's title: that column ascending, or the other way
    /// round when it is already the one.
    pub(super) fn clicked(self, column: Column) -> Self {
        Self {
            column,
            ascending: self.column != column || !self.ascending,
        }
    }
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            column: Column::Name,
            ascending: true,
        }
    }
}

fn compare(a: &Row, b: &Row, sort: Sort) -> Ordering {
    let by = match sort.column {
        Column::Name => Ordering::Equal,
        Column::Status => a.health.label().cmp(b.health.label()),
        Column::Size => a.size.cmp(&b.size),
        Column::Type => a.kind.label().cmp(b.kind.label()),
        Column::Date => a.date.cmp(&b.date),
        Column::Path => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
    };
    let by = if sort.ascending { by } else { by.reverse() };
    let name = a.name.to_lowercase().cmp(&b.name.to_lowercase());
    by.then(if sort.ascending || sort.column != Column::Name {
        name
    } else {
        name.reverse()
    })
}

/// The positions of the rows to show, in order. The list shows every row
/// flat; the tree shows each reference followed by what its source attaches,
/// leaving out what is under a row in `collapsed`.
pub(super) fn order(
    rows: &[Row],
    tree: bool,
    sort: Sort,
    collapsed: &HashSet<RowKey>,
) -> Vec<usize> {
    let mut all: Vec<usize> = (0..rows.len()).collect();
    if !tree {
        all.sort_by(|a, b| compare(&rows[*a], &rows[*b], sort));
        return all;
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut roots: Vec<usize> = all
        .iter()
        .copied()
        .filter(|i| rows[*i].parent.is_none())
        .collect();
    roots.sort_by(|a, b| compare(&rows[*a], &rows[*b], sort));
    // Depth first, with a stack so a deep chain cannot overflow.
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        out.push(i);
        if collapsed.contains(&rows[i].key) {
            continue;
        }
        let mut kids: Vec<usize> = all
            .iter()
            .copied()
            .filter(|k| rows[*k].parent == Some(i))
            .collect();
        kids.sort_by(|a, b| compare(&rows[*a], &rows[*b], sort));
        stack.extend(kids.into_iter().rev());
    }
    out
}

/// Whether any row sits under row `i`.
pub(super) fn has_children(rows: &[Row], i: usize) -> bool {
    rows.iter().any(|r| r.parent == Some(i))
}

/// The rows for the drawing, reading each source's metadata once.
pub(super) fn rows(doc: &Document, host_dir: Option<&Path>) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for xref in doc.xrefs() {
        let Some(root) = doc.block(xref.block) else {
            continue;
        };
        let resolved = if xref.path.trim().is_empty() {
            None
        } else {
            resolve(&xref.path, host_dir).ok()
        };
        let meta = resolved
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok());
        let referenced = !doc.xref_references(xref.block).is_empty();
        let health = health(
            xref,
            root.entities.is_empty(),
            referenced,
            resolved.is_some(),
            meta.as_ref(),
        );
        let (size, date) = match &meta {
            Some(meta) => (Some(meta.len()), modified(meta)),
            None => (
                xref.stamp.map(|stamp| stamp.length),
                xref.stamp.map(|stamp| stamp.modified).filter(|s| *s > 0),
            ),
        };
        let top = out.len();
        out.push(Row {
            key: RowKey {
                block: xref.block,
                nested: None,
            },
            parent: None,
            depth: 0,
            name: root.name.clone(),
            health,
            size,
            kind: xref.kind,
            date,
            path: xref.path.clone(),
            found: match (&resolved, &meta) {
                (Some(path), Some(_)) => path.display().to_string(),
                _ => String::new(),
            },
        });
        let first = out.len();
        for (i, n) in xref.report.nested.iter().enumerate() {
            let parent = n.parent.map(|p| first + p).filter(|p| *p < out.len());
            let depth = parent.map_or(1, |p| out[p].depth + 1);
            let health = match (xref.unloaded, n.loaded) {
                (true, _) => Health::Unloaded,
                (false, false) => Health::NotLoaded,
                (false, true) => Health::Loaded,
            };
            out.push(Row {
                key: RowKey {
                    block: xref.block,
                    nested: Some(i),
                },
                parent: Some(parent.unwrap_or(top)),
                depth,
                name: n.name.clone(),
                health,
                size: n.stamp.map(|s| s.length),
                kind: n.kind,
                date: n.stamp.map(|s| s.modified).filter(|s| *s > 0),
                path: n.path.clone(),
                found: n.found.clone(),
            });
        }
    }
    out
}

fn health(
    xref: &XrefDef,
    root_empty: bool,
    referenced: bool,
    resolved: bool,
    meta: Option<&std::fs::Metadata>,
) -> Health {
    if !referenced {
        return Health::Unreferenced;
    }
    if xref.never_loaded() {
        return Health::NotLoaded;
    }
    if xref.unloaded {
        return Health::Unloaded;
    }
    if !resolved {
        return Health::Unresolved;
    }
    let Some(meta) = meta else {
        return if xref.stamp.is_some() && !root_empty {
            Health::ShowingSaved
        } else {
            Health::NotFound
        };
    };
    match xref.stamp {
        Some(stamp)
            if stamp.length != meta.len()
                || (stamp.modified > 0
                    && modified(meta).is_some_and(|now| now != stamp.modified)) =>
        {
            Health::Changed
        }
        _ => Health::Loaded,
    }
}

fn modified(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64)
}

pub(super) fn size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let value = bytes as f64;
    if value < K {
        format!("{bytes} B")
    } else if value < K * K {
        format!("{:.1} KB", value / K)
    } else {
        format!("{:.1} MB", value / (K * K))
    }
}

/// A date and time from Unix seconds, in UTC.
pub(super) fn date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The civil date of a count of days from 1970-01-01 (Howard Hinnant's
/// algorithm, as `chrono` and the C++ standard library use).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests;
