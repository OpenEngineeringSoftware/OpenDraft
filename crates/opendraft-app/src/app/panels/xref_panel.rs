//! The External References palette, laid out as AutoCAD's: a toolbar, a File
//! References table (a list or a tree, with columns that sort and resize) and
//! below it a Details read-out of the selected reference, the two sections
//! sharing the height by a handle.
//!
//! Rows are built once per drawing revision (`rows.rs`); any edit or the
//! Refresh entry makes a new set.

use std::path::Path;

use eframe::egui;
use opendraft_ui::{i18n::tr, icon_set, icons, key, menu, widgets};

use super::PanelViewer;
use crate::state::AppState;

mod actions;
mod details;
mod layout;
mod rows;
mod table;

use actions::Action;
use rows::{Row, RowKey};
use table::View;

#[derive(Clone, Default)]
struct Cache {
    uid: u64,
    revision: u64,
    rows: Vec<Row>,
}

impl PanelViewer<'_> {
    pub(super) fn xrefs(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme;
        let uid = self.state.docs[self.state.active].uid;
        let revision = self.state.docs[self.state.active].session.revision();
        let cache_id = egui::Id::new(("xref-palette", uid));
        let cached = ui
            .data(|data| data.get_temp::<Cache>(cache_id))
            .filter(|cache| cache.uid == uid && cache.revision == revision);
        let cache = match cached {
            Some(cache) => cache,
            None => {
                let built = {
                    let doc = &self.state.docs[self.state.active];
                    let host_dir = doc.path.as_deref().and_then(Path::parent);
                    Cache {
                        uid,
                        revision,
                        rows: rows::rows(doc.session.document(), host_dir),
                    }
                };
                ui.data_mut(|data| data.insert_temp(cache_id, built.clone()));
                built
            }
        };
        let rows = &cache.rows;

        let selected_id = egui::Id::new(("xref-selected", uid));
        let mut selected: Option<RowKey> = ui
            .data(|data| data.get_temp::<Option<RowKey>>(selected_id))
            .flatten()
            .filter(|key| rows.iter().any(|r| r.key == *key));
        let view_id = egui::Id::new("xref-view");
        let mut view: View = ui.data(|data| data.get_temp(view_id)).unwrap_or_default();

        let selected_row = selected.and_then(|k| rows.iter().find(|r| r.key == k));
        let mut asked = layout::toolbar(ui, theme, selected_row);
        ui.add_space(4.0);

        let order = rows::order(rows, view.tree, view.sort, &view.collapsed);
        let total = ui.available_height();
        let usable = (total - layout::HANDLE).max(2.0 * layout::LEAST);
        let list_height = (usable * view.list_share).clamp(layout::LEAST, usable - layout::LEAST);

        let mut clicked = None;
        let mut tree = view.tree;
        layout::section(
            ui,
            theme,
            list_height,
            tr(key!("File References")).as_ref(),
            |head, body| {
                // Right to left: the tree toggle sits at the far end.
                let tree_on = tree;
                if widgets::icon_toggle(
                    head,
                    theme,
                    icons::XREF_PANEL_TREE,
                    tree_on,
                    &tr(key!("Tree view")),
                )
                .clicked()
                {
                    tree = true;
                }
                if widgets::icon_toggle(
                    head,
                    theme,
                    icons::XREF_PANEL_LIST,
                    !tree_on,
                    &tr(key!("List view")),
                )
                .clicked()
                {
                    tree = false;
                }
                if rows.is_empty() {
                    body.add_space(8.0);
                    body.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(tr(key!(
                                "There are no external references in this drawing."
                            )))
                            .color(theme.colors.text_weak),
                        );
                    });
                    return;
                }
                let mut context = |ui: &mut egui::Ui, row: &Row| {
                    [
                        Action::Reload,
                        Action::Unload,
                        Action::Detach,
                        Action::Path,
                        Action::Bind,
                        Action::Clip,
                    ]
                    .into_iter()
                    .find(|a| {
                        ui.add_enabled_ui(a.refused(Some(row)).is_none(), |ui| {
                            ui.horizontal(|ui| {
                                icon_set::paint(
                                    ui.painter(),
                                    ui.cursor().left_center() + egui::vec2(8.0, 0.0),
                                    egui::Align2::CENTER_CENTER,
                                    a.icon(),
                                    14.0,
                                    theme.colors.text,
                                );
                                ui.add_space(20.0);
                                menu::menu_row(ui, theme, &tr(a.label()), false)
                            })
                            .inner
                        })
                        .inner
                    })
                };
                let out = table::show(body, theme, rows, &order, &mut view, selected, &mut context);
                clicked = out.clicked;
                asked = asked.or(out.action);
            },
        );
        view.tree = tree;

        let moved = layout::handle(ui, theme);
        if moved != 0.0 {
            view.list_share = ((list_height + moved) / usable).clamp(0.1, 0.9);
        }
        let selected_row = selected.and_then(|k| rows.iter().find(|r| r.key == k));
        let details_height = (ui.available_height()).max(layout::LEAST);
        layout::section(
            ui,
            theme,
            details_height,
            tr(key!("Details")).as_ref(),
            |_, body| {
                details::show(body, theme, selected_row);
            },
        );

        if let Some(i) = clicked {
            let row = &rows[i];
            selected = Some(row.key);
            // A reference's own row picks its inserts in the drawing; one
            // inside it has none of its own.
            if !row.is_nested() {
                let ids = self.state.docs[self.state.active]
                    .session
                    .document()
                    .xref_references(row.key.block);
                self.state.select_ids(ids, false);
            }
        }
        if let Some(action) = asked {
            let row = selected.and_then(|k| rows.iter().find(|r| r.key == k));
            if action == Action::Refresh {
                ui.data_mut(|data| data.remove::<Cache>(cache_id));
                ui.ctx().request_repaint();
            } else {
                action.run(self.state, row);
            }
        }
        ui.data_mut(|data| {
            data.insert_temp(selected_id, selected);
            data.insert_temp(view_id, view);
        });
    }
}

impl AppState {
    /// Show the palette (the `xref.palette` command).
    pub(crate) fn show_xrefs_panel(&mut self) {
        self.layout_request = Some(crate::dock::LayoutRequest::Toggle(
            crate::dock::PanelTab::Xrefs,
        ));
    }
}
