//! Bringing one source entity into the host drawing: its layers, linetypes
//! and text styles take the host's ids, and references that only make sense
//! in the source (dimension ties, leader annotations, hatch sources) are
//! cleared rather than carried into a drawing they do not belong to.

use std::collections::HashMap;
use std::sync::Arc;

use opendraft_core::block::{BlockDef, FieldObjectRef, FieldText};
use opendraft_core::mleader::MLeaderContent;
use opendraft_core::{Attr, Attributes, BlockId, Entity, Insert, LayerId, LinetypeId, TextStyleId};

pub(super) struct Remap<'a> {
    pub layers: &'a HashMap<LayerId, LayerId>,
    pub linetypes: &'a HashMap<LinetypeId, LinetypeId>,
    pub styles: &'a HashMap<TextStyleId, TextStyleId>,
    /// Local definitions built so far, by the id they had in the source.
    pub blocks: &'a HashMap<BlockId, Arc<BlockDef>>,
    /// The host id each source block became, for field references.
    pub block_ids: &'a HashMap<BlockId, BlockId>,
    /// The host name of the reference, for namespaced style names.
    pub xref_name: &'a str,
}

impl Remap<'_> {
    pub fn attributes(&self, attrs: &Attributes) -> Attributes {
        Attributes {
            layer: self
                .layers
                .get(&attrs.layer)
                .copied()
                .unwrap_or(LayerId::DEFAULT),
            linetype: match &attrs.linetype {
                Attr::Set(id) => Attr::Set(self.linetype(*id)),
                other => other.clone(),
            },
            ..attrs.clone()
        }
    }

    fn linetype(&self, id: LinetypeId) -> LinetypeId {
        self.linetypes.get(&id).copied().unwrap_or(id)
    }

    fn style(&self, id: TextStyleId) -> TextStyleId {
        self.styles
            .get(&id)
            .copied()
            .unwrap_or(TextStyleId::STANDARD)
    }

    fn fields(&self, text: &FieldText) -> FieldText {
        text.map_refs(&|reference| match reference {
            FieldObjectRef::TextStyle(id) => FieldObjectRef::TextStyle(self.style(*id)),
            FieldObjectRef::Block(id) => self.block_ids.get(id).map_or_else(
                || FieldObjectRef::Unresolved(format!("xref:block:{}", id.0)),
                |block| FieldObjectRef::Block(*block),
            ),
            FieldObjectRef::Entity(id) => {
                FieldObjectRef::Unresolved(format!("xref:entity:{}", id.0))
            }
            FieldObjectRef::Layout(id) => {
                FieldObjectRef::Unresolved(format!("xref:layout:{}", id.0))
            }
            FieldObjectRef::AttributeDefinition(id) => {
                FieldObjectRef::Unresolved(format!("xref:attribute:{}", id.0))
            }
            FieldObjectRef::Unresolved(name) => FieldObjectRef::Unresolved(name.clone()),
        })
    }

    /// The entity with host ids, or `None` when it places a block the source
    /// does not reach (which the caller counts as skipped).
    pub fn entity(&self, entity: &Entity) -> Option<Entity> {
        Some(match entity {
            Entity::Insert(i) => {
                let block = self.blocks.get(&i.block.id)?.clone();
                let mut insert = Insert {
                    block,
                    is_xref: false,
                    dynamic: None,
                    ..(**i).clone()
                };
                for reference in &mut insert.attributes {
                    if let Some(attrs) = &reference.attrs {
                        reference.attrs = Some(self.attributes(attrs));
                    }
                    reference.value = self.fields(&reference.value);
                }
                Entity::Insert(Box::new(insert))
            }
            Entity::MText(m) => {
                let mut m = (**m).clone();
                m.style = self.style(m.style);
                Entity::MText(Box::new(m))
            }
            Entity::Text(t) => {
                let mut t = (**t).clone();
                t.placement.style = self.style(t.placement.style);
                Entity::Text(Box::new(t))
            }
            Entity::AttributeDefinition(d) => {
                let mut d = (**d).clone();
                d.text.style = self.style(d.text.style);
                d.default_value = self.fields(&d.default_value);
                Entity::AttributeDefinition(Box::new(d))
            }
            Entity::Dimension(d) => {
                let mut d = (**d).clone();
                d.style.text_style = self.style(d.style.text_style);
                // The ties name source objects, which are not in this drawing.
                d.assoc.clear();
                Entity::Dimension(Box::new(d))
            }
            Entity::MLeader(m) => {
                let mut m = (**m).clone();
                if let MLeaderContent::MText(text) = &mut m.content {
                    text.text.style = self.style(text.text.style);
                }
                // The style table is host-owned; the name says where it came
                // from without claiming a record this build does not create.
                m.style_name = format!("{}|{}", self.xref_name, m.style_name);
                Entity::MLeader(Box::new(m))
            }
            Entity::Leader(l) => {
                let mut l = (**l).clone();
                l.annotation = None;
                l.dim_style_name = format!("{}|{}", self.xref_name, l.dim_style_name);
                Entity::Leader(Box::new(l))
            }
            Entity::Hatch(h) => {
                let mut h = (**h).clone();
                for hatch_loop in &mut h.loops {
                    hatch_loop.sources.clear();
                }
                h.associative = false;
                Entity::Hatch(Box::new(h))
            }
            other => other.clone(),
        })
    }
}
