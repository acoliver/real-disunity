//! Retained UI replacements only when the available draws, image generations or viewport change.
#[cfg(test)]
#[path = "player_ui_order_tests.rs"]
mod tests;

use crate::{
    PlayError, gpu_player::UiOffer, offscreen::UiFrame, player_textures::PlayerTextures,
    player_ui_materials::GameMaterials,
};
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
    presentation::{UiDrawKind, UiLayers, UiPresentation},
};

/// The textures the UI's graphics draw, by identity.
pub(crate) fn graphic_textures(plan: &UiPresentation) -> impl Iterator<Item = u64> + '_ {
    plan.draws.iter().filter_map(|draw| match draw.texture {
        Some(UiTexture::Image(id)) if draw.kind == UiDrawKind::Graphic => Some(id),
        _ => None,
    })
}

/// The screen the UI is drawn over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Screen {
    /// The size of the screen in pixels.
    pub size: [u32; 2],
    /// The largest side a texture of the device may have, which the layer atlas grows up to.
    pub max_texture: u32,
}

impl Screen {
    fn layers(self, plan: &UiPresentation) -> UiLayers {
        plan.layers(self.size, self.max_texture)
    }
}

#[derive(Default)]
pub(crate) struct PlayerUi {
    plan: Option<UiPresentation>,
    generations: BTreeMap<u64, u64>,
    screen: Screen,
    reported: BTreeSet<(UiTexture, &'static str)>,
    materials: GameMaterials,
    /// The plan the UI frame was built from, and the frame.
    prepared: Option<UiPresentation>,
    frame: Option<UiFrame>,
    /// The render textures the last offer could not give, whose draws the next preparation leaves
    /// out at once so the frame's runs match the overlay's.
    gone: BTreeSet<RenderTextureId>,
    #[cfg(test)]
    pub(crate) updates: u64,
}

impl PlayerUi {
    /// Leaves out of `plan` the draws that cannot be drawn now and builds the UI frame of the
    /// runs of those that a game's shader draws, before the player records its frame so that the
    /// frame can carry the UI frame's work. [`Self::update`] builds the overlay from the plan
    /// after the render textures were offered, and builds the frame again only when that plan
    /// differs from this one.
    pub(crate) fn prepare(
        &mut self,
        plan: &mut UiPresentation,
        screen: Screen,
        textures: &SpriteTextures,
        player: &PlayerTextures,
        mut report: impl FnMut(String),
    ) {
        plan.draws.retain(|draw| match draw.texture {
            Some(UiTexture::Target(id)) => !self.gone.contains(&id),
            _ => true,
        });
        self.available(plan, textures, player, &mut report);
        let layers = screen.layers(plan);
        self.stage_frame(plan, &layers, report);
    }

    /// The overlay for `plan`, when the draws that can be drawn, their images or the viewport
    /// changed since the last one. The draws of a game's shader are planned into runs for the UI
    /// frame ([`Self::frame`]), which draws them every frame, and the overlay shows the runs.
    pub(crate) fn update(
        &mut self,
        mut plan: UiPresentation,
        screen: Screen,
        textures: &SpriteTextures,
        player: &PlayerTextures,
        mut report: impl FnMut(String),
    ) -> Result<Option<RetainedUpdate>, PlayError> {
        self.available(&mut plan, textures, player, &mut report);
        let layers = screen.layers(&plan);
        if self.prepared.as_ref() != Some(&plan) {
            self.stage_frame(&plan, &layers, report);
        }
        let generations = graphic_textures(&plan)
            .map(|id| (id, player.generation(id)))
            .collect();
        if self.plan.as_ref() == Some(&plan)
            && self.generations == generations
            && self.screen == screen
        {
            return Ok(None);
        }
        let overlay =
            UiOverlay::from_presentation(&plan, &textures.textures, &layers).map_err(|error| {
                PlayError::Invalid(format!("the player UI cannot be drawn: {error}"))
            })?;
        self.plan = Some(plan);
        self.generations = generations;
        self.screen = screen;
        #[cfg(test)]
        {
            self.updates += 1;
        }
        Ok(Some(RetainedUpdate::Ui(Arc::new(overlay))))
    }

    /// The work that draws the runs of the last prepared plan, which the player records every
    /// frame because a shader may read the time.
    pub(crate) fn frame(&self) -> Option<&UiFrame> {
        self.frame.as_ref()
    }

    /// The draws with a game material in the last frame, by shader.
    pub(crate) fn game_material_draws(&self) -> &BTreeMap<String, usize> {
        self.materials.by_shader()
    }

    fn stage_frame(
        &mut self,
        plan: &UiPresentation,
        layers: &UiLayers,
        report: impl FnMut(String),
    ) {
        self.materials.name_too_large(plan, layers, report);
        self.frame = UiFrame::new(plan, layers);
        self.prepared = Some(plan.clone());
    }

    /// Keeps the draws of the render textures `offers` answer for, leaves out the others and
    /// names each of those once, and returns the textures the remaining draws read for the viewer
    /// to bind. A texture no frame has written yet stays in: the viewer draws it as transparent
    /// until one does. A draw that a game's shader cannot draw, and so reads no offer, is left out
    /// first and its material named once.
    pub(crate) fn offer_targets(
        &mut self,
        plan: &mut UiPresentation,
        offers: Vec<(RenderTextureId, UiOffer)>,
        mut report: impl FnMut(String),
    ) -> Vec<UiTarget> {
        self.materials.admit(plan, &mut report);
        let read: BTreeSet<_> = plan
            .draws
            .iter()
            .filter_map(|draw| match draw.texture {
                Some(UiTexture::Target(id)) => Some(id),
                _ => None,
            })
            .collect();
        let mut shown = Vec::new();
        let mut gone = BTreeMap::new();
        for (id, offer) in offers.into_iter().filter(|(id, _)| read.contains(id)) {
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
        self.gone = gone.into_keys().collect();
        shown
    }

    /// Leaves out the draws that cannot be drawn now. A graphic the engine draws needs its image in
    /// `textures`; a draw through a game's shader reads its image through the program. Each image
    /// left out is named once, and the draws of each game shader are counted for the report.
    fn available(
        &mut self,
        plan: &mut UiPresentation,
        textures: &SpriteTextures,
        player: &PlayerTextures,
        report: &mut impl FnMut(String),
    ) {
        self.materials.admit(plan, &mut *report);
        plan.draws.retain(|draw| {
            if draw.kind != UiDrawKind::Graphic || draw.material.is_some() {
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
