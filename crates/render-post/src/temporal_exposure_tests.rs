//! Display exposure guard of the temporal path: the history is the scene referred reconstruction
//! copied before the tone stage, so a change of the display exposure with identical radiance must
//! leave the history texture unchanged while it changes the tone mapped bytes. The chain is built
//! from `HdrPostSettings` with a fixed adaptive exposure of `2^ev` (the fixed display exposure of
//! `ToneMapOutput` is limited to 0..=4, the adaptive range reaches 64).
use crate::hdr_temporal_build_tests::settings;
use crate::temporal_scene_test_rig::{OUTPUT, SceneRig};
use crate::temporal_thin_feature_scene::{Fixture, fixtures};
use crate::{AdaptiveExposure, ExposureMetering, HdrPostSettings, ProjectionJitter};
use worldrunner_render_plan::post::RenderScale;

const SWEEP_EV: [i32; 5] = [-6, -3, 0, 3, 6];
/// Pan per frame in canvas pixels: the history is reprojected, not only copied.
const SPEED: f32 = 0.1;

fn exposed(ev: i32) -> HdrPostSettings {
    HdrPostSettings {
        adaptive: Some(AdaptiveExposure {
            metering: ExposureMetering::TrimmedMean,
            log_range: [-12.0, 12.0],
            percentiles: [0.1, 0.9],
            key: 0.18,
            exposure_range: [0.01, 64.0],
            speeds: [3.0, 1.0],
            fixed_delta: Some(0.25),
            fixed_exposure: Some(f32::from(i8::try_from(ev).unwrap()).exp2()),
        }),
        ..settings()
    }
}

fn rig(gpu: &(wgpu::Device, wgpu::Queue), fixture: &Fixture, ev: i32) -> SceneRig {
    let render = RenderScale::new(670_000).unwrap().extent(OUTPUT).unwrap();
    assert!(ProjectionJitter::for_extents(render, OUTPUT).is_ok());
    SceneRig::with_settings(gpu, [render, OUTPUT], fixture.wgsl(), &exposed(ev))
}

/// History texels of every frame of two jitter cycles, and the display bytes of the last frame.
fn run(rig: &SceneRig) -> (Vec<Vec<[f64; 4]>>, Vec<u8>) {
    let frames = 2 * u64::from(rig.jitter.length());
    let mut history = Vec::new();
    for index in 0..frames {
        let pan = SPEED * f32::from(u16::try_from(index).unwrap());
        rig.frame(index, index > 0, ([pan, SPEED, 1.0, 0.0], None));
        history.push(rig.history_image());
    }
    (history, rig.display_bytes())
}

/// One half precision step: 2^-11 relative, with a 2^-24 absolute floor below the smallest normal.
fn within_half_precision(a: f64, b: f64) -> bool {
    (a - b).abs() <= a.abs().max(b.abs()) * 2f64.powi(-11) + 2f64.powi(-24)
}

#[test]
fn display_exposure_leaves_the_pre_tone_history_unchanged() {
    let gpu = crate::gpu_tests::gpu();
    let fixture = fixtures()
        .into_iter()
        .find(|f| f.name == "wire 45")
        .unwrap();
    let (reference, reference_bytes) = run(&rig(&gpu, &fixture, 0));
    for ev in SWEEP_EV.into_iter().filter(|ev| *ev != 0) {
        let (history, bytes) = run(&rig(&gpu, &fixture, ev));
        let differing = bytes
            .iter()
            .zip(&reference_bytes)
            .filter(|(a, b)| a != b)
            .count();
        // The sweep is not vacuous: the exposure changes what the display shows.
        assert!(differing > bytes.len() / 4, "ev {ev}: {differing} bytes");
        for (frame, (a, b)) in history.iter().zip(&reference).enumerate() {
            for (texel, (x, y)) in a.iter().zip(b).enumerate() {
                for channel in 0..3 {
                    assert!(
                        within_half_precision(x[channel], y[channel]),
                        "ev {ev} frame {frame} texel {texel} channel {channel}: {} against {}",
                        x[channel],
                        y[channel]
                    );
                }
            }
        }
    }
}
