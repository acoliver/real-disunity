use super::{ENVIRONMENT_WGSL, gpu_math_tests::evaluate_source};
use crate::{device::GpuContext, sky::render_tests::test_frame};
use glam::Vec3;
use worldrunner_render_plan::{SceneDirectDiffuse, SceneSurfaceDisk};

#[test]
fn prepared_diffuse_matches_albedo_without_pi_or_fresnel() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut frame = test_frame(1.0);
    frame.unlit = false;
    frame.camera_position = Vec3::Z;
    frame.sun_direction = Vec3::Z;
    frame.sun_irradiance = Vec3::splat(2.0);
    frame.ambient_sky = Vec3::ZERO;
    frame.ambient_ground = Vec3::ZERO;
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
 result[0]=vec4(scene_surface_lighting(vec3(0.0),vec3(0.0,0.0,1.0),vec3(0.2),vec3(0.5,0.25,1.0),vec3(0.04)).diffuse,1.0);
}";
    for policy in [
        SceneDirectDiffuse::EnergyConserving,
        SceneDirectDiffuse::LambertianPrepared,
    ] {
        frame.specular_convention.direct_diffuse = policy;
        let actual = evaluate_source(&context, ENVIRONMENT_WGSL, entry, Some(&frame), None);
        let expected = if policy == SceneDirectDiffuse::LambertianPrepared {
            0.2 * 0.75 * 2.0
        } else {
            let fresnel = 0.04 * 0.75 + 0.2 * 0.25;
            (1.0 - fresnel) * 0.2 * 0.75 * 2.0 / std::f64::consts::PI
        };
        assert!((f64::from(actual[0]) - expected).abs() < 2e-7);
    }
}

#[test]
fn finite_surface_disk_has_independent_intensity_and_caps_after_shadow() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut frame = test_frame(1.0);
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
 result[0]=vec4(scene_disk_highlight(vec3(0.0,0.0,1.0),vec3(0.0,0.0,1.0),vec3(0.0,0.0,1.0),vec3(0.04),0.5,0.25),1.0);
}";
    for (intensity, peak) in [(0, 1_000_000), (2_000_000, 1_000_000), (2_000_000, 10_000)] {
        frame.specular_convention.surface_disk =
            Some(SceneSurfaceDisk::new(intensity, peak).unwrap());
        for sun in [0.0, 7.0] {
            frame.sun_irradiance = Vec3::splat(sun);
            let actual = evaluate_source(&context, ENVIRONMENT_WGSL, entry, Some(&frame), None);
            let expected = (0.04 / (4.0 * std::f64::consts::PI * 0.5_f64.powi(4))
                * f64::from(intensity)
                / 1e6
                * 0.25)
                .min(f64::from(peak) / 1e6);
            assert!((f64::from(actual[0]) - expected).abs() < 2e-7);
        }
    }
    assert!(SceneSurfaceDisk::new(1, 0).is_err());
}

#[test]
fn shared_disk_direction_matches_geometric_circle_projection() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let start = ENVIRONMENT_WGSL
        .find("    let half_vector = normalize(view + selected);")
        .unwrap();
    let end = start + ENVIRONMENT_WGSL[start..].find("\n}").unwrap();
    let mut source = ENVIRONMENT_WGSL.to_owned();
    source.replace_range(start..end, "    return selected;");
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
 result[0]=vec4(scene_disk_highlight(vec3(0.0,0.0,1.0),normalize(vec3(-0.2,0.0,1.0)),vec3(0.0,0.0,1.0),vec3(0.04),0.1,1.0),1.0);
}";
    let mut frame = test_frame(1.0);
    for radius_micro in [2700, 17_453] {
        frame.specular_convention.surface_disk = Some(
            SceneSurfaceDisk::new(1_000_000, 1_000_000)
                .unwrap()
                .with_angular_radius_microradians(radius_micro)
                .unwrap(),
        );
        let actual = evaluate_source(&context, &source, entry, Some(&frame), None);
        let radius = f64::from(radius_micro) / 1_000_000.0;
        let expected = [radius.sin(), 0.0, radius.cos()];
        // WGSL sin/cos absolute error is bounded by 2^-11; normalization adds rounding.
        let tolerance = 4.0 * 2.0_f64.powi(-11);
        for (actual, expected) in actual[..3].iter().zip(expected) {
            assert!((f64::from(*actual) - expected).abs() < tolerance);
        }
    }
}

#[test]
fn sun_tint_scales_lambertian_diffuse_but_leaves_ao_relief_untinted() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut frame = test_frame(1.0);
    frame.unlit = false;
    frame.camera_position = Vec3::Z;
    frame.sun_direction = Vec3::Z;
    frame.ambient_sky = Vec3::ZERO;
    frame.ambient_ground = Vec3::ZERO;
    frame.specular_convention.direct_diffuse = SceneDirectDiffuse::LambertianPrepared;
    frame.specular_convention.occlusion =
        worldrunner_render_plan::SceneSpecularOcclusion::RoughnessAware;
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
 result[0]=vec4(scene_surface_lighting(vec3(0.0),vec3(0.0,0.0,1.0),vec3(0.2),vec3(0.5,0.25,0.25),vec3(0.04)).diffuse,1.0);
}";
    for tint in [Vec3::ONE, Vec3::new(0.0, 2.0, 0.5)] {
        frame.sun_tint = tint;
        frame.sun_irradiance = tint * 1.5;
        frame.sun_ao_intensity = (tint != Vec3::ONE).then_some(1.5);
        let actual = evaluate_source(&context, ENVIRONMENT_WGSL, entry, Some(&frame), None);
        let ao = 0.25 + 0.75 * 0.5;
        for (actual, tint) in actual[..3].iter().zip(tint.to_array()) {
            let expected = 0.2 * 0.75 * 1.5 * ao * f64::from(tint);
            assert!((f64::from(*actual) - expected).abs() < 2e-7);
        }
    }
}

#[test]
fn disk_uses_sun_tint_independently_of_direct_intensity_and_limits_after_tint() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut frame = test_frame(1.0);
    frame.sun_tint = Vec3::new(0.0, 2.0, 0.5);
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
 result[0]=vec4(scene_disk_highlight(vec3(0.0,0.0,1.0),vec3(0.0,0.0,1.0),vec3(0.0,0.0,1.0),vec3(0.04),0.5,0.25),1.0);
}";
    for peak in [10_000, 1_000_000] {
        frame.specular_convention.surface_disk =
            Some(SceneSurfaceDisk::new(2_000_000, peak).unwrap());
        for sun in [0.0, 7.0] {
            frame.sun_ao_intensity = Some(sun);
            frame.sun_irradiance = frame.sun_tint * sun;
            let actual = evaluate_source(&context, ENVIRONMENT_WGSL, entry, Some(&frame), None);
            let base = 0.04 / (4.0 * std::f64::consts::PI * 0.5_f64.powi(4)) * 2.0 * 0.25;
            let luminance = base * (2.0 * 0.7152 + 0.5 * 0.0722);
            let scale = (f64::from(peak) / 1e6 / luminance).min(1.0);
            for (actual, tint) in actual[..3].iter().zip(frame.sun_tint.to_array()) {
                assert!((f64::from(*actual) - base * f64::from(tint) * scale).abs() < 2e-7);
            }
        }
    }
}

pub(crate) fn fixed_disk_source(source: &str) -> String {
    let start = source.find("fn scene_disk_direction(").unwrap();
    let end = start + source[start..].find("\n}").unwrap() + 2;
    let mut fixed = source.to_owned();
    fixed.replace_range(start..end, include_str!("disk_reference.wgsl"));
    fixed
}

#[test]
fn default_surface_highlight_bytes_match_fixed_disk() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut frame = test_frame(1.0);
    frame.specular_convention.surface_disk =
        Some(SceneSurfaceDisk::new(1_000_000, 1_000_000).unwrap());
    let fixed = fixed_disk_source(ENVIRONMENT_WGSL);
    for angle in [0.0, 0.001, 0.05, 1.0, 3.0] {
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
 let view=normalize(vec3(sin({angle}),0.0,cos({angle})));
 result[0]=vec4(scene_disk_highlight(vec3(0.0,0.0,1.0),view,vec3(0.0,0.0,1.0),vec3(0.04),0.1,1.0),1.0);
}}"
        );
        let current = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, Some(&frame), None);
        let previous = evaluate_source(&context, &fixed, &entry, Some(&frame), None);
        assert_eq!(
            current.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            previous.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
    }
}
