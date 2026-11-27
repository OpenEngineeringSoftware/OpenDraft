//! Where the VP Freeze switch acts: the viewport being worked on in a layout,
//! a layout's sheet (its paper space, as in AutoCAD), or the Model tab.
//! Freezing in the Model tab is an OpenDraft extension (AutoCAD has no such
//! switch there): the layer is hidden in the Model tab, while layout
//! viewports still show it and plot it.

use std::borrow::Cow;

use opendraft_canvas::Space;
use opendraft_commands::SetLayouts;
use opendraft_core::{Document, Layer, LayerId, Layout, LayoutId, Viewport};

use crate::state::AppState;
use opendraft_ui::key;

/// What the VP Freeze switch freezes layers in.
#[derive(Clone, Copy, Debug)]
pub enum FreezeTarget<'a> {
    Viewport(&'a Viewport),
    /// A layout's sheet, when no viewport is being worked on.
    Paper(&'a Layout),
    Model,
}

impl FreezeTarget<'_> {
    /// The switch as `layer` shows it.
    pub fn switch(self, layer: &Layer) -> VpSwitch {
        let (frozen, place) = match self {
            FreezeTarget::Viewport(v) => (v.is_frozen(layer.id), Place::Viewport),
            FreezeTarget::Paper(l) => (l.is_paper_frozen(layer.id), Place::Paper),
            FreezeTarget::Model => (layer.frozen_in_model, Place::Model),
        };
        VpSwitch { frozen, place }
    }
}

/// Where a switch acts, for its tooltip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Viewport,
    Paper,
    Model,
}

/// One layer's VP Freeze switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VpSwitch {
    pub frozen: bool,
    pub place: Place,
}

impl VpSwitch {
    /// Its tooltip, in the Layers panel and the ribbon's layer list.
    pub fn tip(self) -> Cow<'static, str> {
        match (self.place, self.frozen) {
            (Place::Viewport, true) => rust_i18n::t!("Frozen in this viewport: click to thaw"),
            (Place::Viewport, false) => rust_i18n::t!("Click to freeze in this viewport only"),
            (Place::Paper, true) => {
                rust_i18n::t!("Frozen in this layout's paper space: click to thaw")
            }
            (Place::Paper, false) => {
                rust_i18n::t!("Click to freeze in this layout's paper space only")
            }
            (Place::Model, true) => rust_i18n::t!("Frozen in the Model tab: click to thaw"),
            (Place::Model, false) => rust_i18n::t!("Click to freeze in the Model tab only"),
        }
    }
}

/// The layout of `space` (`None`: model space) and the layers frozen there:
/// the Model tab's, a sheet's paper space's, or a viewport's.
pub fn active_space(document: &Document, space: Space) -> (Option<LayoutId>, Vec<LayerId>) {
    match space {
        Space::Model => (None, document.frozen_in_model()),
        Space::Paper(l) => {
            let frozen = document.layout(l).map(|l| l.paper_frozen.clone());
            (Some(l), frozen.unwrap_or_default())
        }
        Space::Viewport(l, v) => {
            let viewport = document.layout(l).and_then(|layout| layout.viewport(v));
            (
                None,
                viewport.map(|vp| vp.frozen.clone()).unwrap_or_default(),
            )
        }
    }
}

impl AppState {
    /// What the VP Freeze switch acts on: the Model tab, the viewport being
    /// worked on in a layout, or else the layout's sheet.
    pub fn freeze_target(&self) -> Option<FreezeTarget<'_>> {
        match self.space() {
            Space::Model => Some(FreezeTarget::Model),
            Space::Paper(id) if self.target_viewport().is_none() => self
                .session()
                .document()
                .layout(id)
                .map(FreezeTarget::Paper),
            _ => self.target_viewport_ref().map(FreezeTarget::Viewport),
        }
    }

    /// Freeze (`freeze`) or thaw layers where the VP Freeze switch acts, one
    /// undo step.
    pub fn set_layers_in_view(&mut self, layers: &[LayerId], freeze: bool) {
        match self.space() {
            Space::Model => self.set_layers_in_model(layers, freeze),
            Space::Paper(id) if self.target_viewport().is_none() => {
                self.set_layers_in_paper(id, layers, freeze);
            }
            _ => self.set_layers_in_viewport(layers, freeze),
        }
    }

    /// Freeze or thaw layers in the Model tab.
    fn set_layers_in_model(&mut self, layers: &[LayerId], freeze: bool) {
        let layers = self.not_current(layers, freeze);
        let name = if freeze {
            "Freeze layer in Model tab"
        } else {
            "Thaw layer in Model tab"
        };
        self.edit_layers(name, layers, |l| l.frozen_in_model = freeze);
    }

    /// Freeze or thaw layers on a layout's sheet.
    fn set_layers_in_paper(&mut self, id: LayoutId, layers: &[LayerId], freeze: bool) {
        let layers = self.not_current(layers, freeze);
        let mut layouts = self.session().document().layouts().to_vec();
        let Some(layout) = layouts.iter_mut().find(|l| l.id == id) else {
            return;
        };
        let before = layout.paper_frozen.clone();
        layout.set_paper_frozen(&layers, freeze);
        if layout.paper_frozen == before {
            return;
        }
        let name = if freeze {
            "Freeze layer in paper space"
        } else {
            "Thaw layer in paper space"
        };
        self.perform(SetLayouts::new(name, layouts));
    }

    /// `layers`, but not the current one when freezing: it stays thawed, as
    /// Freeze leaves it, because what is drawn on it would vanish.
    fn not_current(&mut self, layers: &[LayerId], freeze: bool) -> Vec<LayerId> {
        let current = self.session().document().current_layer();
        if freeze && layers.contains(&current) {
            self.message = key!("The current layer cannot be frozen").to_owned();
        }
        let kept = layers.iter().copied();
        kept.filter(|&id| !(freeze && id == current)).collect()
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::{Entity, Point};

    use super::*;
    use crate::layers::LayerEdit;

    fn line() -> Entity {
        Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(10000.0, 5000.0),
        }
    }

    /// A line on a new layer, with layer 0 current again.
    fn drawing() -> (AppState, LayerId) {
        let mut state = AppState::new();
        let layer = state.new_layer();
        add_on(&mut state, layer);
        (state, layer)
    }

    /// Add a line on `layer` in the space being edited.
    fn add_on(state: &mut AppState, layer: LayerId) {
        state.set_current_layer(layer);
        state.add_entity(line());
        state.set_current_layer(LayerId::DEFAULT);
    }

    fn freeze(state: &mut AppState, layers: &[LayerId], freeze: bool) {
        state.apply_layer_edit(LayerEdit::SetInViewport(layers.to_vec(), freeze));
        // What the next frame does.
        state.docs[state.active].keep_space_valid();
    }

    fn layer(state: &AppState, id: LayerId) -> Layer {
        state.session().document().layer(id).unwrap().clone()
    }

    fn pickable(state: &AppState) -> usize {
        state.session().document().selectable_objects().count()
    }

    fn enter(state: &mut AppState, space: Space) {
        match space {
            Space::Paper(id) => state.enter_layout(id),
            _ => state.enter_model(),
        }
        state.docs[state.active].keep_space_valid();
    }

    #[test]
    fn in_the_model_tab_the_switch_freezes_there_and_undoes() {
        let (mut state, id) = drawing();
        let target = state.freeze_target().unwrap();
        assert!(matches!(target, FreezeTarget::Model));
        assert_eq!(
            target.switch(&layer(&state, id)),
            VpSwitch {
                frozen: false,
                place: Place::Model
            }
        );
        state.select_all();
        freeze(&mut state, &[id], true);
        let frozen = layer(&state, id);
        assert!(frozen.frozen_in_model && !frozen.frozen && frozen.on);
        assert_eq!(pickable(&state), 0, "hidden from picking and snapping");
        assert!(state.selection().is_empty(), "it leaves the selection");
        assert!(state.session().document().bounds().is_none(), "nor in ZE");
        state.undo();
        state.docs[state.active].keep_space_valid();
        assert!(!layer(&state, id).frozen_in_model);
        assert_eq!(pickable(&state), 1);
    }

    #[test]
    fn the_current_layer_stays_thawed_in_the_model_tab() {
        let (mut state, id) = drawing();
        freeze(&mut state, &[LayerId::DEFAULT, id], true);
        assert_eq!(state.message, "The current layer cannot be frozen");
        assert!(!layer(&state, LayerId::DEFAULT).frozen_in_model);
        assert!(layer(&state, id).frozen_in_model);
    }

    #[test]
    fn viewports_still_show_a_layer_frozen_in_the_model_tab() {
        let (mut state, id) = drawing();
        freeze(&mut state, &[id], true);
        state.new_layout();
        state.mspace();
        let target = state.freeze_target().unwrap();
        assert!(matches!(target, FreezeTarget::Viewport(_)));
        assert!(!target.switch(&layer(&state, id)).frozen);
        state.docs[state.active].keep_space_valid();
        assert_eq!(pickable(&state), 1, "shown in the viewport");
        // There the switch acts on the viewport, not on the Model tab.
        freeze(&mut state, &[id], true);
        assert_eq!(pickable(&state), 0);
        freeze(&mut state, &[id], false);
        assert_eq!(pickable(&state), 1);
        assert!(layer(&state, id).frozen_in_model);
        state.enter_model();
        assert_eq!(pickable(&state), 0, "still frozen in the Model tab");
    }

    #[test]
    fn on_a_sheet_the_switch_freezes_that_layout_s_paper_space_only() {
        let (mut state, id) = drawing();
        state.new_layout();
        let first = state.space();
        add_on(&mut state, id);
        state.new_layout();
        let second = state.space();
        add_on(&mut state, id);
        enter(&mut state, first);
        let target = state.freeze_target().unwrap();
        assert!(matches!(target, FreezeTarget::Paper(_)));
        assert_eq!(target.switch(&layer(&state, id)).place, Place::Paper);
        assert_eq!(pickable(&state), 1);
        freeze(&mut state, &[id], true);
        assert_eq!(pickable(&state), 0, "hidden on this sheet");
        assert!(
            state
                .freeze_target()
                .unwrap()
                .switch(&layer(&state, id))
                .frozen
        );
        let layer_flags = layer(&state, id);
        assert!(!layer_flags.frozen && !layer_flags.frozen_in_model);
        enter(&mut state, second);
        assert_eq!(pickable(&state), 1, "other layouts still show it");
        enter(&mut state, Space::Model);
        assert_eq!(pickable(&state), 1, "and so does the model");
        enter(&mut state, first);
        state.mspace();
        state.docs[state.active].keep_space_valid();
        assert_eq!(pickable(&state), 1, "and the layout's viewport");
        state.undo();
        state.pspace();
        state.docs[state.active].keep_space_valid();
        assert_eq!(pickable(&state), 1, "undone");
    }

    #[test]
    fn the_tooltips_say_where_the_switch_acts() {
        let tip = |frozen, place| VpSwitch { frozen, place }.tip();
        assert_eq!(
            tip(false, Place::Model),
            "Click to freeze in the Model tab only"
        );
        assert_eq!(
            tip(true, Place::Model),
            "Frozen in the Model tab: click to thaw"
        );
        assert_eq!(
            tip(false, Place::Viewport),
            "Click to freeze in this viewport only"
        );
        assert_eq!(
            tip(true, Place::Viewport),
            "Frozen in this viewport: click to thaw"
        );
        assert_eq!(
            tip(false, Place::Paper),
            "Click to freeze in this layout's paper space only"
        );
    }
}
