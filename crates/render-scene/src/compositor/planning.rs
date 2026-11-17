//! Sorted stock geometry planning and checked execution.
use super::{
    AttachmentView, BorrowedFeature, FrameError, FrameOutput, FrameRenderer, PassDeclaration,
    PostChain, ScreenFeature, publication, snapshots,
};

impl FrameRenderer {
    /// Camera-aware composition for stock and external features, including
    /// declaration-requested snapshots. The optional binding supplies shared shadows.
    pub fn encode_frame(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: (&[PassDeclaration], &[AttachmentView]),
        features: &mut [BorrowedFeature<'_>],
        input: (crate::frame::FrameUniform, Option<&wgpu::BindGroup>),
    ) -> Result<(), FrameError> {
        let config = self.targets().config();
        let (frame, shadow) = input;
        self.validate_environment_conventions(frame.specular_convention)?;
        let (passes, views) = plan;
        let mut passes = passes.to_vec();
        let mut views = views.to_vec();
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        let depth_mode = if passes
            .iter()
            .find(|pass| pass.slot == worldrunner_render_post::PassSlot::Opaque)
            .and_then(|pass| pass.depth.as_ref())
            .is_some_and(|depth| depth.format.has_stencil_aspect())
        {
            crate::renderer::depth::DepthMode::Shared
        } else {
            crate::renderer::depth::DepthMode::Independent
        };
        self.set_water_input(device, &frame, shadow, encoder, depth_mode)?;
        let mut combined = reborrow_features(features);
        let screen_index = combined.len();
        let mut screen = ScreenFeature::installed(self.ssao.as_ref(), self.ssr.as_ref(), &frame);
        if let Some(screen) = &mut screen {
            let declaration = screen.schedule(&mut passes, &mut views)?;
            combined.push(BorrowedFeature {
                declaration,
                prepared: screen,
            });
        }
        let water = self
            .water
            .as_ref()
            .map(|water| crate::ssr::water::Inputs::new(water, self.frame_slot));
        let mut reflection = self
            .ssr
            .as_ref()
            .map(|ssr| ssr.feature(&frame, (&passes, &views), config, water))
            .transpose()?;
        if let Some(reflection) = &mut reflection {
            let declaration = reflection.schedule(&mut passes, &mut views)?;
            combined.push(BorrowedFeature {
                declaration,
                prepared: reflection,
            });
        }
        let retained = self.chain.plan_features(&mut passes, &mut views)?;
        let (request, mut snapshot_feature) = snapshots::plan(
            device,
            (&mut self.snapshots, &mut self.overlay),
            &(config, frame, self.frame_slot, self.water.is_some()),
            (&mut passes, &mut views),
            (&combined, &retained),
        )?;
        if let Some((declaration, feature)) = &mut snapshot_feature {
            combined.push(BorrowedFeature {
                declaration: declaration.clone(),
                prepared: feature,
            });
        }
        if let Some(ssao) = &self.ssao {
            let plan = (&mut passes, &mut views);
            let owner = (combined.as_mut_slice(), screen_index);
            crate::ssao::passes::schedule_declared_reads(ssao, plan, owner, &retained)?;
        }
        if let Some(gbuffer) = &self.gbuffer {
            let declared = combined.iter().map(|feature| &feature.declaration);
            gbuffer.schedule(&mut passes, &mut views, declared.chain(&retained))?;
        }
        if let Some(opaque) = passes
            .iter()
            .find(|pass| pass.slot == worldrunner_render_post::PassSlot::Opaque)
        {
            let opaque_formats = opaque
                .additional_colors
                .iter()
                .map(|attachment| attachment.format)
                .collect::<Vec<_>>();
            for feature in &mut combined {
                for pass in &mut feature.declaration.passes {
                    if pass.slot == worldrunner_render_post::PassSlot::Opaque {
                        pass.additional_color_formats.clone_from(&opaque_formats);
                    }
                }
            }
        }
        passes.sort_by_key(|pass| pass.slot);
        encode_planned(
            &mut self.chain,
            device,
            encoder,
            (&passes, &views),
            &mut combined,
        )?;
        self.publish_snapshots(encoder, request);
        self.stage_temporal_history(encoder, &frame)
    }
    pub(super) fn publish_snapshots(
        &self,
        encoder: &wgpu::CommandEncoder,
        request: crate::water::planning::SnapshotRequest,
    ) {
        let ready = self.snapshot_ready.clone();
        let slot = self.frame_slot;
        publication::encode(encoder, &ready.clone(), move || {
            ready.borrow_mut()[slot] = request;
        })
        .expect("checked transaction encoder");
    }
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        self.publish_snapshots(encoder, crate::water::planning::SnapshotRequest::default());
        self.remember_taa_block(None);
        if self.ssao.is_some() || self.water.is_some() || self.ssr.is_some() {
            return Err(FrameError::UnsupportedEffect(
                "SSAO requires stock geometry composition",
            ));
        }
        if self.gbuffer.is_some() {
            return Err(FrameError::UnsupportedEffect(
                "the opaque G-buffer requires stock geometry composition",
            ));
        }
        self.chain.encode_geometry(
            device,
            encoder,
            &worldrunner_render_post::FramePlan {
                passes,
                views,
                requested: &[],
            },
            features,
        )
    }
}

fn reborrow_features<'a>(features: &'a mut [BorrowedFeature<'_>]) -> Vec<BorrowedFeature<'a>> {
    features
        .iter_mut()
        .map(|feature| BorrowedFeature {
            declaration: feature.declaration.clone(),
            prepared: &mut *feature.prepared,
        })
        .collect()
}

fn encode_planned(
    chain: &mut PostChain<FrameOutput>,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    plan: (&[PassDeclaration], &[AttachmentView]),
    features: &mut [BorrowedFeature<'_>],
) -> Result<(), FrameError> {
    chain.encode_geometry(
        device,
        encoder,
        &worldrunner_render_post::FramePlan {
            passes: plan.0,
            views: plan.1,
            requested: &[],
        },
        features,
    )
}
