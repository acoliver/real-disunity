//! Depth fitting without SSAO or SSR still draws the prepass; Opaque must reuse its depth.

use super::*;
use crate::compositor::FrameRenderer;
use crate::draw::PreparedScene;
use crate::renderer::depth::DepthMode;
use crate::shadows::ShadowedTarget;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_post::{Readback, ToneMap};

fn fitting_source() -> RenderScene {
    let base = scene(&[[0, 0, 1000], [400, 500, 800]], 600_000, None);
    let effects = SceneEffects::new(fixture::effects_parts(600_000))
        .unwrap()
        .with_depth_fitting(SceneDepthFitting {
            near_step_mm: 100,
            far_step_mm: 1000,
            minimum_span_mm: 1000,
            minimum_span_policy: SceneDepthMinimumSpan::ExtendNear,
            active_maximum_mm: None,
            splits: SceneDepthSplits::Practical {
                lambda_micro: 500_000,
            },
            adoption: SceneDepthAdoption::FrameIndexed {
                latency_frames: 2,
                contraction_frames: 0,
            },
        })
        .unwrap();
    assert!(effects.ssao().is_none() && effects.ssr().is_none());
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

/// Renders one same-frame fitted capture and returns the scene colour. `independent` gives the
/// frame its own Opaque depth, the reference the paired depth must reproduce.
fn fitted_color(context: &GpuContext, independent: bool) -> Vec<u8> {
    let source = fitting_source();
    let size = [view().viewport().width, view().viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    if independent {
        frame.independent_depth_for_test(context.device());
    }
    let mut renderer = ShadowedScene::new(
        context,
        PreparedScene::new(&source).unwrap(),
        settings(),
        ShadowedTarget::new(wgpu::TextureFormat::Rgba16Float, 1),
    )
    .unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let metadata = frame
        .ssao
        .as_ref()
        .expect("depth fitting installs the frame's prepass metadata");
    assert_eq!(metadata.depth == *depth.texture(), !independent);
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_host(
            context,
            &view(),
            (&color, &depth),
            (Some(0), false, &[]),
            &mut frame,
        )
        .unwrap()
        .expect("a same-frame capture adopts its own fit");
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
        for slot in [
            worldrunner_render_post::PassSlot::DepthNormalPrepass,
            worldrunner_render_post::PassSlot::DepthPyramid(0),
        ] {
            assert_eq!(
                passes.iter().filter(|&&pass| pass == slot).count(),
                1,
                "duplicate or missing producer: {slot:?}"
            );
        }
    });
    assert_eq!(
        frame.forward_depth(&renderer.gpu, &depth),
        if independent {
            DepthMode::Independent
        } else {
            DepthMode::Reuse
        }
    );
    let mut readback = Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        16 * 1024 * 1024,
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    readback.encode(&mut encoder, color.texture()).unwrap();
    let submission = encoder.finish().submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}

#[test]
fn gpu_depth_fitting_without_screen_effects_reuses_its_prepass_depth_in_opaque() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let reuse = fitted_color(&context, false);
    assert!(reuse.chunks_exact(8).any(|pixel| pixel[..6] != [0; 6]));
    assert!(
        reuse == fitted_color(&context, true),
        "paired depth must shade what independent depth shades"
    );
}

/// One same-frame fitted capture with an ambient term replay, the passes it ran and, for the
/// computed reference, its filtered occlusion.
struct Fitted {
    color: Vec<u8>,
    terms: Vec<Vec<u8>>,
    passes: Vec<PassSlot>,
    fit: crate::shadows::sdsm::AdoptedFit,
    /// The computed filtered occlusion (`R32Float`) and the prepass normals (`Rgba16Float`).
    occlusion: Option<(Vec<u8>, Vec<u8>)>,
    /// The visible contour IDs (`R8Uint`), when an object carries one.
    contours: Option<Vec<u8>>,
}

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(
            [texture.width(), texture.height()],
            texture.format(),
            1,
        ),
        16 * 1024 * 1024,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

/// Reads a single-sample `R8Uint` image, one byte per texel, row by row.
fn read_ids(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    assert_eq!(
        (texture.format(), texture.sample_count()),
        (wgpu::TextureFormat::R8Uint, 1)
    );
    let row = texture.width() as usize;
    let stride = row.div_ceil(256) * 256;
    let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("contour ID readback"),
        size: (stride * texture.height() as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(u32::try_from(stride).unwrap()),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    context.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("the readback buffer maps");
    });
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .expect("the queue drains");
    let bytes = slice
        .get_mapped_range()
        .expect("mapped after the queue drained")
        .chunks_exact(stride)
        .flat_map(|line| line[..row].iter().copied())
        .collect();
    buffer.unmap();
    bytes
}

/// Renders the depth-fitting-only frame. `computed` gives it the metadata such frames had
/// before: neutral occlusion computed by the kernel and filter over the whole depth hierarchy.
fn depth_fitted_frame(context: &GpuContext, source: &RenderScene, computed: bool) -> Fitted {
    use crate::compositor::FeatureTarget;
    use crate::compositor::terms::{CaptureTerm, TermCapture};
    let size = [view().viewport().width, view().viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let screen = frame.ssao.as_ref().expect("depth fitting owns the prepass");
    assert!(!screen.passes.computes_occlusion());
    assert_eq!(
        [
            screen.passes.filtered.width(),
            screen.passes.filtered.height()
        ],
        [1, 1]
    );
    assert!(
        frame
            .feature_target(FeatureTarget::AmbientOcclusion)
            .is_none()
    );
    if computed {
        frame.computed_neutral_occlusion_for_test(context.device());
        assert!(
            frame
                .feature_target(FeatureTarget::AmbientOcclusion)
                .is_some()
        );
    }
    let mut renderer = ShadowedScene::new(
        context,
        PreparedScene::new(source).unwrap(),
        settings(),
        ShadowedTarget::new(wgpu::TextureFormat::Rgba16Float, 1),
    )
    .unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let terms = [CaptureTerm::AmbientDiffuse, CaptureTerm::DirectDiffuse]
        .map(|term| TermCapture::new(context.device(), size, term));
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_host(
            context,
            &view(),
            (&color, &depth),
            (Some(0), true, &terms),
            &mut frame,
        )
        .unwrap()
        .expect("a same-frame capture adopts its own fit");
    let passes = crate::scene_metadata::EXECUTED_PASSES.with_borrow(Clone::clone);
    assert_eq!(frame.forward_depth(&renderer.gpu, &depth), DepthMode::Reuse);
    let occlusion = computed.then(|| {
        let screen = frame.ssao.as_ref().expect("metadata");
        (
            read(context, &screen.passes.filtered),
            read(context, &screen.normals),
        )
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let contours = frame
        .encode_contour_capture(context.device(), &mut encoder)
        .unwrap();
    encoder.finish().submit().unwrap();
    Fitted {
        contours: contours.map(|ids| read_ids(context, &ids)),
        color: read(context, color.texture()),
        terms: terms
            .iter()
            .map(|term| read(context, &term.texture))
            .collect(),
        passes,
        fit,
        occlusion,
    }
}

/// The stages a depth-fitting-only frame no longer runs: nothing reads them there.
fn occlusion_stage(slot: PassSlot) -> bool {
    matches!(
        slot,
        PassSlot::Ssao
            | PassSlot::SsaoBlur
            | PassSlot::SsaoBlurHorizontal
            | PassSlot::OcclusionMetadata(_)
    ) || matches!(slot, PassSlot::DepthPyramid(level) if level > 0)
}

/// A frame whose prepass only shadow depth fitting reads skips the occlusion kernel, its filter
/// and the depth hierarchy past level 0, and binds a constant neutral occlusion instead. It must
/// shade, replay its ambient term and fit its cascades exactly as the frame that computed them.
#[test]
fn gpu_depth_fitting_alone_skips_unread_occlusion_stages_with_identical_results() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = fitting_source();
    let skipped = depth_fitted_frame(&context, &source, false);
    let reference = depth_fitted_frame(&context, &source, true);
    assert!(skipped.contours.is_none() && reference.contours.is_none());
    // Receivers read the occlusion at their own covered pixel, where the computed neutral
    // occlusion is exactly 1.0, the value of the constant bound instead. Uncovered texels may sit
    // one ulp below 1.0 (the filter's division; 72 of them on Metal), which nothing reads.
    let (occlusion, normals) = reference.occlusion.as_ref().expect("computed occlusion");
    let (mut covered, mut covered_other, mut largest) = (0usize, 0usize, 0.0f32);
    for (texel, normal) in occlusion.chunks_exact(4).zip(normals.chunks_exact(8)) {
        let value = f32::from_le_bytes(texel.try_into().expect("R32Float texel"));
        let is_covered = normal[6..8] != [0, 0];
        covered += usize::from(is_covered);
        if is_covered && value.to_bits() != 1.0f32.to_bits() {
            covered_other += 1;
        }
        largest = largest.max((value - 1.0).abs());
    }
    assert!(covered > 0, "the fixture covers pixels");
    assert_eq!(covered_other, 0, "covered occlusion texels other than 1.0");
    assert!(
        largest <= f32::EPSILON,
        "neutral occlusion off by {largest:e}"
    );
    for slot in [
        PassSlot::DepthNormalPrepass,
        PassSlot::DepthPyramid(0),
        PassSlot::Opaque,
    ] {
        assert_eq!(
            skipped.passes.iter().filter(|&&pass| pass == slot).count(),
            1,
            "{slot:?}"
        );
    }
    assert!(!skipped.passes.iter().copied().any(occlusion_stage));
    for slot in [
        PassSlot::DepthPyramid(1),
        PassSlot::Ssao,
        PassSlot::SsaoBlur,
    ] {
        assert!(reference.passes.contains(&slot), "reference ran {slot:?}");
    }
    let kept: Vec<_> = reference
        .passes
        .iter()
        .copied()
        .filter(|&slot| !occlusion_stage(slot))
        .collect();
    assert_eq!(skipped.passes, kept, "no other pass changed");
    assert!(
        skipped
            .color
            .chunks_exact(8)
            .any(|pixel| pixel[..6] != [0; 6])
    );
    assert!(skipped.color == reference.color, "scene colour differs");
    assert!(
        skipped.terms[0]
            .chunks_exact(8)
            .any(|pixel| pixel[..6] != [0; 6]),
        "the ambient term samples the occlusion and must not be empty"
    );
    for (index, (term, expected)) in skipped.terms.iter().zip(&reference.terms).enumerate() {
        assert!(term == expected, "term {index} differs");
    }
    assert_eq!(skipped.fit, reference.fit);
}

/// Frames that sample occlusion keep computing it when depth fitting also draws the prepass:
/// SSAO receivers, and reflections' application occlusion.
#[test]
fn gpu_depth_fitting_with_ssao_or_reflections_still_computes_occlusion() {
    use crate::compositor::FeatureTarget;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let fitting = *fitting_source().effects().unwrap().depth_fitting().unwrap();
    let ssao = SceneSsao {
        intensity_micro: 1_000_000,
        ..crate::ssr::neutral_ao()
    };
    let with_ssao = SceneEffects::new(SceneEffectsParts {
        ssao: Some(ssao),
        ..fixture::effects_parts(600_000)
    })
    .unwrap()
    .with_depth_fitting(fitting)
    .unwrap();
    let with_ssr = SceneEffects::new(fixture::effects_parts(600_000))
        .unwrap()
        .with_depth_fitting(fitting)
        .unwrap()
        .with_ssr(SceneSsr {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    for (effects, settings) in [(with_ssao, ssao), (with_ssr, crate::ssr::neutral_ao())] {
        let frame = FrameRenderer::hdr(
            &context,
            [64, 48],
            OffscreenTarget::COLOR_FORMAT,
            1,
            ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
        let screen = frame.ssao.as_ref().expect("prepass metadata");
        assert!(screen.passes.computes_occlusion());
        assert_eq!(screen.settings, settings);
        assert!(screen.passes.depth_levels().len() > 1);
        assert!(
            frame
                .feature_target(FeatureTarget::AmbientOcclusion)
                .is_some()
        );
    }
}

/// A caller feature that declares `reads` in `ScreenSpaceAfterTransparent`, records the extent
/// of each read during preparation and writes nothing to its own output.
struct DeclaredReader {
    reads: Vec<ResourceRef>,
    extents: std::rc::Rc<std::cell::RefCell<Vec<[u32; 2]>>>,
}
impl worldrunner_render_post::PreparedFeature for DeclaredReader {
    fn prepare(
        &mut self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        for &resource in &self.reads {
            let extent = context.resource(resource)?.size();
            self.extents.borrow_mut().push(extent);
        }
        Ok(())
    }
    fn draw(&self, _: &worldrunner_render_post::DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

/// What one depth-fitting-only `encode_frame` with a declared caller reader produced.
struct DeclaredFrame {
    result: Result<(), worldrunner_render_post::FrameError>,
    extents: Vec<[u32; 2]>,
    passes: Vec<PassSlot>,
}

/// Encodes an 8x8 depth-fitting-only HDR frame through the public `encode_frame` with a
/// cleared Opaque pass and a caller feature that reads `reads` into an output of its own.
fn encode_with_declared_reader(context: &GpuContext, reads: &[ResourceRef]) -> DeclaredFrame {
    use worldrunner_render_post::{
        BorrowedFeature, FeatureDeclaration, FeaturePass, PassDeclaration,
    };
    let source = fitting_source();
    let size = [8, 8];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let screen = frame.ssao.as_ref().expect("depth fitting owns the prepass");
    assert!(!screen.passes.computes_occlusion());
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let target =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
    let output = AttachmentView::new(ResourceRef::new("declared reader output"), &target).unwrap();
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(crate::compositor::attachment(
        color.resource(),
        &color,
        false,
    ));
    let mut after = PassDeclaration::new(PassSlot::ScreenSpaceAfterTransparent);
    after.color = Some(crate::compositor::attachment(
        output.resource(),
        &output,
        false,
    ));
    let mut pass = FeaturePass::new(PassSlot::ScreenSpaceAfterTransparent);
    pass.color_format = Some(wgpu::TextureFormat::R32Float);
    pass.reads = reads.to_vec();
    let extents = std::rc::Rc::default();
    let mut reader = DeclaredReader {
        reads: reads.to_vec(),
        extents: std::rc::Rc::clone(&extents),
    };
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let result = frame.encode_frame(
        context.device(),
        &mut encoder,
        (&[opaque, after], &[color, output]),
        None,
        &mut [BorrowedFeature {
            declaration: FeatureDeclaration {
                passes: vec![pass],
                effects: vec![],
            },
            prepared: &mut reader,
        }],
        &(
            crate::frame::FrameUniform::new(&view(), None).unwrap(),
            None,
        ),
    );
    drop(encoder);
    let passes = crate::scene_metadata::EXECUTED_PASSES.with_borrow(Clone::clone);
    let extents = extents.borrow().clone();
    DeclaredFrame {
        result,
        extents,
        passes,
    }
}

fn ran(frame: &DeclaredFrame, slot: PassSlot) -> usize {
    frame.passes.iter().filter(|&&pass| pass == slot).count()
}

/// A caller feature may read a coarser level of the linear depth hierarchy in a frame that
/// draws the prepass only for depth fitting: the frame builds levels 0 to that level, and still
/// none past it and no occlusion.
#[test]
fn gpu_depth_fitting_alone_builds_the_depth_levels_a_caller_feature_reads() {
    use crate::frame::metadata::FrameProduct;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let frame = encode_with_declared_reader(&context, &[FrameProduct::linear_depth(1).resource()]);
    assert_eq!(frame.result, Ok(()));
    assert_eq!(frame.extents, vec![[4, 4]]);
    assert_eq!(ran(&frame, PassSlot::DepthPyramid(0)), 1);
    assert_eq!(ran(&frame, PassSlot::DepthPyramid(1)), 1);
    assert!(
        !frame
            .passes
            .iter()
            .any(|&slot| occlusion_stage(slot) && slot != PassSlot::DepthPyramid(1)),
        "stages no one reads: {:?}",
        frame.passes
    );
}

/// Controls: a caller reading level 0 alone, or no caller reader, leaves the frame at level 0
/// with no occlusion stage; a level the hierarchy does not have fails by name.
#[test]
fn gpu_depth_fitting_alone_skips_depth_levels_and_occlusion_no_feature_reads() {
    use crate::frame::metadata::FrameProduct;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for reads in [vec![FrameProduct::linear_depth(0).resource()], vec![]] {
        let frame = encode_with_declared_reader(&context, &reads);
        assert_eq!(frame.result, Ok(()));
        assert_eq!(frame.extents, vec![[8, 8]; reads.len()]);
        assert_eq!(ran(&frame, PassSlot::DepthPyramid(0)), 1);
        assert!(
            !frame.passes.iter().copied().any(occlusion_stage),
            "stages no one reads: {:?}",
            frame.passes
        );
    }
    // 8x8 has levels 0 to 3 (8, 4, 2 and 1 texels).
    let missing = FrameProduct::linear_depth(4).resource();
    let frame = encode_with_declared_reader(&context, &[missing]);
    assert_eq!(
        frame.result,
        Err(worldrunner_render_post::FrameError::MissingProducer(
            missing
        ))
    );
    assert!(frame.extents.is_empty());
}

/// A caller feature may read the filtered ambient occlusion by its declared resource. The frame
/// then runs the neutral kernel and filter for it, while its receivers keep the constant.
#[test]
fn gpu_depth_fitting_alone_computes_the_occlusion_a_caller_feature_reads() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let frame = encode_with_declared_reader(&context, &[ResourceRef::new("ambient occlusion")]);
    assert_eq!(frame.result, Ok(()));
    assert_eq!(frame.extents, vec![[8, 8]]);
    for slot in [
        PassSlot::DepthPyramid(0),
        PassSlot::Ssao,
        PassSlot::SsaoBlur,
    ] {
        assert_eq!(ran(&frame, slot), 1, "{slot:?}");
    }
    assert_eq!(ran(&frame, PassSlot::DepthPyramid(1)), 0);
}

/// The depth-fitting-only fixture with its first caster carrying contour ID 7.
fn contoured_fitting_source() -> RenderScene {
    let base = fitting_source();
    let mut styles = vec![None; 256];
    styles[7] = Some(SceneContourStyle::default());
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_contours(SceneContours::new([1920, 1080], styles).unwrap())
        .unwrap();
    let mut objects = base.geometry().objects().to_vec();
    objects[1] = objects[1].with_contour_id(7);
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

/// The contour ID pass reads Opaque's completed depth, which the depth-fitting-only frame shares
/// with its prepass, and declares no read of a skipped stage. Its IDs must equal those of the
/// frame that still computed the whole hierarchy and the occlusion.
#[test]
fn gpu_depth_fitting_alone_draws_contour_ids_against_the_shared_prepass_depth() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = contoured_fitting_source();
    let skipped = depth_fitted_frame(&context, &source, false);
    let reference = depth_fitted_frame(&context, &source, true);
    assert!(!skipped.passes.iter().copied().any(occlusion_stage));
    assert!(reference.passes.contains(&PassSlot::Ssao));
    let ids = skipped.contours.as_ref().expect("contour IDs drawn");
    assert!(ids.contains(&7), "the caster is visible");
    assert!(ids.contains(&0), "the caster does not cover the frame");
    assert!(ids.iter().all(|&id| id == 0 || id == 7));
    assert!(
        reference.contours.as_ref() == Some(ids),
        "contour IDs differ"
    );
    assert!(skipped.color == reference.color, "scene colour differs");
}
