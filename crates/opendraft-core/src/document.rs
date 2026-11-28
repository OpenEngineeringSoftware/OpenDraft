//! The drawing: objects in draw order, and the layers they sit on.

use std::collections::HashMap;
use std::sync::Arc;

use crate::block::BlockDef;
use crate::dimension::DimStyle;
use crate::drawing_properties::DrawingProperties;
use crate::layer::{Attr, Attributes, Color, Layer, LayerId, Lineweight};
use crate::layout::{Layout, LayoutId};
use crate::linetype::{LinetypeId, LinetypeTable};
use crate::mleader::{MLeaderStyle, MLeaderStyleId};
use crate::text_style::{TextStyle, TextStyleId};
use crate::units::Units;
use crate::{Entity, Point, PointStyle};

/// Permanent identity of an object. Ids are never reused within a drawing,
/// so selections, undo records and (later) files and plugins can hold them
/// safely while the list changes around them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u64);

/// What an id counter becomes once `id` is in use. It saturates, so an id of
/// `u64::MAX` (a damaged file) cannot wrap a counter to zero or panic a debug
/// build; the counter is then not ahead of the id, and `Document::validate`
/// refuses the drawing.
pub(crate) fn after(id: u64) -> u64 {
    id.saturating_add(1)
}

/// The drawing: objects in draw order and the layers they sit on.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    objects: Vec<Object>,
    next_id: u64,
    /// Position of each object in `objects`, rebuilt when the list changes
    /// shape so lookups by id stay constant time.
    positions: HashMap<EntityId, usize>,
    /// Layers in display order. The default layer is always present.
    layers: Vec<Layer>,
    layer_positions: HashMap<LayerId, usize>,
    next_layer_id: u64,
    /// Where new objects go.
    current_layer: LayerId,
    /// The colour, linetype and lineweight new objects get (`CECOLOR`,
    /// `CELTYPE`, `CELWEIGHT`); ByLayer unless set.
    new_color: Attr<Color>,
    new_linetype: Attr<LinetypeId>,
    new_lineweight: Attr<Lineweight>,
    /// How lengths and angles are shown and typed.
    units: Units,
    /// Title, author and the rest of the drawing's description.
    properties: DrawingProperties,
    /// The style new dimensions are made with.
    dim_style: DimStyle,
    /// Every named dimension style, `Standard` first. The current style
    /// is in it under its own name.
    dim_styles: Vec<DimStyle>,
    /// Sheets that show the drawing through viewports, in tab order.
    layouts: Vec<Layout>,
    /// How the Model tab plots (its own page setup; a layout has its own).
    model_plot: crate::PlotSettings,
    next_layout_id: u64,
    next_viewport_id: u64,
    /// The space being edited: new objects go there, and the object queries
    /// (`visible_objects`, `selectable_objects`, `bounds`) look only there.
    /// Not part of the drawing's history or file.
    active_layout: Option<LayoutId>,
    /// Layers frozen in the viewport being worked through (VPLAYER): hidden
    /// from the object queries like a globally frozen layer.
    active_frozen: Vec<LayerId>,
    /// Objects hidden by isolation (ISOLATEOBJECTS, HIDEOBJECTS), sorted: not
    /// drawn, picked, snapped to or plotted, but still in the drawing. Not
    /// part of the drawing's history or file.
    hidden_objects: Vec<EntityId>,
    /// The layout viewport being worked through, if any. Its layout is not
    /// `active_layout` (which is none there, as new objects go to model
    /// space). Not part of the drawing's history or file.
    active_viewport: Option<(LayoutId, crate::layout::ViewportId)>,
    /// How far the view being worked in turns the drawing on the screen
    /// (radians counter-clockwise), for commands that work along the screen.
    /// Not part of the drawing's history or file.
    view_twist: f64,
    /// The Model tab's custom coordinate system (each layout viewport keeps
    /// its own; see `opendraft_core::ccs`).
    model_ccs: crate::ccs::Ccs,
    /// Coordinate systems saved by name, shared by every space.
    named_ccs: Vec<crate::ccs::NamedCcs>,
    /// The systems each space had before, latest last, for the Previous
    /// option. Kept by the undoable action that changes a system; not saved.
    ccs_previous: Vec<(crate::ccs::CcsSpace, crate::ccs::Ccs)>,
    /// Text styles in table order; `Standard` is always first.
    text_styles: Vec<TextStyle>,
    next_text_style_id: u64,
    /// The style new text is made with.
    current_text_style: TextStyleId,
    /// Multileader styles in table order; `Standard` is always first.
    mleader_styles: Vec<MLeaderStyle>,
    next_mleader_style_id: u64,
    /// The style new multileaders are made with.
    current_mleader_style: MLeaderStyleId,
    /// Block definitions, shared with the inserts that place them.
    blocks: Vec<Arc<BlockDef>>,
    next_block_id: u64,
    /// External references, keyed by root block id. One record per root.
    xrefs: Vec<crate::XrefDef>,
    /// The drawing's `INSBASE`: the point an insert puts at its insertion
    /// point. Zero unless a file or BASE sets it.
    insert_base: Point,
    /// `VISRETAIN`: keep host xref-layer properties on reload (true, the
    /// AutoCAD initial value) or take the source's again (false).
    visretain: bool,
    /// `XCLIPFRAME`: 0 hides clip frames, 1 draws them, 2 draws and plots them.
    xclip_frame: u8,
    /// Only ever `Some` in a Block Editor's drawing: the dynamic schema being
    /// authored. It is not drawing data: no file holds it, `validate` does not
    /// look at it, `clone_tables` leaves it behind, and BSAVE turns it into
    /// the block's schema. It lives here so that editing it is undone with the
    /// editor's other edits.
    block_editor_dynamic: Option<crate::block::DynamicBlock>,
    next_attribute_definition_id: u64,
    /// The linetype table (`CONTINUOUS`, `BYLAYER` and `BYBLOCK` always exist).
    linetypes: LinetypeTable,
    /// The drawing-wide multiplier of every linetype pattern (`LTSCALE`).
    linetype_scale: f64,
    /// How point objects are drawn (`PDMODE`, `PDSIZE`).
    point_style: PointStyle,
    /// What FILLET, CHAMFER, OFFSET and LENGTHEN remember (`FILLETRAD`...).
    modify_settings: crate::ModifySettings,
    /// The associative arrays: what is needed to lay their items out again.
    arrays: Vec<crate::ArrayRecord>,
    /// The tangents and curve frames PEDIT keeps beside polylines.
    polyline_curves: Vec<crate::PolylineCurve>,
    /// Where the objects are, for the queries that look at a part of the
    /// drawing. Derived from `objects`: it takes no part in equality.
    spatial: spatial::SpatialIndex,
}

mod arrays;
mod attributes;
mod bind;
mod blocks;
mod ccs;
mod defaults;
mod layer_table;
mod layouts;
mod leaders;
mod linetypes;
pub use linetypes::LinetypeUse;
#[cfg(test)]
mod bind_tests;
#[cfg(test)]
mod clip_tests;
mod mleader_styles;
mod modify_settings;
mod object;
mod object_list;
mod point_style;
mod polyline_curves;
mod queries;
mod settings;
mod spatial;
#[cfg(test)]
mod spatial_edit_tests;
#[cfg(test)]
mod spatial_probe;
mod spatial_query;
#[cfg(test)]
mod spatial_tests;
mod text_styles;
mod transplant;
#[cfg(test)]
mod xref_tests;
mod xref_validate;
mod xrefs;

pub use object::Object;
pub use spatial::SpatialStats;
pub use transplant::{
    Absorbed, BlockClash, BlockNameClash, BringError, Brought, LibraryPlacement, Placed, Placement,
    TransferWarning,
};

impl Document {
    // ---- objects ----

    /// Objects in draw order.
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.positions.contains_key(&id)
    }

    pub fn object(&self, id: EntityId) -> Option<&Object> {
        let &i = self.positions.get(&id)?;
        Some(&self.objects[i])
    }

    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.object(id).map(|o| &o.entity)
    }

    /// Position of an object in draw order.
    pub fn position(&self, id: EntityId) -> Option<usize> {
        self.positions.get(&id).copied()
    }

    /// An id no object has had or will have.
    pub fn allocate_id(&mut self) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id = after(self.next_id);
        id
    }

    /// A drawing holding `entities` in order, with ids 0, 1, 2... on the
    /// default layer.
    pub fn from_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        let mut doc = Self::default();
        for entity in entities {
            doc.add(entity);
        }
        doc
    }

    /// Make sure ids already used (and since deleted) are never handed out
    /// again: a loaded drawing keeps its counters, not just the largest id
    /// still present. Only ever raises them.
    pub fn reserve_ids(&mut self, next_object: u64, next_layer: u64) {
        self.next_id = self.next_id.max(next_object);
        self.next_layer_id = self.next_layer_id.max(next_layer);
    }

    /// The next object and layer ids that would be handed out.
    pub fn next_ids(&self) -> (u64, u64) {
        (self.next_id, self.next_layer_id)
    }

    /// Append a new entity under a fresh id, on the current layer.
    pub fn add(&mut self, entity: Entity) -> EntityId {
        let attrs = self.new_object_attributes();
        self.add_with(entity, attrs)
    }

    pub fn add_with(&mut self, entity: Entity, attrs: Attributes) -> EntityId {
        let id = self.allocate_id();
        let layout = self.active_layout;
        self.push(Object::new(id, entity, attrs, layout));
        id
    }

    /// Append a new object under an id the caller chose, on the active
    /// layout, raising the counter past it. The caller keeps ids unique
    /// (`validate` refuses a repeat). The Block Editor uses it so an object
    /// keeps the id its block gave it.
    pub fn add_with_id(&mut self, id: EntityId, entity: Entity, attrs: Attributes) {
        let layout = self.active_layout;
        self.push(Object::new(id, entity, attrs, layout));
    }

    /// Append an object that already has an id (redo, undo of an erase).
    pub fn push(&mut self, object: Object) {
        self.next_id = self.next_id.max(after(object.id.0));
        self.note_attribute_id(&object.entity);
        self.positions.insert(object.id, self.objects.len());
        self.objects.push(object);
        if let Some(added) = self.objects.last() {
            self.spatial.added(&[added], &self.objects);
        }
    }

    /// Append objects that already have ids (redo, copies), updating the
    /// spatial index once rather than for each. See [`Document::push`].
    pub fn push_many(&mut self, objects: Vec<Object>) {
        let first = self.objects.len();
        for object in objects {
            self.next_id = self.next_id.max(after(object.id.0));
            self.note_attribute_id(&object.entity);
            self.positions.insert(object.id, self.objects.len());
            self.objects.push(object);
        }
        let added: Vec<&Object> = self.objects[first..].iter().collect();
        self.spatial.added(&added, &self.objects);
    }

    /// Swap the geometry of an object, keeping its id and position. Returns
    /// the entity it held, or `None` (changing nothing) for an unknown id.
    pub fn replace(&mut self, id: EntityId, entity: Entity) -> Option<Entity> {
        self.replace_many(vec![(id, entity)]).pop().flatten()
    }

    /// Swap the geometry of several objects, each keeping its id and
    /// position. Returns, for each pair in order, the entity that object held,
    /// or `None` (changing nothing) for an unknown id. The spatial index is
    /// updated entry by entry for a few objects and rebuilt once for many.
    pub fn replace_many(&mut self, changes: Vec<(EntityId, Entity)>) -> Vec<Option<Entity>> {
        let mut seen = std::collections::HashSet::new();
        let mut touched: Vec<(EntityId, Option<LayoutId>)> = Vec::new();
        for (id, _) in &changes {
            if let Some(&i) = self.positions.get(id) {
                if seen.insert(*id) {
                    touched.push((*id, self.objects[i].layout));
                }
            }
        }
        let rebuild = self.spatial.begin_replace(&touched);
        let old = changes
            .into_iter()
            .map(|(id, entity)| {
                let &i = self.positions.get(&id)?;
                let object = &mut self.objects[i];
                object.forget_bounds();
                Some(std::mem::replace(&mut object.entity, entity))
            })
            .collect();
        let after: Vec<&Object> = touched
            .iter()
            .filter_map(|(id, _)| self.positions.get(id).map(|&i| &self.objects[i]))
            .collect();
        self.spatial.finish_replace(&after, rebuild, &self.objects);
        old
    }

    /// Swap the attributes of an object. Returns the old ones, or `None`
    /// (changing nothing) for an unknown id.
    pub fn replace_attrs(&mut self, id: EntityId, attrs: Attributes) -> Option<Attributes> {
        let &i = self.positions.get(&id)?;
        Some(std::mem::replace(&mut self.objects[i].attrs, attrs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attr, Point};

    fn circle(r: f64) -> Entity {
        Entity::Circle {
            center: Point::new(0.0, 0.0),
            radius: r,
        }
    }

    #[test]
    fn objects_here_are_those_of_the_space_being_edited() {
        let mut doc = Document::default();
        assert!(!doc.has_objects_here());
        let layout = doc.new_layout("Sheet", 297.0, 210.0);
        let id = layout.id;
        doc.set_layouts(vec![layout]);
        doc.add(circle(1.0));
        assert!(doc.has_objects_here() && doc.len_here() == 1);
        doc.set_active_layout(Some(id));
        assert!(!doc.has_objects_here() && doc.len_here() == 0);
    }

    #[test]
    fn empty_document_has_no_bounds() {
        assert_eq!(Document::default().bounds(), None);
    }

    #[test]
    fn bounds_cover_visible_entities() {
        let mut doc = Document::default();
        doc.add(Entity::Line {
            start: Point::new(10.0, 20.0),
            end: Point::new(-5.0, 0.0),
        });
        doc.add(circle(30.0));
        let b = doc.bounds().unwrap();
        assert_eq!(b.min, Point::new(-30.0, -30.0));
        assert_eq!(b.max, Point::new(30.0, 30.0));

        // Hide the layer and nothing is left to frame.
        let mut layer = doc.layers()[0].clone();
        layer.on = false;
        doc.replace_layer(layer);
        assert_eq!(doc.bounds(), None);
    }

    #[test]
    fn ids_are_never_reused_after_removal() {
        let mut doc = Document::default();
        let a = doc.add(circle(1.0));
        let b = doc.add(circle(2.0));
        doc.remove(b);
        let c = doc.add(circle(3.0));
        assert_ne!(c, b);
        assert!(c > a);
    }

    #[test]
    fn lookups_follow_ids_through_removal_and_reinsert() {
        let mut doc = Document::default();
        let ids: Vec<_> = (1..=4).map(|r| doc.add(circle(r as f64))).collect();
        let (position, object) = doc.remove(ids[1]).unwrap();
        assert_eq!((position, doc.position(ids[2])), (1, Some(1)));
        assert!(!doc.contains(ids[1]));
        doc.insert(position, object);
        assert_eq!(doc.position(ids[1]), Some(1));
        assert_eq!(doc.position(ids[3]), Some(3));
        assert_eq!(doc.get(ids[2]), Some(&circle(3.0)));
    }

    /// A drawing of `n` circles (ids 0 to n - 1) and the same objects as a
    /// plain list: the reference the bulk calls are checked against.
    fn numbered(n: usize) -> (Document, Vec<Object>) {
        let mut doc = Document::default();
        for r in 0..n {
            doc.add(circle(r as f64 + 1.0));
        }
        let list = doc.objects().to_vec();
        (doc, list)
    }

    /// The lookups say where every object is.
    fn assert_consistent(doc: &Document) {
        for (i, object) in doc.objects().iter().enumerate() {
            assert_eq!(doc.position(object.id), Some(i));
        }
        assert_eq!(doc.len(), doc.objects().len());
    }

    #[test]
    fn removing_any_set_gives_what_a_plain_list_gives_and_putting_back_undoes_it() {
        let (base, objects) = numbered(7);
        for mask in 0u32..128 {
            let chosen = |i: &usize| mask >> i & 1 == 1;
            let ids: Vec<EntityId> = (0..7).filter(chosen).map(|i| objects[i].id).collect();
            // Asked for out of order, with a repeat and an id that is not there.
            let mut asked: Vec<EntityId> = ids.iter().rev().copied().collect();
            asked.extend(ids.first().copied());
            asked.push(EntityId(999));

            let mut doc = base.clone();
            let removed = doc.remove_many(&asked);
            let taken: Vec<(usize, Object)> = (0..7)
                .filter(chosen)
                .map(|i| (i, objects[i].clone()))
                .collect();
            let kept: Vec<Object> = (0..7)
                .filter(|i| !chosen(i))
                .map(|i| objects[i].clone())
                .collect();
            assert_eq!(removed, taken, "mask {mask:07b}");
            assert_eq!(doc.objects(), kept.as_slice(), "mask {mask:07b}");
            assert_consistent(&doc);
            assert!(removed.iter().all(|(_, o)| !doc.contains(o.id)));

            doc.insert_many(removed);
            assert_eq!(doc, base, "mask {mask:07b}");
        }
    }

    #[test]
    fn inserting_many_gives_what_a_plain_list_gives_even_past_the_end() {
        let (base, _) = numbered(4);
        let spare = |n: usize| -> Vec<Object> {
            (0..n)
                .map(|i| {
                    Object::new(
                        EntityId(100 + i as u64),
                        circle(50.0 + i as f64),
                        Attributes::on_layer(LayerId::DEFAULT),
                        None,
                    )
                })
                .collect()
        };
        let cases: [&[usize]; 8] = [
            &[0],
            &[4],
            &[9],
            &[0, 1, 2],
            &[1, 3, 5],
            &[0, 7, 8],
            &[5, 6, 7],
            &[3, 1, 2],
        ];
        for positions in cases {
            let items: Vec<(usize, Object)> = positions
                .iter()
                .copied()
                .zip(spare(positions.len()))
                .collect();
            // Ascending, each clamped to the end, as `Vec::insert` does.
            let mut reference = base.objects().to_vec();
            let mut sorted = items.clone();
            sorted.sort_by_key(|(position, _)| *position);
            for (position, object) in sorted {
                reference.insert(position.min(reference.len()), object);
            }
            let mut doc = base.clone();
            doc.insert_many(items);
            assert_eq!(doc.objects(), reference.as_slice(), "{positions:?}");
            assert_consistent(&doc);
        }
    }

    #[test]
    fn the_single_object_calls_agree_with_the_bulk_ones() {
        let (mut doc, objects) = numbered(5);
        let (position, object) = doc.remove(objects[2].id).unwrap();
        assert_eq!((position, &object), (2, &objects[2]));
        assert_eq!(doc.remove(objects[2].id), None, "already gone");
        assert_consistent(&doc);
        doc.insert(position, object);
        assert_eq!(doc.objects(), objects.as_slice());
        assert_consistent(&doc);
    }

    #[test]
    fn replace_each_swaps_chosen_objects_for_pieces_in_place() {
        let (mut doc, objects) = numbered(5);
        let attrs = objects[0].attrs.clone();
        // Objects 1 and 3 become two and none; the rest stay.
        let replaced = doc.replace_each(|o| {
            if o.id == objects[1].id {
                Some(vec![
                    (circle(10.0), attrs.clone()),
                    (circle(11.0), attrs.clone()),
                ])
            } else if o.id == objects[3].id {
                Some(Vec::new())
            } else {
                None
            }
        });
        assert_eq!(replaced, 2);
        let radii: Vec<f64> = doc
            .objects()
            .iter()
            .filter_map(|o| match o.entity {
                Entity::Circle { radius, .. } => Some(radius),
                _ => None,
            })
            .collect();
        assert_eq!(radii, [1.0, 10.0, 11.0, 3.0, 5.0]);
        assert_consistent(&doc);
        assert!(doc.contains(objects[0].id) && !doc.contains(objects[1].id));
        // Fresh ids: none of the pieces reuse an id the drawing has had.
        assert!(doc.objects()[1].id.0 >= 5 && doc.objects()[2].id.0 >= 5);
        assert_eq!(doc.replace_each(|_| None), 0);
    }

    #[test]
    fn layers_frozen_in_the_active_viewport_leave_the_queries_but_not_the_drawing() {
        let (mut doc, walls) = doc_with_layer("Walls");
        let kept = doc.add(circle(1.0));
        let hidden = doc.add_with(circle(2.0), Attributes::on_layer(walls));
        doc.set_active_frozen(vec![walls]);
        let ids = |d: &Document| d.visible_objects().map(|o| o.id).collect::<Vec<_>>();
        assert_eq!(ids(&doc), [kept]);
        assert_eq!(doc.selectable_objects().count(), 1);
        assert_eq!(doc.bounds().unwrap().max.x, 1.0);
        assert!(doc.contains(hidden), "still in the drawing");
        assert_eq!(
            doc.visible_in(None).count(),
            2,
            "other viewports still show it"
        );
        doc.set_active_frozen(Vec::new());
        assert_eq!(doc.visible_objects().count(), 2);
    }

    #[test]
    fn replace_keeps_id_and_position() {
        let mut doc = Document::default();
        let id = doc.add(circle(1.0));
        assert_eq!(doc.replace(id, circle(5.0)), Some(circle(1.0)));
        assert_eq!(doc.get(id), Some(&circle(5.0)));
        assert_eq!(doc.replace(EntityId(99), circle(1.0)), None);
    }

    fn doc_with_layer(name: &str) -> (Document, LayerId) {
        let mut doc = Document::default();
        let id = doc.allocate_layer_id();
        doc.push_layer(Layer::new(id, name));
        (doc, id)
    }

    #[test]
    fn a_new_drawing_has_layer_zero_current() {
        let doc = Document::default();
        assert_eq!(doc.layers().len(), 1);
        assert_eq!(doc.layers()[0].name, "0");
        assert_eq!(doc.current_layer(), LayerId::DEFAULT);
    }

    #[test]
    fn new_objects_go_on_the_current_layer() {
        let (mut doc, walls) = doc_with_layer("Walls");
        doc.set_current_layer(walls);
        let id = doc.add(circle(1.0));
        assert_eq!(doc.object(id).unwrap().attrs.layer, walls);
        doc.set_current_layer(LayerId(99)); // unknown: ignored
        assert_eq!(doc.current_layer(), walls);
    }

    #[test]
    fn attributes_follow_the_layer_unless_overridden() {
        let (mut doc, walls) = doc_with_layer("Walls");
        let red = Color::Rgb(255, 0, 0);
        let mut layer = doc.layer(walls).unwrap().clone();
        layer.color = red;
        layer.lineweight = Lineweight(50);
        doc.replace_layer(layer);

        let id = doc.add_with(circle(1.0), Attributes::on_layer(walls));
        let object = doc.object(id).unwrap().clone();
        assert_eq!(doc.color_of(&object), red);
        assert_eq!(doc.lineweight_of(&object), Lineweight(50));

        let mut attrs = object.attrs.clone();
        attrs.color = Attr::Set(Color::Rgb(0, 0, 255));
        doc.replace_attrs(id, attrs);
        let object = doc.object(id).unwrap().clone();
        assert_eq!(doc.color_of(&object), Color::Rgb(0, 0, 255));
        assert_eq!(doc.lineweight_of(&object), Lineweight(50), "still ByLayer");
    }

    #[test]
    fn hidden_and_locked_layers_limit_what_can_be_picked() {
        let (mut doc, id) = doc_with_layer("Walls");
        let object = doc.add_with(circle(1.0), Attributes::on_layer(id));
        let object = doc.object(object).unwrap().clone();
        assert!(doc.is_visible(&object) && doc.is_selectable(&object));

        let mut layer = doc.layer(id).unwrap().clone();
        layer.locked = true;
        doc.replace_layer(layer.clone());
        assert!(doc.is_visible(&object), "locked layers still show");
        assert!(!doc.is_selectable(&object));

        layer.locked = false;
        layer.frozen = true;
        doc.replace_layer(layer);
        assert!(!doc.is_visible(&object) && !doc.is_selectable(&object));
        assert_eq!(doc.visible_objects().count(), 0);
    }

    #[test]
    fn layer_zero_current_layer_and_layers_in_use_cannot_be_deleted() {
        let (mut doc, walls) = doc_with_layer("Walls");
        assert!(doc.layer_delete_blocker(LayerId::DEFAULT).is_some());
        assert_eq!(doc.layer_delete_blocker(walls), None);

        doc.set_current_layer(walls);
        assert!(doc.layer_delete_blocker(walls).is_some());
        doc.set_current_layer(LayerId::DEFAULT);

        doc.add_with(circle(1.0), Attributes::on_layer(walls));
        assert!(doc.layer_delete_blocker(walls).is_some());
        assert!(doc.remove_layer(LayerId::DEFAULT).is_none());
    }

    #[test]
    fn layer_names_are_unique_ignoring_case() {
        let (doc, walls) = doc_with_layer("Walls");
        assert!(doc.layer_name_taken("walls", None));
        assert!(!doc.layer_name_taken("walls", Some(walls)), "its own name");
        assert_eq!(doc.unused_layer_name(), "Layer1");
    }

    #[test]
    fn layer_ids_are_never_reused() {
        let (mut doc, walls) = doc_with_layer("Walls");
        doc.remove_layer(walls);
        assert_ne!(doc.allocate_layer_id(), walls);
    }
}
