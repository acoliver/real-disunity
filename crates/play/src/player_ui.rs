//! Retained UI replacements only when the available draws, image generations or viewport change.
#[cfg(test)]
#[path = "player_ui_order_tests.rs"]
mod tests;

use crate::{
    PlayError, offscreen::UiFrame, player_textures::PlayerTextures,
    player_ui_materials::GameMaterials,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use worldrunner_render_plan::simulation::SpriteTextures;
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_scene_viewer::RetainedUpdate;
use worldrunner_world_simulation::ui::presentation::{UiDrawKind, UiPresentation};

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
    frame: Option<UiFrame>,
    #[cfg(test)]
    pub(crate) updates: u64,
}

impl PlayerUi {
    /// The overlay for `plan`, when the draws that can be drawn, their images or the viewport
    /// changed since the last one. The draws of a game's shader are planned into runs for the UI
    /// frame ([`Self::frame`]), which draws them every frame, and the overlay shows the runs.
    pub(crate) fn update(
        &mut self,
        plan: UiPresentation,
        viewport: [u32; 2],
        textures: &SpriteTextures,
        player: &PlayerTextures,
        mut report: impl FnMut(String),
    ) -> Result<Option<RetainedUpdate>, PlayError> {
        let plan = self.available(plan, textures, player, &mut report);
        let layers = plan.layers(viewport, viewport);
        self.materials.name_too_large(&plan, &layers, report);
        self.frame = UiFrame::new(&plan, &layers);
        let generations = graphic_textures(&plan)
            .map(|id| (id, player.generation(id)))
            .collect();
        if self.plan.as_ref() == Some(&plan)
            && self.generations == generations
            && self.viewport == viewport
        {
            return Ok(None);
        }
        let overlay =
            UiOverlay::from_presentation(&plan, &textures.textures, &layers).map_err(|error| {
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

    /// The work that draws the last update's runs of UI draws, which the player records every
    /// frame because a shader may read the time.
    pub(crate) fn frame(&self) -> Option<&UiFrame> {
        self.frame.as_ref()
    }

    /// The draws with a game material in the last frame, by shader.
    pub(crate) fn game_material_draws(&self) -> &BTreeMap<String, usize> {
        self.materials.by_shader()
    }

    /// The draws of `plan` that can be drawn now. A graphic the engine draws needs its image in
    /// `textures`; a draw through a game's shader reads its image through the program. Each image
    /// left out is named once, and the draws of each game shader are counted for the report.
    fn available(
        &mut self,
        mut plan: UiPresentation,
        textures: &SpriteTextures,
        player: &PlayerTextures,
        report: &mut impl FnMut(String),
    ) -> UiPresentation {
        self.materials.count(&mut plan);
        plan.draws.retain(|draw| {
            if draw.kind != UiDrawKind::Graphic || draw.material.is_some() {
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
        plan
    }
}
