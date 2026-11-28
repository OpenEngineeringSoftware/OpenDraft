//! Blocks from DXF: the named definitions in the BLOCKS section and the
//! `INSERT` entities that place them. Definitions are read when the BLOCKS
//! section begins (after the layer table, before any entity that inserts
//! them) and built by `block_import`. Anonymous blocks (`*U`, the stored
//! form of a modified dynamic block) become ordinary blocks, but only those an
//! INSERT places: the pictures of dimensions and hatches are never inserted
//! and stay out of the table. External references and the spaces' own blocks
//! are not definitions here.

use std::collections::{HashMap, HashSet};

use opendraft_core::block::AttributeReference;
use opendraft_core::{Entity, Point};

mod dynamic_import;

use self::dynamic_import::Plan;
use super::layouts::Spaces;
use super::polyline::Vertex;
use super::{decode, Importer, Record};
use crate::block_import::{self, ImportedBlocks, Item, PendingBlock, PendingInsert};

/// A block in the file that is an external reference's root: its path and
/// kind, kept for the reference record built after the blocks.
#[derive(Clone, Debug)]
pub(super) struct PendingXref {
    pub key: String,
    pub name: String,
    pub path: String,
    pub kind: opendraft_core::XrefKind,
    pub unit: opendraft_core::InsertUnits,
}

/// The importer's block state.
#[derive(Default)]
pub(super) struct BlockReading {
    /// The objects of the definition being read, while one is.
    collecting: Option<Vec<(Item<String>, opendraft_core::Attributes)>>,
    /// The DXF handle of each of those objects, in the same order.
    handles: Vec<Option<String>>,
    /// Whether each of them is marked invisible, in the same order. A
    /// visibility parameter's hidden states are written that way.
    hidden: Vec<bool>,
    /// What the file's dynamic blocks come to.
    plan: Plan,
    built: ImportedBlocks<String>,
    /// References to anonymous blocks, which were imported as ordinary blocks.
    anonymous_references: u64,
    /// External references in the file, by their block header.
    pub(super) xrefs: Vec<PendingXref>,
    /// Layers the file marks as dependent on a reference (flags 16).
    pub(super) dependent_layers: std::collections::HashSet<u64>,
    /// The block table's insert units, by lower-cased name.
    units: HashMap<String, opendraft_core::InsertUnits>,
    /// Inserts waiting for their attributes.
    pub(super) attributes: super::attribute::Reading,
}

impl Importer {
    /// A record of the TABLES section that is read: a layer, a linetype or
    /// a block record, each only in its own table.
    pub(super) fn table_record(&mut self, record: &Record, section: &str, table: &str) {
        match (record.name.as_str(), section, table) {
            ("LAYER", "TABLES", "LAYER") => self.layer_record(record),
            ("LTYPE", "TABLES", "LTYPE") => self.linetype_record(record),
            ("BLOCK_RECORD", "TABLES", "BLOCK_RECORD") => self.blocks.table_record(record),
            ("UCS", "TABLES", "UCS") => self.ucs_record(record),
            _ => {}
        }
    }
}

impl BlockReading {
    /// A BLOCK_RECORD of the block table: what units its block is in.
    pub(super) fn table_record(&mut self, record: &Record) {
        let units = record
            .int(70)
            .and_then(opendraft_core::InsertUnits::from_code);
        if let (Some(name), Some(units)) = (record.text(2), units) {
            self.units.insert(decode(name).to_lowercase(), units);
        }
    }

    pub(super) fn warnings(&self) -> Vec<String> {
        let mut out = self.built.warnings();
        out.extend(self.attributes.warnings());
        out.extend(self.plan.warnings());
        if self.anonymous_references > 0 {
            out.push(block_import::anonymous_references(
                self.anonymous_references,
            ));
        }
        if !self.xrefs.is_empty() {
            out.push(block_import::external_references(self.xrefs.len() as u64));
        }
        out
    }
}

/// The lower-cased names of the blocks any INSERT in the file places.
fn inserted_names(records: &[Record]) -> HashSet<String> {
    records
        .iter()
        .filter(|r| r.name == "INSERT")
        .map(|r| decode(r.text(2).unwrap_or("")).to_lowercase())
        .collect()
}

/// Whether a block is one of the spaces' own (`*Model_Space`, R12's
/// `$MODEL_SPACE`, a layout's sheet).
fn is_space(name: &str, spaces: &Spaces) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("*model_space")
        || lower.starts_with("*paper_space")
        || lower == "$model_space"
        || lower == "$paper_space"
        || spaces.is_sheet_block(name)
}

impl Importer {
    /// Reads the definitions in a BLOCKS section, `records` starting after
    /// its SECTION record, and builds them into the block table.
    pub(super) fn read_blocks(&mut self, records: &[Record], spaces: &Spaces) {
        let mut pending: Vec<(PendingBlock<String>, bool)> = Vec::new();
        let mut current: Option<(PendingBlock<String>, bool)> = None;
        let mut open: Option<(&Record, Vec<Vertex>)> = None;
        for record in records {
            match record.name.as_str() {
                "ENDSEC" => break,
                "BLOCK" => {
                    let name = decode(record.text(2).unwrap_or(""));
                    let flags = record.int(70).unwrap_or(0);
                    let key = name.to_lowercase();
                    current = None;
                    if is_space(&name, spaces) {
                        continue;
                    } else if flags & (4 | 8) != 0 {
                        self.blocks.xrefs.push(PendingXref {
                            key: key.clone(),
                            name: name.clone(),
                            path: decode(record.text(1).unwrap_or("")),
                            kind: if flags & 8 != 0 {
                                opendraft_core::XrefKind::Overlay
                            } else {
                                opendraft_core::XrefKind::Attach
                            },
                            unit: self.blocks.units.get(&key).copied().unwrap_or_default(),
                        });
                    } else {
                        let anonymous = flags & 1 != 0 || name.starts_with('*');
                        let unit = self.blocks.units.get(&key).copied().unwrap_or_default();
                        let block = PendingBlock {
                            key,
                            name,
                            base: Self::point(record, 10, 20).unwrap_or(Point::new(0.0, 0.0)),
                            description: decode(record.text(4).unwrap_or("")),
                            unit,
                            items: Vec::new(),
                            dynamic: None,
                        };
                        current = Some((block, anonymous));
                        self.blocks.collecting = Some(Vec::new());
                        self.blocks.handles.clear();
                        self.blocks.hidden.clear();
                    }
                }
                "ENDBLK" => {
                    self.drop_open_insert();
                    if let Some((mut block, anonymous)) = current.take() {
                        block.items = self.blocks.collecting.take().unwrap_or_default();
                        self.drop_hidden(&mut block);
                        block.dynamic = self.blocks.plan.schema(&block.key, &self.blocks.handles);
                        if block.dynamic.is_none() {
                            self.drop_hidden_all(&mut block);
                        }
                        pending.push((block, anonymous));
                    }
                }
                _ if current.is_some() => self.shape(record, &mut open),
                _ => {}
            }
        }
        self.blocks.collecting = None;
        let mut pending = self.keep_inserted(pending, records);
        let empty: Vec<PendingBlock<String>> = self
            .blocks
            .xrefs
            .iter()
            .map(|xref| PendingBlock {
                key: xref.key.clone(),
                name: xref.name.clone(),
                base: Point::new(0.0, 0.0),
                description: String::new(),
                unit: xref.unit,
                items: Vec::new(),
                dynamic: None,
            })
            .collect();
        pending.extend(empty);
        self.blocks.built = ImportedBlocks::build(&mut self.document, pending);
        self.finish_xrefs();
    }

    /// The external references the file named: an empty root (their content
    /// is not in this file) and the host layers bound to their source layers.
    fn finish_xrefs(&mut self) {
        if self.blocks.xrefs.is_empty() {
            return;
        }
        let mut records = Vec::new();
        for xref in &self.blocks.xrefs {
            let Some(root) = self.blocks.built.block(&xref.key) else {
                continue;
            };
            let prefix = format!("{}|", root.name);
            let mut layers: Vec<opendraft_core::XrefLayer> = self
                .document
                .layers()
                .iter()
                .filter(|layer| self.blocks.dependent_layers.contains(&layer.id.0))
                .filter_map(|layer| {
                    let source = layer.name.strip_prefix(&prefix)?;
                    (!source.is_empty()).then(|| opendraft_core::XrefLayer {
                        source: source.to_owned(),
                        layer: layer.id,
                    })
                })
                .collect();
            layers.sort_by_key(|l| l.source.to_lowercase());
            records.push(opendraft_core::XrefDef {
                block: root.id,
                path: xref.path.clone(),
                path_type: crate::xref::classify(&xref.path),
                kind: xref.kind,
                unloaded: true,
                stamp: None,
                layers,
                blocks: Vec::new(),
                report: opendraft_core::XrefReport::default(),
            });
        }
        self.document.set_xrefs(records);
    }

    /// Takes the objects marked invisible out of a block, except those a
    /// visibility parameter governs, which are the objects of its other
    /// states. (Objects marked invisible and governed by nothing stay hidden
    /// in AutoCAD, so they are left out and counted.)
    fn drop_hidden(&mut self, block: &mut PendingBlock<String>) {
        let keep: Vec<bool> = self
            .blocks
            .hidden
            .iter()
            .zip(&self.blocks.handles)
            .map(|(hidden, handle)| {
                !hidden
                    || handle
                        .as_deref()
                        .is_some_and(|h| self.blocks.plan.governs(&block.key, h))
            })
            .collect();
        self.retain_objects(block, &keep);
    }

    /// Takes every object marked invisible out of a block that did not come
    /// in as a dynamic block with a visibility parameter.
    fn drop_hidden_all(&mut self, block: &mut PendingBlock<String>) {
        let keep: Vec<bool> = self.blocks.hidden.iter().map(|h| !h).collect();
        self.retain_objects(block, &keep);
    }

    fn retain_objects(&mut self, block: &mut PendingBlock<String>, keep: &[bool]) {
        let dropped = keep.iter().filter(|k| !**k).count();
        if dropped == 0 {
            return;
        }
        self.invisible += dropped;
        self.imported = self.imported.saturating_sub(dropped as _);
        let mut at = 0;
        block.items.retain(|_| {
            at += 1;
            keep[at - 1]
        });
        let mut at = 0;
        self.blocks.handles.retain(|_| {
            at += 1;
            keep[at - 1]
        });
        let mut at = 0;
        self.blocks.hidden.retain(|_| {
            at += 1;
            keep[at - 1]
        });
    }

    /// The definitions to build: every named block, and the anonymous ones an
    /// INSERT places, under a name of their own. Counts the references.
    fn keep_inserted(
        &mut self,
        blocks: Vec<(PendingBlock<String>, bool)>,
        records: &[Record],
    ) -> Vec<PendingBlock<String>> {
        let inserted = inserted_names(records);
        let mut taken: HashSet<String> = blocks
            .iter()
            .filter(|(_, anonymous)| !anonymous)
            .map(|(b, _)| b.name.trim().to_lowercase())
            .collect();
        let mut kept = Vec::new();
        for (mut block, anonymous) in blocks {
            if anonymous {
                // A modified dynamic block's picture is not needed once its
                // references are rebuilt from the definition.
                if !inserted.contains(&block.key) || self.blocks.plan.instance(&block.key).is_some()
                {
                    continue;
                }
                block.name = block_import::anonymous_name(&block.name, &mut taken);
                let references = records
                    .iter()
                    .filter(|r| r.name == "INSERT")
                    .filter(|r| decode(r.text(2).unwrap_or("")).to_lowercase() == block.key)
                    .count();
                self.blocks.anonymous_references += references as u64;
            }
            kept.push(block);
        }
        kept
    }

    /// An INSERT entity. One with rows or columns (a MINSERT array) is
    /// skipped; the ATTRIB records that may follow it are skipped on their own.
    pub(super) fn insert(&mut self, record: &Record) {
        self.drop_open_insert();
        let columns = record.int(70).unwrap_or(1);
        let rows = record.int(71).unwrap_or(1);
        if columns > 1 || rows > 1 {
            self.skip("INSERT (array)");
            return;
        }
        if Self::point(record, 10, 20).is_none() {
            self.damaged += 1;
            return;
        }
        if record.int(66) == Some(1) {
            self.open_insert(record);
        } else {
            self.place_insert(record, Vec::new());
        }
    }

    /// Places an INSERT with the attributes that followed it (in the file's
    /// coordinates; `block_import` brings them into the block's).
    pub(super) fn place_insert(&mut self, record: &Record, attributes: Vec<AttributeReference>) {
        let Some(insertion) = Self::point(record, 10, 20) else {
            self.damaged += 1;
            return;
        };
        let key = decode(record.text(2).unwrap_or("")).to_lowercase();
        let is_xref = self.blocks.xrefs.iter().any(|xref| xref.key == key);
        let (block, dynamic) = match self.blocks.plan.instance(&key) {
            Some(m) => (m.definition.clone(), m.values.clone()),
            None => (key, Vec::new()),
        };
        let insert = PendingInsert {
            block,
            insertion,
            scale_x: record.number(41).unwrap_or(1.0),
            scale_y: record.number(42).unwrap_or(1.0),
            rotation: record.number(50).unwrap_or(0.0).to_radians(),
            attributes,
            dynamic,
            is_xref,
        };
        self.add_item(record, Item::Insert(insert));
    }

    /// Decodes the file's dynamic blocks, before any block is read.
    pub(super) fn scan_dynamic(&mut self, records: &[Record]) {
        self.blocks.plan = Plan::scan(records);
    }

    /// Whether a block definition is being read (its objects go to it).
    pub(super) fn reading_block(&self) -> bool {
        self.blocks.collecting.is_some()
    }

    /// Adds an object to the definition being read, or to the drawing.
    pub(super) fn add_item(&mut self, record: &Record, item: Item<String>) {
        // A hidden object (group 60 is 1, as a dynamic block's other
        // visibility states are) is left out, as the DWG reader does.
        let hidden = record.int(60) == Some(1);
        if hidden && self.blocks.collecting.is_none() {
            self.invisible += 1;
            return;
        }
        let attrs = self.attributes(record);
        if let Some(items) = self.blocks.collecting.as_mut() {
            items.push((item, attrs));
            self.blocks.handles.push(record.text(5).map(str::to_owned));
            self.blocks.hidden.push(hidden);
            self.imported += 1;
            return;
        }
        let entity: Option<Entity> = match item {
            Item::Entity(entity) => Some(entity),
            Item::Insert(insert) => self.blocks.built.insert(insert),
        };
        if let Some(entity) = entity {
            let id = self.document.add_with(entity, attrs);
            if let Some(handle) = record.text(5) {
                self.handles.insert(handle.to_uppercase(), id);
            }
            self.imported += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::{Attr, Color, LayerId};

    use super::super::read;
    use super::*;

    fn dxf(blocks: &[(i32, &str)], entities: &[(i32, &str)]) -> Vec<u8> {
        let mut pairs: Vec<(i32, &str)> = vec![(0, "SECTION"), (2, "BLOCKS")];
        pairs.extend_from_slice(blocks);
        pairs.extend([(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
        pairs.extend_from_slice(entities);
        pairs.extend([(0, "ENDSEC"), (0, "EOF")]);
        let mut text = String::new();
        for (code, value) in pairs {
            text.push_str(&format!("{code:>3}\r\n{value}\r\n"));
        }
        text.into_bytes()
    }

    const LINE: [(i32, &str); 6] = [
        (0, "LINE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
    ];

    fn block<'a>(name: &'a str, flags: &'a str, body: &[(i32, &'a str)]) -> Vec<(i32, &'a str)> {
        let mut out = vec![
            (0, "BLOCK"),
            (8, "0"),
            (2, name),
            (70, flags),
            (10, "1"),
            (20, "2"),
            (4, "A tick"),
        ];
        out.extend_from_slice(body);
        out.push((0, "ENDBLK"));
        out
    }

    #[test]
    fn a_block_and_its_reference_come_in() {
        let mut blocks = block("*Model_Space", "0", &[]);
        blocks.extend(block("Tick", "0", &LINE));
        let insert = [
            (0, "INSERT"),
            (8, "Marks"),
            (62, "3"),
            (2, "TICK"),
            (10, "30"),
            (20, "25"),
            (41, "2"),
            (42, "-1"),
            (50, "90"),
        ];
        let loaded = read(&dxf(&blocks, &insert)).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let doc = &loaded.document;
        doc.validate().unwrap();
        assert_eq!(doc.blocks().len(), 1);
        let tick = &doc.blocks()[0];
        assert_eq!(tick.name, "Tick");
        assert_eq!(
            (tick.base, tick.description.as_str()),
            (Point::new(1.0, 2.0), "A tick")
        );
        assert_eq!(tick.entities.len(), 1);
        assert_eq!(tick.entities[0].attrs.layer, LayerId::DEFAULT);
        assert_eq!(doc.len(), 1);
        let object = &doc.objects()[0];
        let Entity::Insert(i) = &object.entity else {
            panic!("not a block reference");
        };
        assert!(std::sync::Arc::ptr_eq(&i.block, tick));
        assert_eq!(i.insertion, Point::new(30.0, 25.0));
        assert_eq!((i.scale_x, i.scale_y), (2.0, -1.0));
        assert!((i.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(object.attrs.color, Attr::Set(Color::Rgb(0, 255, 0)));
        assert_ne!(object.attrs.layer, LayerId::DEFAULT);
    }

    #[test]
    fn nested_blocks_may_come_in_any_order() {
        let nested = [(0, "INSERT"), (8, "0"), (2, "Inner"), (10, "0"), (20, "0")];
        let mut blocks = block("Outer", "0", &nested);
        blocks.extend(block("Inner", "0", &LINE));
        let loaded = read(&dxf(&blocks, &[])).unwrap();
        let doc = &loaded.document;
        doc.validate().unwrap();
        let names: Vec<&str> = doc.blocks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Outer", "Inner"]);
        assert!(doc.blocks()[0].holds(doc.blocks()[1].id));
    }

    #[test]
    fn external_blocks_are_kept_as_unloaded_references() {
        let blocks = block("Site", "4", &[]);
        let refs = [
            (0, "INSERT"),
            (8, "0"),
            (2, "Site"),
            (10, "0"),
            (20, "0"),
            (0, "INSERT"),
            (8, "0"),
            (2, "Site"),
            (10, "0"),
            (20, "0"),
            (70, "2"),
        ];
        let loaded = read(&dxf(&blocks, &refs)).unwrap();
        let doc = &loaded.document;
        doc.validate().unwrap();
        assert_eq!(doc.xrefs().len(), 1, "the reference is kept");
        assert!(doc.block(doc.xrefs()[0].block).unwrap().entities.is_empty());
        assert_eq!(doc.objects().len(), 1);
        let Entity::Insert(insert) = &doc.objects()[0].entity else {
            panic!("an xref insert");
        };
        assert!(insert.is_xref);
        assert_eq!(
            loaded.warnings,
            [
                "Skipped 1 unsupported object: 1 INSERT (array).",
                &block_import::external_references(1),
            ]
        );
    }

    fn insert_of(name: &str) -> Vec<(i32, &str)> {
        vec![(0, "INSERT"), (8, "0"), (2, name), (10, "5"), (20, "6")]
    }

    #[test]
    fn a_referenced_anonymous_block_comes_in_as_an_ordinary_block() {
        let mut blocks = block("*U1", "1", &LINE);
        blocks.extend(block("*D0", "1", &LINE));
        let loaded = read(&dxf(&blocks, &insert_of("*U1"))).unwrap();
        let doc = &loaded.document;
        doc.validate().unwrap();
        // The dimension picture nothing places stays out of the table.
        assert_eq!(doc.blocks().len(), 1);
        assert_eq!(doc.blocks()[0].name, "Anonymous_U1");
        assert_eq!(doc.blocks()[0].entities.len(), 1);
        let Entity::Insert(i) = &doc.objects()[0].entity else {
            panic!("not a block reference");
        };
        assert_eq!(i.insertion, Point::new(5.0, 6.0));
        assert!(std::sync::Arc::ptr_eq(&i.block, &doc.blocks()[0]));
        assert_eq!(loaded.warnings, [block_import::anonymous_references(1)]);
    }

    #[test]
    fn a_named_block_may_place_an_anonymous_one_and_names_stay_unique() {
        let mut blocks = block("Outer", "0", &insert_of("*U7"));
        blocks.extend(block("*U7", "1", &LINE));
        blocks.extend(block("Anonymous_U7", "0", &LINE));
        let loaded = read(&dxf(&blocks, &insert_of("Outer"))).unwrap();
        let doc = &loaded.document;
        doc.validate().unwrap();
        let names: Vec<&str> = doc.blocks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Outer", "Anonymous_U7_2", "Anonymous_U7"]);
        assert!(doc.blocks()[0].holds(doc.blocks()[1].id));
        assert_eq!(loaded.warnings, [block_import::anonymous_references(1)]);
    }

    #[test]
    fn a_file_without_anonymous_references_has_no_note() {
        let loaded = read(&dxf(&block("*D0", "1", &LINE), &[])).unwrap();
        assert!(loaded.document.blocks().is_empty());
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn objects_marked_invisible_are_left_out_and_counted() {
        let hidden = [
            (0, "LINE"),
            (8, "0"),
            (60, "1"),
            (10, "0"),
            (20, "0"),
            (11, "2"),
            (21, "0"),
        ];
        let mut body = LINE.to_vec();
        body.extend(hidden);
        let mut blocks = block("*Model_Space", "0", &[]);
        blocks.extend(block("Switch", "0", &body));
        let mut entities = insert_of("Switch");
        entities.extend(hidden);
        let loaded = read(&dxf(&blocks, &entities)).unwrap();
        let doc = &loaded.document;
        doc.validate().unwrap();
        // The block keeps its visible line; the hidden line outside it is gone.
        assert_eq!(doc.blocks()[0].entities.len(), 1);
        assert_eq!(doc.len(), 1);
        assert_eq!(loaded.warnings, ["Skipped 2 objects marked invisible."]);
    }

    #[test]
    fn by_block_properties_are_kept() {
        let body = [
            (0, "LINE"),
            (8, "0"),
            (62, "0"),
            (6, "BYBLOCK"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
        ];
        let loaded = read(&dxf(&block("B", "0", &body), &[])).unwrap();
        let e = &loaded.document.blocks()[0].entities[0];
        assert_eq!(
            (&e.attrs.color, &e.attrs.linetype),
            (&Attr::ByBlock, &Attr::ByBlock)
        );
        assert!(loaded.warnings.is_empty());
    }
}
