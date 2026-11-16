//! Retained UI replacements only when the available draws, image generations or viewport change.
#[cfg(test)]
#[path = "player_ui_order_tests.rs"]
mod tests;

use crate::{
    PlayError,
    player_textures::{PlayerTextures, SCREEN_IMAGE},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use worldrunner_render_plan::{
    simulation::SpriteTextures,
    ui::{UiDraw, UiDrawKind, UiPresentation},
};
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_scene_viewer::RetainedUpdate;

/// The draw of the game's own frame (the screen image): one graphic over the whole viewport, drawn
/// as the lowest of the UI so every draw the game's UI makes covers it. The overlay paints draws
/// in the order of their sorting layer, sorting order and hierarchy order, and a draw of equal
/// keys after another in the plan, so the least keys and the first place put it under all.
fn screen_draw(viewport: [u32; 2]) -> UiDraw {
    // A viewport side past 65535 pixels saturates, and no window reaches it.
    let [width, height] = viewport.map(|side| f32::from(u16::try_from(side).unwrap_or(u16::MAX)));
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [
            [0.0, 0.0, 0.0],
            [width, 0.0, 0.0],
            [width, height, 0.0],
            [0.0, height, 0.0],
        ],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        color: [1.0; 4],
        corner_colors: None,
        distance_field: None,
        texture: Some(SCREEN_IMAGE),
        sorting_layer: i32::MIN,
        sorting_order: i32::MIN,
        hierarchy_order: 0,
        clip_rect: [0, 0, viewport[0], viewport[1]],
    }
}

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
        if textures.textures.contains_key(&SCREEN_IMAGE) {
            plan.draws.insert(0, screen_draw(viewport));
        }
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
