//! EXPLODE: break rectangles and polylines into lines and arcs, block
//! references into the objects of their block, and pattern hatches into their
//! strokes. The geometry is `Entity::explode` and `Insert::exploded` in
//! `opendraft_core`.

use opendraft_commands::{Cancelled, Editor};
use opendraft_core::hatch::{HatchFill, HatchLimits};
use opendraft_core::HatchEntity;
use opendraft_core::{Attributes, Document, Entity, EntityId, Object};
use opendraft_ui::key;

use crate::hatch_cmd::check_tolerance;

use opendraft_commands::Wording;

/// EXPLODE: select the objects (or use those already selected). Each
/// rectangle or polyline is replaced by its lines and arcs, which keep its
/// layer and attributes. A block reference is replaced by its block's
/// objects, one level down, each with its own layer and attributes as in
/// AutoCAD, except that a ByBlock colour, linetype or lineweight takes the
/// reference's. A pattern hatch becomes the lines (and points) of its pattern,
/// cut to its boundary; a solid or gradient hatch is left, with a note.
/// Other objects are left alone. One undo step.
pub fn explode_objects(ed: &mut Editor) -> Result<(), Cancelled> {
    let chosen = ed.pickset_or_select(key!("Select objects to explode"))?;
    let (found, filled, refused) = ed.read(move |doc| {
        let refused = chosen
            .iter()
            .filter(|id| doc.xref_for_object(**id).is_some())
            .count();
        let found = chosen
            .iter()
            .filter(|id| doc.xref_for_object(**id).is_none())
            .filter_map(|id| doc.object(*id).map(|o| (*id, parts_of(doc, o))))
            .collect::<Vec<_>>();
        let filled = chosen
            .iter()
            .filter_map(|id| doc.object(*id))
            .filter(|o| matches!(&o.entity, Entity::Hatch(h) if !is_pattern(h)))
            .count();
        (found, filled, refused)
    })?;
    let mut gone = Vec::new();
    let mut parts: Vec<(Entity, Attributes)> = Vec::new();
    for (id, pieces) in found {
        let Some(pieces) = pieces else { continue };
        gone.push(id);
        parts.extend(pieces);
    }
    if filled > 0 {
        ed.say(key!("Solid and gradient hatches cannot be exploded yet."));
    }
    if refused > 0 {
        ed.say(key!(
            "An external reference cannot be exploded; detach it or edit the source drawing."
        ));
    }
    if gone.is_empty() && (filled > 0 || refused > 0) {
        return Ok(());
    }
    if gone.is_empty() {
        ed.say(key!(
            "Nothing to explode: lines, circles, arcs, ellipses and splines are already simple."
        ));
        return Ok(());
    }
    let (from, to) = (gone.len(), parts.len());
    let made = ed.transaction("Explode", move |tx| {
        let before = tx.document().objects().len();
        tx.add_with(parts);
        let made: Vec<EntityId> = tx.document().objects()[before..]
            .iter()
            .map(|o| o.id)
            .collect();
        tx.remove(gone);
        Ok(made)
    })?;
    // As in AutoCAD, the pieces end up selected.
    ed.select(made);
    let said = Wording::new(key!("Exploded %{objects} into %{into}."), &[]);
    let said = said.part("objects", crate::text::objects(from));
    ed.say(said.part("into", crate::text::objects(to)));
    Ok(())
}

/// What an object explodes into, with the attributes each part keeps, or
/// `None` when it is already simple.
fn parts_of(doc: &Document, object: &Object) -> Option<Vec<(Entity, Attributes)>> {
    if let Entity::Insert(insert) = &object.entity {
        let parts = insert
            .exploded(&object.attrs, &|l| doc.layer_look(l))
            .into_iter();
        return Some(parts.map(|p| (p.entity, p.attrs)).collect());
    }
    if let Entity::Hatch(h) = &object.entity {
        return hatch_strokes(h).map(|parts| {
            parts
                .into_iter()
                .map(|e| (e, object.attrs.clone()))
                .collect()
        });
    }
    let pieces = object.entity.explode()?;
    Some(
        pieces
            .into_iter()
            .map(|e| (e, object.attrs.clone()))
            .collect(),
    )
}

fn is_pattern(h: &HatchEntity) -> bool {
    matches!(h.fill, HatchFill::Pattern(_))
}

/// The lines and points a pattern hatch draws, or `None` for a solid or
/// gradient fill, or a hatch whose boundary cannot be followed.
fn hatch_strokes(h: &HatchEntity) -> Option<Vec<Entity>> {
    if !is_pattern(h) {
        return None;
    }
    let done = h
        .evaluate(check_tolerance(h), &HatchLimits::default())
        .ok()?;
    let lines = done.strokes.iter().map(|s| Entity::Line {
        start: s[0],
        end: s[1],
    });
    let points = done.dots.iter().map(|at| Entity::Point { at: *at });
    Some(lines.chain(points).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use opendraft_core::Point;

    fn entities(state: &AppState) -> Vec<Entity> {
        let objects = state.session().document().objects();
        objects.iter().map(|o| o.entity.clone()).collect()
    }

    fn exploding(items: &[Entity]) -> AppState {
        let mut state = AppState::new();
        for e in items {
            state.add_entity(e.clone());
        }
        state.select_all();
        state.start_command("Explode", explode_objects);
        state.settle_command();
        state
    }

    #[test]
    fn a_rectangle_becomes_four_lines_in_one_undo_step() {
        let circle = Entity::Circle {
            center: Point::new(9.0, 9.0),
            radius: 1.0,
        };
        let mut state = exploding(&[
            Entity::Rectangle {
                min: Point::new(0.0, 0.0),
                max: Point::new(2.0, 1.0),
            },
            circle.clone(),
        ]);
        let now = entities(&state);
        assert_eq!(now.len(), 5);
        assert!(now.contains(&circle));
        assert_eq!(
            now.iter()
                .filter(|e| matches!(e, Entity::Line { .. }))
                .count(),
            4
        );
        let selected = state.selection().len();
        assert_eq!(selected, 5, "the pieces and the circle stay selected");
        state.undo();
        assert_eq!(entities(&state).len(), 2);
    }

    #[test]
    fn simple_objects_are_left_alone() {
        let state = exploding(&[Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
        }]);
        assert_eq!(entities(&state).len(), 1);
    }

    /// A square hatch, 0 to 4, filled with `fill`.
    fn hatch(fill: HatchFill) -> Entity {
        use opendraft_core::hatch::{HatchBoundary, HatchLoop, HatchLoopFlags, HatchPolyline};
        let square = HatchPolyline {
            vertices: vec![
                Point::new(0.0, 0.0),
                Point::new(4.0, 0.0),
                Point::new(4.0, 4.0),
                Point::new(0.0, 4.0),
            ],
            bulges: Vec::new(),
            closed: true,
        };
        let outer = HatchLoop {
            flags: HatchLoopFlags(HatchLoopFlags::EXTERNAL | HatchLoopFlags::POLYLINE),
            boundary: HatchBoundary::Polyline(square),
            sources: Vec::new(),
        };
        Entity::Hatch(Box::new(HatchEntity::new(vec![outer], fill)))
    }

    fn rules() -> HatchFill {
        use opendraft_core::hatch::{HatchPattern, PatternLine, PatternType};
        HatchFill::Pattern(HatchPattern {
            pattern_type: PatternType::UserDefined,
            name: "USER".to_owned(),
            angle: 0.0,
            scale_or_spacing: 1.0,
            double: false,
            lines: vec![PatternLine {
                angle: 0.0,
                base: Point::new(0.0, 0.5),
                offset: [0.0, 1.0],
                dashes: Vec::new(),
            }],
            origin: Point::new(0.0, 0.0),
        })
    }

    #[test]
    fn a_pattern_hatch_becomes_its_lines_cut_to_the_boundary() {
        let mut state = exploding(&[hatch(rules())]);
        let now = entities(&state);
        assert_eq!(now.len(), 4, "{now:?}");
        for e in &now {
            let Entity::Line { start, end } = e else {
                panic!("{e:?}")
            };
            assert!((start.y - end.y).abs() < 1e-9);
            assert!(start.x.min(end.x) >= -1e-6 && start.x.max(end.x) <= 4.0 + 1e-6);
        }
        state.undo();
        assert!(matches!(entities(&state)[0], Entity::Hatch(_)));
    }

    #[test]
    fn a_solid_hatch_is_left_with_a_note() {
        let state = exploding(&[hatch(HatchFill::Solid)]);
        assert!(matches!(entities(&state)[0], Entity::Hatch(_)));
        assert!(state
            .cmdline
            .log
            .iter()
            .any(|l| l.contains("Solid and gradient hatches cannot be exploded yet.")));
    }
}
