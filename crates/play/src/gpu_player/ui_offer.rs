//! The render textures the UI reads, as the viewer or a capture is given them.

use std::collections::BTreeSet;
use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_plan::{TextureFilter, TextureSampler, TextureWrap};
use worldrunner_render_program::sampler::{Filter, Wrap};
use worldrunner_render_scene::simulation::UiTarget;
use worldrunner_world_simulation::graphics::{RenderTextureId, WorldSampler};
use worldrunner_world_simulation::ui::{UiTexture, presentation::UiPresentation};

use super::PlayerGpu;
use crate::offscreen::sampler_of_world;

/// What the host holds of a render texture a UI draw reads.
#[derive(Clone, Debug)]
pub(crate) enum UiOffer {
    /// A frame the host built wrote the texture.
    Written(UiTarget),
    /// The texture exists in the world and no frame has drawn it yet, or the host holds no storage
    /// for it: the overlay shows transparent black.
    Unwritten(UiTarget),
    /// The draw is left out, for this reason: the world no longer holds the render texture
    /// (`destroyed`), the overlay cannot sample what the host holds (the name of the
    /// `UiTargetRefusal`), or the run has no GPU work (`no-gpu`).
    Unavailable(&'static str),
}

/// The render textures the UI's draws read.
fn target_ids(plan: &UiPresentation) -> BTreeSet<RenderTextureId> {
    plan.draws
        .iter()
        .filter_map(|draw| match draw.texture {
            Some(UiTexture::Target(id)) => Some(id),
            _ => None,
        })
        .collect()
}

impl PlayerGpu {
    /// Answers for each render texture the draws of `plan` read, with the sampler `host`'s world
    /// holds for it.
    pub(crate) fn ui_offers(
        &self,
        host: &DisunityHost,
        plan: &UiPresentation,
    ) -> Vec<(RenderTextureId, UiOffer)> {
        self.ui_textures(target_ids(plan), |id| {
            host.render_texture(id.to_bits())
                .ok()
                .map(|held| WorldSampler {
                    wrap_u: held.wrap_u,
                    wrap_v: held.wrap_v,
                    wrap_w: held.wrap_w,
                    filter: held.filter,
                    aniso_level: held.aniso_level,
                    mip_bias: held.mip_bias,
                })
        })
    }

    /// Answers for each render texture of `ids`. `describe` gives the sampler the world holds for
    /// a render texture, or `None` when the world holds none by that id, which is a texture the
    /// managed side destroyed.
    pub(crate) fn ui_textures(
        &self,
        ids: impl IntoIterator<Item = RenderTextureId>,
        mut describe: impl FnMut(RenderTextureId) -> Option<WorldSampler>,
    ) -> Vec<(RenderTextureId, UiOffer)> {
        ids.into_iter()
            .map(|id| {
                let offer = match describe(id) {
                    None => UiOffer::Unavailable("destroyed"),
                    Some(sampler) => {
                        let read = self.offscreen.borrow_mut().render_texture_for_ui(id);
                        match UiTarget::new(id, read.texture, ui_sampler(&sampler)) {
                            Ok(target) if read.written => UiOffer::Written(target),
                            Ok(target) => UiOffer::Unwritten(target),
                            Err(refusal) => UiOffer::Unavailable(refusal.reason()),
                        }
                    }
                };
                (id, offer)
            })
            .collect()
    }
}

/// The sampler the overlay uses for a render texture: the world's wrap and filter modes, read as
/// the offscreen host reads them for a draw.
fn ui_sampler(sampler: &WorldSampler) -> TextureSampler {
    let state = sampler_of_world(sampler);
    let wrap = |wrap| match wrap {
        Wrap::Repeat => TextureWrap::Repeat,
        Wrap::Clamp => TextureWrap::Clamp,
        Wrap::Mirror => TextureWrap::Mirror,
    };
    TextureSampler {
        filter: match state.filter {
            Filter::Point => TextureFilter::Point,
            Filter::Bilinear => TextureFilter::Bilinear,
            Filter::Trilinear => TextureFilter::Trilinear,
        },
        wrap_u: wrap(state.wrap_u),
        wrap_v: wrap(state.wrap_v),
    }
}

#[cfg(test)]
#[path = "ui_offer_tests.rs"]
mod tests;
