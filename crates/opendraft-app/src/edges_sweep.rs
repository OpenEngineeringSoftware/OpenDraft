//! TRIM and EXTEND on many objects at once: a fence (typed with the Fence
//! option, or dragged freehand from empty space) changes every object it
//! crosses, where it crosses it; a crossing window (started by a click in
//! empty space) changes every object it touches, on the part
//! inside it. One gesture is one undo step.

use opendraft_canvas::{select_in, Region};
use opendraft_commands::{Answer, Cancelled, Editor, GhostStyle, PointOptions, Transaction};
use opendraft_core::{Attributes, Document, Entity, EntityId, Point};

use crate::edges::{plan, Mode};

/// Where a fence through `path` crosses `target`: the points to change it
/// at, in the order the fence meets them.
pub fn fence_hits(target: &Entity, path: &[Point]) -> Vec<Point> {
    path.windows(2)
        .flat_map(|leg| {
            let (from, to) = (leg[0], leg[1]);
            let mut hits = target.intersections(&Entity::Line {
                start: from,
                end: to,
            });
            let gap = |p: &Point| (p.x - from.x).hypot(p.y - from.y);
            hits.sort_by(|a, b| gap(a).total_cmp(&gap(b)));
            hits
        })
        .collect()
}

/// Where a crossing window with these corners changes `target`: the point
/// of its part inside the window nearest the window's centre. `None` when
/// no part of it is inside.
pub fn window_hit(target: &Entity, corners: &[Point]) -> Option<Point> {
    let (Some(lo), Some(hi)) = (
        corners
            .iter()
            .copied()
            .reduce(|a, b| Point::new(a.x.min(b.x), a.y.min(b.y))),
        corners
            .iter()
            .copied()
            .reduce(|a, b| Point::new(a.x.max(b.x), a.y.max(b.y))),
    ) else {
        return None;
    };
    let centre = Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
    // Strictly inside: a part outside touches the window only on its edge.
    let margin = 1e-9 * (1.0 + hi.x.abs().max(hi.y.abs()).max(lo.x.abs()).max(lo.y.abs()));
    let inside = |p: &Point| {
        p.x > lo.x + margin && p.x < hi.x - margin && p.y > lo.y + margin && p.y < hi.y - margin
    };
    let sides = [
        (lo, Point::new(hi.x, lo.y)),
        (Point::new(hi.x, lo.y), hi),
        (hi, Point::new(lo.x, hi.y)),
        (Point::new(lo.x, hi.y), lo),
    ];
    let crossings: Vec<Point> = sides
        .iter()
        .flat_map(|&(start, end)| target.intersections(&Entity::Line { start, end }))
        .collect();
    let pieces = match crossings.is_empty() {
        true => vec![target.clone()],
        false => target.split_at(&crossings),
    };
    let gap = |p: &Point| (p.x - centre.x).hypot(p.y - centre.y);
    pieces
        .iter()
        .filter_map(|piece| piece.nearest_point(centre))
        .filter(inside)
        .min_by(|a, b| gap(a).total_cmp(&gap(b)))
}

/// The points at which `region` changes `entity`.
fn hits(entity: &Entity, region: &Region) -> Vec<Point> {
    match region {
        Region::Fence(path) => fence_hits(entity, path),
        Region::Window(corners) | Region::Crossing(corners) => {
            window_hit(entity, corners).into_iter().collect()
        }
    }
}

/// The objects the others are cut or extended against: the chosen edges, or
/// every visible object when none were chosen; never `id` itself.
pub(crate) fn edges_for(
    doc: &Document,
    edges: &Option<Vec<EntityId>>,
    id: EntityId,
) -> Vec<Entity> {
    match edges {
        Some(ids) => ids
            .iter()
            .filter(|other| **other != id)
            .filter_map(|other| doc.object(*other))
            .map(|o| o.entity.clone())
            .collect(),
        None => doc
            .visible_objects()
            .filter(|o| o.id != id)
            .map(|o| o.entity.clone())
            .collect(),
    }
}

/// Put what is left of object `id` in the drawing: nothing removes it, the
/// first piece keeps its identity and place in the draw order, any others
/// are new objects with its attributes.
pub(crate) fn put_back(
    tx: &mut Transaction,
    id: EntityId,
    attrs: Attributes,
    mut left: Vec<Entity>,
) {
    if left.is_empty() {
        tx.remove(vec![id]);
        return;
    }
    let first = left.remove(0);
    tx.modify(id, move |_| first);
    if !left.is_empty() {
        tx.add_with(left.into_iter().map(|e| (e, attrs.clone())).collect());
    }
}

/// Trim or extend every object `region` meets, in one undo step. Returns
/// how many changes were made.
pub(crate) fn change_swept(
    ed: &Editor,
    edges: &Option<Vec<EntityId>>,
    mode: Mode,
    region: Region,
    erase: bool,
) -> Result<usize, Cancelled> {
    let edges = edges.clone();
    let name = match mode {
        Mode::Trim => "Trim",
        Mode::Extend => "Extend",
    };
    let done = ed.transaction(name, move |tx| {
        let doc = tx.document();
        let targets: Vec<(EntityId, Point)> = select_in(doc, &region)
            .into_iter()
            .filter_map(|id| doc.object(id))
            .flat_map(|o| {
                hits(&o.entity, &region)
                    .into_iter()
                    .map(move |at| (o.id, at))
            })
            .collect();
        let mut done = 0;
        for (id, at) in targets {
            let doc = tx.document();
            // The object at `at` now: `id`, or a piece an earlier cut of this
            // gesture made of it.
            let tolerance = 1e-6 * (1.0 + at.x.abs().max(at.y.abs()));
            let on = |e: &Entity| e.distance_to(at) <= tolerance;
            let Some(target) = doc
                .object(id)
                .filter(|o| on(&o.entity))
                .or_else(|| doc.visible_objects().find(|o| on(&o.entity)))
            else {
                continue;
            };
            let others = edges_for(doc, &edges, target.id);
            if let Ok(left) = plan(mode, &target.entity, &others, at, erase) {
                let (id, attrs) = (target.id, target.attrs.clone());
                put_back(tx, id, attrs, left);
                done += 1;
            }
        }
        Ok(done)
    })?;
    if done == 0 {
        ed.say(match mode {
            Mode::Trim => "Nothing to trim there.",
            Mode::Extend => "Nothing to extend there.",
        });
    }
    Ok(done)
}

/// The rest of a crossing window started at `first`: its four corners.
pub(crate) fn ask_window(ed: &Editor, first: Point) -> Result<Vec<Point>, Cancelled> {
    let options = PointOptions::new("Specify opposite corner")
        .ghost_style(GhostStyle::Crossing)
        .ghost(move |at| vec![outline(Region::rect_corners(first, at), true)]);
    let opposite = ed.get_point(options)?;
    Ok(Region::rect_corners(first, opposite))
}

/// The Fence option: points of a fence, Enter to finish. `None` when fewer
/// than two were given.
pub(crate) fn ask_fence(ed: &Editor) -> Result<Option<Vec<Point>>, Cancelled> {
    let mut points: Vec<Point> = Vec::new();
    loop {
        let message = match points.is_empty() {
            true => "Specify first fence point",
            false => "Specify next fence point (Enter to finish)",
        };
        let so_far = points.clone();
        let options = PointOptions::new(message)
            .ghost_style(GhostStyle::Crossing)
            .ghost(move |at| {
                let mut path = so_far.clone();
                path.push(at);
                vec![outline(path, false)]
            });
        match ed.ask_point(options)? {
            Answer::Value(at) if points.last() != Some(&at) => points.push(at),
            Answer::Value(_) => {}
            _ => return Ok((points.len() >= 2).then_some(points)),
        }
    }
}

fn outline(points: Vec<Point>, closed: bool) -> Entity {
    Entity::Polyline {
        points,
        closed,
        bulges: Vec::new(),
        widths: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edges::{extend_objects, trim_objects};
    use crate::state::AppState;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn line(a: Point, b: Point) -> Entity {
        Entity::Line { start: a, end: b }
    }

    /// A horizontal edge along y = 0 and twenty lines standing across it,
    /// from y = -10 to 10 at x = 1..20, as in the user's drawing.
    fn comb() -> Vec<Entity> {
        let mut all = vec![line(p(-1.0, 0.0), p(21.0, 0.0))];
        all.extend((1..=20).map(|x| line(p(f64::from(x), -10.0), p(f64::from(x), 10.0))));
        all
    }

    fn running(command: fn(&mut Editor) -> Result<(), Cancelled>, entities: &[Entity]) -> AppState {
        let mut state = AppState::new();
        for e in entities {
            state.add_entity(e.clone());
        }
        state.start_command("Trim", command);
        state.settle_command();
        state
    }

    fn entities(state: &AppState) -> Vec<Entity> {
        let objects = state.session().document().objects();
        objects.iter().map(|o| o.entity.clone()).collect()
    }

    /// Every standing line cut back to the edge, the edge itself whole.
    fn tops_gone(state: &AppState) -> bool {
        let all = entities(state);
        all.len() == 21
            && all[0] == line(p(-1.0, 0.0), p(21.0, 0.0))
            && (1..=20).all(|x| {
                let x = f64::from(x);
                all.contains(&line(p(x, -10.0), p(x, 0.0)))
            })
    }

    #[test]
    fn a_fence_hits_where_it_crosses_in_order() {
        let target = line(p(0.0, -5.0), p(0.0, 5.0));
        let path = [p(-1.0, 3.0), p(1.0, 3.0), p(1.0, -3.0), p(-1.0, -3.0)];
        assert_eq!(fence_hits(&target, &path), vec![p(0.0, 3.0), p(0.0, -3.0)]);
        assert!(fence_hits(&target, &[p(2.0, 0.0), p(3.0, 0.0)]).is_empty());
    }

    #[test]
    fn a_window_hits_the_part_inside_it() {
        let target = line(p(5.0, -10.0), p(5.0, 10.0));
        let window = Region::rect_corners(p(0.0, 2.0), p(10.0, 8.0));
        assert_eq!(window_hit(&target, &window), Some(p(5.0, 5.0)));
        // Off centre, the nearest point of the part inside.
        let low = Region::rect_corners(p(0.0, 2.0), p(10.0, 4.0));
        assert_eq!(
            window_hit(&line(p(8.0, -10.0), p(8.0, 10.0)), &low),
            Some(p(8.0, 3.0))
        );
        // A line that ends inside it, and one that misses it.
        assert_eq!(
            window_hit(&line(p(5.0, 0.0), p(5.0, 3.0)), &window),
            Some(p(5.0, 3.0))
        );
        assert_eq!(window_hit(&line(p(20.0, 0.0), p(20.0, 9.0)), &window), None);
    }

    #[test]
    fn a_path_dragged_across_the_lines_trims_them_all_in_one_undo_step() {
        let mut state = running(trim_objects, &comb());
        state.answer_path(vec![p(0.0, 5.0), p(10.0, 6.0), p(21.0, 5.0)], false);
        state.settle_command();
        assert!(tops_gone(&state), "got {:?}", entities(&state));
        assert!(state.command_running(), "still asking for the next object");
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(entities(&state), comb(), "one Undo takes it all back");
    }

    #[test]
    fn a_click_on_nothing_opens_a_crossing_window_that_trims_what_it_touches() {
        let mut state = running(trim_objects, &comb());
        state.answer_object(p(0.0, 3.0), None, false);
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Specify opposite corner"));
        state.answer_point(p(21.0, 6.0));
        state.settle_command();
        assert!(tops_gone(&state), "got {:?}", entities(&state));
    }

    #[test]
    fn the_fence_option_does_the_same_from_the_keyboard() {
        let mut state = running(trim_objects, &comb());
        state.cmd_submit("f");
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Specify first fence point"));
        state.cmd_submit("0,5");
        state.settle_command();
        state.cmd_submit("21,5");
        state.settle_command();
        state.cmd_submit("");
        state.settle_command();
        assert!(tops_gone(&state), "got {:?}", entities(&state));
        let options = state.cmd_prompt().options;
        assert!(
            !options.iter().any(|o| o.label == "Crossing"),
            "a click starts it"
        );
    }

    #[test]
    fn a_path_at_extend_lengthens_what_it_crosses_and_shift_trims() {
        // Short lines from y = 2 to 5 and a boundary along y = 10.
        let mut all = vec![line(p(-1.0, 10.0), p(21.0, 10.0))];
        all.extend((1..=3).map(|x| line(p(f64::from(x), 2.0), p(f64::from(x), 5.0))));
        let mut state = running(extend_objects, &all);
        state.answer_path(vec![p(0.0, 4.0), p(4.0, 4.0)], false);
        state.settle_command();
        let grown = entities(&state);
        assert!((1..=3).all(|x| grown.contains(&line(p(f64::from(x), 2.0), p(f64::from(x), 10.0)))));

        // At TRIM, Shift extends instead.
        let mut state = running(trim_objects, &all);
        state.answer_path(vec![p(0.0, 4.0), p(4.0, 4.0)], true);
        state.settle_command();
        assert_eq!(entities(&state), grown);
    }
}
