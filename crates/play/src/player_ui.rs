//! Retained UI replacements only when the available draws, image generations or viewport change.
#[cfg(test)]
#[path = "player_ui_order_tests.rs"]
mod tests;

use crate::{PlayError, gpu_player::UiOffer, player_textures::PlayerTextures};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use worldrunner_render_plan::simulation::SpriteTextures;
use worldrunner_render_scene::simulation::{UiOverlay, UiTarget};
use worldrunner_scene_viewer::RetainedUpdate;
use worldrunner_world_simulation::graphics::RenderTextureId;
use worldrunner_world_simulation::ui::{
    UiTexture,
    presentation::{UiDrawKind, UiPresentation},
};

/// The textures the UI's graphics draw, by identity.
pub(crate) fn graphic_textures(plan: &UiPresentation) -> impl Iterator<Item = u64> + '_ {
    plan.draws.iter().filter_map(|draw| match draw.texture {
        Some(UiTexture::Image(id)) if draw.kind == UiDrawKind::Graphic => Some(id),
        _ => None,
    })
}

#[derive(Default)]
pub(crate) struct PlayerUi {
    plan: Option<UiPresentation>,
    generations: BTreeMap<u64, u64>,
    viewport: [u32; 2],
    reported: BTreeSet<(UiTexture, &'static str)>,
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

    /// Keeps the draws of the render textures `offers` answer for, leaves out the others and names
    /// each of those once, and returns the textures the viewer binds. A texture no frame has
    /// written yet stays in: the viewer draws it as transparent until one does.
    pub(crate) fn offer_targets(
        &mut self,
        plan: &mut UiPresentation,
        offers: Vec<(RenderTextureId, UiOffer)>,
        mut report: impl FnMut(String),
    ) -> Vec<UiTarget> {
        let mut shown = Vec::new();
        let mut gone = BTreeMap::new();
        for (id, offer) in offers {
            match offer {
                UiOffer::Written(target) => shown.push(target),
                UiOffer::Unwritten(target) => {
                    if self.reported.insert((UiTexture::Target(id), "unwritten")) {
                        report(format!(
                            "UI render texture {id:?} shows nothing until a frame draws it"
                        ));
                    }
                    shown.push(target);
                }
                UiOffer::Unavailable(reason) => {
                    gone.insert(id, reason);
                }
            }
        }
        plan.draws.retain(|draw| {
            let Some(UiTexture::Target(id)) = draw.texture else {
                return true;
            };
            let Some(reason) = gone.get(&id) else {
                return true;
            };
            if self.reported.insert((UiTexture::Target(id), reason)) {
                report(format!("UI render texture {id:?} is left out: {reason}"));
            }
            false
        });
        shown
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
            let reason = match draw.texture {
                Some(UiTexture::Image(id)) if !textures.textures.contains_key(&id) => {
                    format!(
                        "UI image {id} is left out: {}",
                        player.unavailable_reason(id)
                    )
                }
                _ => return true,
            };
            if draw
                .texture
                .is_some_and(|texture| self.reported.insert((texture, "left out")))
            {
                report(reason);
            }
            false
        });
    }
}
