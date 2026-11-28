//! Binding an external reference: turning its snapshot and dependent symbols
//! into ordinary host records, in one plan the app applies as one undo step.

mod finish;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::block::{BlockDef, BlockId};
use crate::xref_bind::{BindError, BindPlan, BindType, XbindSymbol};
use crate::{Document, Entity, LayerId, LinetypeId, TextStyleId};

impl Document {
    /// Make `roots` ordinary blocks and layers/blocks/styles/linetypes.
    pub fn plan_bind(&self, roots: &[BlockId], kind: BindType) -> Result<BindPlan, BindError> {
        let mut binder = Binder::new(self, kind);
        for &root in roots {
            let Some(record) = self.xref(root) else {
                return Err(BindError::NotAnXref(root));
            };
            binder.bind_reference(root, record)?;
        }
        Ok(binder.finish())
    }

    /// Make selected dependent symbols ordinary host records, keeping the
    /// references themselves.
    pub fn plan_xbind(
        &self,
        symbols: &[XbindSymbol],
        kind: BindType,
    ) -> Result<BindPlan, BindError> {
        let mut binder = Binder::new(self, kind);
        for &symbol in symbols {
            binder.bind_symbol(symbol)?;
        }
        Ok(binder.finish())
    }
}

struct Binder<'a> {
    doc: &'a Document,
    kind: BindType,
    /// Local blocks moving into the host table, with their new names.
    moving: HashMap<BlockId, String>,
    /// References whose records go (a full Bind).
    dropped: HashSet<BlockId>,
    /// Layer bindings XBIND takes off a record.
    unbound_layers: HashMap<BlockId, HashSet<LayerId>>,
    /// Local blocks XBIND takes off a record.
    unbound_blocks: HashMap<BlockId, HashSet<BlockId>>,
    /// Layers that keep their id under a new name.
    layer_rename: HashMap<LayerId, String>,
    /// Bound layers merged into an existing host layer.
    layer_merge: HashMap<LayerId, LayerId>,
    style_rename: HashMap<TextStyleId, String>,
    linetype_rename: HashMap<LinetypeId, String>,
    /// Names already used, lower-cased, per symbol kind.
    taken_layers: HashMap<String, LayerId>,
    taken_styles: HashSet<String>,
    taken_linetypes: HashSet<String>,
    taken_blocks: HashSet<String>,
    notes: Vec<String>,
}

impl<'a> Binder<'a> {
    fn new(doc: &'a Document, kind: BindType) -> Self {
        Self {
            doc,
            kind,
            moving: HashMap::new(),
            dropped: HashSet::new(),
            unbound_layers: HashMap::new(),
            unbound_blocks: HashMap::new(),
            layer_rename: HashMap::new(),
            layer_merge: HashMap::new(),
            style_rename: HashMap::new(),
            linetype_rename: HashMap::new(),
            taken_layers: doc
                .layers()
                .iter()
                .map(|l| (l.name.to_lowercase(), l.id))
                .collect(),
            taken_styles: doc
                .text_styles()
                .iter()
                .map(|s| s.name.to_lowercase())
                .collect(),
            taken_linetypes: doc
                .linetypes()
                .records()
                .iter()
                .map(|l| l.name.to_lowercase())
                .collect(),
            taken_blocks: doc.blocks().iter().map(|b| b.name.to_lowercase()).collect(),
            notes: Vec::new(),
        }
    }

    fn xref_name(&self, xref: BlockId) -> String {
        self.doc
            .block(xref)
            .map(|b| b.name.clone())
            .unwrap_or_default()
    }

    fn bind_reference(&mut self, root: BlockId, record: &crate::XrefDef) -> Result<(), BindError> {
        let name = self.xref_name(root);
        let root_def = self.doc.block(root).ok_or(BindError::NotAnXref(root))?;
        self.check_content(&name, root_def)?;
        self.dropped.insert(root);
        for bound in &record.layers {
            self.plan_layer(&name, &bound.source, bound.layer);
        }
        for block in &record.blocks {
            self.check_content(&name, block)?;
            self.plan_block(&name, block);
        }
        self.plan_prefix_symbols(&name);
        Ok(())
    }

    fn bind_symbol(&mut self, symbol: XbindSymbol) -> Result<(), BindError> {
        match symbol {
            XbindSymbol::Layer { xref, layer } => {
                let Some(record) = self.doc.xref(xref) else {
                    return Err(BindError::NotAnXref(xref));
                };
                let Some(bound) = record.layers.iter().find(|l| l.layer == layer) else {
                    return Err(BindError::NotDependent {
                        xref,
                        what: format!("layer {}", layer.0),
                    });
                };
                let source = bound.source.clone();
                let name = self.xref_name(xref);
                self.unbound_layers.entry(xref).or_default().insert(layer);
                self.plan_layer(&name, &source, layer);
            }
            XbindSymbol::Block { xref, block } => {
                let Some(record) = self.doc.xref(xref) else {
                    return Err(BindError::NotAnXref(xref));
                };
                if !record.blocks.iter().any(|b| b.id == block) {
                    return Err(BindError::NotDependent {
                        xref,
                        what: format!("block {}", block.0),
                    });
                }
                let name = self.xref_name(xref);
                let locals: HashMap<BlockId, Arc<BlockDef>> =
                    record.blocks.iter().map(|b| (b.id, b.clone())).collect();
                let mut stack = vec![block];
                while let Some(id) = stack.pop() {
                    let Some(def) = locals.get(&id) else {
                        continue;
                    };
                    self.check_content(&name, def)?;
                    self.unbound_blocks.entry(xref).or_default().insert(id);
                    self.plan_block(&name, def);
                    for child in &def.entities {
                        if let Entity::Insert(i) = &child.entity {
                            if locals.contains_key(&i.block.id) {
                                stack.push(i.block.id);
                            }
                        }
                    }
                }
            }
            XbindSymbol::TextStyle { xref, style } => {
                if self.doc.xref(xref).is_none() {
                    return Err(BindError::NotAnXref(xref));
                }
                let prefix = format!("{}|", self.xref_name(xref));
                let Some(record) = self.doc.text_style(style) else {
                    return Err(BindError::NotDependent {
                        xref,
                        what: format!("text style {}", style.0),
                    });
                };
                let Some(source) = record.name.strip_prefix(&prefix).map(str::to_owned) else {
                    return Err(BindError::NotDependent {
                        xref,
                        what: record.name.clone(),
                    });
                };
                let name = self.xref_name(xref);
                self.plan_style(&name, &source, style);
            }
            XbindSymbol::Linetype { xref, linetype } => {
                if self.doc.xref(xref).is_none() {
                    return Err(BindError::NotAnXref(xref));
                }
                let prefix = format!("{}|", self.xref_name(xref));
                let Some(record) = self.doc.linetypes().get(linetype) else {
                    return Err(BindError::NotDependent {
                        xref,
                        what: format!("linetype {}", linetype.0),
                    });
                };
                let Some(source) = record.name.strip_prefix(&prefix).map(str::to_owned) else {
                    return Err(BindError::NotDependent {
                        xref,
                        what: record.name.clone(),
                    });
                };
                let name = self.xref_name(xref);
                self.plan_linetype(&name, &source, linetype);
            }
        }
        Ok(())
    }

    fn check_content(&self, reference: &str, def: &BlockDef) -> Result<(), BindError> {
        for child in &def.entities {
            let what = match &child.entity {
                Entity::Hatch(_) => Some("hatch"),
                Entity::Xline { .. } => Some("construction line"),
                Entity::Ray { .. } => Some("ray"),
                _ => None,
            };
            if let Some(what) = what {
                return Err(BindError::UnsupportedContent {
                    reference: reference.to_owned(),
                    what,
                });
            }
        }
        Ok(())
    }

    fn plan_layer(&mut self, xref: &str, source: &str, layer: LayerId) {
        if self.layer_rename.contains_key(&layer) || self.layer_merge.contains_key(&layer) {
            return;
        }
        if self.kind == BindType::Insert {
            let key = source.to_lowercase();
            if let Some(existing) = self.taken_layers.get(&key).copied() {
                if existing != layer {
                    self.layer_merge.insert(layer, existing);
                    self.notes.push(format!(
                        "Layer \"{source}\" was merged into the drawing's own layer."
                    ));
                }
                return;
            }
            self.taken_layers.insert(key, layer);
            self.layer_rename.insert(layer, source.to_owned());
            return;
        }
        let name = prefixed_name(xref, source, |name| self.taken_layers.contains_key(name));
        self.taken_layers.insert(name.to_lowercase(), layer);
        self.layer_rename.insert(layer, name);
    }

    fn plan_style(&mut self, xref: &str, source: &str, style: TextStyleId) {
        if self.style_rename.contains_key(&style) {
            return;
        }
        let name = self.symbol_name(xref, source, &self.taken_styles);
        self.taken_styles.insert(name.to_lowercase());
        if self
            .doc
            .text_style(style)
            .is_some_and(|s| !s.name.eq_ignore_ascii_case(&name))
        {
            self.style_rename.insert(style, name);
        }
    }

    fn plan_linetype(&mut self, xref: &str, source: &str, linetype: LinetypeId) {
        if self.linetype_rename.contains_key(&linetype) {
            return;
        }
        let name = self.symbol_name(xref, source, &self.taken_linetypes);
        self.taken_linetypes.insert(name.to_lowercase());
        if self
            .doc
            .linetypes()
            .get(linetype)
            .is_some_and(|l| !l.name.eq_ignore_ascii_case(&name))
        {
            self.linetype_rename.insert(linetype, name);
        }
    }

    fn plan_block(&mut self, xref: &str, def: &BlockDef) {
        if self.moving.contains_key(&def.id) {
            return;
        }
        let name = self.symbol_name(xref, &def.name, &self.taken_blocks);
        if !name.eq_ignore_ascii_case(&def.name) {
            self.notes
                .push(format!("Block \"{}\" was bound as \"{name}\".", def.name));
        }
        self.taken_blocks.insert(name.to_lowercase());
        self.moving.insert(def.id, name);
    }

    fn plan_prefix_symbols(&mut self, xref: &str) {
        let prefix = format!("{xref}|");
        let styles: Vec<(TextStyleId, String)> = self
            .doc
            .text_styles()
            .iter()
            .filter_map(|s| {
                let source = s.name.strip_prefix(&prefix)?;
                (!source.is_empty()).then(|| (s.id, source.to_owned()))
            })
            .collect();
        for (id, source) in styles {
            self.plan_style(xref, &source, id);
        }
        let linetypes: Vec<(LinetypeId, String)> = self
            .doc
            .linetypes()
            .records()
            .iter()
            .filter_map(|l| {
                let source = l.name.strip_prefix(&prefix)?;
                (!source.is_empty()).then(|| (l.id, source.to_owned()))
            })
            .collect();
        for (id, source) in linetypes {
            self.plan_linetype(xref, &source, id);
        }
    }

    /// The bare name in Insert mode when it is free, otherwise `xref$n$name`.
    fn symbol_name(&self, xref: &str, symbol: &str, taken: &HashSet<String>) -> String {
        if self.kind == BindType::Insert && !taken.contains(&symbol.to_lowercase()) {
            return symbol.to_owned();
        }
        prefixed_name(xref, symbol, |name| taken.contains(name))
    }
}

fn prefixed_name(xref: &str, symbol: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut n = 0u32;
    loop {
        let name = format!("{xref}${n}${symbol}");
        if !taken(&name.to_lowercase()) {
            return name;
        }
        n += 1;
    }
}
