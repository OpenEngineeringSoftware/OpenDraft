//! The Details section: what is known about the selected reference.

use eframe::egui::{self, Ui};
use opendraft_ui::{i18n::tr, key, Theme};

use super::rows::{date, size, Row};

/// The rows of the read-out: label and value, for the selected reference.
fn fields(row: &Row) -> [(&'static str, String); 7] {
    [
        (key!("Reference Name"), row.name.clone()),
        (key!("Status"), tr(row.health.label()).into_owned()),
        (key!("Size"), row.size.map_or_else(String::new, size)),
        (key!("Type"), tr(row.kind.label()).into_owned()),
        (key!("Date"), row.date.map_or_else(String::new, date)),
        (key!("Found At"), row.found.clone()),
        (key!("Saved Path"), row.path.clone()),
    ]
}

pub(super) fn show(ui: &mut Ui, theme: &Theme, row: Option<&Row>) {
    let c = &theme.colors;
    egui::ScrollArea::vertical()
        .id_salt("xref-details-down")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            let Some(row) = row else {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(tr(key!("Select a reference to see its details.")))
                            .color(c.text_weak),
                    );
                });
                return;
            };
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(8, 0))
                .show(ui, |ui| {
                    egui::Grid::new("xref-details")
                        .num_columns(2)
                        .spacing(egui::vec2(12.0, 3.0))
                        .show(ui, |ui| {
                            for (label, value) in fields(row) {
                                ui.label(egui::RichText::new(tr(label)).color(c.text_weak));
                                let dim = label == key!("Status") && !row.health.is_problem();
                                let text = egui::RichText::new(&value).color(if dim {
                                    c.text_weak
                                } else {
                                    c.text
                                });
                                ui.add(egui::Label::new(text).selectable(true).truncate())
                                    .on_hover_text(&value);
                                ui.end_row();
                            }
                        });
                });
        });
}
