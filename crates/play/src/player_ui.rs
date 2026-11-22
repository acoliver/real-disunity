//! Retained UI replacements only when the available draws, image generations or viewport change.
#[cfg(test)]
#[path = "player_ui_order_tests.rs"]
mod tests;

use crate::{PlayError, player_textures::PlayerTextures, player_ui_materials::GameMaterials};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use worldrunner_render_plan::simulation::SpriteTextures;
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_scene_viewer::RetainedUpdate;
use worldrunner_world_simulation::ui::presentation::{UiDrawKind, UiLayers, UiPresentation};

/// The textures the UI's graphics draw, by identity.
pub(crate) fn graphic_textures(plan: &UiPresentation) -> impl Iterator<Item = u64> + '_ {
    plan.draws
        .iter()
        .filter(|draw| draw.kind == UiDrawKind::Graphic)
        .filter_map(|draw| draw.texture)
}

#[derive(Default)]
pub(crate) struct PlayerUi {
    plan: Option<UiPresentation>,
    generations: BTreeMap<u64, u64>,
    viewport: [u32; 2],
    reported: BTreeSet<u64>,
    materials: GameMaterials,
    #[cfg(test)]
    pub(crate) updates: u64,
}

impl PlayerUi {
    pub(crate) fn update(
        &mut self,
        mut plan: UiPresentation,
        viewport: [u32; 2],
        textures: &SpriteTextures,
        player: &PlayerTextures,
        report: impl FnMut(String),
    ) -> Result<Option<RetainedUpdate>, PlayError> {
        self.filter_unavailable(&mut plan, textures, player, report);
        let generations = graphic_textures(&plan)
            .map(|id| (id, player.generation(id)))
            .collect();
        if self.plan.as_ref() == Some(&plan)
            && self.generations == generations
            && self.viewport == viewport
        {
            return Ok(None);
        }
        let overlay = UiOverlay::from_presentation(&plan, &textures.textures, &UiLayers::default()).map_err(|error| {
            PlayError::Invalid(format!("the player UI cannot be drawn: {error}"))
        })?;
        self.plan = Some(plan);
        self.generations = generations;
        self.viewport = viewport;
        #[cfg(test)]
        {
            self.updates += 1;
        }
        Ok(Some(RetainedUpdate::Ui(Arc::new(overlay))))
    }

    /// The draws the last update left out because their material is a game's own, by shader.
    pub(crate) fn game_material_draws(&self) -> &BTreeMap<String, usize> {
        self.materials.by_shader()
    }

    fn filter_unavailable(
        &mut self,
        plan: &mut UiPresentation,
        textures: &SpriteTextures,
        player: &PlayerTextures,
        mut report: impl FnMut(String),
    ) {
        self.materials.leave_out(plan, &mut report);
        plan.draws.retain(|draw| {
            if draw.kind != UiDrawKind::Graphic {
                return true;
            }
            let Some(id) = draw
                .texture
                .filter(|id| !textures.textures.contains_key(id))
            else {
                return true;
            };
            if self.reported.insert(id) {
                report(format!(
                    "UI image {id} is left out: {}",
                    player.unavailable_reason(id)
                ));
            }
            false
        });
    }
}
