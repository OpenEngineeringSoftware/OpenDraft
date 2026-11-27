//! The layer table: layers in display order, the current layer, and which
//! layers may be deleted.

use super::{after, Document};
use crate::layer::{Attr, Attributes, Color, Layer, LayerId, Lineweight};
use crate::linetype::LinetypeId;

impl Document {
    /// Layers in display order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Layers as lists show them: by name, ignoring case, numbers by value.
    pub fn layers_by_name(&self) -> Vec<&Layer> {
        let mut layers: Vec<&Layer> = self.layers.iter().collect();
        layers.sort_by(|a, b| crate::name_order::compare(&a.name, &b.name));
        layers
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        let &i = self.layer_positions.get(&id)?;
        Some(&self.layers[i])
    }

    pub fn layer_position(&self, id: LayerId) -> Option<usize> {
        self.layer_positions.get(&id).copied()
    }

    pub fn current_layer(&self) -> LayerId {
        self.current_layer
    }

    /// Make `id` the layer new objects go on. Ignores unknown ids.
    pub fn set_current_layer(&mut self, id: LayerId) {
        if self.layer_positions.contains_key(&id) {
            self.current_layer = id;
        }
    }

    /// The attributes a new object is drawn with: the current layer, and the
    /// current colour, linetype and lineweight (ByLayer unless set).
    pub fn new_object_attributes(&self) -> Attributes {
        Attributes {
            color: self.new_color.clone(),
            linetype: self.new_linetype.clone(),
            lineweight: self.new_lineweight.clone(),
            ..Attributes::on_layer(self.current_layer)
        }
    }

    /// The current colour, linetype and lineweight, in that order.
    pub fn new_object_style(&self) -> (&Attr<Color>, &Attr<LinetypeId>, &Attr<Lineweight>) {
        (&self.new_color, &self.new_linetype, &self.new_lineweight)
    }

    pub fn set_new_object_style(
        &mut self,
        color: Attr<Color>,
        linetype: Attr<LinetypeId>,
        lineweight: Attr<Lineweight>,
    ) {
        self.new_color = color;
        self.new_linetype = linetype;
        self.new_lineweight = lineweight;
    }

    pub fn allocate_layer_id(&mut self) -> LayerId {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id = after(self.next_layer_id);
        id
    }

    /// Whether a layer is already called `name` (ignoring case, as in
    /// AutoCAD), other than `except`.
    pub fn layer_name_taken(&self, name: &str, except: Option<LayerId>) -> bool {
        self.layers
            .iter()
            .any(|l| Some(l.id) != except && l.name.eq_ignore_ascii_case(name))
    }

    /// A name no layer has: "Layer1", "Layer2", ...
    pub fn unused_layer_name(&self) -> String {
        (1..)
            .map(|n| format!("Layer{n}"))
            .find(|name| !self.layer_name_taken(name, None))
            .unwrap_or_default()
    }

    /// Whether any object is on the layer, including the objects inside
    /// block definitions.
    pub fn layer_in_use(&self, id: LayerId) -> bool {
        self.objects.iter().any(|o| {
            o.attrs.layer == id
                || matches!(&o.entity, crate::Entity::Insert(i) if i.uses_layer_for_attributes(id))
        }) || self.blocks.iter().any(|b| b.uses_layer(id))
    }

    /// The layers that [`layer_in_use`](Self::layer_in_use) says are in use,
    /// found in one pass over the objects instead of one for each layer.
    pub fn layers_in_use(&self) -> std::collections::HashSet<LayerId> {
        let mut used: std::collections::HashSet<LayerId> =
            self.objects.iter().map(|o| o.attrs.layer).collect();
        let inserts: Vec<_> = self
            .objects
            .iter()
            .filter_map(|o| match &o.entity {
                crate::Entity::Insert(i) => Some(i),
                _ => None,
            })
            .collect();
        for layer in &self.layers {
            let id = layer.id;
            if !used.contains(&id)
                && (inserts.iter().any(|i| i.uses_layer_for_attributes(id))
                    || self.blocks.iter().any(|b| b.uses_layer(id)))
            {
                used.insert(id);
            }
        }
        used
    }

    /// Why a layer cannot be deleted, or `None` when it can.
    pub fn layer_delete_blocker(&self, id: LayerId) -> Option<&'static str> {
        if id == LayerId::DEFAULT {
            Some(crate::key!("Layer 0 cannot be deleted"))
        } else if id == self.current_layer {
            Some(crate::key!("The current layer cannot be deleted"))
        } else if self.layer_in_use(id) {
            Some(crate::key!("A layer with objects on it cannot be deleted"))
        } else {
            None
        }
    }

    /// Append a layer that already has an id (a new layer, or redo).
    pub fn push_layer(&mut self, layer: Layer) {
        self.next_layer_id = self.next_layer_id.max(after(layer.id.0));
        self.layer_positions.insert(layer.id, self.layers.len());
        self.layers.push(layer);
    }

    pub fn insert_layer(&mut self, position: usize, layer: Layer) {
        self.next_layer_id = self.next_layer_id.max(after(layer.id.0));
        self.layers.insert(position.min(self.layers.len()), layer);
        self.reindex_layers();
    }

    /// Remove a layer, returning where it was and what it was. The default
    /// layer is never removed.
    pub fn remove_layer(&mut self, id: LayerId) -> Option<(usize, Layer)> {
        if id == LayerId::DEFAULT {
            return None;
        }
        let position = self.layer_position(id)?;
        let layer = self.layers.remove(position);
        self.reindex_layers();
        Some((position, layer))
    }

    /// Swap a layer's record, keeping its position. Returns the old one, or
    /// `None` (changing nothing) for an unknown id.
    pub fn replace_layer(&mut self, layer: Layer) -> Option<Layer> {
        let &i = self.layer_positions.get(&layer.id)?;
        Some(std::mem::replace(&mut self.layers[i], layer))
    }

    fn reindex_layers(&mut self) {
        self.layer_positions = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id, i))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attributes, Entity, Point};

    #[test]
    fn the_one_pass_answer_is_the_per_layer_answer() {
        let mut doc = Document::default();
        let (used, unused) = (doc.allocate_layer_id(), doc.allocate_layer_id());
        doc.push_layer(Layer::new(used, "Used"));
        doc.push_layer(Layer::new(unused, "Unused"));
        doc.add_with(
            Entity::Line {
                start: Point::new(0.0, 0.0),
                end: Point::new(1.0, 0.0),
            },
            Attributes::on_layer(used),
        );
        let set = doc.layers_in_use();
        for layer in doc.layers() {
            assert_eq!(set.contains(&layer.id), doc.layer_in_use(layer.id));
        }
        assert!(set.contains(&used) && !set.contains(&unused));
    }

    #[test]
    fn lists_show_layers_by_name_but_the_table_keeps_its_order() {
        let mut doc = Document::default();
        for name in ["walls", "Level 10", "Doors", "Level 2"] {
            let id = doc.allocate_layer_id();
            doc.push_layer(Layer::new(id, name));
        }
        let names = |layers: Vec<&Layer>| -> Vec<String> {
            layers.iter().map(|l| l.name.clone()).collect()
        };
        assert_eq!(
            names(doc.layers_by_name()),
            ["0", "Doors", "Level 2", "Level 10", "walls"]
        );
        assert_eq!(
            names(doc.layers().iter().collect()),
            ["0", "walls", "Level 10", "Doors", "Level 2"],
            "the file and undo still see the order layers were made in"
        );
    }
}
