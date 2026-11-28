//! The Attach External Reference dialog XATTACH opens, laid out as AutoCAD's:
//! the name and path on top; the reference type and path type side by side;
//! the insertion point, scale and rotation below, each "specified on-screen"
//! or typed. Anything left on-screen is asked in the drawing after OK.

use eframe::egui;
use opendraft_commands::{Reply, Wants};
use opendraft_core::{Point, XrefAttachDraft, XrefKind, XrefPathType};
use opendraft_ui::widgets;

use super::OpenDraftApp;

const WIDTH: f32 = 560.0;
const DRAWINGS: (&str, &[&str]) = ("Drawings", &["odraft", "dxf", "dwg"]);

impl OpenDraftApp {
    pub(super) fn xref_attach_window(&mut self, ctx: &egui::Context) {
        let wanted = self
            .state
            .waiting()
            .filter(|ask| ask.wants == Wants::XrefAttachDialog)
            .map(|ask| ask.xref_attach.clone());
        let Some(start) = wanted else {
            self.state.command_dialogs.xref_attach = None;
            return;
        };
        if self.state.command_dialogs.xref_attach.is_none() {
            self.state.command_dialogs.xref_attach = start.map(|draft| *draft);
        }
        let Some(mut draft) = self.state.command_dialogs.xref_attach.take() else {
            return;
        };
        let problem = draft.problem(self.state.session().document());
        let dir = self.state.xref_host_dir().map(std::path::PathBuf::from);
        let picker = &mut self.state.picker;
        let theme = &self.theme;
        let (mut open, mut accepted, mut cancel) = (true, false, false);

        widgets::dialog(
            ctx,
            theme,
            "Attach External Reference",
            &mut open,
            WIDTH,
            |ui| {
                name_and_path(ui, theme, &mut draft, picker.as_mut(), dir.as_deref());
                ui.add_space(8.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        widgets::group_box(ui, theme, "Reference Type", |ui| {
                            ui.radio_value(&mut draft.kind, XrefKind::Attach, "Attachment");
                            ui.radio_value(&mut draft.kind, XrefKind::Overlay, "Overlay");
                        });
                    });
                    ui.vertical(|ui| {
                        widgets::group_box(ui, theme, "Path Type", |ui| {
                            ui.radio_value(&mut draft.path_type, XrefPathType::Full, "Full path");
                            ui.radio_value(
                                &mut draft.path_type,
                                XrefPathType::Relative,
                                "Relative path",
                            );
                            ui.radio_value(&mut draft.path_type, XrefPathType::NoPath, "No path");
                        });
                    });
                });
                ui.add_space(8.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        widgets::group_box(ui, theme, "Insertion Point", |ui| {
                            insertion(ui, theme, &mut draft.insertion);
                        });
                    });
                    ui.vertical(|ui| {
                        widgets::group_box(ui, theme, "Scale", |ui| {
                            scale(ui, theme, &mut draft.scale);
                        });
                    });
                    ui.vertical(|ui| {
                        widgets::group_box(ui, theme, "Rotation", |ui| {
                            rotation(ui, theme, &mut draft.rotation);
                        });
                    });
                });
                ui.add_space(6.0);
                if let Some(why) = problem.as_ref() {
                    ui.colored_label(ui.visuals().error_fg_color, why);
                }
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(problem.is_none(), |ui| {
                            if widgets::primary_button(ui, theme, "OK").clicked() {
                                accepted = true;
                            }
                        });
                        cancel = widgets::secondary_button(ui, theme, "Cancel").clicked();
                    });
                });
            },
        );

        if accepted {
            // The label was drawn with last frame's draft; check the accepted
            // one, so a name or scale typed in this frame cannot slip through.
            if draft.problem(self.state.session().document()).is_some() {
                self.state.command_dialogs.xref_attach = Some(draft);
            } else {
                self.state.command_dialogs.xref_attach = None;
                self.state.reply(Reply::XrefAttach(Box::new(draft)));
            }
        } else if cancel || !open {
            self.state.command_dialogs.xref_attach = None;
            self.state.cancel_command();
        } else {
            self.state.command_dialogs.xref_attach = Some(draft);
        }
    }
}

fn name_and_path(
    ui: &mut egui::Ui,
    theme: &opendraft_ui::Theme,
    draft: &mut XrefAttachDraft,
    picker: &mut dyn crate::picker::FilePicker,
    dir: Option<&std::path::Path>,
) {
    egui::Grid::new("xref_attach_source")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Name:");
            ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(f32::INFINITY));
            ui.end_row();
            ui.label("Path:");
            ui.horizontal(|ui| {
                let room = (ui.available_width() - 88.0).max(120.0);
                ui.add(egui::TextEdit::singleline(&mut draft.source_path).desired_width(room));
                if widgets::secondary_button(ui, theme, "Browse...").clicked() {
                    let old_stem = stem_of(&draft.source_path);
                    if let Some(path) = picker.open_file("Attach External Reference", dir, DRAWINGS)
                    {
                        let stem = stem_of(&path.to_string_lossy());
                        if draft.name.trim().is_empty() || draft.name == old_stem {
                            draft.name = stem;
                        }
                        draft.source_path = path.to_string_lossy().into_owned();
                    }
                }
            });
            ui.end_row();
        });
}

fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "xref".to_owned())
}

fn insertion(ui: &mut egui::Ui, theme: &opendraft_ui::Theme, value: &mut Option<Point>) {
    specify_on_screen(ui, value.is_none(), value, || Some(Point::new(0.0, 0.0)));
    if let Some(point) = value.as_mut() {
        egui::Grid::new("xref_attach_point")
            .num_columns(2)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                ui.label("X:");
                widgets::number_field(ui, theme, &mut point.x, "", 0.1);
                ui.end_row();
                ui.label("Y:");
                widgets::number_field(ui, theme, &mut point.y, "", 0.1);
                ui.end_row();
            });
    }
}

fn scale(ui: &mut egui::Ui, theme: &opendraft_ui::Theme, value: &mut Option<(f64, f64)>) {
    specify_on_screen(ui, value.is_none(), value, || Some((1.0, 1.0)));
    if let Some((x, y)) = value.as_mut() {
        let mut factor = *x;
        if widgets::number_field(ui, theme, &mut factor, "", 0.1).changed() {
            *x = factor;
            *y = factor;
        }
    }
}

fn rotation(ui: &mut egui::Ui, theme: &opendraft_ui::Theme, value: &mut Option<f64>) {
    specify_on_screen(ui, value.is_none(), value, || Some(0.0));
    if let Some(angle) = value.as_mut() {
        let mut degrees = angle.to_degrees();
        if widgets::number_field(ui, theme, &mut degrees, "\u{b0}", 1.0).changed() {
            *angle = degrees.to_radians();
        }
    }
}

/// The "Specify On-screen" switch for one group: an on-screen value is
/// `None`, and turning the switch off starts the typed value from `typed`.
fn specify_on_screen<T>(
    ui: &mut egui::Ui,
    on_screen: bool,
    value: &mut Option<T>,
    typed: impl FnOnce() -> Option<T>,
) {
    let mut on_screen = on_screen;
    if ui.checkbox(&mut on_screen, "Specify On-screen").changed() {
        *value = if on_screen { None } else { typed() };
    }
}
