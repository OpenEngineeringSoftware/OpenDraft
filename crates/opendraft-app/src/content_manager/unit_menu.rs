//! A drawing block's unit in its details, with a pencil beside it that
//! opens the list of units: choosing one puts the block in that unit.

use eframe::egui;
use opendraft_core::InsertUnits;
use opendraft_ui::{icons, Theme};

use super::chrome::icon_button;

/// What the unit shows as, beside a block's name.
pub(super) fn unit_text(unit: InsertUnits) -> String {
    if unit == InsertUnits::UNITLESS {
        "unitless".to_owned()
    } else {
        unit.label().to_owned()
    }
}

/// The unit, then its pencil and list. Returns the unit chosen this frame,
/// and whether the list is open (keys then belong to it: Escape closes it).
pub(super) fn unit_with_pencil(
    ui: &mut egui::Ui,
    theme: &Theme,
    unit: InsertUnits,
) -> (Option<InsertUnits>, bool) {
    ui.weak(unit_text(unit));
    let tip = "Change the block's unit";
    let pencil = icon_button(ui, theme, icons::CONTENT_RENAME, false, true, tip);
    let id = egui::Id::new("content_manager_unit_menu");
    if pencil.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    let mut chosen = None;
    // Not on any click: the list scrolls, and a click on its bar must not close it.
    let close = egui::PopupCloseBehavior::CloseOnClickOutside;
    egui::popup_below_widget(ui, id, &pencil, close, |ui| {
        ui.set_min_width(170.0);
        egui::ScrollArea::vertical()
            .max_height(260.0)
            .show(ui, |ui| {
                for choice in InsertUnits::all() {
                    if ui
                        .selectable_label(choice == unit, choice.label())
                        .clicked()
                    {
                        chosen = Some(choice);
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
            });
    });
    (chosen, ui.memory(|m| m.is_popup_open(id)))
}
