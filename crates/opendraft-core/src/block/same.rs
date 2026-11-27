//! Whether two blocks of two drawings hold the same thing, so that taking
//! one from a library into a drawing that has the other changes nothing.

use crate::{Attr, Attributes, BlockDef, Document, Entity};

impl BlockDef {
    /// Whether this block (a definition of `here`) and `other` (of
    /// `there`) have the same base point, unit and objects: the
    /// same shapes, on layers of the same names, with the same colours,
    /// lineweights and linetypes (by name), a nested block by its name and
    /// placement. Ids differ from drawing to drawing, so none is compared.
    ///
    /// A "no" is safe: the caller asks, or adds the block under a new name.
    /// Attribute values of a nested block and dynamic values are compared
    /// as written, so a block that has them may be called different.
    pub fn same_as(&self, here: &Document, other: &BlockDef, there: &Document) -> bool {
        self.base == other.base
            && self.unit == other.unit
            && self.entities.len() == other.entities.len()
            && format!("{:?}", self.dynamic) == format!("{:?}", other.dynamic)
            && self.entities.iter().zip(&other.entities).all(|(a, b)| {
                same_entity(&a.entity, &b.entity) && same_look(&a.attrs, here, &b.attrs, there)
            })
    }
}

fn same_entity(a: &Entity, b: &Entity) -> bool {
    match (a, b) {
        (Entity::Insert(a), Entity::Insert(b)) => {
            a.block.name == b.block.name
                && a.insertion == b.insertion
                && a.scale_x == b.scale_x
                && a.scale_y == b.scale_y
                && a.rotation == b.rotation
                && a.attributes.len() == b.attributes.len()
                && format!("{:?}", a.dynamic) == format!("{:?}", b.dynamic)
        }
        (Entity::Insert(_), _) | (_, Entity::Insert(_)) => false,
        _ => format!("{a:?}") == format!("{b:?}"),
    }
}

fn same_look(a: &Attributes, here: &Document, b: &Attributes, there: &Document) -> bool {
    let layer =
        |attrs: &Attributes, doc: &Document| doc.layer(attrs.layer).map(|l| l.name.to_lowercase());
    let linetype = |attr: &Attr<crate::LinetypeId>, doc: &Document| match attr {
        Attr::Set(id) => Attr::Set(doc.linetypes().get(*id).map(|l| l.name.to_lowercase())),
        Attr::ByLayer => Attr::ByLayer,
        Attr::ByBlock => Attr::ByBlock,
    };
    layer(a, here).is_some()
        && layer(a, here) == layer(b, there)
        && a.color == b.color
        && a.lineweight == b.lineweight
        && linetype(&a.linetype, here) == linetype(&b.linetype, there)
        && a.linetype_scale == b.linetype_scale
        && a.continuous_generation == b.continuous_generation
}

#[cfg(test)]
#[path = "same_tests.rs"]
mod tests;
