use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use crate::draw::{DrawCount, ObjectDraws, ObjectRecording, ViewStats};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_program::unity::runtime::{ProgramArena, ProgramRuntime, TargetSignature};

pub(super) fn admit_empty_programs(
    context: &GpuContext,
    renderer: &mut ShadowedScene,
) -> (ProgramRuntime, ProgramArena) {
    const ARENA: u64 = 4096;
    let mut allocator = ConfiguredFrameRenderer::new(
        context,
        RendererConfiguration::new(
            [1, 1],
            OffscreenTarget::COLOR_FORMAT,
            worldrunner_render_post::ToneMap::Clamp,
        ),
        AllocationBudget(1 << 26),
    )
    .unwrap();
    let reservation = allocator
        .reserve(context, ProgramRuntime::arena_allocations(ARENA))
        .unwrap();
    let mut runtime = ProgramRuntime::new(
        context.device().clone(),
        context.submission().clone(),
        AllocationBudget(ARENA),
    );
    let arena = runtime.open_arena(reservation).unwrap();
    let recording = context.submission().begin(context.device()).unwrap();
    let signature = TargetSignature::new(
        &[wgpu::TextureFormat::Rgba16Float],
        Some(wgpu::TextureFormat::Depth32Float),
        1,
    )
    .unwrap();
    // Empty packets still acquire the arena lease and produce admitted player work.
    let admitted = runtime
        .admit_draws(&recording, &arena, signature, &[])
        .unwrap();
    renderer.scene_mut().admit_programs(admitted);
    recording.finish().cancel();
    (runtime, arena)
}

fn hide_last_caster(renderer: &mut ShadowedScene) {
    let handle = renderer.scene_mut().instance_handles().nth(2).unwrap();
    renderer
        .scene_mut()
        .set_instance_visible(handle, false)
        .unwrap();
}

fn assert_selection(renderer: &ShadowedScene, visible: usize) {
    assert_eq!(
        renderer.view_stats(),
        ViewStats {
            considered: 3,
            culled: 3 - visible,
            drawn_per_lod: vec![visible]
        }
    );
    let passes = renderer.gpu.statistics().passes;
    assert_eq!(
        passes.prepass_objects.meshes,
        DrawCount {
            calls: 2,
            instances: u64::try_from(visible).unwrap()
        }
    );
    assert_eq!(passes.prepass_objects.impostors, DrawCount::default());
    assert_eq!(passes.prepass_objects.recording, ObjectRecording::Compacted);
    assert_eq!(passes.opaque_objects, ObjectDraws::default());
}

pub(super) fn assert_pass_count(slot: worldrunner_render_post::PassSlot, expected: usize) {
    crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
        assert_eq!(
            passes.iter().filter(|&&pass| pass == slot).count(),
            expected,
            "{slot:?}"
        );
    });
}

pub(super) fn completed_color(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    id: u64,
) -> Vec<u8> {
    let (color, depth) = attachments(frame);
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let fit = renderer
        .render_depth_fitted_with(context, &view(), (&color, &depth), id, frame)
        .unwrap()
        .unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (id, id));
    assert_pass_count(PassSlot::DepthNormalPrepass, 1);
    assert_pass_count(PassSlot::Opaque, 1);
    assert_pass_count(PassSlot::OpaqueLighting, 1);
    assert_pass_count(PassSlot::OpaqueGbuffer, 0);
    read(context, color.texture())
}

pub(super) fn refuse_suffix(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    source_frame: u64,
) {
    let (color, depth) = attachments(frame);
    let before = renderer.gpu.statistics();
    let before_selection = renderer.view_stats();
    let serial = context.submission().submitted_serial();
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let refused =
        renderer.render_depth_fitted_with(context, &view(), (&color, &depth), source_frame, frame);
    assert!(
        matches!(
            refused,
            Err(ShadowError::UnsupportedFeature("player programs"))
        ),
        "{refused:?}"
    );
    assert_eq!(
        context.submission().submitted_serial(),
        serial + 1,
        "only the prefix submitted"
    );
    assert_pass_count(PassSlot::DepthNormalPrepass, 1);
    assert_pass_count(PassSlot::Opaque, 0);
    assert_eq!(
        renderer.gpu.statistics().passes.prepass_objects.meshes,
        before.passes.prepass_objects.meshes
    );
    assert_eq!(renderer.gpu.statistics().passes, before.passes);
    assert_eq!(renderer.view_stats(), before_selection);
    assert!(
        renderer.gpu.has_programs(),
        "suffix refusal must retain admitted work"
    );
    assert!(renderer.gpu.take_programs().is_some());
    assert!(renderer.gpu.take_programs().is_none());
}

#[test]
fn gpu_same_frame_gbuffer_suffix_refusal_preserves_admission_and_next_color_publication() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = render_source();
    let (mut frame, mut renderer) = fitted_frame(&context, &source);
    let (_runtime, mut arena) = admit_empty_programs(&context, &mut renderer);
    refuse_suffix(&context, &mut frame, &mut renderer, 0);
    assert_eq!(
        renderer.gpu.statistics().passes.prepass_objects.meshes,
        DrawCount::default()
    );
    assert_eq!(renderer.view_stats(), ViewStats::default());
    arena.retire().unwrap();
    hide_last_caster(&mut renderer);
    let recovered = completed_color(&context, &mut frame, &mut renderer, 1);
    assert_selection(&renderer, 2);
    let counts = renderer.gpu.statistics();
    assert_eq!(
        completed_color(&context, &mut frame, &mut renderer, 2),
        recovered
    );
    assert_eq!(
        renderer.gpu.statistics(),
        counts,
        "a second suffix must not accumulate draws"
    );
    assert_selection(&renderer, 2);
    let (mut baseline_frame, mut baseline_renderer) = fitted_frame(&context, &source);
    hide_last_caster(&mut baseline_renderer);
    assert_eq!(
        completed_color(&context, &mut baseline_frame, &mut baseline_renderer, 1),
        recovered
    );
    assert_eq!(
        renderer.gpu.statistics(),
        baseline_renderer.gpu.statistics()
    );
    assert_eq!(renderer.view_stats(), baseline_renderer.view_stats());
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn gpu_same_frame_gbuffer_suffix_refusal_preserves_completed_pass_counts() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = render_source();
    let (mut frame, mut renderer) = fitted_frame(&context, &source);
    let completed = completed_color(&context, &mut frame, &mut renderer, 0);
    assert_selection(&renderer, 3);
    let counts = renderer.gpu.statistics();
    let (_runtime, mut arena) = admit_empty_programs(&context, &mut renderer);
    refuse_suffix(&context, &mut frame, &mut renderer, 1);
    assert_eq!(renderer.gpu.statistics(), counts);
    assert_selection(&renderer, 3);
    arena.retire().unwrap();
    assert_eq!(
        completed_color(&context, &mut frame, &mut renderer, 1),
        completed
    );
    assert_eq!(renderer.gpu.statistics(), counts);
    assert_selection(&renderer, 3);
    assert!(pollster::block_on(scope.pop()).is_none());
}
