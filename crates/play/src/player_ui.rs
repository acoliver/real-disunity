//! Retained UI replacements only when the available draws, image generations or viewport change.
use crate::{PlayError, player_textures::PlayerTextures};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
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
    reported: BTreeSet<u64>,
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

    fn filter_unavailable(
        &mut self,
        plan: &mut UiPresentation,
        textures: &SpriteTextures,
        player: &PlayerTextures,
        mut report: impl FnMut(String),
    ) {
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
