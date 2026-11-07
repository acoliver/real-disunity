//! Frames that player programs cannot join are refused by name, before anything is drawn.

use super::super::*;
use super::rig::{ARENA, BLACK as BLACK_PIXEL, Fixture, RED as RED_PIXEL, engine_scene, view};
use super::{COLOR_FORMAT, DEPTH_FORMAT, settings};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use crate::draw::DrawCount;
use crate::offscreen::OffscreenTarget;
use crate::shadows::{DirectionalShadowSettings, ShadowError, ShadowedScene};
use worldrunner_render_gpu::configuration::{AllocationBudget, ConfigurationError};
use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_plan::post::{UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{
    AttachmentView, FrameConfig, FrameError, HdrPostSettings, PostResolve, ResourceRef, ToneMap,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::unity::runtime::{Geometry, ProgramRuntime, TargetSignature};

const HDR: TextureFormat = TextureFormat::Rgba16Float;

fn stock() -> Fixture {
    Fixture::new(settings(), |device| {
        ScenePipelines::new(device, COLOR_FORMAT)
    })
}

/// The text of a refusal from the player programs, which the frame error names them for.
fn refusal(error: FrameError) -> String {
    match error {
        FrameError::FeaturePreparation {
            feature: "player programs",
            reason,
        } => reason,
        other => panic!("expected a refusal by the player programs, got {other:?}"),
    }
}

/// Encodes a frame with the fixture's patches admitted for `signature`, then submits it with a
/// copy of the scene color target, and returns the encode result and those bytes.
fn attempt(fixture: &mut Fixture, signature: TargetSignature) -> (Result<(), FrameError>, Vec<u8>) {
    let authority = fixture.context.submission().clone();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    let admitted = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(admitted);
    let encoded = fixture.encode(&mut recording);
    let readback = fixture.copy_scene(&mut recording);
    recording.finish().submit().unwrap();
    (encoded, fixture.read(&readback).bytes().to_vec())
}

#[test]
fn draws_prepared_for_other_attachments_are_refused_before_any_draw() {
    let mut fixture = stock();
    let prepared = TargetSignature::new(&[HDR], Some(DEPTH_FORMAT), 1).unwrap();
    let (encoded, scene) = attempt(&mut fixture, prepared);
    assert_eq!(
        refusal(encoded.unwrap_err()),
        ProgramError::TargetMismatch {
            prepared,
            pass: fixture.signature()
        }
        .to_string()
    );
    assert!(scene.iter().all(|byte| *byte == 0), "nothing was drawn");
}

fn expect_unsupported(mut fixture: Fixture, effect: &'static str) {
    let signature = fixture.signature();
    let (encoded, scene) = attempt(&mut fixture, signature);
    assert!(
        matches!(encoded, Err(FrameError::UnsupportedEffect(name)) if name == effect),
        "expected {effect}, got {encoded:?}"
    );
    assert!(scene.iter().all(|byte| *byte == 0), "nothing was drawn");
}

#[test]
fn a_multisampled_scene_refuses_programs_by_name() {
    let mut settings = settings();
    settings.frame = FrameConfig::new([TARGET; 2], COLOR_FORMAT, 4);
    let fixture = Fixture::new(settings, |device| {
        ScenePipelines::with_samples(device, COLOR_FORMAT, 4).unwrap()
    });
    expect_unsupported(fixture, "player programs with multisampling");
}

#[test]
fn a_temporally_reconstructed_scene_refuses_programs_by_name() {
    let mut settings = RendererConfiguration::legacy([TARGET; 2], COLOR_FORMAT);
    settings.frame = FrameConfig::new([TARGET; 2], HDR, 1);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post = HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    // Plain pipelines write no velocity; the jitter alone makes the frame temporal.
    let fixture = Fixture::new(settings, |device| ScenePipelines::new(device, HDR));
    expect_unsupported(fixture, "player programs with temporal reconstruction");
}

#[test]
fn a_scene_with_screen_space_passes_refuses_programs_by_name() {
    let mut settings = RendererConfiguration::legacy([TARGET; 2], COLOR_FORMAT);
    settings.frame = FrameConfig::new([TARGET; 2], HDR, 1);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.ssao = Some(super::super::ssao::settings(1_000_000));
    let fixture = Fixture::with_scene(
        &super::super::ssao::source(1_000_000, true, 255),
        settings,
        |device| ScenePipelines::with_ssao(device, HDR, 1).unwrap(),
    );
    expect_unsupported(fixture, "player programs with screen-space passes");
}

/// Admits the fixture's patches in a frame that is then cancelled, so only the draws remain.
fn admitted_then_cancelled(fixture: &mut Fixture) {
    let authority = fixture.context.submission().clone();
    let recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let admitted = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(admitted);
    recording.finish().cancel();
}

#[test]
fn draws_whose_arena_retired_before_encoding_fail_the_frame_by_name() {
    let mut fixture = stock();
    admitted_then_cancelled(&mut fixture);
    fixture.arena.retire().unwrap();
    let authority = fixture.context.submission().clone();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    assert_eq!(
        refusal(fixture.encode(&mut recording).unwrap_err()),
        ProgramError::Lease(LeaseError::Expired).to_string()
    );
}

#[test]
fn draws_whose_arena_was_dropped_in_the_recording_frame_fail_the_encode_by_name() {
    let mut fixture = stock();
    let authority = fixture.context.submission().clone();
    let serial = authority.submitted_serial();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let admitted = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(admitted);
    fixture.drop_arena();
    // The frame already holds the dropped arena, so submitting it would fail whatever was
    // recorded; the encode has to be the step that refuses.
    assert_eq!(
        refusal(fixture.encode(&mut recording).unwrap_err()),
        ProgramError::Lease(LeaseError::Expired).to_string()
    );
    assert_eq!(
        fixture.gpu.statistics().meshes,
        DrawCount::default(),
        "no stock draw was recorded"
    );
    recording.finish().cancel();
    assert_eq!(authority.submitted_serial(), serial);
}

#[test]
fn draws_with_a_range_past_their_buffers_are_refused_at_admission_and_the_frame_stays_stock() {
    let mut fixture = stock();
    let authority = fixture.context.submission().clone();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    // Each patch's buffer holds six vertices of 12 bytes; a seventh would end at byte 84.
    let refused = fixture.admit_geometry(
        &recording,
        signature,
        Geometry::Vertices { first: 0, count: 7 },
    );
    assert_eq!(
        refused.unwrap_err(),
        ProgramError::VertexRangePastBuffer {
            slot: 0,
            needed: 84,
            have: 72
        }
    );
    fixture.encode(&mut recording).unwrap();
    let readback = fixture.output_copy(&mut recording);
    recording.finish().submit().unwrap();
    let image = fixture.read(&readback);
    assert_eq!(image.pixel(30, 20), RED_PIXEL, "the stock scene drew");
    assert_eq!(image.pixel(54, 32), BLACK_PIXEL, "no program drew");
}

#[test]
fn programs_admitted_to_a_scene_encoded_on_the_resident_path_are_refused_by_name() {
    let mut fixture = stock();
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let mut recording = authority.begin(&device).unwrap();
    let signature = fixture.signature();
    let admitted = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(admitted);
    // The resident path draws with a frame the caller has already set.
    fixture
        .gpu
        .set_frame(fixture.context.queue(), super::rig::frame());
    let mut renderer = crate::compositor::FrameRenderer::with_output(
        &device,
        FrameConfig::new([TARGET; 2], COLOR_FORMAT, 1),
        COLOR_FORMAT,
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let encoded = crate::compositor::encode_resident(
        &mut renderer,
        &device,
        &mut recording,
        (&fixture.gpu, &fixture.pipelines),
        (&color, &depth),
    );
    assert!(
        matches!(
            encoded,
            Err(FrameError::UnsupportedEffect(
                "player programs on the resident path"
            ))
        ),
        "got {encoded:?}"
    );
    recording.finish().cancel();
}

#[test]
fn programs_admitted_to_a_shadowed_scene_are_refused_by_name() {
    let mut fixture = stock();
    let settings = DirectionalShadowSettings {
        focus_meters: glam::Vec3::new(0.0, 0.0, 0.5),
        half_extent_meters: 4.0,
        depth_half_extent_meters: 4.0,
        resolution: 512,
        depth_bias: 0.001,
        normal_bias_meters: 0.01,
        refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy::default(),
    };
    let mut shadowed = ShadowedScene::new(&fixture.context, &engine_scene(), settings).unwrap();
    let target = OffscreenTarget::new(fixture.context.device(), view().viewport()).unwrap();
    let authority = fixture.context.submission().clone();
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let admitted = fixture.admit(&recording, signature).unwrap();
    shadowed.scene_mut().admit_programs(admitted);
    let encoded = shadowed.encode(
        &fixture.context,
        &view(),
        &target.color_attachment(),
        &target.depth_attachment(),
        &mut recording,
    );
    assert!(
        matches!(
            encoded,
            Err(ShadowError::UnsupportedFeature("player programs"))
        ),
        "got {encoded:?}"
    );
    recording.finish().cancel();
}

#[test]
fn draws_encoded_into_a_frame_of_another_authority_fail_by_name() {
    let mut fixture = stock();
    admitted_then_cancelled(&mut fixture);
    let other = context();
    let mut recording = other.submission().begin(other.device()).unwrap();
    assert_eq!(
        refusal(fixture.encode(&mut recording).unwrap_err()),
        ProgramError::Lease(LeaseError::ForeignAuthority).to_string()
    );
}

#[test]
fn an_arena_over_the_renderers_budget_is_refused_at_reservation() {
    let context = context();
    let settings = settings();
    let limits = context.device().limits();
    let own = settings.allocations().validate(&limits).unwrap();
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, settings, AllocationBudget(own + ARENA / 2))
            .unwrap();
    let before = renderer.resident_bytes(&limits).unwrap();
    assert!(matches!(
        renderer.reserve(&context, ProgramRuntime::arena_allocations(ARENA)),
        Err(ConfigurationError::Budget { .. })
    ));
    assert_eq!(renderer.resident_bytes(&limits).unwrap(), before);
    renderer
        .reserve(&context, ProgramRuntime::arena_allocations(ARENA / 2))
        .expect("an arena that fits is admitted");
}
