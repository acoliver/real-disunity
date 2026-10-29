//! CMAA2 followed by CAS in the HDR post chain: CAS is scheduled after the resolve exactly as
//! it is after FXAA and SMAA, and with CAS off the CMAA2 stages are the five of CMAA2 alone.
use crate::chain::post_effect_tests::{chain, pixels, step};
use crate::{CasSettings, Cmaa2Preset, HdrPostSettings, PostResolve};

use super::smaa_gpu_tests::{Rig, changed};

/// Radiance ramps from 0.5 to 2.5 over six pixels: a soft vertical edge with mid-range contrast
/// that CAS sharpens.
const SOFT_EDGE: &str =
    "let v=0.5+2.0*clamp((p.x-8.0)/6.0,0.0,1.0);return vec4<f32>(v,v*0.8,v*0.6,0.75);";
const STEPPED: &str =
    "let edge=select(8.0,9.0,p.y>8.0);let v=select(0.0,3.0,p.x>edge);return vec4<f32>(v,v,v,1.0);";
const CMAA2_STAGES: [&str; 5] = [
    "cmaa2_clear",
    "cmaa2_edges",
    "cmaa2_candidates",
    "cmaa2_apply",
    "cmaa2_present",
];

fn settings(sharpen: Option<CasSettings>) -> HdrPostSettings {
    HdrPostSettings {
        resolve: PostResolve::cmaa2(Cmaa2Preset::High),
        sharpen,
        ..HdrPostSettings::default()
    }
}

fn stage_names(rig: &Rig, settings: &HdrPostSettings) -> Vec<&'static str> {
    chain(&rig.device, settings, [8, 8]).configured[0]
        .stages
        .iter()
        .map(|stage| stage.id.name)
        .collect()
}

fn render(rig: &Rig, settings: &HdrPostSettings, size: [u32; 2], body: &str) -> Vec<u8> {
    let chain = chain(&rig.device, settings, size);
    let out = step(&rig.device, &rig.queue, &chain, 0, body, false);
    pixels(&rig.device, &rig.queue, &out)
}

#[test]
fn cas_off_schedules_only_the_cmaa2_stages() {
    let rig = Rig::new();
    let mut expected = vec!["tone"];
    expected.extend(CMAA2_STAGES);
    assert_eq!(stage_names(&rig, &settings(None)), expected);
    assert_eq!(
        settings(None),
        HdrPostSettings {
            resolve: PostResolve::cmaa2(Cmaa2Preset::High),
            ..HdrPostSettings::default()
        }
    );
}

#[test]
fn cmaa2_with_cas_off_is_byte_identical_to_cmaa2_alone() {
    let rig = Rig::new();
    let size = [24, 24];
    for body in [SOFT_EDGE, STEPPED] {
        let alone = render(&rig, &settings(None), size, body);
        let default = render(
            &rig,
            &HdrPostSettings {
                resolve: PostResolve::cmaa2(Cmaa2Preset::High),
                ..HdrPostSettings::default()
            },
            size,
            body,
        );
        assert_eq!(alone, default);
    }
    // The stepped edge is blended, so the identity above is not a no-op comparison.
    let plain = render(&rig, &HdrPostSettings::default(), size, STEPPED);
    let blended = render(&rig, &settings(None), size, STEPPED);
    assert!(!changed(&plain, &blended, size[0]).is_empty());
}

#[test]
fn one_cas_pass_follows_the_cmaa2_present_pass() {
    let rig = Rig::new();
    let mut expected = vec!["tone"];
    expected.extend(CMAA2_STAGES);
    expected.push("cas");
    assert_eq!(
        stage_names(&rig, &settings(Some(CasSettings::new(0.5)))),
        expected
    );
}

#[test]
fn cas_sharpens_a_soft_edge_after_cmaa2_and_keeps_alpha() {
    let rig = Rig::new();
    let size = [24, 8];
    let plain = render(&rig, &settings(None), size, SOFT_EDGE);
    let soft = render(
        &rig,
        &settings(Some(CasSettings::new(0.0))),
        size,
        SOFT_EDGE,
    );
    let hard = render(
        &rig,
        &settings(Some(CasSettings::new(1.0))),
        size,
        SOFT_EDGE,
    );
    assert_ne!(plain, soft);
    assert_ne!(soft, hard);
    for sharpened in [&soft, &hard] {
        assert!(
            plain
                .chunks_exact(4)
                .zip(sharpened.chunks_exact(4))
                .all(|(a, b)| a[3] == b[3]),
            "alpha is not filtered"
        );
    }
}

#[test]
fn constant_frames_and_odd_extents_are_unchanged_by_cmaa2_then_cas() {
    let rig = Rig::new();
    let flat = "return vec4<f32>(0.5,1.25,2.0,0.75);";
    for size in [[1, 1], [2, 1], [3, 3], [17, 9]] {
        let plain = render(&rig, &settings(None), size, flat);
        for sharpness in [0.0, 1.0] {
            let sharp = render(
                &rig,
                &settings(Some(CasSettings::new(sharpness))),
                size,
                flat,
            );
            assert_eq!(plain, sharp, "{size:?} sharpness {sharpness}");
        }
    }
}
