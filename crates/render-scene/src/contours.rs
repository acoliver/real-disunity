//! Visible-object ID targets, isolated by admitted frame slot. No colour composite.
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeatureDeclaration, FeaturePass,
    FrameError, PassDeclaration, PassEntry, PassSlot, ResourceRef,
};

pub(crate) mod capture;
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Uint;
pub(crate) const RESOURCE: ResourceRef = ResourceRef::new("visible contour IDs");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Work {
    Absent,
    Remove,
    Draw,
}

fn work(members: usize, retained: bool) -> Work {
    match (members, retained) {
        (0, false) => Work::Absent,
        (0, true) => Work::Remove,
        _ => Work::Draw,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContourStatistics {
    pub passes: usize,
    pub targets: usize,
    pub samples: u32,
}

pub(crate) struct Targets {
    admitted: bool,
    pending: [Option<wgpu::Texture>; 3],
    pub(crate) published: Rc<RefCell<[Option<wgpu::Texture>; 3]>>,
    encoded: Option<(worldrunner_render_gpu::submission::Publications, usize)>,
}

impl Default for Targets {
    fn default() -> Self {
        Self::reserved(true)
    }
}

impl Targets {
    pub(crate) fn reserved(admitted: bool) -> Self {
        Self {
            admitted,
            pending: Default::default(),
            published: Rc::default(),
            encoded: None,
        }
    }

    pub(crate) fn target(&self, slot: usize) -> Option<wgpu::Texture> {
        self.published.borrow()[slot].clone()
    }

    pub(crate) fn capture_source(
        &self,
        encoder: &wgpu::CommandEncoder,
        slot: usize,
    ) -> Result<Option<wgpu::Texture>, FrameError> {
        let owner = worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        if self
            .encoded
            .as_ref()
            .is_some_and(|(encoded, encoded_slot)| {
                *encoded_slot == slot && owner.same_transaction(encoded)
            })
        {
            Ok(self.pending[slot].clone())
        } else {
            Ok(self.target(slot))
        }
    }

    pub(crate) fn validate(
        &self,
        members: usize,
        features: wgpu::TextureFormatFeatures,
        passes: &[PassDeclaration],
    ) -> Result<(), FrameError> {
        if members == 0 {
            return Ok(());
        }
        if !self.admitted {
            return Err(FrameError::UnsupportedEffect(
                "contour ID targets are not reserved in RendererConfiguration",
            ));
        }
        let depth = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::Opaque)
            .and_then(|pass| pass.depth.as_ref())
            .ok_or(FrameError::AttachmentContract(PassEntry::new(
                PassSlot::ContourIds,
            )))?;
        validate_format(features, depth.samples)
    }

    pub(crate) fn provision(
        &mut self,
        device: &wgpu::Device,
        slot: usize,
        members: usize,
        depth: Option<&AttachmentDeclaration>,
    ) -> Option<wgpu::Texture> {
        if members == 0 {
            return None;
        }
        let depth = depth.expect("validated opaque depth");
        if self.pending[slot].as_ref().is_none_or(|texture| {
            [texture.width(), texture.height()] != depth.size
                || texture.sample_count() != depth.samples
        }) {
            self.pending[slot] = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some(RESOURCE.name),
                size: wgpu::Extent3d {
                    width: depth.size[0],
                    height: depth.size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: depth.samples,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
        }
        self.pending[slot].clone()
    }

    pub(crate) fn schedule(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        input: (usize, usize),
        plan: (
            &mut Vec<PassDeclaration>,
            &mut Vec<AttachmentView>,
            &mut FeatureDeclaration,
        ),
    ) -> Result<(), FrameError> {
        let (slot, members) = input;
        let (passes, views, declaration) = plan;
        let owner = worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        let retained = self.pending[slot].is_some() || self.published.borrow()[slot].is_some();
        match work(members, retained) {
            Work::Absent => return Ok(()),
            Work::Remove => self.pending[slot] = None,
            Work::Draw => {
                let depth = passes
                    .iter()
                    .find(|pass| pass.slot == PassSlot::Opaque)
                    .and_then(|pass| pass.depth.as_ref())
                    .expect("validated opaque depth");
                let texture = self.pending[slot].as_ref().expect("active contour target");
                views.push(AttachmentView::new(RESOURCE, texture)?);
                let pass = pass(depth);
                let mut feature = FeaturePass::new(PassSlot::ContourIds);
                feature.color_format = Some(FORMAT);
                feature.depth_format = pass.depth.as_ref().map(|depth| depth.format);
                feature.samples = texture.sample_count();
                declaration.passes.push(feature);
                passes.push(pass);
            }
        }
        let texture = self.pending[slot].clone();
        let published = self.published.clone();
        crate::compositor::publication::encode(encoder, &published.clone(), move || {
            published.borrow_mut()[slot] = texture;
        })?;
        self.encoded = Some((owner, slot));
        Ok(())
    }
}

pub(crate) fn validate_format(
    features: wgpu::TextureFormatFeatures,
    samples: u32,
) -> Result<(), FrameError> {
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    if !features.allowed_usages.contains(usage) || !features.flags.sample_count_supported(samples) {
        return Err(FrameError::FeaturePreparation {
            feature: "contour IDs",
            reason: format!("R8Uint does not support render/sample usage at {samples} samples"),
        });
    }
    Ok(())
}

fn pass(depth: &AttachmentDeclaration) -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::ContourIds);
    pass.color = Some(AttachmentDeclaration {
        resource: RESOURCE,
        format: FORMAT,
        size: depth.size,
        samples: depth.samples,
        load: AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    pass.depth = Some(AttachmentDeclaration {
        load: AttachmentLoad::Load,
        store: wgpu::StoreOp::Store,
        read_only: true,
        ..depth.clone()
    });
    pass
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contour_empty_membership_omits_render_work_including_active_to_empty() {
        assert_eq!(work(0, false), Work::Absent);
        assert_eq!(work(1, false), Work::Draw);
        assert_eq!(work(1, true), Work::Draw);
        assert_eq!(work(0, true), Work::Remove);
        assert_eq!(work(0, false), Work::Absent);
    }

    #[test]
    fn contour_admission_is_cpu_only_and_empty_membership_needs_no_resources() {
        let features = FORMAT.guaranteed_format_features(wgpu::Features::empty());
        let targets = Targets::reserved(false);
        assert!(targets.validate(0, features, &[]).is_ok());
        assert!(matches!(
            targets.validate(1, features, &[]),
            Err(FrameError::UnsupportedEffect(_))
        ));
        assert!(targets.pending.iter().all(Option::is_none));
        assert!(targets.published.borrow().iter().all(Option::is_none));
        assert!(targets.encoded.is_none());

        let targets = Targets::reserved(true);
        let mut opaque = PassDeclaration::new(PassSlot::Opaque);
        opaque.depth = Some(AttachmentDeclaration {
            resource: ResourceRef::new("scene depth"),
            format: wgpu::TextureFormat::Depth32Float,
            size: [32, 24],
            samples: 3,
            load: AttachmentLoad::ClearDepth(1.0),
            store: wgpu::StoreOp::Store,
            read_only: false,
        });
        assert!(
            targets
                .validate(1, features, std::slice::from_ref(&opaque))
                .is_err()
        );
        opaque.depth.as_mut().unwrap().samples = 4;
        assert!(targets.validate(1, features, &[opaque]).is_ok());
        assert!(targets.validate(0, features, &[]).is_ok());
        assert!(targets.pending.iter().all(Option::is_none));
        assert!(targets.published.borrow().iter().all(Option::is_none));
    }

    #[test]
    fn contour_pass_follows_completed_opaque_depth() {
        let depth = AttachmentDeclaration {
            resource: ResourceRef::new("scene depth"),
            format: wgpu::TextureFormat::Depth32Float,
            size: [32, 24],
            samples: 4,
            load: AttachmentLoad::ClearDepth(1.0),
            store: wgpu::StoreOp::Store,
            read_only: false,
        };
        let mut passes = [PassDeclaration::new(PassSlot::Opaque), pass(&depth)];
        passes.sort_by_key(|pass| pass.slot);
        let contour = passes
            .iter()
            .position(|pass| pass.slot == PassSlot::ContourIds)
            .unwrap();
        assert_eq!(passes[contour - 1].slot, PassSlot::Opaque);
        let read = passes[contour].depth.as_ref().unwrap();
        assert!(read.read_only);
        assert_eq!(read.load, AttachmentLoad::Load);
        assert_eq!(read.store, wgpu::StoreOp::Store);
        assert_eq!(read.resource, depth.resource);
        assert_eq!(
            passes[contour].color.as_ref().unwrap().samples,
            depth.samples
        );
    }

    #[test]
    fn gpu_contour_provisioning_reuses_slots_and_defers_publication() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let context = crate::device::GpuContext::shared_for_test().unwrap();
        let device = context.device();
        let mut targets = Targets::default();
        assert!(targets.provision(device, 2, 0, None).is_none());
        assert!(targets.pending.iter().all(Option::is_none));
        assert!(targets.published.borrow().iter().all(Option::is_none));
        assert!(targets.encoded.is_none());
        let mut depth = AttachmentDeclaration {
            resource: ResourceRef::new("scene depth"),
            format: wgpu::TextureFormat::Depth32Float,
            size: [32, 24],
            samples: 1,
            load: AttachmentLoad::ClearDepth(1.0),
            store: wgpu::StoreOp::Store,
            read_only: false,
        };
        let first = targets.provision(device, 1, 1, Some(&depth)).unwrap();
        assert_eq!(
            targets.provision(device, 1, 1, Some(&depth)),
            Some(first.clone())
        );
        assert!(targets.target(1).is_none());
        assert!(targets.encoded.is_none());
        let other = targets.provision(device, 0, 1, Some(&depth)).unwrap();
        assert_ne!(first, other);
        let mut opaque = PassDeclaration::new(PassSlot::Opaque);
        opaque.depth = Some(depth.clone());
        let mut passes = vec![opaque];
        let mut views = Vec::new();
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        let encoder = context.submission().begin(device).unwrap();
        targets
            .schedule(
                &encoder,
                (1, 1),
                (&mut passes, &mut views, &mut declaration),
            )
            .unwrap();
        assert_eq!(views[0].texture(), &first);
        assert!(targets.target(1).is_none());
        drop(encoder);
        assert!(targets.target(1).is_none());
        let encoder = context.submission().begin(device).unwrap();
        targets
            .schedule(
                &encoder,
                (1, 1),
                (&mut passes, &mut views, &mut declaration),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
        assert_eq!(targets.target(1), Some(first.clone()));
        depth.size = [48, 36];
        let resized = targets.provision(device, 1, 1, Some(&depth)).unwrap();
        assert_ne!(resized, first);
        assert_eq!([resized.width(), resized.height()], depth.size);
        passes[0].depth = Some(depth);
        let encoder = context.submission().begin(device).unwrap();
        targets
            .schedule(
                &encoder,
                (1, 1),
                (&mut passes, &mut views, &mut declaration),
            )
            .unwrap();
        drop(encoder);
        assert_eq!(targets.target(1), Some(first));
        assert!(targets.target(0).is_none());
        assert!(targets.target(2).is_none());
    }

    #[test]
    fn contour_format_admission_rejects_unsupported_counts() {
        let features = FORMAT.guaranteed_format_features(wgpu::Features::empty());
        assert!(validate_format(features, 1).is_ok());
        assert!(validate_format(features, 4).is_ok());
        for samples in [0, 3, 16] {
            assert!(validate_format(features, samples).is_err());
        }
    }
}
