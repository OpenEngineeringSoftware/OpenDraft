//! A reference's clip boundary (`XCLIP`) and the geometry that keeps its
//! content inside it.
//!
//! The boundary is a closed polygon in the root block's own coordinates, so
//! it moves, turns and scales with the insert. Clipping happens when a
//! reference is expanded into pieces (`Document::insert_pieces`), so paint,
//! picking, snapping and plotting all see the same clipped drawing.
//!
//! Lines and curves are cut at the boundary and come back as polylines.
//! Objects that are not a set of strokes — text, hatches, dimensions, leaders
//! — are kept whole when their box meets the boundary and dropped when it does
//! not; AutoCAD trims those visually, which this does not.

use crate::{arc, bulge, Entity, Point};

/// Most corners a clip boundary may have.
pub const MAX_CLIP_POINTS: usize = 10_000;

/// How closely cut curves are followed, as a share of the boundary's size.
const CURVE_TOLERANCE: f64 = 2e-3;

/// A reference's clip boundary (`XCLIP`).
#[derive(Clone, Debug, PartialEq)]
pub struct XrefClip {
    /// Corners in the root block's coordinates, at least three, the first
    /// not repeated at the end.
    pub boundary: Vec<Point>,
    /// `XCLIP`'s ON/OFF: off keeps the boundary but draws everything.
    pub enabled: bool,
}

impl XrefClip {
    /// A boundary that clips.
    pub fn new(boundary: Vec<Point>) -> Self {
        Self {
            boundary,
            enabled: true,
        }
    }

    /// The box around the boundary.
    pub fn bounds(&self) -> Option<crate::Bounds> {
        let (first, rest) = self.boundary.split_first()?;
        Some(
            rest.iter()
                .fold(crate::Bounds::from_points(*first, *first), |b, p| {
                    b.union(crate::Bounds::from_points(*p, *p))
                }),
        )
    }
}

/// Why a clip boundary cannot be used, or `None` when it can.
pub fn clip_problem(clip: &XrefClip) -> Option<&'static str> {
    let points = &clip.boundary;
    if points.len() < 3 {
        return Some("a clip boundary needs at least three corners");
    }
    if points.len() > MAX_CLIP_POINTS {
        return Some("a clip boundary has too many corners");
    }
    if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Some("a clip boundary has a non-finite corner");
    }
    let Some(bounds) = clip.bounds() else {
        return Some("a clip boundary has no corners");
    };
    let size = (bounds.max.x - bounds.min.x).hypot(bounds.max.y - bounds.min.y);
    let area = (shoelace(points) * 0.5).abs();
    if area <= 1e-9 * (size * size).max(1.0) {
        return Some("a clip boundary encloses no area");
    }
    // The ray test and the cutting assume a simple polygon; a boundary that
    // crosses itself is refused rather than drawn wrongly.
    if points.len() <= 400 && !crate::outline::polygon_is_simple(points) {
        return Some("a clip boundary crosses itself");
    }
    None
}

/// Twice the signed area of the polygon through `points`.
fn shoelace(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

/// Whether `p` is inside the closed polygon (ray casting; the boundary is a
/// simple polygon, which validation guarantees).
fn inside(p: Point, polygon: &[Point]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// A path of the object, flattened to straight pieces.
enum Path {
    Open(Vec<Point>),
    Closed(Vec<Point>),
}

impl Path {
    fn points(&self) -> &[Point] {
        match self {
            Self::Open(p) | Self::Closed(p) => p,
        }
    }

    fn closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }
}

/// The object's strokes for clipping, or `None` for a kind that is not drawn
/// as strokes.
fn paths_of(entity: &Entity, tolerance: f64) -> Option<Vec<Path>> {
    let path = match entity {
        Entity::Line { start, end } => Path::Open(vec![*start, *end]),
        Entity::Rectangle { min, max } => Path::Closed(vec![
            *min,
            Point::new(max.x, min.y),
            *max,
            Point::new(min.x, max.y),
        ]),
        Entity::Polyline {
            points,
            closed,
            bulges,
            ..
        } => {
            let points = bulge::flatten_path(points, bulges, *closed, tolerance);
            if *closed {
                Path::Closed(points)
            } else {
                Path::Open(points)
            }
        }
        Entity::Arc {
            center,
            radius,
            start,
            end,
        } => Path::Open(arc::flatten(*center, *radius, *start, *end, tolerance)),
        Entity::Circle { center, radius } => {
            let mut points =
                arc::flatten(*center, radius.abs(), 0.0, std::f64::consts::TAU, tolerance);
            // The last point repeats the first.
            points.pop();
            Path::Closed(points)
        }
        Entity::Ellipse(e) => {
            let points = e.flatten(tolerance).ok()?;
            if e.is_full() {
                let mut points = points;
                points.pop();
                Path::Closed(points)
            } else {
                Path::Open(points)
            }
        }
        Entity::Spline(s) => Path::Open(s.flatten(tolerance).ok()?),
        _ => return None,
    };
    Some(vec![path])
}

/// The parts of `a`-`b` inside the boundary, in order along it.
fn clip_segment(a: Point, b: Point, polygon: &[Point]) -> Vec<(Point, Point)> {
    let n = polygon.len();
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut cuts = vec![0.0, 1.0];
    for i in 0..n {
        let (p, q) = (polygon[i], polygon[(i + 1) % n]);
        let (ex, ey) = (q.x - p.x, q.y - p.y);
        let den = dx * ey - dy * ex;
        if den.abs() <= 1e-12 * (dx.hypot(dy) * ex.hypot(ey)).max(1e-12) {
            continue;
        }
        let (px, py) = (p.x - a.x, p.y - a.y);
        let t = (px * ey - py * ex) / den;
        let u = (px * dy - py * dx) / den;
        if (-1e-12..=1.0 + 1e-12).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&u) {
            cuts.push(t.clamp(0.0, 1.0));
        }
    }
    cuts.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    for pair in cuts.windows(2) {
        let (t0, t1) = (pair[0], pair[1]);
        if t1 - t0 <= 1e-12 {
            continue;
        }
        let mid = (t0 + t1) * 0.5;
        let point = Point::new(a.x + dx * mid, a.y + dy * mid);
        if inside(point, polygon) {
            out.push((
                Point::new(a.x + dx * t0, a.y + dy * t0),
                Point::new(a.x + dx * t1, a.y + dy * t1),
            ));
        }
    }
    out
}

/// One path cut to the boundary: what remains, already chained into runs.
fn clip_path(path: &Path, polygon: &[Point]) -> Vec<Vec<Point>> {
    let points = path.points();
    if points.len() < 2 {
        return Vec::new();
    }
    let all_inside = points.iter().all(|p| inside(*p, polygon));
    if all_inside && !path.closed() {
        return vec![points.to_vec()];
    }
    let n = points.len();
    let segments = if path.closed() { n } else { n - 1 };
    let mut runs: Vec<Vec<Point>> = Vec::new();
    let mut run: Vec<Point> = Vec::new();
    let same = |a: Point, b: Point| (a.x - b.x).abs() <= 1e-9 && (a.y - b.y).abs() <= 1e-9;
    for i in 0..segments {
        for (a, b) in clip_segment(points[i], points[(i + 1) % n], polygon) {
            match run.last() {
                Some(last) if same(*last, a) => run.push(b),
                _ => {
                    if run.len() >= 2 {
                        runs.push(std::mem::take(&mut run));
                    }
                    run = vec![a, b];
                }
            }
        }
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    runs
}

/// The entity cut to `boundary`, or an empty list when none of it remains.
/// `boundary` is a closed polygon of at least three corners.
pub(crate) fn clip_entity(entity: &Entity, boundary: &[Point]) -> Vec<Entity> {
    let Some(polygon_bounds) = bounds_of(boundary) else {
        return vec![entity.clone()];
    };
    match entity {
        Entity::Point { at } => {
            if inside(*at, boundary) {
                vec![entity.clone()]
            } else {
                Vec::new()
            }
        }
        // An infinite line has no box to compare; it is left whole.
        Entity::Xline { .. } | Entity::Ray { .. } => vec![entity.clone()],
        _ => {
            let bounds = entity.bounds();
            if !bounds.intersects(&polygon_bounds) {
                return Vec::new();
            }
            let size = (polygon_bounds.max.x - polygon_bounds.min.x)
                .hypot(polygon_bounds.max.y - polygon_bounds.min.y);
            let tolerance = (size * CURVE_TOLERANCE).max(1e-9);
            let Some(paths) = paths_of(entity, tolerance) else {
                // Not strokes (text, hatch, dimension, leader): kept whole
                // when its box meets the boundary.
                return vec![entity.clone()];
            };
            let mut out = Vec::new();
            for path in &paths {
                // A closed path wholly inside stays closed and whole.
                let points = path.points();
                if path.closed() && points.iter().all(|p| inside(*p, boundary)) {
                    out.push(Entity::Polyline {
                        points: points.to_vec(),
                        closed: true,
                        bulges: Vec::new(),
                        widths: Vec::new(),
                    });
                    continue;
                }
                for run in clip_path(path, boundary) {
                    if run.len() == 2 {
                        out.push(Entity::Line {
                            start: run[0],
                            end: run[1],
                        });
                    } else {
                        out.push(Entity::Polyline {
                            points: run,
                            closed: false,
                            bulges: Vec::new(),
                            widths: Vec::new(),
                        });
                    }
                }
            }
            out
        }
    }
}

fn bounds_of(points: &[Point]) -> Option<crate::Bounds> {
    let (first, rest) = points.split_first()?;
    Some(
        rest.iter()
            .fold(crate::Bounds::from_points(*first, *first), |b, p| {
                b.union(crate::Bounds::from_points(*p, *p))
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(min: f64, max: f64) -> Vec<Point> {
        vec![
            Point::new(min, min),
            Point::new(max, min),
            Point::new(max, max),
            Point::new(min, max),
        ]
    }

    #[test]
    fn a_line_across_the_boundary_comes_back_as_the_part_inside() {
        let line = Entity::Line {
            start: Point::new(-5.0, 5.0),
            end: Point::new(15.0, 5.0),
        };
        let kept = clip_entity(&line, &square(0.0, 10.0));
        assert_eq!(kept.len(), 1);
        let Entity::Line { start, end } = kept[0] else {
            panic!("a line was expected")
        };
        assert!((start.x - 0.0).abs() < 1e-9 && (end.x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_line_outside_goes_and_one_inside_stays() {
        let outside = Entity::Line {
            start: Point::new(-5.0, -5.0),
            end: Point::new(-1.0, -1.0),
        };
        assert!(clip_entity(&outside, &square(0.0, 10.0)).is_empty());
        let inside = Entity::Line {
            start: Point::new(2.0, 2.0),
            end: Point::new(8.0, 8.0),
        };
        assert_eq!(
            clip_entity(&inside, &square(0.0, 10.0)),
            vec![inside.clone()]
        );
    }

    #[test]
    fn a_circle_crossing_the_boundary_is_cut_into_strokes() {
        let circle = Entity::Circle {
            center: Point::new(10.0, 5.0),
            radius: 5.0,
        };
        let kept = clip_entity(&circle, &square(0.0, 10.0));
        assert!(!kept.is_empty());
        for entity in &kept {
            for point in match entity {
                Entity::Polyline { points, .. } => points.clone(),
                Entity::Line { start, end } => vec![*start, *end],
                _ => panic!("strokes were expected"),
            } {
                assert!(point.x <= 10.0 + 1e-6, "{point:?}");
            }
        }
    }

    #[test]
    fn a_concave_boundary_keeps_only_the_near_arm() {
        // An L shape: the line runs through the notch and back in.
        let boundary = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 4.0),
            Point::new(4.0, 4.0),
            Point::new(4.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let line = Entity::Line {
            start: Point::new(-1.0, 5.0),
            end: Point::new(12.0, 5.0),
        };
        let kept = clip_entity(&line, &boundary);
        assert_eq!(kept.len(), 1, "{kept:?}");
        let Entity::Line { start, end } = kept[0] else {
            panic!("a line was expected")
        };
        assert!((start.x - 0.0).abs() < 1e-9);
        assert!((end.x - 4.0).abs() < 1e-9, "{end:?}");
    }

    #[test]
    fn text_whose_box_is_outside_is_dropped_and_text_across_is_kept() {
        use crate::{MText, TextStyleId};

        let text = Entity::MText(Box::new(MText::new(
            Point::new(20.0, 20.0),
            2.5,
            "note",
            TextStyleId::STANDARD,
        )));
        assert!(clip_entity(&text, &square(0.0, 10.0)).is_empty());
        let across = Entity::MText(Box::new(MText::new(
            Point::new(9.0, 5.0),
            2.5,
            "note",
            TextStyleId::STANDARD,
        )));
        assert_eq!(clip_entity(&across, &square(0.0, 10.0)).len(), 1);
    }

    #[test]
    fn a_boundary_needs_three_corners_and_an_area() {
        let mut clip = XrefClip::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        assert!(clip_problem(&clip).is_some());
        clip.boundary = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
        ];
        assert!(clip_problem(&clip).is_some(), "a flat boundary");
        clip.boundary = square(0.0, 1.0);
        assert_eq!(clip_problem(&clip), None);
    }

    #[test]
    fn a_boundary_that_crosses_itself_is_refused() {
        let clip = XrefClip::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(2.0, 6.0),
            Point::new(2.0, -2.0),
            Point::new(0.0, 4.0),
        ]);
        assert_eq!(clip_problem(&clip), Some("a clip boundary crosses itself"));
    }
}
