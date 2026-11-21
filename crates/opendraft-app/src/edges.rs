//! TRIM and EXTEND: change objects against the edges other objects make. As in
//! AutoCAD, two modes (`TRIMEXTENDMODE`, the mOde option): in Quick mode every
//! object is an edge without asking, in Standard mode the command asks for the
//! edges first; objects selected before the command are the edges in both.
//! Then it asks for objects to change, and Shift-click does the other one's
//! job. Written against the `Editor` like the other commands; the geometry is
//! `opendraft_core`'s `intersections`, `split_at` and `extended_to`.

use opendraft_canvas::Region;
use opendraft_commands::{Answer, Cancelled, Editor, Options, Picked, Swept};
use opendraft_core::{End, Entity, EntityId, Point};

use crate::defaults::EditorDefaults;
use crate::edges_sweep::{ask_fence, ask_window, change_swept, edges_for, put_back};
use crate::text::plural;

/// What is left of `target` when the piece nearest `at` is trimmed away
/// where `cutters` cross it. An empty list means the whole object goes (it
/// can only happen when every piece is the clicked one, which a trim of two or
/// more pieces never leaves). `Err` says why nothing can be done.
pub fn trim_plan(
    target: &Entity,
    cutters: &[Entity],
    at: Point,
) -> Result<Vec<Entity>, &'static str> {
    if matches!(target, Entity::Spline(_)) {
        return Err("TRIM does not support splines yet.");
    }
    let crossings: Vec<Point> = cutters
        .iter()
        .flat_map(|cutter| target.intersections(cutter))
        .collect();
    if crossings.is_empty() {
        let near = target.bounds();
        if cutters
            .iter()
            .any(|c| matches!(c, Entity::Spline(_)) && c.bounds().intersects(&near))
        {
            return Err("Splines cannot be cutting edges yet.");
        }
        return Err(NO_EDGE);
    }
    let pieces = target.split_at(&crossings);
    // One piece means the cuts did not divide it (a circle cut once, a
    // closed shape cut once, a cut at an end): there is nothing to remove.
    if pieces.len() < 2 {
        return Err("Object cannot be trimmed there.");
    }
    let nearest = nearest_piece(&pieces, at).ok_or("Object cannot be trimmed there.")?;
    // Cut again only at the crossings that bound the clicked piece, so the
    // rest stays whole: a line trimmed at one end is shortened, not broken
    // at every other crossing as well (and can be trimmed there next).
    let tolerance = 1e-9 * (1.0 + at.x.abs().max(at.y.abs()));
    let bounds: Vec<Point> = crossings
        .into_iter()
        .filter(|c| pieces[nearest].distance_to(*c) <= tolerance)
        .collect();
    let pieces = target.split_at(&bounds);
    // If rounding left the clicked piece's ends out of `bounds`, the object
    // comes back whole and trimming it would erase it.
    if pieces.len() < 2 {
        return Err("Object cannot be trimmed there.");
    }
    let nearest = nearest_piece(&pieces, at).ok_or("Object cannot be trimmed there.")?;
    Ok(pieces
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i != nearest)
        .map(|(_, piece)| piece)
        .collect())
}

/// Why `trim_plan` cannot trim an object that no edge crosses.
const NO_EDGE: &str = "Object does not intersect an edge.";

/// What TRIM (or EXTEND) leaves of `target`. With `erase` (Quick mode with
/// every object as an edge), trimming an object that no edge divides erases
/// it, as in AutoCAD: one that nothing crosses, or one an edge only touches
/// at an end.
pub(crate) fn plan(
    mode: Mode,
    target: &Entity,
    others: &[Entity],
    at: Point,
    erase: bool,
) -> Result<Vec<Entity>, &'static str> {
    match mode {
        Mode::Trim => match trim_plan(target, others, at) {
            Err(NO_EDGE) if erase => Ok(Vec::new()),
            Err(_) if erase && undivided(target, others) => Ok(Vec::new()),
            made => made,
        },
        Mode::Extend => extend_plan(target, others, at),
    }
}

/// Whether edges meet `target` without dividing it (a line touched at an
/// end, a circle cut once), so the whole of it is the part clicked.
fn undivided(target: &Entity, others: &[Entity]) -> bool {
    if matches!(target, Entity::Spline(_)) {
        return false;
    }
    let crossings: Vec<Point> = others
        .iter()
        .flat_map(|other| target.intersections(other))
        .collect();
    !crossings.is_empty() && target.split_at(&crossings).len() < 2
}

/// The index of the piece nearest `at`.
fn nearest_piece(pieces: &[Entity], at: Point) -> Option<usize> {
    pieces
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.distance_to(at).total_cmp(&b.1.distance_to(at)))
        .map(|(i, _)| i)
}

/// What extending `target` at the end nearest `at` does against `boundaries`:
/// the extended object. `Err` says why not.
pub fn extend_plan(
    target: &Entity,
    boundaries: &[Entity],
    at: Point,
) -> Result<Vec<Entity>, &'static str> {
    if matches!(target, Entity::Spline(_)) {
        return Err("EXTEND does not support splines yet.");
    }
    let ends = match target {
        Entity::Line { start, end } => Some((*start, *end)),
        Entity::Arc {
            center,
            radius,
            start,
            end,
        } => Some((
            opendraft_core::arc::point_at(*center, *radius, *start),
            opendraft_core::arc::point_at(*center, *radius, *end),
        )),
        Entity::Ellipse(e) if !e.is_full() => Some((e.start_point(), e.end_point())),
        Entity::Polyline {
            points,
            closed: false,
            ..
        } if points.len() >= 2 => Some((points[0], points[points.len() - 1])),
        _ => None,
    };
    let Some((first, last)) = ends else {
        return Err("Object cannot be extended.");
    };
    let gap = |p: Point| (p.x - at.x).hypot(p.y - at.y);
    let end = if gap(first) <= gap(last) {
        End::Start
    } else {
        End::End
    };
    let splines = boundaries.iter().any(|b| matches!(b, Entity::Spline(_)));
    target
        .extended_to(end, boundaries)
        .map(|longer| vec![longer])
        .ok_or(if splines {
            "No boundary in the way (splines cannot be boundary edges yet)."
        } else {
            "No boundary in the way."
        })
}

/// Which of the two commands is running.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Trim,
    Extend,
}

impl Mode {
    fn other(self) -> Mode {
        match self {
            Mode::Trim => Mode::Extend,
            Mode::Extend => Mode::Trim,
        }
    }
}

/// TRIM: click the part of each object to remove. Shift-click extends
/// instead. A path dragged from empty space (or the Fence option) trims every
/// object it crosses; a click in empty space starts a crossing window that
/// trims every object it touches (see `edges_sweep.rs`). In Quick mode with
/// every object as an edge, an object that nothing divides is erased. Each
/// click or gesture is its own undo step; Undo takes back the last one; Enter
/// or Esc ends.
pub fn trim_objects(ed: &mut Editor) -> Result<(), Cancelled> {
    edit_against_edges(
        ed,
        Mode::Trim,
        "select cutting edges first",
        "select object to trim or shift-select to extend",
    )
}

/// EXTEND: click each object near the end to lengthen. Shift-click trims
/// instead.
pub fn extend_objects(ed: &mut Editor) -> Result<(), Cancelled> {
    edit_against_edges(
        ed,
        Mode::Extend,
        "select boundary edges first",
        "select object to extend or shift-select to trim",
    )
}

fn edit_against_edges(
    ed: &mut Editor,
    mode: Mode,
    edges_message: &str,
    object_message: &str,
) -> Result<(), Cancelled> {
    let mut quick = ed.defaults().trim.get().quick;
    let mut edges = choose_edges(ed, &mut quick, ed.pickset().to_vec(), edges_message)?;
    let mut changed = 0_usize;
    loop {
        // Quick mode is named here, its only prompt; Standard mode was named
        // at the edge question.
        let message = if quick {
            with_mode(true, object_message)
        } else {
            sentence(object_message)
        };
        // No Crossing option: a click in empty space starts the window.
        let mut options = Options::<Swept>::new(message)
            .keyword("Fence", "F")
            .keyword("mOde", "O");
        if changed > 0 {
            options = options.keyword("Undo", "U");
        }
        let doing = |shift: bool| if shift { mode.other() } else { mode };
        // Quick mode with every object as an edge erases what nothing divides.
        let erase = quick && edges.is_none();
        let done = match ed.ask_object_or_sweep(options) {
            Err(Cancelled) | Ok(Answer::Enter) => return Ok(()),
            Ok(Answer::Keyword(word)) => match word.as_str() {
                "Fence" => match ask_fence(ed)? {
                    Some(path) => change_swept(ed, &edges, mode, Region::Fence(path), erase)? > 0,
                    None => false,
                },
                "mOde" => {
                    if switch_mode(ed, &mut quick)? {
                        edges = choose_edges(ed, &mut quick, Vec::new(), edges_message)?;
                    }
                    false
                }
                _ => {
                    ed.undo()?;
                    changed -= 1;
                    false
                }
            },
            Ok(Answer::Value(Swept::Object(picked))) => {
                change_one(ed, &edges, doing(picked.shift), picked, erase)?
            }
            Ok(Answer::Value(Swept::Missed { at, shift })) => {
                let corners = ask_window(ed, at)?;
                let region = Region::Crossing(corners);
                change_swept(ed, &edges, doing(shift), region, erase)? > 0
            }
            Ok(Answer::Value(Swept::Path { points, shift })) => {
                change_swept(ed, &edges, doing(shift), Region::Fence(points), erase)? > 0
            }
        };
        if done {
            changed += 1;
        }
    }
}

/// `message` after the mode, as a reminder: "Quick mode, select object to
/// trim".
fn with_mode(quick: bool, message: &str) -> String {
    let named = if quick { "Quick mode" } else { "Standard mode" };
    format!("{named}, {message}")
}

/// `message` with a capital first letter, as a prompt starts.
fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The edges: the objects selected before the command (`chosen`); with none,
/// every object (`None`) in Quick mode, or those the user selects in Standard
/// mode, where Enter takes every object and mOde can switch to Quick.
fn choose_edges(
    ed: &Editor,
    quick: &mut bool,
    mut chosen: Vec<EntityId>,
    edges_message: &str,
) -> Result<Option<Vec<EntityId>>, Cancelled> {
    while chosen.is_empty() && !*quick {
        let options = Options::<()>::new(with_mode(false, edges_message))
            .allow_empty()
            .keyword("mOde", "O");
        match ed.ask_selection_or_keyword(options, "select all")? {
            Answer::Value(ids) => {
                if ids.is_empty() {
                    break;
                }
                chosen = ids;
            }
            Answer::Keyword(_) => {
                switch_mode(ed, quick)?;
            }
            Answer::Enter => break,
        }
    }
    if chosen.is_empty() {
        ed.say("Using every object as an edge.");
        return Ok(None);
    }
    ed.say(format!(
        "Using the {} selected as edges.",
        plural(chosen.len(), "object")
    ));
    Ok(Some(chosen))
}

/// The mOde option: Quick or Standard, the current one by default. The
/// choice is remembered (`TRIMEXTENDMODE`). Returns whether it changed.
fn switch_mode(ed: &Editor, quick: &mut bool) -> Result<bool, Cancelled> {
    let current = if *quick { "Quick" } else { "Standard" };
    let options = Options::<String>::new("Enter a trim mode option")
        .keyword("Quick", "Q")
        .keyword("Standard", "S")
        .default(current.to_owned());
    let now = match ed.ask_keyword(options)?.as_deref() {
        Some("Quick") => true,
        Some("Standard") => false,
        _ => *quick,
    };
    if now == *quick {
        return Ok(false);
    }
    *quick = now;
    ed.defaults().trim.change(|m| m.quick = now);
    Ok(true)
}

/// Trim or extend the picked object. Returns whether the drawing changed.
fn change_one(
    ed: &Editor,
    edges: &Option<Vec<EntityId>>,
    mode: Mode,
    picked: Picked,
    erase: bool,
) -> Result<bool, Cancelled> {
    let Picked { id, at, .. } = picked;
    let edges = edges.clone();
    let plan = ed.read(move |doc| {
        let target = doc.object(id)?;
        let others = edges_for(doc, &edges, id);
        let made = plan(mode, &target.entity, &others, at, erase);
        Some(made.map(|left| (target.attrs.clone(), left)))
    })?;
    let (attrs, left) = match plan {
        None => return Ok(false),
        Some(Err(why)) => {
            ed.say(why);
            return Ok(false);
        }
        Some(Ok(done)) => done,
    };
    let name = match mode {
        Mode::Trim => "Trim",
        Mode::Extend => "Extend",
    };
    ed.transaction(name, move |tx| {
        put_back(tx, id, attrs, left);
        Ok(())
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn trimming_a_construction_line_between_two_edges_leaves_two_rays() {
        use opendraft_core::Point;
        let target = Entity::Xline {
            origin: Point::new(0.0, 0.0),
            direction: Point::new(1.0, 0.0),
        };
        let wall = |x: f64| Entity::Line {
            start: Point::new(x, -5.0),
            end: Point::new(x, 5.0),
        };
        let left = trim_plan(&target, &[wall(3.0), wall(8.0)], Point::new(5.0, 0.0)).unwrap();
        assert_eq!(left.len(), 2);
        assert!(left.iter().all(|e| matches!(e, Entity::Ray { .. })));
        let (origin, ..) = left
            .iter()
            .filter_map(Entity::infinite_line)
            .find(|(_, d, _)| d.x > 0.0)
            .unwrap();
        assert!((origin.x - 8.0).abs() < 1e-6);
    }

    use super::*;
    use crate::state::AppState;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn line(a: Point, b: Point) -> Entity {
        Entity::Line { start: a, end: b }
    }

    fn wall(x: f64) -> Entity {
        line(p(x, -10.0), p(x, 10.0))
    }

    #[test]
    fn the_clicked_piece_goes_and_the_rest_stays() {
        let target = line(p(0.0, 0.0), p(10.0, 0.0));
        let cutters = [wall(3.0), wall(7.0)];
        // Click the middle: the ends remain.
        assert_eq!(
            trim_plan(&target, &cutters, p(5.0, 0.0)).unwrap(),
            [
                line(p(0.0, 0.0), p(3.0, 0.0)),
                line(p(7.0, 0.0), p(10.0, 0.0))
            ]
        );
        // Click an end: it shortens, still one line.
        assert_eq!(
            trim_plan(&target, &cutters, p(9.0, 0.0)).unwrap(),
            [line(p(0.0, 0.0), p(7.0, 0.0))]
        );
    }

    #[test]
    fn a_line_through_a_circle_is_trimmed_on_both_sides_in_turn() {
        let circle = Entity::Circle {
            center: p(0.0, 0.0),
            radius: 5.0,
        };
        let target = line(p(-10.0, 0.0), p(10.0, 0.0));
        let left = trim_plan(&target, std::slice::from_ref(&circle), p(-8.0, 0.0)).unwrap();
        let [shorter] = left.as_slice() else {
            panic!("one line, got {left:?}")
        };
        let right = trim_plan(shorter, std::slice::from_ref(&circle), p(8.0, 0.0)).unwrap();
        let [Entity::Line { start, end }] = right.as_slice() else {
            panic!("one line, got {right:?}")
        };
        assert!((start.x + 5.0).abs() < 1e-9 && (end.x - 5.0).abs() < 1e-9);
    }

    #[test]
    fn ellipses_are_trimmed_extended_and_cut_against() {
        use std::f64::consts::{PI, TAU};
        let ellipse = |start: f64, end: f64| {
            Entity::Ellipse(
                opendraft_core::Ellipse::new(p(0.0, 0.0), p(4.0, 0.0), 0.5, start, end).unwrap(),
            )
        };
        // A full ellipse cut by the y axis: the side clicked goes.
        let kept = trim_plan(&ellipse(0.0, TAU), &[wall(0.0)], p(4.0, 0.0)).unwrap();
        let [Entity::Ellipse(left)] = kept.as_slice() else {
            panic!("one elliptical arc, got {kept:?}")
        };
        assert!(left.start_point().x.abs() < 1e-9 && left.mid_point().x < -3.9);
        // A line across the ellipse is trimmed back to it.
        let across = line(p(-10.0, 0.0), p(10.0, 0.0));
        let kept = trim_plan(&across, &[ellipse(0.0, TAU)], p(0.0, 0.0)).unwrap();
        assert_eq!(kept.len(), 2, "the part inside goes");
        // The upper half grows clockwise from its start (the major end) down to x = 3.
        let grown = extend_plan(&ellipse(0.0, PI), &[wall(3.0)], p(4.0, 0.1)).unwrap();
        let [Entity::Ellipse(e)] = grown.as_slice() else {
            panic!("an elliptical arc, got {grown:?}")
        };
        assert!((e.start_point().x - 3.0).abs() < 1e-9 && e.start_point().y < 0.0);
        // A line grows to the ellipse.
        let short = line(p(-10.0, 0.0), p(-8.0, 0.0));
        let grown = extend_plan(&short, &[ellipse(0.0, TAU)], p(-8.0, 0.0)).unwrap();
        assert_eq!(grown, vec![line(p(-10.0, 0.0), p(-4.0, 0.0))]);
        assert_eq!(
            extend_plan(&ellipse(0.0, TAU), &[wall(9.0)], p(4.0, 0.0)),
            Err("Object cannot be extended.")
        );
    }

    #[test]
    fn splines_are_refused_by_name_as_objects_and_as_edges() {
        let s = Entity::Spline(Box::new(
            opendraft_core::Spline::from_control_points(1, vec![p(-5.0, 0.0), p(5.0, 0.0)])
                .unwrap(),
        ));
        assert_eq!(
            trim_plan(&s, &[wall(0.0)], p(1.0, 0.0)),
            Err("TRIM does not support splines yet.")
        );
        assert_eq!(
            extend_plan(&s, &[wall(9.0)], p(4.0, 0.0)),
            Err("EXTEND does not support splines yet.")
        );
        let across = line(p(0.0, -5.0), p(0.0, 5.0));
        assert_eq!(
            trim_plan(&across, std::slice::from_ref(&s), p(0.0, 1.0)),
            Err("Splines cannot be cutting edges yet.")
        );
        let short = line(p(0.0, -5.0), p(0.0, -3.0));
        assert_eq!(
            extend_plan(&short, &[s], p(0.0, -3.0)),
            Err("No boundary in the way (splines cannot be boundary edges yet).")
        );
    }

    #[test]
    fn nothing_to_cut_against_or_no_division_is_explained() {
        let target = line(p(0.0, 0.0), p(10.0, 0.0));
        assert_eq!(
            trim_plan(&target, &[], p(5.0, 0.0)),
            Err("Object does not intersect an edge.")
        );
        assert_eq!(
            trim_plan(&target, &[wall(50.0)], p(5.0, 0.0)),
            Err("Object does not intersect an edge.")
        );
        // A cutter touching only the end does not divide the line.
        assert_eq!(
            trim_plan(&target, &[wall(10.0)], p(5.0, 0.0)),
            Err("Object cannot be trimmed there.")
        );
        // A circle needs two cuts.
        let circle = Entity::Circle {
            center: p(0.0, 0.0),
            radius: 2.0,
        };
        let tangent = wall(2.0);
        assert_eq!(
            trim_plan(&circle, &[tangent], p(2.0, 0.0)),
            Err("Object cannot be trimmed there.")
        );
    }

    #[test]
    fn a_circle_trims_to_an_arc() {
        let circle = Entity::Circle {
            center: p(0.0, 0.0),
            radius: 2.0,
        };
        // A vertical cutter through the middle cuts it top and bottom;
        // clicking the right-hand side leaves the left half as an arc.
        let left = trim_plan(&circle, &[wall(0.0)], p(2.0, 0.0)).unwrap();
        assert_eq!(left.len(), 1);
        let Entity::Arc { .. } = left[0] else {
            panic!("a circle trims to an arc, got {:?}", left[0]);
        };
        assert!(left[0].distance_to(p(-2.0, 0.0)) < 1e-9);
        assert!(
            left[0].distance_to(p(2.0, 0.0)) > 1.0,
            "the right side is gone"
        );
    }

    /// Start TRIM with objects already in the drawing.
    fn trimming(entities: &[Entity]) -> AppState {
        let mut state = AppState::new();
        for e in entities {
            state.add_entity(e.clone());
        }
        state.start_command("Trim", trim_objects);
        state.settle_command();
        state
    }

    fn entities(state: &AppState) -> Vec<Entity> {
        let objects = state.session().document().objects();
        objects.iter().map(|o| o.entity.clone()).collect()
    }

    fn id(state: &AppState, index: usize) -> EntityId {
        state.session().document().objects()[index].id
    }

    #[test]
    fn every_object_is_an_edge_without_asking_and_each_click_is_one_undo_step() {
        let mut state = trimming(&[line(p(0.0, 0.0), p(10.0, 0.0)), wall(4.0), wall(6.0)]);
        assert!(state.cmdline.log.iter().any(|l| l.contains("every object")));
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Quick mode, select object to trim"));
        let target = id(&state, 0);
        state.answer_object(p(5.0, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(
            entities(&state)[0],
            line(p(0.0, 0.0), p(4.0, 0.0)),
            "the first piece keeps the object"
        );
        assert!(entities(&state).contains(&line(p(6.0, 0.0), p(10.0, 0.0))));
        assert_eq!(
            entities(&state).len(),
            4,
            "two walls and the two pieces left"
        );
        // Undo (the keyword) takes back that one trim.
        assert!(state.cmd_prompt().options.iter().any(|o| o.label == "Undo"));
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(entities(&state)[0], line(p(0.0, 0.0), p(10.0, 0.0)));
        assert_eq!(entities(&state).len(), 3);
    }

    #[test]
    fn objects_selected_beforehand_are_the_edges_and_the_first_question_is_skipped() {
        let mut state = AppState::new();
        for e in [line(p(0.0, 0.0), p(10.0, 0.0)), wall(4.0), wall(6.0)] {
            state.add_entity(e);
        }
        let near = id(&state, 1);
        state.select_ids([near], false);
        state.start_command("Trim", trim_objects);
        state.settle_command();
        // Straight to choosing what to trim, using only the selected wall.
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Quick mode, select object to trim"));
        assert!(state
            .cmdline
            .log
            .iter()
            .any(|l| l.contains("1 object selected")));
        let target = id(&state, 0);
        state.answer_object(p(8.0, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state)[0], line(p(0.0, 0.0), p(4.0, 0.0)));
        // Extend skips it the same way.
        state.cancel();
        state.cancel();
        state.select_ids([id(&state, 2)], false);
        state.start_command("Extend", extend_objects);
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Quick mode, select object to extend"));
    }

    #[test]
    fn with_every_object_as_edges_trim_erases_what_nothing_crosses() {
        // AutoCAD's Quick mode: TRIM erases it, EXTEND leaves it alone.
        let mut state = trimming(&[line(p(0.0, 0.0), p(10.0, 0.0)), wall(40.0)]);
        let target = id(&state, 0);
        state.answer_object(p(5.0, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state), vec![wall(40.0)]);
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(entities(&state).len(), 2, "Undo brings it back");
        // A path across it erases it too.
        state.answer_path(vec![p(5.0, 1.0), p(5.0, -1.0)], false);
        state.settle_command();
        assert_eq!(entities(&state), vec![wall(40.0)]);

        let mut state = extending(&[line(p(0.0, 0.0), p(10.0, 0.0)), wall(-40.0)]);
        let target = id(&state, 0);
        state.answer_object(p(9.0, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state).len(), 2);
        assert!(state
            .cmdline
            .log
            .last()
            .unwrap()
            .contains("No boundary in the way"));
    }

    #[test]
    fn with_every_object_as_edges_trim_erases_a_line_an_edge_only_touches_at_its_end() {
        // AutoCAD 2027: a slanted line that ends on a long one is erased.
        let edge = line(p(-5.0, 5.0), p(15.0, 5.0));
        let slanted = line(p(0.0, 0.0), p(3.0, 5.0));
        let mut state = trimming(&[edge.clone(), slanted.clone()]);
        let target = id(&state, 1);
        state.answer_object(p(1.5, 2.5), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state), vec![edge.clone()]);

        // With the long line selected as the edge, it is not erased.
        let mut state = AppState::new();
        for e in [edge, slanted] {
            state.add_entity(e);
        }
        state.select_ids([id(&state, 0)], false);
        state.start_command("Trim", trim_objects);
        state.settle_command();
        let target = id(&state, 1);
        state.answer_object(p(1.5, 2.5), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state).len(), 2);
        assert!(state
            .cmdline
            .log
            .last()
            .unwrap()
            .contains("cannot be trimmed there"));
    }

    #[test]
    fn the_mode_option_switches_to_standard_which_asks_for_edges_and_erases_nothing() {
        let mut state = trimming(&[line(p(0.0, 0.0), p(10.0, 0.0)), wall(40.0)]);
        assert!(state.cmd_prompt().options.iter().any(|o| o.label == "mOde"));
        state.cmd_submit("o");
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Enter a trim mode option"));
        state.cmd_submit("s");
        state.settle_command();
        assert_eq!(
            state.cmd_prompt().plain(),
            "TRIM Standard mode, select cutting edges first or <select all> [mOde]:"
        );
        assert!(!state.defaults().trim.get().quick, "remembered");
        state.answer_enter();
        state.settle_command();
        assert!(state.cmd_prompt().text.starts_with("Select object to trim"));
        let target = id(&state, 0);
        state.answer_object(p(5.0, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state).len(), 2, "Standard mode erases nothing");
        // The next TRIM starts in Standard mode, asking for the edges.
        state.cancel();
        state.cancel();
        state.start_command("Trim", trim_objects);
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Standard mode, select cutting edges first"));
        // And back to Quick from the edge question: no edges asked any more.
        state.cmd_submit("o");
        state.settle_command();
        state.cmd_submit("q");
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Quick mode, select object to trim"));
        assert!(state.defaults().trim.get().quick);
    }

    #[test]
    fn an_object_that_no_selected_edge_crosses_is_left_alone_and_the_user_told() {
        let mut state = AppState::new();
        for e in [line(p(0.0, 0.0), p(10.0, 0.0)), wall(40.0)] {
            state.add_entity(e);
        }
        state.select_ids([id(&state, 1)], false);
        state.start_command("Trim", trim_objects);
        state.settle_command();
        let target = id(&state, 0);
        state.answer_object(p(5.0, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state).len(), 2);
        assert!(state
            .cmdline
            .log
            .last()
            .unwrap()
            .contains("does not intersect"));
        assert!(state.command_running(), "still asking for the next object");
        // A click on nothing starts a crossing window; one that meets
        // nothing changes nothing and says so.
        state.answer_object(p(1.0, 1.0), None, false);
        state.settle_command();
        assert!(state
            .cmd_prompt()
            .text
            .starts_with("Specify opposite corner"));
        state.answer_point(p(2.0, 2.0));
        state.settle_command();
        assert_eq!(entities(&state).len(), 2);
        assert!(state
            .cmdline
            .log
            .iter()
            .any(|l| l == "Nothing to trim there."));
        assert!(state.command_running());
        state.cmd_submit("");
        state.settle_command();
        assert!(!state.command_running(), "Enter ends it");
    }

    #[test]
    fn extend_plan_lengthens_the_end_nearest_the_click() {
        let target = line(p(0.0, 0.0), p(2.0, 0.0));
        let walls = [wall(5.0), wall(-3.0)];
        assert_eq!(
            extend_plan(&target, &walls, p(1.9, 0.0)).unwrap(),
            [line(p(0.0, 0.0), p(5.0, 0.0))]
        );
        assert_eq!(
            extend_plan(&target, &walls, p(0.1, 0.0)).unwrap(),
            [line(p(-3.0, 0.0), p(2.0, 0.0))]
        );
        assert_eq!(
            extend_plan(&target, &[wall(-50.0)], p(1.9, 0.0)),
            Err("No boundary in the way.")
        );
        let circle = Entity::Circle {
            center: p(0.0, 0.0),
            radius: 1.0,
        };
        assert_eq!(
            extend_plan(&circle, &walls, p(1.0, 0.0)),
            Err("Object cannot be extended.")
        );
    }

    fn extending(entities: &[Entity]) -> AppState {
        let mut state = AppState::new();
        for e in entities {
            state.add_entity(e.clone());
        }
        state.start_command("Extend", extend_objects);
        state.settle_command();
        state
    }

    #[test]
    fn extend_grows_an_object_and_shift_trims_it_instead() {
        let mut state = extending(&[line(p(0.0, 0.0), p(2.0, 0.0)), wall(6.0), wall(1.0)]);
        let target = id(&state, 0);
        // Near the right end: grows to the wall at 6 (the wall at 1 is behind).
        state.answer_object(p(1.9, 0.0), Some(target), false);
        state.settle_command();
        assert_eq!(entities(&state)[0], line(p(0.0, 0.0), p(6.0, 0.0)));
        // Shift-click trims instead: the wall at 1 cuts the line and the
        // part clicked at 4 goes, leaving 0 to 1.
        state.answer_object(p(4.0, 0.0), Some(target), true);
        state.settle_command();
        assert_eq!(entities(&state)[0], line(p(0.0, 0.0), p(1.0, 0.0)));
        assert_eq!(entities(&state).len(), 3);
    }

    #[test]
    fn shift_extends_while_trimming() {
        let mut state = trimming(&[line(p(0.0, 0.0), p(2.0, 0.0)), wall(6.0)]);
        let target = id(&state, 0);
        state.answer_object(p(1.9, 0.0), Some(target), true);
        state.settle_command();
        assert_eq!(entities(&state)[0], line(p(0.0, 0.0), p(6.0, 0.0)));
    }
}
