//! The planner's tables: linetypes, layers and text styles, and the host
//! records they reuse or add.

use opendraft_core::{Layer, LayerId, TextStyle, TextStyleId, XrefProblem};

use super::Planner;

impl Planner<'_> {
    pub(super) fn map_linetypes(&mut self) -> Result<(), super::XrefPlanError> {
        let source = self.source;
        for record in source.linetypes().records() {
            if record.id.is_special() {
                self.linetypes.insert(record.id, record.id);
                continue;
            }
            // A reference's own linetype, which that reference brings itself.
            if self.is_nested_symbol(&record.name) {
                continue;
            }
            if let Some(host) = self.host.linetypes().named(&record.name) {
                self.linetypes.insert(record.id, host.id);
                continue;
            }
            let table = self
                .linetypes_plan
                .get_or_insert_with(|| self.host.linetypes().clone());
            if table.records().len() as u64 >= self.limits.max_linetypes {
                return Err(super::XrefPlanError::TooMany {
                    what: "linetypes",
                    limit: self.limits.max_linetypes,
                });
            }
            let mut name = format!("{}|{}", self.name, record.name);
            let mut n = 2;
            while table.named(&name).is_some() {
                name = format!("{}|{}~{n}", self.name, record.name);
                n += 1;
            }
            if name != format!("{}|{}", self.name, record.name) {
                self.notes.push(format!(
                    "linetype \"{}\" was added as \"{name}\" because the drawing already has one",
                    record.name
                ));
            }
            let id = table
                .add(&name, &record.description, record.elements.clone())
                .map_err(|e| super::XrefPlanError::Message(e.to_string()))?;
            self.linetypes.insert(record.id, id);
        }
        Ok(())
    }

    pub(super) fn map_layers(&mut self) -> Result<(), super::XrefPlanError> {
        self.layers.insert(LayerId::DEFAULT, LayerId::DEFAULT);
        let source = self.source;
        for layer in source.layers() {
            if layer.id == LayerId::DEFAULT || self.is_nested_symbol(&layer.name) {
                continue;
            }
            let want = format!("{}|{}", self.name, layer.name);
            let bound = self
                .existing
                .as_ref()
                .and_then(|x| x.layer_for(&format!("{}{}", self.prefix, layer.name)));
            if let Some(id) = bound {
                if self
                    .host
                    .layer(id)
                    .is_some_and(|l| l.name.eq_ignore_ascii_case(&want))
                {
                    self.layers.insert(layer.id, id);
                    self.reset_bound_layer(layer, id);
                    continue;
                }
            }
            if self
                .host
                .layers()
                .iter()
                .any(|l| l.name.eq_ignore_ascii_case(&want))
            {
                return Err(super::XrefPlanError::Problem(
                    XrefProblem::LayerBoundElsewhere(want),
                ));
            }
            if self.added_layers.len() as u64 >= self.limits.max_layers {
                return Err(super::XrefPlanError::TooMany {
                    what: "layers",
                    limit: self.limits.max_layers,
                });
            }
            let id = LayerId(self.next_layer);
            self.next_layer =
                self.next_layer
                    .checked_add(1)
                    .ok_or(super::XrefPlanError::TooMany {
                        what: "layers",
                        limit: u64::MAX,
                    })?;
            let linetype = self
                .linetypes
                .get(&layer.linetype)
                .copied()
                .unwrap_or(layer.linetype);
            self.added_layers.push(Layer {
                id,
                name: want,
                color: layer.color,
                linetype,
                lineweight: layer.lineweight,
                on: layer.on,
                frozen: layer.frozen,
                locked: layer.locked,
                frozen_in_model: false,
                plot: layer.plot,
            });
            self.layers.insert(layer.id, id);
        }
        Ok(())
    }

    /// Keep the host's bound layer when `VISRETAIN` says so; otherwise take
    /// the source's properties again.
    fn reset_bound_layer(&mut self, source: &Layer, id: LayerId) {
        if self.host.visretain() {
            return;
        }
        let Some(current) = self.host.layer(id) else {
            return;
        };
        let linetype = self
            .linetypes
            .get(&source.linetype)
            .copied()
            .unwrap_or(source.linetype);
        let wanted = Layer {
            linetype,
            color: source.color,
            lineweight: source.lineweight,
            on: source.on,
            frozen: source.frozen,
            locked: source.locked,
            ..current.clone()
        };
        if *current != wanted {
            self.changed_layers.push(wanted);
        }
    }

    pub(super) fn map_styles(&mut self) -> Result<(), super::XrefPlanError> {
        let source = self.source;
        for style in source.text_styles() {
            if style.id == TextStyleId::STANDARD || style.name.eq_ignore_ascii_case("Standard") {
                self.styles.insert(style.id, TextStyleId::STANDARD);
                continue;
            }
            if self.is_nested_symbol(&style.name) {
                continue;
            }
            let want = format!("{}|{}", self.name, style.name);
            if let Some(host) = self.host.text_style_named(&want) {
                if same_style(host, style) {
                    self.styles.insert(style.id, host.id);
                    continue;
                }
            }
            if self.style_table.len() as u64 >= self.limits.max_text_styles {
                return Err(super::XrefPlanError::TooMany {
                    what: "text styles",
                    limit: self.limits.max_text_styles,
                });
            }
            let mut name = want.clone();
            let mut n = 2;
            while self
                .style_table
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(&name))
            {
                name = format!("{want}~{n}");
                n += 1;
            }
            if name != want {
                self.notes.push(format!(
                    "text style \"{}\" was added as \"{name}\" because the drawing has another of that name",
                    style.name
                ));
            }
            let mut made = style.clone();
            made.id = TextStyleId(self.next_style);
            made.name = name;
            self.next_style = self.next_style.saturating_add(1);
            self.style_table.push(made);
            self.styles
                .insert(style.id, TextStyleId(self.next_style - 1));
        }
        Ok(())
    }
}

fn same_style(a: &TextStyle, b: &TextStyle) -> bool {
    let mut a = a.clone();
    a.id = TextStyleId::STANDARD;
    a.name.clear();
    let mut b = b.clone();
    b.id = TextStyleId::STANDARD;
    b.name.clear();
    a == b
}
