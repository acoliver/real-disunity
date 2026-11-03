//! Frames that player programs cannot join are refused by name, before anything is drawn.

use super::super::*;
use super::rig::{ARENA, Fixture};
use super::{COLOR_FORMAT, DEPTH_FORMAT, settings};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::{AllocationBudget, ConfigurationError};
use worldrunner_render_gpu::submission::LeaseError;
use worldrunner_render_plan::post::{UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{FrameConfig, FrameError, HdrPostSettings, PostResolve, ToneMap};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{ProgramRuntime, TargetSignature};

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
        super::super::ssao::source(1_000_000, true, 255),
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
