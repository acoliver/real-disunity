use super::publication::{admit_empty_programs, assert_pass_count, refuse_suffix};
use super::velocity::{temporal_fixture, temporal_source};
use super::*;
use crate::terrain::deferred_tests::halves;
use std::{cell::Cell, rc::Rc};
use worldrunner_render_plan::SceneSsr;
use worldrunner_render_post::{
    BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PassDeclaration,
    PassEntry, PassSlot, PrepareContext, PreparedFeature,
};

const TOLERANCE_UV: f32 = 1e-3;

#[derive(Clone, Default)]
pub(super) struct CallerPreparation {
    armed: Rc<Cell<bool>>,
    preparations: Rc<Cell<u32>>,
}

impl CallerPreparation {
    pub(super) fn refusing() -> Self {
        Self {
            armed: Rc::new(Cell::new(true)),
            preparations: Rc::default(),
        }
    }

    fn preparations(&self) -> u32 {
        self.preparations.get()
    }
}

pub(super) struct CallerRegistration(pub(super) CallerPreparation);

impl FrameFeature for CallerRegistration {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration::new(PassSlot::Opaque)
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        _: &mut Vec<worldrunner_render_post::AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let entry = PassEntry::new(PassSlot::Opaque);
        let opaque = passes
            .iter()
            .find(|pass| pass.entry() == entry)
            .ok_or(FrameError::AttachmentContract(entry))?;
        let mut declaration = self.declare();
        let pass = &mut declaration.passes[0];
        pass.color_format = opaque.color.as_ref().map(|color| color.format);
        pass.depth_format = opaque.depth.as_ref().map(|depth| depth.format);
        pass.additional_color_formats = opaque
            .additional_colors
            .iter()
            .map(|color| color.format)
            .collect();
        Ok(declaration)
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.0.clone()))
    }
}

impl PreparedFeature for CallerPreparation {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.preparations.set(self.preparations.get() + 1);
        if self.armed.replace(false) {
            return Err(FrameError::FeaturePreparation {
                feature: "retained caller preparation fixture",
                reason: "caller refused suffix preparation".into(),
            });
        }
        Ok(())
    }

    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

#[derive(Clone, Copy)]
enum ScheduleRefusal {
    None,
    ProgramAdmission,
    CallerPreparation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Products {
    color: Vec<u8>,
    g3: Vec<u8>,
    velocity: Vec<u8>,
    ssr_confidence: Vec<u8>,
}

fn ssr_source() -> RenderScene {
    let base = temporal_source();
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_ssr(SceneSsr {
            enabled: true,
            max_steps: 256,
            thickness_millimeters: 100,
            roughness_cutoff_micro: 600_000,
            resolution_scale_micro: 1_000_000,
            ..Default::default()
        })
        .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: base.terrain().cloned(),
        effects: Some(effects),
    })
    .unwrap()
}

fn products(context: &GpuContext, frame: &FrameRenderer) -> Products {
    let g3 = frame
        .feature_target(FeatureTarget::Gbuffer(GbufferTarget::G3))
        .unwrap();
    let velocity = frame.targets().velocity_texture().unwrap();
    let confidence = frame
        .feature_capture(FeatureTarget::SsrConfidence)
        .expect("enabled SSR publishes a confidence capture");
    let viewport = view().viewport();
    assert_eq!([g3.width(), g3.height()], [viewport.width, viewport.height]);
    assert_eq!(
        [g3.width(), g3.height()],
        [velocity.width(), velocity.height()]
    );
    let pixels = u64::from(g3.width()) * u64::from(g3.height());
    let g3_bytes = read(context, g3);
    let velocity_bytes = read(context, velocity);
    assert_eq!(u64::try_from(g3_bytes.len()).unwrap(), pixels * 2);
    assert_eq!(u64::try_from(velocity_bytes.len()).unwrap(), pixels * 4);
    Products {
        color: read(context, frame.targets().scene().texture()),
        g3: g3_bytes,
        velocity: velocity_bytes,
        ssr_confidence: read(context, confidence.texture),
    }
}
fn seed_confidence(context: &GpuContext, frame: &FrameRenderer) {
    let capture = frame.feature_capture(FeatureTarget::SsrConfidence).unwrap();
    assert_eq!(capture.texture.format(), wgpu::TextureFormat::Rgba16Float);
    let view = capture.texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_array_layer: capture.layer,
        array_layer_count: Some(1),
        ..Default::default()
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        a: 0.5,
                        ..wgpu::Color::BLACK
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    encoder.finish().submit().unwrap();
}

fn assert_confidence(products: &Products, expected: f32) {
    let values = halves(&products.ssr_confidence);
    assert!(!values.is_empty());
    assert!(
        values
            .chunks_exact(4)
            .all(|pixel| pixel[3].to_bits() == expected.to_bits())
    );
}

fn move_retained_instance(renderer: &mut ShadowedScene) {
    let handle = renderer.scene_mut().instance_handles().next().unwrap();
    let original = *renderer.scene_mut().instance(handle).unwrap().0.transform();
    let mut moved = original;
    moved.position_millimeters[0] += 250;
    renderer
        .scene_mut()
        .set_instance_transform(handle, moved)
        .unwrap();
}

fn completed_compacted(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    id: u64,
) {
    let (color, depth) = attachments(frame);
    let serial = context.submission().submitted_serial();
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_host(
            context,
            &view(),
            (&color, &depth),
            (Some(id), false, &[]),
            frame,
        )
        .unwrap()
        .unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (id, id));
    assert_eq!(
        context.submission().submitted_serial(),
        serial + 2,
        "one prefix and one suffix submitted before product readbacks"
    );
    assert_pass_count(PassSlot::DepthNormalPrepass, 1);
    assert_pass_count(PassSlot::Opaque, 1);
    assert_pass_count(PassSlot::OpaqueLighting, 1);
    assert_pass_count(PassSlot::OpaqueGbuffer, 0);
    let objects = renderer.gpu.statistics().passes.prepass_objects;
    assert_eq!(objects.recording, crate::draw::ObjectRecording::Compacted);
    assert!(objects.meshes.instances > 0);
}

fn fitted_temporal_fixture(
    context: &GpuContext,
    source: &RenderScene,
) -> (FrameRenderer, ShadowedScene) {
    let (mut frame, mut renderer) = temporal_fixture(context, source);
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    frame.set_frame_time(std::time::Duration::from_secs_f32(0.125));
    (frame, renderer)
}

fn register_caller(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    refusal: ScheduleRefusal,
) -> Option<CallerPreparation> {
    if !matches!(refusal, ScheduleRefusal::CallerPreparation) {
        return None;
    }
    let caller = CallerPreparation::refusing();
    frame
        .register(
            context.device(),
            Box::new(CallerRegistration(caller.clone())),
        )
        .unwrap();
    assert_eq!(caller.preparations(), 0);
    Some(caller)
}

pub(super) fn assert_preparations(caller: Option<&CallerPreparation>, expected: u32) {
    if let Some(caller) = caller {
        assert_eq!(caller.preparations(), expected);
    }
}

pub(super) fn refuse_caller_suffix(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
) {
    let (color, depth) = attachments(frame);
    let serial = context.submission().submitted_serial();
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let refused = renderer.render_depth_fitted_host(
        context,
        &view(),
        (&color, &depth),
        (Some(1), false, &[]),
        frame,
    );
    assert!(
        matches!(
            &refused,
            Err(ShadowError::Schedule(FrameError::FeaturePreparation { feature, reason }))
                if *feature == "retained caller preparation fixture"
                    && reason == "caller refused suffix preparation"
        ),
        "{refused:?}"
    );
    assert_eq!(
        context.submission().submitted_serial(),
        serial + 1,
        "only the prefix submitted before product readbacks"
    );
    assert_pass_count(PassSlot::DepthNormalPrepass, 1);
    assert_pass_count(PassSlot::Opaque, 0);
    assert_pass_count(PassSlot::OpaqueLighting, 0);
    assert_pass_count(PassSlot::OpaqueGbuffer, 0);
}

fn refused_products(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    refusal: ScheduleRefusal,
) -> Option<Products> {
    if matches!(refusal, ScheduleRefusal::None) {
        return None;
    }
    let statistics = renderer.gpu.statistics();
    let selection = renderer.view_stats();
    let temporal_index = frame.projection_jitter_index();
    assert!(frame.has_temporal_history());
    match refusal {
        ScheduleRefusal::None => unreachable!(),
        ScheduleRefusal::ProgramAdmission => {
            let (_runtime, mut arena) = admit_empty_programs(context, renderer);
            refuse_suffix(context, frame, renderer, 1);
            arena.retire().unwrap();
        }
        ScheduleRefusal::CallerPreparation => refuse_caller_suffix(context, frame, renderer),
    }
    assert_eq!(renderer.gpu.statistics(), statistics);
    assert_eq!(renderer.view_stats(), selection);
    assert_eq!(frame.projection_jitter_index(), temporal_index);
    assert!(frame.has_temporal_history());
    Some(products(context, frame))
}

fn schedule(
    context: &GpuContext,
    source: &RenderScene,
    refusal: ScheduleRefusal,
) -> (Products, Products, Products) {
    let (mut frame, mut renderer) = fitted_temporal_fixture(context, source);
    completed_compacted(context, &mut frame, &mut renderer, 0);
    let caller = register_caller(context, &mut frame, refusal);
    // A nonzero sentinel distinguishes retained confidence from an accidental zero write.
    seed_confidence(context, &frame);
    let initial = products(context, &frame);
    assert_confidence(&initial, 0.5);
    move_retained_instance(&mut renderer);
    let refused = refused_products(context, &mut frame, &mut renderer, refusal);
    if let Some(refused) = &refused {
        assert_pre_suffix_stable(&initial, refused);
    }
    assert_preparations(caller.as_ref(), 1);
    completed_compacted(context, &mut frame, &mut renderer, 1);
    assert_preparations(caller.as_ref(), 2);
    let fresh = products(context, &frame);
    assert_confidence(&fresh, 0.0);
    if let Some(refused) = refused {
        assert_eq!(
            refused.g3, fresh.g3,
            "retry changed the completed prefix surfaces"
        );
        assert_eq!(
            refused.velocity, fresh.velocity,
            "refused suffix advanced the prefix motion history"
        );
    }
    completed_compacted(context, &mut frame, &mut renderer, 2);
    assert_preparations(caller.as_ref(), 3);
    let settled = products(context, &frame);
    assert_motion_settled(&settled);
    (initial, fresh, settled)
}

fn assert_motion_settled(products: &Products) {
    let velocity = halves(&products.velocity);
    assert!(velocity.iter().all(|value| value.is_finite()));
    assert!(
        velocity.iter().all(|value| value.abs() < TOLERANCE_UV),
        "unchanged completed frame retained instance motion"
    );
}

fn assert_pre_suffix_stable(expected: &Products, actual: &Products) {
    assert_eq!(
        actual.color, expected.color,
        "failed suffix changed scene color"
    );
    assert_eq!(
        actual.ssr_confidence, expected.ssr_confidence,
        "failed suffix changed SSR confidence"
    );
}

fn assert_products_equal(expected: &Products, actual: &Products) {
    assert_eq!(actual.color, expected.color, "scene color publication");
    assert_eq!(actual.g3, expected.g3, "G3 publication");
    assert_eq!(actual.velocity, expected.velocity, "velocity publication");
    assert_eq!(
        actual.ssr_confidence, expected.ssr_confidence,
        "SSR confidence publication"
    );
    let expected_velocity = halves(&expected.velocity);
    let actual_velocity = halves(&actual.velocity);
    assert_eq!(expected_velocity.len(), actual_velocity.len());
    for (got, want) in actual_velocity.iter().zip(expected_velocity) {
        assert!(
            (got - want).abs() < TOLERANCE_UV,
            "velocity {got} vs {want}"
        );
    }
}

fn assert_instance_motion(initial: &Products, fresh: &Products) {
    let before = halves(&initial.velocity);
    let after = halves(&fresh.velocity);
    let (mut moving, mut largest) = (0, 0.0_f32);
    for (index, code) in fresh.g3.chunks_exact(2).enumerate() {
        if u16::from_le_bytes([code[0], code[1]]) == 0 {
            continue;
        }
        let offset = index * 2;
        let residual = [after[offset], after[offset + 1]];
        assert!(residual.iter().all(|value| value.is_finite()));
        let change = (residual[0] - before[offset])
            .abs()
            .max((residual[1] - before[offset + 1]).abs());
        if change > 1e-6 {
            moving += 1;
            largest = largest.max(change);
        }
    }
    assert!(
        moving >= 200,
        "only {moving} covered texels changed velocity"
    );
    assert!(
        largest > 0.01,
        "the retained transform residual was lost: {largest}"
    );
}

#[test]
fn gpu_completed_retained_instance_motion_settles_on_the_next_unchanged_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = ssr_source();
    let (initial, moved, settled) = schedule(&context, &source, ScheduleRefusal::None);
    assert_instance_motion(&initial, &moved);
    assert_motion_settled(&settled);
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn gpu_retained_instance_motion_survives_gbuffer_suffix_refusal_with_taa_and_ssr() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = ssr_source();
    let (normal_initial, normal_fresh, normal_settled) =
        schedule(&context, &source, ScheduleRefusal::None);
    let (retry_initial, retry_fresh, retry_settled) =
        schedule(&context, &source, ScheduleRefusal::ProgramAdmission);
    assert_products_equal(&normal_initial, &retry_initial);
    assert_products_equal(&normal_fresh, &retry_fresh);
    assert_products_equal(&normal_settled, &retry_settled);
    assert_instance_motion(&normal_initial, &normal_fresh);
    assert_instance_motion(&retry_initial, &retry_fresh);
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn gpu_retained_instance_motion_survives_caller_preparation_refusal_with_taa_and_ssr() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = ssr_source();
    let (normal_initial, normal_fresh, normal_settled) =
        schedule(&context, &source, ScheduleRefusal::None);
    let (retry_initial, retry_fresh, retry_settled) =
        schedule(&context, &source, ScheduleRefusal::CallerPreparation);
    assert_products_equal(&normal_initial, &retry_initial);
    assert_products_equal(&normal_fresh, &retry_fresh);
    assert_products_equal(&normal_settled, &retry_settled);
    assert_instance_motion(&normal_initial, &normal_fresh);
    assert_instance_motion(&retry_initial, &retry_fresh);
    assert!(pollster::block_on(scope.pop()).is_none());
}
