//! `XCLIP`: a boundary that limits how much of an external reference draws.
//!
//! A new boundary is picked in the drawing and stored in each reference's own
//! block coordinates, so it moves, turns and scales with the insert. ON and
//! OFF keep the boundary and show or hide the clipped part; Delete removes it.

use std::collections::HashSet;

use opendraft_commands::{Answer, Cancelled, Editor, Options, PointOptions, SetXrefs, Wording};
use opendraft_core::{clip_problem, BlockId, Entity, EntityId, Point, XrefClip, MAX_CLIP_POINTS};
use opendraft_ui::key;

/// The command: select references, then choose what to do with the boundary.
pub(crate) fn xclip(ed: &mut Editor) -> Result<(), Cancelled> {
    let chosen = ed.pickset_or_select(key!("Select external references to clip"))?;
    let picked: Vec<(BlockId, EntityId)> = ed.read(move |doc| {
        chosen
            .iter()
            .filter_map(|id| doc.xref_for_object(*id).map(|xref| (xref.block, *id)))
            .collect()
    })?;
    if picked.is_empty() {
        ed.say(key!(
            "Select an external reference: only a reference has a clip boundary."
        ));
        return Ok(());
    }
    let options = Options::<String>::new(key!(
        "Enter an option [New boundary/ON/OFF/Delete] <New boundary>"
    ))
    .keyword("New boundary", "N")
    .keyword("ON", "ON")
    .keyword("OFF", "OF")
    .keyword("Delete", "D")
    .default("New boundary".to_owned());
    match ed.get_keyword(options)?.as_str() {
        "ON" => set_enabled(ed, &picked, true),
        "OFF" => set_enabled(ed, &picked, false),
        "Delete" => delete(ed, &picked),
        _ => new_boundary(ed, &picked),
    }
}

/// Turn the boundary on or off, keeping it.
fn set_enabled(
    ed: &mut Editor,
    picked: &[(BlockId, EntityId)],
    enabled: bool,
) -> Result<(), Cancelled> {
    let blocks: Vec<BlockId> = unique_blocks(picked);
    let exists = ed.read(move |doc| {
        blocks
            .iter()
            .any(|block| doc.xref(*block).is_some_and(|xref| xref.clip.is_some()))
    })?;
    if !exists {
        ed.say(key!("The selected references have no clip boundary."));
        return Ok(());
    }
    let blocks = unique_blocks(picked);
    edit_records(
        ed,
        if enabled { "Clip on" } else { "Clip off" },
        blocks,
        move |xref| {
            if let Some(clip) = &mut xref.clip {
                clip.enabled = enabled;
            }
        },
    )
}

/// Remove the boundary from the selected references.
fn delete(ed: &mut Editor, picked: &[(BlockId, EntityId)]) -> Result<(), Cancelled> {
    let blocks = unique_blocks(picked);
    edit_records(ed, "Delete clip boundary", blocks, |xref| {
        xref.clip = None;
    })
}

/// Pick a new boundary: two corners, or a polygon closed with Enter.
fn new_boundary(ed: &mut Editor, picked: &[(BlockId, EntityId)]) -> Result<(), Cancelled> {
    let options = Options::<String>::new(key!(
        "Enter boundary type [Rectangular/Polygonal] <Rectangular>"
    ))
    .keyword("Rectangular", "R")
    .keyword("Polygonal", "P")
    .default("Rectangular".to_owned());
    let world = if ed.get_keyword(options)? == "Polygonal" {
        let mut points = Vec::new();
        loop {
            let prompt = if points.is_empty() {
                key!("Specify first point of the clip boundary")
            } else {
                key!("Specify next point, or press Enter to close the boundary")
            };
            match ed.ask_point(PointOptions::new(prompt))? {
                Answer::Value(point) => {
                    points.push(point);
                    if points.len() >= MAX_CLIP_POINTS {
                        break;
                    }
                }
                _ => break,
            }
        }
        points
    } else {
        let first = ed.get_point(PointOptions::new(key!(
            "Specify first corner of the clip boundary"
        )))?;
        let second = ed.get_point(PointOptions::new(key!("Specify opposite corner")))?;
        vec![
            first,
            Point::new(second.x, first.y),
            second,
            Point::new(first.x, second.y),
        ]
    };
    if world.len() < 3 || clip_problem(&XrefClip::new(world.clone())).is_some() {
        ed.say(key!(
            "A clip boundary needs at least three points enclosing an area."
        ));
        return Ok(());
    }
    let picked = picked.to_vec();
    let planned: Vec<(BlockId, Vec<Point>)> = ed.read(move |doc| {
        let mut seen = HashSet::new();
        picked
            .iter()
            .filter_map(|(block, id)| {
                if !seen.insert(*block) {
                    return None;
                }
                let inverse = match doc.object(*id).map(|object| &object.entity) {
                    Some(Entity::Insert(insert)) => insert.transform().inverse()?,
                    _ => return None,
                };
                Some((
                    *block,
                    world.iter().map(|point| inverse.apply(*point)).collect(),
                ))
            })
            .collect()
    })?;
    let count = planned.len();
    let blocks: Vec<BlockId> = planned.iter().map(|(block, _)| *block).collect();
    if count == 0 {
        ed.say(key!("The selected references could not be clipped."));
        return Ok(());
    }
    edit_records(ed, "Clip external reference", blocks, move |xref| {
        if let Some((_, boundary)) = planned.iter().find(|(block, _)| *block == xref.block) {
            xref.clip = Some(XrefClip::new(boundary.clone()));
        }
    })?;
    ed.say(Wording::count(
        count,
        key!("Clipped %{count} external reference."),
        key!("Clipped %{count} external references."),
    ));
    Ok(())
}

/// The references' blocks, once each, in selection order.
fn unique_blocks(picked: &[(BlockId, EntityId)]) -> Vec<BlockId> {
    let mut seen = HashSet::new();
    picked
        .iter()
        .map(|(block, _)| *block)
        .filter(|block| seen.insert(*block))
        .collect()
}

/// Change the selected records' clips as one undo step.
fn edit_records(
    ed: &mut Editor,
    title: &'static str,
    blocks: Vec<BlockId>,
    change: impl Fn(&mut opendraft_core::XrefDef) + Send + 'static,
) -> Result<(), Cancelled> {
    ed.transaction(title, move |tx| {
        let mut records = tx.document().xrefs().to_vec();
        for record in &mut records {
            if blocks.contains(&record.block) {
                change(record);
            }
        }
        tx.perform(SetXrefs::new(title, records));
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use opendraft_core::XrefLoadRequest;
    use opendraft_core::{BlockId, Document, Entity, Point, XrefPathType};
    use opendraft_files::{plan_xref, XrefLimits};

    use crate::state::AppState;
    use crate::xref_apply::attach;

    fn request() -> XrefLoadRequest {
        XrefLoadRequest {
            block: None,
            name: "base".to_owned(),
            source_path: "/work/base.dwg".to_owned(),
            path_type: XrefPathType::Relative,
            kind: opendraft_core::XrefKind::Attach,
            host_dir: None,
            host_path: None,
        }
    }

    /// A drawing with an attached reference whose one line runs from x=0 to
    /// x=2 at the origin.
    fn attached() -> (AppState, BlockId) {
        let source = Document::from_entities([Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(2.0, 0.0),
        }]);
        let mut state = AppState::new();
        let plan = plan_xref(
            state.session().document(),
            &source,
            &request(),
            &XrefLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let root = plan.xref.block;
        state.start_command("XATTACH", move |ed| {
            attach(ed, plan, Point::default(), (1.0, 1.0), 0.0)
        });
        state.settle_command();
        (state, root)
    }

    fn run(state: &mut AppState, option: &str) {
        state.select_all();
        state.start_command("XCLIP", super::xclip);
        state.settle_command();
        state.answer_keyword(option);
        state.settle_command();
    }

    fn clip_of(state: &AppState, root: BlockId) -> Option<opendraft_core::XrefClip> {
        state
            .session()
            .document()
            .xref(root)
            .and_then(|xref| xref.clip.clone())
    }

    #[test]
    fn a_rectangular_clip_cuts_the_reference_in_one_undo_step() {
        let (mut state, root) = attached();
        state.select_all();
        state.start_command("XCLIP", super::xclip);
        state.settle_command();
        state.answer_enter();
        state.settle_command();
        state.answer_keyword("Rectangular");
        state.settle_command();
        state.answer_point(Point::new(0.0, -1.0));
        state.settle_command();
        state.answer_point(Point::new(1.5, 1.0));
        state.settle_command();
        let clip = clip_of(&state, root).expect("the clip was made");
        assert!(clip.enabled);
        assert_eq!(clip.boundary.len(), 4);
        {
            let doc = state.session().document();
            let pieces = doc.insert_pieces(&doc.objects()[0]).unwrap();
            assert_eq!(pieces.len(), 1);
            let Entity::Line { end, .. } = &pieces[0].entity else {
                panic!("a clipped line was expected")
            };
            assert!((end.x - 1.5).abs() < 1e-9, "{end:?}");
            assert_eq!(doc.validate(), Ok(()));
        }
        state.undo();
        assert!(clip_of(&state, root).is_none(), "the clip is one step");
        state.redo();
        assert!(clip_of(&state, root).is_some());
    }

    #[test]
    fn off_on_and_delete_keep_the_reference() {
        let (mut state, root) = attached();
        run(&mut state, "OFF");
        assert!(
            clip_of(&state, root).is_none(),
            "off needs a boundary first"
        );

        state.select_all();
        state.start_command("XCLIP", super::xclip);
        state.settle_command();
        state.answer_enter();
        state.settle_command();
        state.answer_keyword("Rectangular");
        state.settle_command();
        state.answer_point(Point::new(0.0, -1.0));
        state.settle_command();
        state.answer_point(Point::new(1.0, 1.0));
        state.settle_command();

        run(&mut state, "OFF");
        assert_eq!(clip_of(&state, root).map(|clip| clip.enabled), Some(false));
        run(&mut state, "ON");
        assert_eq!(clip_of(&state, root).map(|clip| clip.enabled), Some(true));
        run(&mut state, "Delete");
        assert!(clip_of(&state, root).is_none());
        assert_eq!(state.session().document().xrefs().len(), 1);
        assert_eq!(state.session().document().validate(), Ok(()));
    }

    #[test]
    fn a_polygonal_clip_closes_on_enter() {
        let (mut state, root) = attached();
        state.select_all();
        state.start_command("XCLIP", super::xclip);
        state.settle_command();
        state.answer_enter();
        state.settle_command();
        state.answer_keyword("Polygonal");
        state.settle_command();
        for point in [
            Point::new(0.0, -1.0),
            Point::new(1.5, -1.0),
            Point::new(1.5, 1.0),
        ] {
            state.answer_point(point);
            state.settle_command();
        }
        state.answer_enter();
        state.settle_command();
        let clip = clip_of(&state, root).expect("the polygon was kept");
        assert_eq!(clip.boundary.len(), 3);
    }
}
