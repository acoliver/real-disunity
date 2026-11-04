//! Retained UI replacements only when the prepared draws, image generations or viewport change.
#[cfg(test)]
#[path = "player_ui_order_tests.rs"]
mod tests;

use crate::{PlayError, player_textures::PlayerTextures};
use std::{collections::BTreeMap, sync::Arc};
use worldrunner_render_plan::{
    simulation::SpriteTextures,
    ui::{UiDrawKind, UiPresentation},
};
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_scene_viewer::RetainedUpdate;

#[derive(Default)]
pub(crate) struct PlayerUi {
    plan: Option<UiPresentation>,
    generations: BTreeMap<u64, u64>,
    viewport: [u32; 2],
    #[cfg(test)]
    pub(crate) updates: u64,
}

impl PlayerUi {
    pub(crate) fn update(
        &mut self,
        plan: UiPresentation,
        viewport: [u32; 2],
        textures: &SpriteTextures,
        player: &PlayerTextures,
    ) -> Result<Option<RetainedUpdate>, PlayError> {
        let generations = plan
            .draws
            .iter()
            .filter(|draw| draw.kind == UiDrawKind::Graphic)
            .filter_map(|draw| draw.texture)
            .map(|id| (id, player.generation(id)))
            .collect();
        if self.plan.as_ref() == Some(&plan)
            && self.generations == generations
            && self.viewport == viewport
        {
            return Ok(None);
        }
        let overlay = UiOverlay::from_presentation(&plan, &textures.textures).map_err(|error| {
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
}
