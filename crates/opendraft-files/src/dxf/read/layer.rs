//! The LAYER table: one record per layer, with its look, its switches and
//! OpenDraft's own Model-tab freeze, kept as XDATA (see `dxf::model_freeze`).

use opendraft_core::{Color, Layer, LayerId, LinetypeId, Lineweight};

use super::{aci_color, decode, true_color, Group, Importer, Record};
use crate::dxf::model_freeze::{APP, FROZEN_IN_MODEL};

impl Importer {
    pub(super) fn layer_record(&mut self, record: &Record) {
        let name = decode(record.text(2).unwrap_or("0"));
        let flags = record.int(70).unwrap_or(0);
        let index = record.int(62).unwrap_or(7);
        let mut color = match record.int(420) {
            Some(value) => true_color(value),
            None => aci_color(index.abs()),
        };
        if index == 0 {
            color = Color::Foreground;
        }
        let lineweight = match record.int(370) {
            Some(w) if w >= 0 => Lineweight(u16::try_from(w).unwrap_or(u16::MAX)),
            _ => Lineweight::DEFAULT,
        };
        let id = if name == "0" {
            LayerId::DEFAULT
        } else {
            self.layer_named(&name)
        };
        let layer = Layer {
            id,
            name: if id == LayerId::DEFAULT {
                "0".to_owned()
            } else {
                name
            },
            color,
            linetype: self
                .linetype_ref(record.text(6))
                .unwrap_or(LinetypeId::CONTINUOUS),
            lineweight,
            on: index >= 0,
            frozen: flags & 1 != 0,
            locked: flags & 4 != 0,
            frozen_in_model: frozen_in_model(&record.groups),
            plot: record.int(290).is_none_or(|v| v != 0),
        };
        self.layers.insert(layer.name.to_lowercase(), id);
        if flags & 16 != 0 {
            self.blocks.dependent_layers.insert(id.0);
        }
        self.document.replace_layer(layer);
    }
}

/// Whether the layer's XDATA says it is frozen in the Model tab: a
/// `1000 FrozenInModel` and a non-zero `1070` in the OpenDraft section.
fn frozen_in_model(groups: &[Group]) -> bool {
    let mut section = groups
        .iter()
        .skip_while(|g| !(g.code == 1001 && g.value.trim() == APP))
        .skip(1)
        .take_while(|g| g.code != 1001);
    section.any(|g| g.code == 1000 && g.value.trim() == FROZEN_IN_MODEL)
        && section
            .next()
            .is_some_and(|g| g.code == 1070 && g.value.trim().parse::<i64>().is_ok_and(|v| v != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf::model_freeze::{APP, FROZEN_IN_MODEL};

    fn read(pairs: &[(i32, &str)]) -> bool {
        let groups: Vec<Group> = pairs
            .iter()
            .map(|&(code, value)| Group {
                code,
                value: value.to_owned(),
            })
            .collect();
        frozen_in_model(&groups)
    }

    #[test]
    fn only_a_non_zero_flag_in_opendraft_data_counts() {
        let ours = |value| [(1001, APP), (1000, FROZEN_IN_MODEL), (1070, value)];
        assert!(read(&ours("1")));
        assert!(!read(&ours("0")));
        assert!(!read(&[
            (1001, "OTHER"),
            (1000, FROZEN_IN_MODEL),
            (1070, "1")
        ]));
        let elsewhere = [
            (1001, APP),
            (1001, "OTHER"),
            (1000, FROZEN_IN_MODEL),
            (1070, "1"),
        ];
        assert!(!read(&elsewhere));
        assert!(!read(&[(70, "0")]));
    }

    #[test]
    fn the_plot_flag_survives_a_round_trip_and_forces_r2000() {
        let mut doc = opendraft_core::Document::default();
        for (name, plot) in [("Sketch", false), ("Walls", true)] {
            let id = doc.allocate_layer_id();
            doc.push_layer(Layer {
                plot,
                ..Layer::new(id, name)
            });
        }
        let bytes = crate::dxf::write::write(&doc).unwrap().bytes;
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("AC1015"), "R12 has no plot flag");
        let back = crate::dxf::read::read(&bytes).unwrap().document;
        let plots = |name: &str| back.layers().iter().find(|l| l.name == name).unwrap().plot;
        assert!(!plots("Sketch"));
        assert!(plots("Walls") && plots("0"));
    }
}
