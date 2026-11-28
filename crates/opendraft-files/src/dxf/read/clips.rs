//! Clip boundaries (`SPATIAL_FILTER`, XCLIP) read from a DXF's OBJECTS
//! section, and the warning for the ones that cannot be kept.
//!
//! An INSERT names its extension dictionary; the dictionary's `ACAD_FILTER`
//! entry leads to a dictionary whose `SPATIAL` entry is the `SPATIAL_FILTER`.
//! Its boundary points are in the clip boundary's coordinate system, which the
//! two matrices it stores relate to the drawing and to the block; with the
//! identity matrices a block-space boundary is stored as it is, as this app
//! writes it.

use std::collections::HashMap;

use opendraft_core::{clip_problem, Affine, Entity, Point, XrefClip};

use super::Record;

/// The clip boundaries an OBJECTS section holds, by handle.
#[derive(Default)]
pub(super) struct Filters {
    /// Each dictionary's entries: name and target handle.
    dictionaries: HashMap<String, Vec<(String, String)>>,
    /// Each spatial filter's boundary, by handle.
    spatial: HashMap<String, Spatial>,
}

struct Spatial {
    points: Vec<Point>,
    enabled: bool,
    /// The points and matrices made a boundary this app can use.
    complete: bool,
}

impl Filters {
    /// How many `SPATIAL_FILTER` objects the file held.
    pub(super) fn count(&self) -> usize {
        self.spatial.len()
    }

    /// The clip boundary `extension` (an INSERT's extension dictionary)
    /// names, in block coordinates, and whether clipping is on. `None` when it
    /// does not lead to a boundary this app can use.
    pub(super) fn boundary(&self, extension: &str) -> Option<(Vec<Point>, bool)> {
        let entries = self.dictionaries.get(extension)?;
        let target = entry(entries, "ACAD_FILTER")?;
        // `ACAD_FILTER` is a dictionary of filters, but some writers point it
        // straight at the one filter.
        if let Some(spatial) = self.spatial.get(&target) {
            return spatial.boundary();
        }
        let entries = self.dictionaries.get(&target)?;
        let target = entry(entries, "SPATIAL")?;
        self.spatial.get(&target)?.boundary()
    }
}

fn entry(entries: &[(String, String)], name: &str) -> Option<String> {
    entries
        .iter()
        .find(|(entry, _)| entry.eq_ignore_ascii_case(name))
        .map(|(_, target)| target.clone())
}

impl Spatial {
    fn boundary(&self) -> Option<(Vec<Point>, bool)> {
        (self.complete && self.points.len() >= 3).then(|| (self.points.clone(), self.enabled))
    }
}

/// Read the OBJECTS section's dictionaries and spatial filters.
pub(super) fn scan(records: &[Record]) -> Filters {
    let mut out = Filters::default();
    for record in records {
        match record.name.as_str() {
            "DICTIONARY" => {
                let Some(handle) = handle_of(record) else {
                    continue;
                };
                out.dictionaries.insert(handle, entries_of(record));
            }
            "SPATIAL_FILTER" => {
                if let (Some(handle), Some(spatial)) = (handle_of(record), spatial_of(record)) {
                    out.spatial.insert(handle, spatial);
                }
            }
            _ => {}
        }
    }
    out
}

fn handle_of(record: &Record) -> Option<String> {
    record.text(5).map(|handle| handle.trim().to_uppercase())
}

/// The name and target handle pairs of a dictionary.
fn entries_of(record: &Record) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut groups = record.groups.iter().peekable();
    while let Some(group) = groups.next() {
        if group.code != 3 {
            continue;
        }
        let Some(next) = groups.peek() else {
            break;
        };
        if next.code == 350 || next.code == 360 {
            entries.push((
                group.value.trim().to_owned(),
                next.value.trim().to_uppercase(),
            ));
            groups.next();
        }
    }
    entries
}

fn spatial_of(record: &Record) -> Option<Spatial> {
    let count = record.int(70).unwrap_or(0).max(0) as usize;
    let mut points = Vec::new();
    let mut x: Option<f64> = None;
    for group in &record.groups {
        match group.code {
            10 => x = group.value.trim().parse().ok(),
            20 => {
                let y = group.value.trim().parse::<f64>().ok();
                if let (Some(x), Some(y)) = (x.take(), y) {
                    if x.is_finite() && y.is_finite() {
                        points.push(Point::new(x, y));
                    }
                }
            }
            _ => {}
        }
    }
    let enabled = record.int(71).unwrap_or(1) != 0;
    // A non-planar normal puts the points in a system this reader does not
    // map; the clip is then reported, not kept.
    let planar = [210, 220, 230].iter().all(|code| {
        record.number(*code).is_none_or(|value| match code {
            210 | 220 => value == 0.0,
            _ => value.abs() == 1.0,
        })
    });
    let stated = points.len();
    let mut points = if points.len() == 2 {
        // Two corners are the smallest rectangle around them.
        let (a, b) = (points[0], points[1]);
        vec![
            Point::new(a.x.min(b.x), a.y.min(b.y)),
            Point::new(a.x.max(b.x), a.y.min(b.y)),
            Point::new(a.x.max(b.x), a.y.max(b.y)),
            Point::new(a.x.min(b.x), a.y.max(b.y)),
        ]
    } else {
        points
    };
    let matrix_values: Vec<f64> = record
        .groups
        .iter()
        .filter(|group| group.code == 40)
        .filter_map(|group| group.value.trim().parse().ok())
        .collect();
    // The declared count says how many points the record should hold; a
    // rectangle states two for its four corners.
    let count_ok = count == 0 || stated == count.max(2);
    let mut complete = planar && count_ok && stated >= 2 && points.len() >= 3;
    if complete && matrix_values.len() >= 24 {
        match to_block(&matrix_values[matrix_values.len() - 24..], &points) {
            Some(mapped) => points = mapped,
            None => complete = false,
        }
    }
    Some(Spatial {
        points,
        enabled,
        complete,
    })
}

/// `points` (in the clip boundary's coordinate system) in block coordinates,
/// through the stored inverse-insert and transform matrices.
fn to_block(values: &[f64], points: &[Point]) -> Option<Vec<Point>> {
    let affine = |v: &[f64]| Affine {
        xx: v[0],
        xy: v[1],
        yx: v[4],
        yy: v[5],
        tx: v[3],
        ty: v[7],
    };
    let inverse_insert = affine(&values[..12]);
    let transform = affine(&values[12..24]);
    // The boundary to the drawing, then the drawing into the block.
    let map = transform.inverse()?.then(&inverse_insert);
    if ![map.xx, map.xy, map.yx, map.yy, map.tx, map.ty]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    Some(points.iter().map(|point| map.apply(*point)).collect())
}

/// The extension dictionary an entity's `102 {ACAD_XDICTIONARY ...}` group
/// names, if any.
pub(super) fn extension_dictionary(record: &Record) -> Option<String> {
    let mut inside = false;
    for group in &record.groups {
        match group.code {
            102 => {
                let value = group.value.trim();
                inside = value.contains("ACAD_XDICTIONARY");
                if value == "}" {
                    inside = false;
                }
            }
            360 if inside => return Some(group.value.trim().to_uppercase()),
            _ => {}
        }
    }
    None
}

/// What to tell the user about `count` clip boundaries that could not be kept.
pub(super) fn warning(count: usize) -> Option<String> {
    (count > 0).then(|| {
        format!(
            "{} not preserved: clipped references were imported unclipped. \
             Keep the original file until all clipping is built.",
            crate::plural(count as u64, "clip boundary")
        )
    })
}

impl super::Importer {
    /// Put the clip boundaries the file held onto the references they belong
    /// to. Boundaries that cannot be used are counted for the warning.
    pub(super) fn apply_clips(&mut self) {
        let total = self.clips.count();
        let pending = std::mem::take(&mut self.clip_inserts);
        if total == 0 && pending.is_empty() {
            return;
        }
        let mut records = self.document.xrefs().to_vec();
        let mut kept = 0;
        for (id, dictionary) in pending {
            let Some(object) = self.document.object(id) else {
                continue;
            };
            let Entity::Insert(insert) = &object.entity else {
                continue;
            };
            let root = insert.block.id;
            let Some((boundary, enabled)) = self.clips.boundary(&dictionary) else {
                continue;
            };
            let clip = XrefClip { boundary, enabled };
            if clip_problem(&clip).is_some() {
                continue;
            }
            if let Some(record) = records.iter_mut().find(|record| record.block == root) {
                record.clip = Some(clip);
                kept += 1;
            }
        }
        self.document.set_xrefs(records);
        self.clipped = total.saturating_sub(kept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf::read::{Group, Record};

    type Pairs = Vec<(i32, &'static str)>;

    fn record(name: &str, handle: &str, groups: &[(i32, &str)]) -> Record {
        let mut all = vec![Group {
            code: 5,
            value: handle.to_owned(),
        }];
        all.extend(groups.iter().map(|(code, value)| Group {
            code: *code,
            value: (*value).to_owned(),
        }));
        Record {
            name: name.to_owned(),
            groups: all,
        }
    }

    fn matrix(values: [&'static str; 12]) -> Pairs {
        values.iter().map(|value| (40, *value)).collect()
    }

    fn identity() -> Pairs {
        matrix([
            "1.0", "0.0", "0.0", "0.0", "0.0", "1.0", "0.0", "0.0", "0.0", "0.0", "1.0", "0.0",
        ])
    }

    fn both(first: Pairs, second: Pairs) -> Pairs {
        let mut out = first;
        out.extend(second);
        out
    }

    #[test]
    fn a_rectangle_stated_as_two_corners_becomes_four() {
        let mut pairs = vec![(70, "2"), (10, "0"), (20, "0"), (10, "4"), (20, "3")];
        pairs.push((71, "1"));
        pairs.extend(both(identity(), identity()));
        let filters = scan(&[record("SPATIAL_FILTER", "2F", &pairs)]);
        let (points, enabled) = filters.spatial["2F"].boundary().expect("a boundary");
        assert!(enabled);
        assert_eq!(
            points,
            [
                Point::new(0.0, 0.0),
                Point::new(4.0, 0.0),
                Point::new(4.0, 3.0),
                Point::new(0.0, 3.0)
            ]
        );
    }

    #[test]
    fn the_stored_matrices_map_boundary_points_into_the_block() {
        let mut pairs = vec![
            (70, "3"),
            (10, "0"),
            (20, "0"),
            (10, "1"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
            (71, "0"),
        ];
        // The inverse insert moves the block's points by ten in x.
        let inverse = matrix([
            "1.0", "0.0", "0.0", "10.0", "0.0", "1.0", "0.0", "0.0", "0.0", "0.0", "1.0", "0.0",
        ]);
        pairs.extend(both(inverse, identity()));
        let filters = scan(&[record("SPATIAL_FILTER", "2F", &pairs)]);
        let (points, enabled) = filters.spatial["2F"].boundary().expect("a boundary");
        assert!(!enabled);
        assert!((points[1].x - 11.0).abs() < 1e-9, "{points:?}");
    }

    #[test]
    fn an_inserts_extension_dictionary_through_acad_filter_names_the_boundary() {
        let dictionary = |handle: &str, name: &str, target: &str| {
            record("DICTIONARY", handle, &[(3, name), (350, target)])
        };
        let mut spatial = vec![
            (70, "3"),
            (10, "0"),
            (20, "0"),
            (10, "1"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ];
        spatial.extend(both(identity(), identity()));
        let filters = scan(&[
            dictionary("200", "ACAD_FILTER", "201"),
            dictionary("201", "SPATIAL", "202"),
            record("SPATIAL_FILTER", "202", &spatial),
        ]);
        let (points, _) = filters.boundary("200").expect("the boundary");
        assert_eq!(points.len(), 3);
    }
}
