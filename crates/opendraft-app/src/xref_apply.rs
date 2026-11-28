//! Applying a planned external reference through commands: layers and tables
//! first, then the record, then the root and the inserts. Each operation is
//! one transaction, so attach, reload, unload, detach and path edits undo in
//! one step and a failure leaves the drawing as it was.

use opendraft_commands::{
    AddLayer, Cancelled, Editor, ModifyAttributes, ModifyEntities, ModifyLayers, RemoveEntities,
    RemoveLayer, SetBlocks, SetLayouts, SetLinetypes, SetTextStyles, SetXrefs, Transaction,
};
use opendraft_core::{
    BindPlan, BindType, BlockId, Entity, Insert, Point, XbindSymbol, XrefDef, XrefPathType,
    XrefPlan,
};

/// Put the plan's layers, styles and linetypes into the transaction.
fn tables(tx: &mut Transaction, plan: &XrefPlan) {
    for layer in &plan.added_layers {
        tx.perform(AddLayer::new(layer.clone()));
    }
    if !plan.changed_layers.is_empty() {
        tx.perform(ModifyLayers::new(
            "External reference layers",
            plan.changed_layers.clone(),
        ));
    }
    if let Some(styles) = &plan.text_styles {
        tx.perform(SetTextStyles::new(
            styles.styles.clone(),
            styles.current,
            styles.next_id,
        ));
    }
    if let Some(table) = &plan.linetypes {
        let scale = tx.document().linetype_scale();
        tx.perform(SetLinetypes::new(table.clone(), scale));
    }
}

/// The table with `record` inserted or replacing the record for its root.
fn with_record(doc: &opendraft_core::Document, record: &XrefDef) -> Vec<XrefDef> {
    let mut xrefs = doc.xrefs().to_vec();
    match xrefs.iter_mut().find(|x| x.block == record.block) {
        Some(slot) => *slot = record.clone(),
        None => xrefs.push(record.clone()),
    }
    xrefs
}

/// Attach: add the snapshot and one insert at `at`.
pub(crate) fn attach(
    ed: &mut Editor,
    plan: XrefPlan,
    at: Point,
    scale: (f64, f64),
    rotation: f64,
) -> Result<(), Cancelled> {
    let unit = plan.unit_factor;
    ed.transaction("Attach external reference", move |tx| {
        tables(tx, &plan);
        let xrefs = with_record(tx.document(), &plan.xref);
        tx.perform(SetXrefs::new("Attach external reference", xrefs));
        let mut blocks = tx.document().blocks().to_vec();
        blocks.push(plan.root.clone());
        tx.perform(SetBlocks::new("Attach external reference", blocks));
        let mut insert = Insert::new_xref(plan.root.clone(), at);
        insert.scale_x = scale.0 * unit;
        insert.scale_y = scale.1 * unit;
        insert.rotation = rotation;
        tx.add(Entity::Insert(Box::new(insert)));
        Ok(())
    })
}

/// Reload: replace the snapshot and repoint every existing insert in one
/// step. The root's id is stable, so selection and undo see the same objects.
pub(crate) fn reload(ed: &mut Editor, plan: XrefPlan) -> Result<(), Cancelled> {
    let root = plan.root.clone();
    let redefinition = ed.read(move |doc| doc.redefined_blocks((*root).clone()))?;
    let Ok(redefinition) = redefinition else {
        ed.say("Could not reload the external reference: its blocks form a loop.");
        return Ok(());
    };
    ed.transaction("Reload external reference", move |tx| {
        tables(tx, &plan);
        let xrefs = with_record(tx.document(), &plan.xref);
        tx.perform(SetXrefs::new("Reload external reference", xrefs));
        tx.perform(SetBlocks::new(
            "Reload external reference",
            redefinition.blocks,
        ));
        tx.perform(ModifyEntities::new(
            "Reload external reference",
            redefinition.objects,
        ));
        Ok(())
    })
}

/// Unload: empty the root and mark the record, keeping path, stamp and layer
/// bindings. The insert stays selectable at its insertion point.
pub(crate) fn unload(ed: &mut Editor, block: BlockId) -> Result<(), Cancelled> {
    let redefinition = ed.read(move |doc| {
        let root = doc.block(block)?;
        let mut emptied = (**root).clone();
        emptied.entities.clear();
        doc.redefined_blocks(emptied).ok()
    })?;
    let Some(redefinition) = redefinition else {
        ed.say("That external reference is not in the drawing.");
        return Ok(());
    };
    ed.transaction("Unload external reference", move |tx| {
        let doc = tx.document();
        let Some(record) = doc.xref(block).cloned() else {
            return Ok(());
        };
        let mut record = record;
        record.unloaded = true;
        record.blocks.clear();
        let xrefs = with_record(doc, &record);
        tx.perform(SetXrefs::new("Unload external reference", xrefs));
        tx.perform(SetBlocks::new(
            "Unload external reference",
            redefinition.blocks,
        ));
        tx.perform(ModifyEntities::new(
            "Unload external reference",
            redefinition.objects,
        ));
        Ok(())
    })
}

/// Detach: remove every insert, the record, the root, the local definitions
/// and the bound layers that nothing else uses, in one step.
pub(crate) fn detach(ed: &mut Editor, blocks: Vec<BlockId>) -> Result<(), Cancelled> {
    ed.transaction("Detach external reference", move |tx| {
        for block in &blocks {
            let doc = tx.document();
            let Some(record) = doc.xref(*block).cloned() else {
                continue;
            };
            let inserts = doc.xref_references(*block);
            let bound: Vec<_> = record.layers.iter().map(|l| l.layer).collect();
            let mut xrefs = doc.xrefs().to_vec();
            xrefs.retain(|x| x.block != *block);
            let mut table = doc.blocks().to_vec();
            table.retain(|b| b.id != *block);
            tx.perform(SetXrefs::new("Detach external reference", xrefs));
            tx.perform(RemoveEntities::new(inserts));
            tx.perform(SetBlocks::new("Detach external reference", table));
            for layer in bound {
                if tx.document().layer_delete_blocker(layer).is_none() {
                    tx.perform(RemoveLayer::new(layer));
                }
            }
        }
        Ok(())
    })
}

/// Bind: make the snapshot and its dependent symbols ordinary host records.
pub(crate) fn bind(ed: &mut Editor, blocks: Vec<BlockId>, kind: BindType) -> Result<(), Cancelled> {
    let plan = ed.read(move |doc| doc.plan_bind(&blocks, kind))?;
    match plan {
        Ok(plan) => apply_plan(ed, plan, "Bind external reference"),
        Err(why) => {
            ed.say(format!("Could not bind the external reference: {why}"));
            Ok(())
        }
    }
}

/// XBIND: make selected dependent symbols ordinary host records, keeping the
/// reference itself.
pub(crate) fn xbind(
    ed: &mut Editor,
    symbols: Vec<XbindSymbol>,
    kind: BindType,
) -> Result<(), Cancelled> {
    if symbols.is_empty() {
        ed.say("No dependent symbol was chosen.");
        return Ok(());
    }
    let plan = ed.read(move |doc| doc.plan_xbind(&symbols, kind))?;
    match plan {
        Ok(plan) => apply_plan(ed, plan, "Bind dependent symbols"),
        Err(why) => {
            ed.say(format!("Could not bind the dependent symbols: {why}"));
            Ok(())
        }
    }
}

fn apply_plan(ed: &mut Editor, plan: BindPlan, title: &'static str) -> Result<(), Cancelled> {
    let notes = plan.notes.clone();
    ed.transaction(title, move |tx| {
        tx.perform(SetXrefs::new(title, plan.xrefs));
        tx.perform(SetBlocks::new(title, plan.blocks));
        if !plan.entities.is_empty() {
            tx.perform(ModifyEntities::new(title, plan.entities));
        }
        if !plan.attributes.is_empty() {
            tx.perform(ModifyAttributes::new(title, plan.attributes));
        }
        if !plan.layers.is_empty() {
            tx.perform(ModifyLayers::new(title, plan.layers));
        }
        if !plan.removed_layers.is_empty() {
            tx.perform(RemoveLayer::many(plan.removed_layers));
        }
        if let Some(styles) = plan.text_styles {
            tx.perform(SetTextStyles::new(
                styles,
                plan.current_text_style,
                plan.next_text_style,
            ));
        }
        if let Some(table) = plan.linetypes {
            let scale = tx.document().linetype_scale();
            tx.perform(SetLinetypes::new(table, scale));
        }
        if let Some(layouts) = plan.layouts {
            tx.perform(SetLayouts::new(title, layouts));
        }
        Ok(())
    })?;
    for note in notes {
        ed.say(note);
    }
    Ok(())
}

/// Edit one reference's saved path. Reload is not implied, as in AutoCAD.
pub(crate) fn edit_path(
    ed: &mut Editor,
    block: BlockId,
    path: String,
    path_type: XrefPathType,
) -> Result<(), Cancelled> {
    ed.transaction("Edit external reference path", move |tx| {
        let doc = tx.document();
        let Some(record) = doc.xref(block) else {
            return Ok(());
        };
        let mut record = record.clone();
        record.path = path;
        record.path_type = path_type;
        let xrefs = with_record(doc, &record);
        tx.perform(SetXrefs::new("Edit external reference path", xrefs));
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use opendraft_core::{Document, Entity, LayerId, Point};
    use opendraft_files::{plan_xref, XrefLimits, XrefPlanRequest};

    use crate::state::AppState;

    use super::*;

    fn source() -> Document {
        Document::from_entities([Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(2.0, 0.0),
        }])
    }

    fn request() -> XrefPlanRequest {
        XrefPlanRequest {
            block: None,
            name: "base".to_owned(),
            source_path: "/work/base.dwg".to_owned(),
            path_type: XrefPathType::Relative,
            kind: opendraft_core::XrefKind::Attach,
            host_dir: None,
            host_path: None,
        }
    }

    fn planned(host: &Document, request: XrefPlanRequest) -> XrefPlan {
        plan_xref(
            host,
            &source(),
            &request,
            &XrefLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn attach_unload_reload_and_detach_are_each_one_undo_step() {
        let mut state = AppState::new();
        let plan = planned(state.session().document(), request());
        let root = plan.xref.block;
        state.start_command("XATTACH", move |ed| {
            attach(ed, plan, Point::new(10.0, 5.0), (2.0, 2.0), 0.0)
        });
        state.settle_command();
        {
            let doc = state.session().document();
            assert_eq!(doc.xrefs().len(), 1);
            assert_eq!(doc.objects().len(), 1);
            let Entity::Insert(insert) = &doc.objects()[0].entity else {
                panic!("an insert was expected")
            };
            assert!(insert.is_xref);
            assert_eq!(insert.insertion, Point::new(10.0, 5.0));
            assert_eq!((insert.scale_x, insert.scale_y), (2.0, 2.0));
            assert_eq!(doc.objects()[0].attrs.layer, LayerId::DEFAULT);
            assert_eq!(doc.validate(), Ok(()));
        }
        state.undo();
        assert!(state.session().document().xrefs().is_empty());
        assert!(state.session().document().objects().is_empty());
        state.redo();
        assert_eq!(state.session().document().xrefs().len(), 1);

        state.start_command("Unload", move |ed| unload(ed, root));
        state.settle_command();
        {
            let doc = state.session().document();
            assert!(doc.xref_is_unloaded(root));
            assert!(doc.block(root).unwrap().entities.is_empty());
            assert_eq!(doc.objects().len(), 1, "the insert stays selectable");
            assert_eq!(doc.validate(), Ok(()));
        }

        let mut reload = request();
        reload.block = Some(root);
        let plan = planned(state.session().document(), reload);
        state.start_command("Reload", move |ed| reload_plan(ed, plan));
        state.settle_command();
        {
            let doc = state.session().document();
            assert!(!doc.xref_is_unloaded(root));
            assert_eq!(doc.block(root).unwrap().entities.len(), 1);
            assert_eq!(doc.validate(), Ok(()));
        }

        state.start_command("Detach", move |ed| detach(ed, vec![root]));
        state.settle_command();
        let doc = state.session().document();
        assert!(doc.xrefs().is_empty());
        assert!(doc.objects().is_empty());
        assert!(doc.block(root).is_none());
        assert_eq!(doc.validate(), Ok(()));
    }

    #[test]
    fn bind_makes_the_reference_local_and_undoes_as_one_step() {
        use std::sync::Arc;

        use opendraft_core::{
            Attributes, BlockDef, BlockEntity, BlockObjectId, Layer, XbindSymbol,
        };

        let mut source = Document::default();
        let walls = source.allocate_layer_id();
        source.push_layer(Layer::new(walls, "Walls"));
        let chair_id = source.allocate_block_id();
        let chair = Arc::new(BlockDef {
            id: chair_id,
            name: "Chair".to_owned(),
            base: Point::default(),
            description: String::new(),
            unit: Default::default(),
            entities: vec![BlockEntity::new(
                BlockObjectId(0),
                Entity::Circle {
                    center: Point::default(),
                    radius: 1.0,
                },
                Attributes::on_layer(walls),
            )],
            dynamic: None,
        });
        source.push_block(chair.clone());
        source.add_with(
            Entity::Line {
                start: Point::default(),
                end: Point::new(2.0, 0.0),
            },
            Attributes::on_layer(walls),
        );
        source.add(Entity::Insert(Box::new(Insert::new(
            chair,
            Point::new(4.0, 4.0),
        ))));
        source.validate().unwrap();

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

        state.start_command("Bind", move |ed| {
            super::bind(ed, vec![root], BindType::Bind)
        });
        state.settle_command();
        {
            let doc = state.session().document();
            assert!(doc.xrefs().is_empty());
            assert!(doc.block_named("base$0$Chair").is_some());
            assert!(doc.layers().iter().any(|l| l.name == "base$0$Walls"));
            let Entity::Insert(insert) = &doc.objects()[0].entity else {
                panic!("an insert was expected")
            };
            assert!(!insert.is_xref);
            assert_eq!(doc.validate(), Ok(()));
        }
        state.undo();
        {
            let doc = state.session().document();
            assert_eq!(doc.xrefs().len(), 1);
            assert!(doc.layers().iter().any(|l| l.name == "base|Walls"));
            assert_eq!(doc.validate(), Ok(()));
        }
        state.redo();
        assert!(state.session().document().xrefs().is_empty());

        // XBIND keeps the reference and makes only the layer local.
        state.undo();
        let (root, layer) = {
            let doc = state.session().document();
            let root = doc.block_named("base").unwrap().id;
            let layer = doc.xref(root).unwrap().layers[0].layer;
            (root, layer)
        };
        state.start_command("XBIND", move |ed| {
            super::xbind(
                ed,
                vec![XbindSymbol::Layer { xref: root, layer }],
                BindType::Bind,
            )
        });
        state.settle_command();
        {
            let doc = state.session().document();
            assert_eq!(doc.xrefs().len(), 1);
            assert!(doc.xrefs()[0].layers.is_empty());
            assert!(doc.layers().iter().any(|l| l.name == "base$0$Walls"));
            assert_eq!(doc.validate(), Ok(()));
        }
        state.undo();
        assert_eq!(
            state.session().document().xrefs()[0].layers.len(),
            1,
            "undo puts the layer binding back"
        );
    }

    /// The public `reload` name clashes with the local variable above.
    fn reload_plan(ed: &mut Editor, plan: XrefPlan) -> Result<(), Cancelled> {
        super::reload(ed, plan)
    }
}
