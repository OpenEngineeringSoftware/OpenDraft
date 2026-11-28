//! Turning a source drawing into a snapshot of the host: its layers, styles,
//! linetypes, local blocks and root, plus the record that keeps the reference.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use opendraft_core::block::block_name_problem;
use opendraft_core::{
    BlockDef, BlockId, Document, InsertUnits, Layer, LayerId, LinetypeId, LinetypeTable, TextStyle,
    TextStyleId, TextStylePlan, XrefDef, XrefKind, XrefLayer, XrefPathType, XrefPlan, XrefReport,
};

use super::error::XrefPlanError;
use super::path::{format_saved_path, resolve};
use super::source::XrefLimits;

mod blocks;
mod mapping;

/// What the app asks the planner for.
#[derive(Clone, Debug, PartialEq)]
pub struct XrefPlanRequest {
    /// The existing root on reload; `None` for a new attach.
    pub block: Option<BlockId>,
    pub name: String,
    /// The source as chosen: absolute, or resolvable against `host_dir`.
    pub source_path: String,
    pub path_type: XrefPathType,
    pub kind: XrefKind,
    /// The host drawing's folder, for relative paths.
    pub host_dir: Option<String>,
    /// The host drawing's own file, to refuse attaching itself.
    pub host_path: Option<String>,
}

/// Plan a reference of `source` in `host`. Nothing is changed in either
/// drawing; the caller applies the plan through commands.
pub fn plan_xref(
    host: &Document,
    source: &Document,
    request: &XrefPlanRequest,
    limits: &XrefLimits,
    cancelled: &AtomicBool,
) -> Result<XrefPlan, XrefPlanError> {
    source
        .validate()
        .map_err(|e| XrefPlanError::SourceInvalid(e.to_string()))?;
    let name = request.name.trim().to_owned();
    if let Some(why) = block_name_problem(&name) {
        return Err(XrefPlanError::Message(why.to_owned()));
    }
    let existing = request.block.and_then(|b| host.xref(b)).cloned();
    if let Some(taken) = host.block_named(&name) {
        if Some(taken.id) != request.block {
            return Err(XrefPlanError::NameTaken(name));
        }
    }
    if let Some(root) = request.block {
        if !host.is_xref_block(root) {
            return Err(XrefPlanError::NotAnXref(root));
        }
    }
    if let Some(existing) = &existing {
        if let Some(root) = host.block(existing.block) {
            if !root.name.eq_ignore_ascii_case(&name) {
                return Err(XrefPlanError::Message(
                    "renaming an external reference is not built yet".to_owned(),
                ));
            }
        }
    }
    let host_dir = request.host_dir.as_deref().map(PathBuf::from);
    if let Some(host_path) = &request.host_path {
        if same_file(&request.source_path, host_path, host_dir.as_deref()) {
            return Err(XrefPlanError::SelfReference);
        }
    }

    let next_block = host.next_block_id();
    let root_id = request.block.unwrap_or(BlockId(next_block));
    let mut planner = Planner {
        host,
        source,
        name,
        kind: request.kind,
        existing,
        layers: HashMap::new(),
        linetypes: HashMap::new(),
        styles: HashMap::new(),
        added_layers: Vec::new(),
        changed_layers: Vec::new(),
        linetypes_plan: None,
        style_table: host.text_styles().to_vec(),
        next_style: host.next_text_style_id(),
        next_layer: host.next_ids().1,
        next_block: if request.block.is_some() {
            next_block
        } else {
            next_block.saturating_add(1)
        },
        blocks: HashMap::new(),
        block_ids: HashMap::new(),
        visiting: HashSet::new(),
        skipped: BTreeMap::new(),
        notes: Vec::new(),
        nested: 0,
        limits: *limits,
        cancelled,
        steps: 0,
        cleared_associations: false,
        dynamic_states: false,
    };
    planner.map_linetypes()?;
    planner.map_layers()?;
    planner.map_styles()?;

    if planner.cancelled.load(Ordering::Relaxed) {
        return Err(XrefPlanError::Cancelled);
    }
    let mut order = Vec::new();
    planner.collect_model(&mut order)?;
    planner.build_locals(&order)?;
    let root_entities = planner.root_entities()?;
    let layers = planner.bindings();
    let mut report = planner.report(root_entities.len() as u64);
    let mut notes = std::mem::take(&mut planner.notes);
    let source_file = resolve(request.source_path.trim(), host_dir.as_deref())
        .unwrap_or_else(|_| PathBuf::from(request.source_path.trim()));
    let (saved, path_type, path_note) =
        format_saved_path(&source_file, host_dir.as_deref(), request.path_type);
    if let Some(note) = path_note {
        notes.push(note);
    }
    let root = Arc::new(BlockDef {
        id: root_id,
        name: planner.name.clone(),
        base: source.insert_base(),
        description: source.properties().subject.clone(),
        unit: source.units().insert,
        entities: root_entities,
        dynamic: None,
    });
    let added_layers = std::mem::take(&mut planner.added_layers);
    let changed_layers = std::mem::take(&mut planner.changed_layers);
    let text_styles = planner.style_plan();
    let linetypes = planner.linetypes_plan.take();
    let kind = planner.kind;
    report.notes.extend(notes);
    // A reload keeps the reference's clip boundary: the boundary lives on the
    // host, in block coordinates the source's units do not change.
    let clip = planner.existing.as_ref().and_then(|old| old.clip.clone());
    let xref = XrefDef {
        block: root_id,
        path: saved,
        path_type,
        kind,
        unloaded: false,
        stamp: None,
        layers,
        blocks: planner.locals(),
        clip,
        report: report.capped(),
    };
    Ok(XrefPlan {
        xref,
        root,
        added_layers,
        changed_layers,
        text_styles,
        linetypes,
        unit_factor: InsertUnits::ratio(source.units().insert, host.units().insert),
    })
}

fn same_file(source: &str, host: &str, dir: Option<&Path>) -> bool {
    let (Ok(a), Ok(b)) = (resolve(source, dir), resolve(host, None)) else {
        return false;
    };
    let a = std::fs::canonicalize(&a).unwrap_or(a);
    let b = std::fs::canonicalize(&b).unwrap_or(b);
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

struct Planner<'a> {
    host: &'a Document,
    source: &'a Document,
    name: String,
    kind: XrefKind,
    existing: Option<XrefDef>,
    layers: HashMap<LayerId, LayerId>,
    linetypes: HashMap<LinetypeId, LinetypeId>,
    styles: HashMap<TextStyleId, TextStyleId>,
    added_layers: Vec<Layer>,
    changed_layers: Vec<Layer>,
    linetypes_plan: Option<LinetypeTable>,
    style_table: Vec<TextStyle>,
    next_style: u64,
    next_layer: u64,
    next_block: u64,
    blocks: HashMap<BlockId, Arc<BlockDef>>,
    block_ids: HashMap<BlockId, BlockId>,
    visiting: HashSet<BlockId>,
    skipped: BTreeMap<String, u64>,
    notes: Vec<String>,
    nested: u64,
    limits: XrefLimits,
    cancelled: &'a AtomicBool,
    steps: u64,
    cleared_associations: bool,
    dynamic_states: bool,
}

impl Planner<'_> {
    fn check(&mut self) -> Result<(), XrefPlanError> {
        self.steps += 1;
        if self.steps.is_multiple_of(1024) && self.cancelled.load(Ordering::Relaxed) {
            return Err(XrefPlanError::Cancelled);
        }
        Ok(())
    }

    fn skip(&mut self, what: &str) {
        *self.skipped.entry(what.to_owned()).or_insert(0) += 1;
    }

    fn bindings(&self) -> Vec<XrefLayer> {
        let mut out: Vec<XrefLayer> = self
            .layers
            .iter()
            .filter(|(source, host)| **source != LayerId::DEFAULT && **host != LayerId::DEFAULT)
            .filter_map(|(source, host)| {
                self.source.layer(*source).map(|l| XrefLayer {
                    source: l.name.clone(),
                    layer: *host,
                })
            })
            .collect();
        out.sort_by_key(|a| a.source.to_lowercase());
        out
    }

    fn locals(&self) -> Vec<Arc<BlockDef>> {
        let mut out: Vec<Arc<BlockDef>> = self.blocks.values().cloned().collect();
        out.sort_by_key(|b| b.id.0);
        out
    }

    fn style_plan(&self) -> Option<TextStylePlan> {
        if self.style_table.len() == self.host.text_styles().len() {
            return None;
        }
        Some(TextStylePlan {
            styles: self.style_table.clone(),
            current: self.host.current_text_style(),
            next_id: self.next_style,
        })
    }

    fn report(&self, model_entities: u64) -> XrefReport {
        let mut report = XrefReport {
            model_entities,
            local_blocks: self.blocks.len() as u64,
            skipped: self.skipped.clone(),
            notes: Vec::new(),
        };
        if self.nested > 0 {
            report.notes.push(format!(
                "Skipped {} nested external reference(s); nested xrefs are not loaded yet.",
                self.nested
            ));
        }
        if self.cleared_associations {
            report.notes.push(
                "Dimension ties, leader annotations and hatch sources were cleared.".to_owned(),
            );
        }
        if self.dynamic_states {
            report.notes.push(
                "Dynamic block values were left at the source's authored objects.".to_owned(),
            );
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use opendraft_core::{
        Attributes, BlockDef, BlockEntity, BlockId, BlockObjectId, Color, Entity, Insert, Layer,
        MText, Point, TextStyle, XrefLayer, XrefProblem, XrefReport, XrefStamp,
    };

    use super::*;

    fn paths() -> (&'static str, &'static str, &'static str) {
        if cfg!(windows) {
            ("C:\\work\\base.dwg", "C:\\work\\sheets", "..\\base.dwg")
        } else {
            ("/work/base.dwg", "/work/sheets", "../base.dwg")
        }
    }

    fn source() -> Document {
        let mut source = Document::default();
        let walls = source.allocate_layer_id();
        let mut layer = Layer::new(walls, "Walls");
        layer.color = Color::Rgb(10, 20, 30);
        source.push_layer(layer);
        let notes = source.allocate_text_style_id();
        source.push_text_style(TextStyle::new(notes, "Notes"));

        let chair_id = source.allocate_block_id();
        let chair = Arc::new(BlockDef {
            id: chair_id,
            name: "Chair".to_owned(),
            base: Point::default(),
            description: String::new(),
            unit: Default::default(),
            entities: vec![BlockEntity::new(
                BlockObjectId(0),
                Entity::Line {
                    start: Point::new(0.0, 0.0),
                    end: Point::new(1.0, 0.0),
                },
                Attributes::on_layer(LayerId::DEFAULT),
            )],
            dynamic: None,
        });
        source.push_block(chair.clone());
        source.add_with(
            Entity::Circle {
                center: Point::new(2.0, 2.0),
                radius: 1.0,
            },
            Attributes::on_layer(walls),
        );
        source.add_with(
            Entity::MText(Box::new(MText::new(Point::default(), 2.5, "hello", notes))),
            Attributes::on_layer(walls),
        );
        source.add(Entity::Insert(Box::new(Insert::new(
            chair,
            Point::new(5.0, 0.0),
        ))));
        source.validate().unwrap();
        source
    }

    fn request(name: &str) -> XrefPlanRequest {
        let (source_path, host_dir, _) = paths();
        XrefPlanRequest {
            block: None,
            name: name.to_owned(),
            source_path: source_path.to_owned(),
            path_type: XrefPathType::Relative,
            kind: XrefKind::Attach,
            host_dir: Some(host_dir.to_owned()),
            host_path: None,
        }
    }

    fn plan(host: &Document, source: &Document, request: &XrefPlanRequest) -> XrefPlan {
        plan_xref(
            host,
            source,
            request,
            &XrefLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn a_source_becomes_layers_styles_locals_and_a_root() {
        let host = Document::default();
        let plan = plan(&host, &source(), &request("base"));

        assert_eq!(plan.added_layers.len(), 1);
        assert_eq!(plan.added_layers[0].name, "base|Walls");
        assert_eq!(plan.added_layers[0].color, Color::Rgb(10, 20, 30));
        assert_eq!(plan.xref.layers.len(), 1);
        assert_eq!(plan.xref.layers[0].source, "Walls");
        assert_eq!(plan.xref.layers[0].layer, plan.added_layers[0].id);

        let styles = plan.text_styles.as_ref().expect("a style was added");
        assert!(styles.styles.iter().any(|s| s.name == "base|Notes"));
        assert_eq!(styles.current, TextStyleId::STANDARD);

        assert_eq!(plan.xref.blocks.len(), 1);
        assert_eq!(plan.xref.blocks[0].name, "Chair");
        assert_eq!(plan.root.entities.len(), 3);
        assert!(
            plan.root
                .entities
                .iter()
                .any(|e| matches!(e.entity, Entity::Insert(_))),
            "the insert is part of the snapshot"
        );
        assert_eq!(plan.xref.report.model_entities, 3);

        let (_, _, wanted) = paths();
        assert_eq!(plan.xref.path, wanted);
        assert_eq!(plan.xref.path_type, XrefPathType::Relative);
        assert_eq!(plan.xref.kind, XrefKind::Attach);
        assert!(!plan.xref.unloaded);
        assert_eq!(host.layers().len(), 1, "the host is untouched");
        assert_eq!(plan.unit_factor, 1.0);
    }

    #[test]
    fn the_same_source_plans_the_same_way_twice() {
        let host = Document::default();
        let source = source();
        assert_eq!(
            plan(&host, &source, &request("base")),
            plan(&host, &source, &request("base"))
        );
    }

    #[test]
    fn a_nested_xref_is_skipped_and_reported() {
        let mut source = Document::default();
        let walls = source.allocate_layer_id();
        source.push_layer(Layer::new(walls, "inner|Walls"));
        let root_id = source.allocate_block_id();
        let root = Arc::new(BlockDef {
            id: root_id,
            name: "inner".to_owned(),
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
        source.push_block(root.clone());
        source.set_xrefs(vec![XrefDef {
            block: root_id,
            path: "inner.dwg".to_owned(),
            path_type: XrefPathType::Relative,
            kind: XrefKind::Attach,
            unloaded: false,
            stamp: Some(XrefStamp {
                length: 1,
                modified: 1,
            }),
            layers: vec![XrefLayer {
                source: "Walls".to_owned(),
                layer: walls,
            }],
            blocks: Vec::new(),
            clip: None,
            report: XrefReport::default(),
        }]);
        source.add(Entity::Insert(Box::new(Insert::new_xref(
            root,
            Point::default(),
        ))));
        source.validate().unwrap();

        let plan = plan(&Document::default(), &source, &request("outer"));
        assert!(plan.root.entities.is_empty());
        assert!(plan
            .xref
            .report
            .notes
            .iter()
            .any(|n| n.contains("nested external reference")));
    }

    fn host_with_reference() -> (Document, LayerId, BlockId) {
        let mut host = Document::default();
        let layer = host.allocate_layer_id();
        let mut record = Layer::new(layer, "base|Walls");
        record.color = Color::Rgb(1, 1, 1);
        host.push_layer(record);
        let root_id = host.allocate_block_id();
        host.push_block(Arc::new(BlockDef {
            id: root_id,
            name: "base".to_owned(),
            base: Point::default(),
            description: String::new(),
            unit: Default::default(),
            entities: Vec::new(),
            dynamic: None,
        }));
        host.set_xrefs(vec![XrefDef {
            block: root_id,
            path: "base.dwg".to_owned(),
            path_type: XrefPathType::Relative,
            kind: XrefKind::Attach,
            unloaded: false,
            stamp: None,
            layers: vec![XrefLayer {
                source: "Walls".to_owned(),
                layer,
            }],
            blocks: Vec::new(),
            clip: None,
            report: XrefReport::default(),
        }]);
        (host, layer, root_id)
    }

    #[test]
    fn reload_reuses_bound_layers_and_visretain_decides_properties() {
        let (mut host, layer, root) = host_with_reference();
        let source = source();
        let mut reload = request("base");
        reload.block = Some(root);

        let kept = plan(&host, &source, &reload);
        assert!(kept.added_layers.is_empty(), "the bound layer is reused");
        assert!(
            kept.changed_layers.is_empty(),
            "VISRETAIN keeps host values"
        );
        assert_eq!(kept.xref.layers[0].layer, layer);

        host.set_visretain(false);
        let reset = plan(&host, &source, &reload);
        assert!(reset.added_layers.is_empty());
        assert_eq!(reset.changed_layers.len(), 1);
        assert_eq!(reset.changed_layers[0].color, Color::Rgb(10, 20, 30));
        assert_eq!(reset.changed_layers[0].id, layer);
    }

    #[test]
    fn names_that_are_taken_and_self_references_are_refused() {
        let (host, _, _) = host_with_reference();
        let error = plan_xref(
            &host,
            &source(),
            &request("base"),
            &XrefLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(matches!(error, XrefPlanError::NameTaken(_)));

        let mut self_ref = request("other");
        self_ref.host_path = Some(self_ref.source_path.clone());
        let error = plan_xref(
            &host,
            &source(),
            &self_ref,
            &XrefLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error, XrefPlanError::SelfReference);
    }

    #[test]
    fn a_taken_unbound_layer_refuses_the_plan() {
        let mut host = Document::default();
        let id = host.allocate_layer_id();
        host.push_layer(Layer::new(id, "base|Walls"));
        let error = plan_xref(
            &host,
            &source(),
            &request("base"),
            &XrefLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            XrefPlanError::Problem(XrefProblem::LayerBoundElsewhere(_))
        ));
    }

    #[test]
    fn a_cancelled_plan_changes_nothing() {
        let error = plan_xref(
            &Document::default(),
            &source(),
            &request("base"),
            &XrefLimits::default(),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(error, XrefPlanError::Cancelled);
    }
}
