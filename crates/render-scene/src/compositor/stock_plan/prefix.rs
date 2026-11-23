//! The plan of the depth prefix that a depth-fitted frame submits ahead of the frame proper.
use super::cache::Lookup;
use super::frame::{Authored, Environment, probe};
use super::{LogicalPrefix, PlanCache};
use crate::compositor::FrameRenderer;
use crate::scene_metadata::SceneMetadata;
use std::rc::Rc;
use worldrunner_render_post::{AttachmentView, FeaturePass, FrameError, PassSlot};

/// A prefix's schedule and the views its attachments select.
pub(crate) struct PrefixPlan {
    pub schedule: Rc<LogicalPrefix>,
    pub views: Rc<Vec<AttachmentView>>,
}

impl FrameRenderer {
    /// Plans the prefix that completes `source`'s depth and normals.
    ///
    /// # Errors
    /// When an attachment cannot be viewed.
    pub(crate) fn plan_prefix(
        &self,
        source: &SceneMetadata,
        frame: &crate::frame::FrameUniform,
    ) -> Result<PrefixPlan, FrameError> {
        let environment = Environment {
            config: self.targets().config(),
            frame: *frame,
            slot: self.frame_slot,
            ssao: Some(source),
            ssr: self.ssr.as_ref(),
            water: None,
            velocity: self.targets().velocity_texture().cloned(),
            requested: (self.ssao_requested, self.ssr_requested),
            overlay_depth: false,
            shadow: false,
        };
        let authored = Authored {
            passes: &[],
            views: &[],
            features: &[],
        };
        let probe = probe(&environment, &authored, None);
        let mut cache = self.plan_cache.borrow_mut();
        let cache: &mut PlanCache = &mut cache;
        if let Lookup::Bound {
            schedule,
            views,
            binding,
        } = cache.prefixes.lookup(&probe)
        {
            #[cfg(any(test, debug_assertions))]
            {
                let (fresh, fresh_views) = self.derive_prefix(source).expect("a cached prefix");
                assert_eq!(
                    fresh, *schedule,
                    "the cached prefix differs from a fresh build"
                );
                assert_eq!(
                    super::shape::identities(&fresh_views),
                    super::shape::identities(&views),
                    "the cached prefix bindings differ from a fresh build"
                );
            }
            cache.counters.reused(binding);
            return Ok(PrefixPlan { schedule, views });
        }
        let built = self.derive_prefix(source)?;
        let (schedule, views) = cache.prefixes.store(&mut cache.counters, probe, built);
        Ok(PrefixPlan { schedule, views })
    }

    pub(in crate::compositor) fn derive_prefix(
        &self,
        source: &SceneMetadata,
    ) -> Result<(LogicalPrefix, Vec<AttachmentView>), FrameError> {
        let mut passes = Vec::new();
        let mut views = Vec::new();
        let material = self.ssr.as_ref().map(crate::ssr::Ssr::material_target);
        crate::ssao::passes::prepass(source, material, &mut passes, &mut views)?;
        let screen = source.passes.schedule_metadata(&mut passes, &mut views)?;
        let mut prepass = FeaturePass::new(PassSlot::DepthNormalPrepass);
        prepass.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        prepass.depth_format = Some(source.depth.format());
        prepass.additional_color_formats = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::DepthNormalPrepass)
            .expect("prepass declaration")
            .additional_colors
            .iter()
            .map(|attachment| attachment.format)
            .collect();
        Ok((
            LogicalPrefix {
                passes,
                prepass,
                screen,
            },
            views,
        ))
    }
}
