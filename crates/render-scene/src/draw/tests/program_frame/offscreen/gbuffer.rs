//! Offscreen operations precede the stock G-buffer and lighting in the same transaction.

mod queued;

use super::{Fixture, build, check_offscreen_stages, check_timing, engine_scene, player, settings};
use crate::{
    compositor::ReceiverGradientReservation, device::GpuContext, draw::DrawCount,
    renderer::ScenePipelines, test_support::SceneTestSession,
};
use worldrunner_render_plan::{SceneEffects, SceneOpaquePath};
use worldrunner_render_post::{FrameConfig, PassEntry, PassSlot, TimingMode, ToneMap};

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[test]
fn offscreen_work_and_fused_gbuffer_lighting_share_one_submission() {
    assert_mixed_frame(SceneOpaquePath::GBUFFER_RENDER);
}

#[test]
fn offscreen_work_and_gbuffer_observation_share_one_submission() {
    assert_mixed_frame(SceneOpaquePath::GBUFFER_OBSERVATION);
}

fn fixture(path: SceneOpaquePath) -> Fixture {
    let mut config = settings();
    config.frame = FrameConfig::new(config.frame.size, HDR, 1);
    config.hdr = (1.0, ToneMap::Reinhard);
    config.ssao = Some(crate::ssr::neutral_ao());
    config.opaque_path = path;
    config.receiver_gradients = ReceiverGradientReservation::None;
    config.timing = TimingMode::Required;
    let context = pollster::block_on(GpuContext::request_with_features_for_test(
        crate::device::REQUIRED_FEATURES | wgpu::Features::TIMESTAMP_QUERY,
    ))
    .expect("a device with timestamp queries");
    let mut parts = super::effect_parts();
    parts.ssao = config.ssao;
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_opaque_path(path)
        .unwrap();
    let source = engine_scene();
    let source = source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    Fixture::with_context(context, &source, config, |device| {
        ScenePipelines::with_ssao(device, HDR, 1).unwrap()
    })
}

fn assert_mixed_frame(path: SceneOpaquePath) {
    let _session = SceneTestSession::acquire();
    let mut fixture = fixture(path);
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let before = authority.submitted_serial();
    let before_passes = fixture.gpu.statistics().passes;
    let (operations, readbacks) = build(&mut player);
    let mut recording = authority.begin(&device).unwrap();
    fixture.renderer.begin_timing(9).unwrap();
    let admitted = operations.admit(&recording).unwrap();
    let ids: Vec<_> = admitted.ids().collect();
    fixture.gpu.admit_offscreen(admitted);
    fixture.encode(&mut recording).unwrap();
    let output = fixture.copy_scene(&mut recording);
    fixture
        .renderer
        .renderer_mut()
        .finish_timing(&mut recording)
        .unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before,
        "recording must not submit"
    );
    assert_eq!(fixture.gpu.statistics().passes, before_passes);
    assert!(
        !fixture.gpu.has_offscreen(),
        "encoding consumes the admitted work"
    );
    recording.finish().submit().unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "one frame, one submission"
    );
    check_offscreen_stages(readbacks, &device);
    let image = fixture.read(&output);
    // Linear f16 bytes avoid display-transfer rounding in this execution-order test.
    assert_eq!(hdr_pixel(image.bytes(), 30, 20), [0, 60, 0, 0, 0, 0, 0, 60]);
    assert_eq!(hdr_pixel(image.bytes(), 3, 3), [0, 0, 0, 0, 0, 0, 0, 60]);
    assert_scene_counts(&fixture, path);
    assert_eq!(authority.submitted_serial(), before + 1);
    check_timing(&fixture, &ids, &geometry(path));
    assert_eq!(
        authority.submitted_serial(),
        before + 2,
        "timestamp collection adds one resolve submission"
    );
}

fn hdr_pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 8] {
    let at = (y * super::super::TARGET as usize + x) * 8;
    bytes[at..at + 8].try_into().unwrap()
}

fn assert_scene_counts(fixture: &Fixture, path: SceneOpaquePath) {
    let passes = fixture.gpu.statistics().passes;
    let card = DrawCount {
        calls: 1,
        instances: 1,
    };
    assert_eq!(passes.prepass_objects.meshes, card);
    assert_eq!(passes.prepass_objects.impostors, DrawCount::default());
    if path == SceneOpaquePath::GBUFFER_RENDER {
        assert_eq!(passes.opaque_objects, crate::draw::ObjectDraws::default());
    } else {
        assert_eq!(passes.opaque_objects.meshes, card);
        assert_eq!(passes.opaque_objects.impostors, DrawCount::default());
    }
}

fn geometry(path: SceneOpaquePath) -> Vec<PassEntry> {
    let mut slots = vec![PassSlot::DepthNormalPrepass];
    slots.extend((0..=super::super::TARGET.ilog2()).map(PassSlot::DepthPyramid));
    slots.extend([PassSlot::Ssao, PassSlot::SsaoBlur]);
    if path == SceneOpaquePath::GBUFFER_OBSERVATION {
        slots.push(PassSlot::OpaqueGbuffer);
    }
    slots.extend([PassSlot::OpaqueLighting, PassSlot::Opaque]);
    slots.into_iter().map(PassEntry::new).collect()
}
