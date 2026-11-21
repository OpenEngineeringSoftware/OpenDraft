//! The command line panel: log, prompt with clickable options, input and the
//! suggestion list.

use eframe::egui;
use opendraft_ui::{Colors, Theme};

use crate::cmdline::Suggestion;

use super::panels::PanelViewer;

/// Space the input bar extends above the row it holds.
const INPUT_BAR_PAD: f32 = 4.0;

/// Space kept clear along the top and bottom of the panel.
pub(super) const MARGIN: f32 = 4.0;

/// Where the log and the input row go in a panel's inside: the row keeps
/// its height along the bottom, whatever the panel's, and the log has what
/// is left `gap` above it.
fn rows(inside: egui::Rect, row_height: f32, gap: f32) -> (egui::Rect, egui::Rect) {
    let row = inside.with_min_y(inside.bottom() - row_height);
    let log = inside.with_max_y((row.top() - gap).max(inside.top()));
    (log, row)
}

impl PanelViewer<'_> {
    /// The command line: a scrolling log above one input row that shows the
    /// current prompt (with clickable options) and the text being typed.
    pub(super) fn command_line(&mut self, ui: &mut egui::Ui) {
        if let Some((side, rows)) = self.command_line_strip {
            let ui = &mut self.command_line_grip(ui, side, &rows);
            // A strip shows whatever history fits: one line or more.
            self.command_line_body(ui, rows.height(1) - rows.tabs - 0.5);
        } else {
            self.command_line_body(ui, 90.0);
        }
    }

    /// The panel proper. The log shows when the panel is over `history_from`
    /// points high.
    fn command_line_body(&mut self, ui: &mut egui::Ui, history_from: f32) {
        use crate::cmdline::{CmdOption, ALIASES};

        let theme = self.theme;
        let input_id = egui::Id::new("command_line_input");
        // Commands that can run now, for filtering suggestions.
        let runnable: Vec<&str> = ALIASES
            .iter()
            .map(|a| a.2)
            .filter(|id| self.registry.is_enabled(id, &*self.state))
            .collect();
        let enabled = |id: &str| runnable.contains(&id);

        let prompt = self.state.cmd_prompt();
        // Typed text keeps its spaces; everywhere else a space is Enter.
        let wants_text = self
            .state
            .dynamic_ask()
            .is_some_and(|ask| ask.wants == opendraft_commands::Wants::Text);
        let had_focus = ui.memory(|m| m.has_focus(input_id));
        let requested_focus = std::mem::take(&mut self.state.cmdline.focus);
        if requested_focus {
            ui.memory_mut(|m| m.request_focus(input_id));
        }
        let owns_keyboard = had_focus || requested_focus;
        let mut chosen: Option<CmdOption> = None;
        let mut input = std::mem::take(&mut self.state.cmdline.input);

        // Arrow keys and Tab drive the suggestion list while typing.
        if owns_keyboard {
            let count = self.state.cmdline.listed;
            if count > 0 {
                let cl = &mut self.state.cmdline;
                ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                        cl.selected = (cl.selected + 1) % count;
                    }
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                        cl.selected = (cl.selected + count - 1) % count;
                    }
                });
            }
        }
        let mut submit = false;
        let accept_tab;
        let escape;
        // Judged on the whole panel, before the margin: short panels (collapsed or
        // dragged small) show only the input row.
        let expanded = ui.available_height() > history_from;
        let text_rect;
        // The input row is pinned to the bottom of the panel, so it shows at
        // any height; the log takes what is left above it.
        let panel = ui.max_rect();
        let row_height = ui.spacing().interact_size.y.max(22.0);
        let (log_rect, row_rect) = rows(
            panel.shrink2(egui::vec2(8.0, MARGIN)),
            row_height,
            ui.spacing().item_spacing.y,
        );
        // The input line sits on a lighter band than the log above it, as
        // in AutoCAD, so it reads as the line being typed into.
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(panel.left(), row_rect.top() - INPUT_BAR_PAD),
            panel.right_bottom(),
        );
        ui.painter()
            .rect_filled(bar_rect, 0.0, theme.colors.panel_raised);
        if expanded {
            let mut log = ui.new_child(egui::UiBuilder::new().max_rect(log_rect).id_salt("log"));
            egui::ScrollArea::vertical()
                .max_height(log_rect.height())
                .min_scrolled_height(0.0)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(&mut log, |ui| {
                    for line in &self.state.cmdline.log {
                        ui.label(
                            egui::RichText::new(line)
                                .size(theme.font_size_small)
                                .color(theme.colors.text_weak),
                        );
                    }
                });
        }
        {
            let ui = &mut ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row_rect)
                    .id_salt("row")
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            ui.spacing_mut().item_spacing.x = 4.0;
            let (icon, tip) = if expanded {
                (
                    opendraft_ui::icons::COMMAND_LINE_COLLAPSE,
                    "Collapse the command history",
                )
            } else {
                (
                    opendraft_ui::icons::COMMAND_LINE_EXPAND,
                    "Show the command history",
                )
            };
            let chevron = ui
                .add(
                    opendraft_ui::icon_set::Icon::new(icon, 13.0)
                        .clickable()
                        .color(theme.colors.text_weak),
                )
                .on_hover_text(tip)
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if chevron.clicked() {
                self.state.cmdline.resize = Some(expanded);
            }
            if !prompt.command.is_empty() {
                ui.label(
                    egui::RichText::new(prompt.command.as_ref())
                        .strong()
                        .color(theme.colors.accent_text),
                );
            }
            // With no options the colon joins the text directly.
            let text = if prompt.options.is_empty() && !prompt.command.is_empty() {
                format!("{}:", prompt.text)
            } else {
                prompt.text.to_string()
            };
            ui.label(egui::RichText::new(text).color(theme.colors.text));
            if !prompt.options.is_empty() {
                ui.label(egui::RichText::new("[").color(theme.colors.text_weak));
                for option in &prompt.options {
                    if option_chip(ui, &theme.colors, option).clicked() {
                        chosen = Some(option.clone());
                    }
                }
                ui.label(egui::RichText::new("]").color(theme.colors.text_weak));
            }
            if !prompt.command.is_empty() && !prompt.options.is_empty() {
                ui.label(egui::RichText::new(":").color(theme.colors.text_weak));
            }
            let response = ui.add(
                egui::TextEdit::singleline(&mut input)
                    .id(input_id)
                    .frame(false)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY),
            );
            text_rect = response.rect;
            let (enter, tab, esc) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Tab),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if response.changed() {
                self.state.cmdline.selected = 0;
                self.state.cmdline.history_at = None;
                // Space acts like Enter, except inside "x, y" or "d<a",
                // or when text is being typed.
                submit |= !wants_text
                    && input.ends_with(' ')
                    && !input.contains(',')
                    && !input.contains('<');
            }
            submit |= owns_keyboard && enter;
            accept_tab = owns_keyboard && tab;
            escape = (owns_keyboard || response.has_focus()) && esc;
        }

        let suggestions = self.state.cmd_suggestions(&input, enabled);
        self.state.cmdline.listed = suggestions.len();
        let selected = self
            .state
            .cmdline
            .selected
            .min(suggestions.len().saturating_sub(1));
        let mut picked: Option<&'static str> = None;

        if accept_tab {
            if let Some(s) = suggestions.get(selected) {
                input = s.alias.to_owned();
            }
        }
        if escape {
            if input.is_empty() {
                picked = Some("edit.cancel");
            }
            input.clear();
            self.state.cmdline.history_at = None;
        }
        if submit && !escape {
            let text = std::mem::take(&mut input);
            picked = match suggestions.get(selected) {
                Some(s) => Some(s.command),
                None => self.state.cmd_submit(&text),
            };
            self.state.cmdline.focus = true;
        }
        if let Some(option) = chosen {
            self.state.cmd_choose(option);
            self.state.cmdline.focus = true;
        }

        // Suggestion list above the input row. Pressing a row takes focus from
        // the input, so the list stays up while the pointer is on it or the
        // click would never land.
        let popup_id = egui::Id::new("command_suggestions");
        let on_popup = ui.ctx().memory(|m| m.area_rect(popup_id)).is_some_and(|r| {
            ui.input(|i| i.pointer.interact_pos())
                .is_some_and(|p| r.contains(p))
        });
        let browsing = self.state.cmdline.history_at.is_some();
        if (owns_keyboard || on_popup) && !suggestions.is_empty() && !submit && !browsing {
            if let Some(command) =
                suggestion_popup(ui.ctx(), theme, &suggestions, selected, panel, text_rect)
            {
                picked = Some(command);
                input.clear();
                self.state.cmdline.focus = true;
            }
        }

        self.state.cmdline.input = input;
        if picked.is_some() {
            self.run = picked;
        }
    }
}

/// Where an option's typed key is in its label: the capitals AutoCAD writes
/// it with ("eXit", "mOde", "PAth"), else the start.
fn key_span(label: &str, key: &str) -> (usize, usize) {
    match label.find(key).filter(|_| !key.is_empty()) {
        Some(at) => (at, at + key.len()),
        None => (0, key.len().min(label.len())),
    }
}

/// An option in the prompt: its typed key is highlighted, like AutoCAD.
fn option_chip(
    ui: &mut egui::Ui,
    colors: &Colors,
    option: &crate::cmdline::CmdOption,
) -> egui::Response {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    let (from, to) = key_span(&option.label, &option.key);
    let format = |color| egui::TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let label = &option.label;
    let (before, key, after) = match (label.get(..from), label.get(from..to), label.get(to..)) {
        (Some(before), Some(key), Some(after)) => (before, key, after),
        _ => ("", "", &label[..]),
    };
    job.append(before, 0.0, format(colors.text));
    job.append(key, 0.0, format(colors.accent_text));
    job.append(after, 0.0, format(colors.text));
    ui.add(egui::Label::new(job).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The suggestion list floating above the input row. Returns the command
/// clicked, if any.
fn suggestion_popup(
    ctx: &egui::Context,
    theme: &Theme,
    suggestions: &[Suggestion],
    selected: usize,
    panel_rect: egui::Rect,
    text_rect: egui::Rect,
) -> Option<&'static str> {
    let mut clicked = None;
    egui::Area::new(egui::Id::new("command_suggestions"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(panel_rect.left(), text_rect.top() - 6.0))
        .pivot(egui::Align2::LEFT_BOTTOM)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme.colors.panel_raised)
                .inner_margin(egui::Margin::same(2))
                .show(ui, |ui| {
                    // An Area starts with the viewport's available width. Give
                    // its contents an exact width so rows cannot make the popup
                    // wider than the dock panel. Account for the frame's two
                    // 2 px horizontal margins so its outer edges line up too.
                    ui.set_width((panel_rect.width() - 4.0).max(0.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (i, s) in suggestions.iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 22.0),
                            egui::Sense::click(),
                        );
                        if i == selected {
                            ui.painter().rect_filled(rect, 2.0, theme.colors.accent);
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 2.0, theme.colors.hover);
                        }
                        let color = if i == selected {
                            theme.colors.text_on_accent
                        } else {
                            theme.colors.text
                        };
                        ui.painter().text(
                            rect.left_center() + egui::vec2(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            s.label(),
                            egui::FontId::proportional(theme.font_size_body),
                            color,
                        );
                        if response.clicked() {
                            clicked = Some(s.command);
                        }
                    }
                });
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_input_row_keeps_the_bottom_of_the_panel_at_any_height() {
        for height in [10.0, 26.0, 40.0, 74.0, 300.0] {
            let inside =
                egui::Rect::from_min_size(egui::pos2(8.0, 500.0), egui::vec2(600.0, height));
            let (log, row) = rows(inside, 26.0, 4.0);
            assert_eq!(row.bottom(), inside.bottom(), "{height}");
            assert_eq!(row.height(), 26.0, "{height}");
            // The log never runs into the row.
            assert!(log.bottom() <= row.top().max(inside.top()), "{height}");
            assert_eq!(log.top(), inside.top(), "{height}");
            assert_eq!(log.height(), (height - 30.0).max(0.0), "{height}");
        }
    }

    #[test]
    fn an_options_key_is_found_where_its_capitals_are() {
        assert_eq!(key_span("Fence", "F"), (0, 1));
        assert_eq!(key_span("mOde", "O"), (1, 2));
        assert_eq!(key_span("eXit", "X"), (1, 2));
        assert_eq!(key_span("PAth", "PA"), (0, 2));
        // A key not written in the label: its first letters, as before.
        assert_eq!(key_span("Undo", "u"), (0, 1));
    }
}
