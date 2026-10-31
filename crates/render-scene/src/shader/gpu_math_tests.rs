use super::gpu_math_bindings::{ground_binding, math_layout};
#[test]
fn foliage_wind_fixed_time_and_integer_period_are_deterministic() {
    let context = pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))
    .unwrap();
    for (time, phase, expected) in [
        (0, 0, 0.0),
        (250, 0, 0.5),
        (1250, 0, 0.5),
        (0, 250_000, 0.5),
        (750, 0, -0.5),
    ] {
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0] = vec4<f32>(scene_wind_offset(vec4<f32>(1.0,0.0,0.0,0.5), vec2<u32>(1000u,{phase}u), {time}u), 1.0);
}}"
        );
        let result = evaluate(&context, &entry, None);
        assert!((result[0] - expected).abs() < 1e-6);
        assert_eq!(&result[1..], &[0.0, 0.0, 1.0]);
    }
}

#[test]
fn foliage_normals_follow_inverse_transpose_under_mirrored_nonuniform_scale() {
    let context = pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))
    .unwrap();
    for scale in [
        glam::Vec3::new(-2.0, 3.0, 0.5),
        glam::Vec3::new(2.0, 3.0, 0.5),
        glam::Vec3::splat(-2.0),
    ] {
        let normal = glam::Vec3::new(1.0, 2.0, 3.0).normalize();
        let expected = (normal / scale).normalize();
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let linear = mat3x3<f32>(vec3<f32>({},0.0,0.0), vec3<f32>(0.0,{},0.0), vec3<f32>(0.0,0.0,{}));
    result[0] = vec4<f32>(scene_normal(linear, normalize(vec3<f32>(1.0,2.0,3.0))), 1.0);
}}",
            scale.x, scale.y, scale.z
        );
        let result = evaluate(&context, &entry, None);
        for (actual, expected) in result.iter().zip(expected.to_array()) {
            assert!((actual - expected).abs() < 1e-6, "{scale:?}: {result:?}");
        }
    }
}

use super::ENVIRONMENT_WGSL;
use crate::device::GpuContext;
use worldrunner_render_lighting::ExposureOwnership;

#[test]
fn production_shader_preserves_guarded_specular_peak_and_shared_post_math() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let values = evaluate(&context, MATH_ENTRY, None);
    let expected_peak = 0.045_f32.powi(4) / 1e-7;
    assert!(
        (values[0] - expected_peak).abs() < 0.01,
        "specular guard changed: {} vs {expected_peak}",
        values[0]
    );
    assert!((values[1] - 0.25).abs() < 1e-6);
    assert!((values[2] - 0.5).abs() < 1e-6);
    assert!((values[3] - 0.04).abs() < 1e-6);
}

const MATH_ENTRY: &str = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    result[0] = vec4<f32>(distribution_ggx(1.0, 0.045),
        visibility_smith(1.0, 1.0, 1.0),
        rd_post_exposure(vec3<f32>(0.25), 2.0).x,
        fresnel_schlick(1.0, vec3<f32>(0.04)).x);
}
";

#[test]
fn configured_environment_reaches_production_lighting_with_exactly_one_exposure_owner() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for (exposure, multiplier) in [
        (ExposureOwnership::SceneLinear(0.25), 0.25),
        (
            ExposureOwnership::PostProcess {
                authored_stops: 0.36,
            },
            1.0,
        ),
    ] {
        let frame = configured_frame(exposure);
        let values = evaluate(
            &context,
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    result[0] = vec4(apply_exposure(surface_lighting(vec3(0.0), vec3(0.0, 0.0, 1.0),
        vec3(0.5), 1.0, 0.0, 1.0)), frame.grading.x);
}
",
            Some(&frame),
        );
        let expected = (0.98 / std::f32::consts::PI + 0.4) * multiplier;
        for actual in &values[..3] {
            assert!(
                (actual - expected).abs() < 1e-5,
                "{exposure:?}: {actual} != {expected}"
            );
        }
        assert!((values[3] - multiplier).abs() < 1e-6);
    }
}

pub(super) fn configured_frame(
    exposure: worldrunner_render_lighting::ExposureOwnership,
) -> crate::frame::FrameUniform {
    use glam::{Mat3, Mat4, Vec3};
    use worldrunner_render_lighting::{
        AmbientLight, DirectionSemantics, DirectionalLight, EnvironmentConfig, LightingFrame,
    };
    let environment = EnvironmentConfig {
        frame: LightingFrame {
            up: Vec3::Z,
            coefficient_from_world: Mat3::IDENTITY,
            direction_semantics: DirectionSemantics::LightTravel,
        },
        direct: DirectionalLight {
            direction: Vec3::NEG_Z * 4.0,
            radiance: Vec3::splat(2.0),
        },
        shadow_direction: Some(Vec3::NEG_Y),
        ambient: AmbientLight::Hemisphere {
            sky: Vec3::splat(0.8),
            ground: Vec3::splat(0.2),
        },
        exposure,
    }
    .prepare()
    .unwrap();
    crate::frame::FrameUniform {
        view_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        background_far_plane_meters: 0.0,
        specular_convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        diffuse_environment: worldrunner_render_plan::SceneDiffuseEnvironment::Hemisphere,
        height_fog: None,
        unlit: false,
        animation_time_milliseconds: 0,
        temporal: None,
        view: glam::Mat4::IDENTITY,
        view_projection: Mat4::IDENTITY,
        camera_position: Vec3::Z,
        camera_position_millimeters: [0, 0, 1_000],
        sun_direction: environment.toward_light,
        shadow_direction: environment
            .toward_shadow_light
            .unwrap_or(environment.toward_light),
        sun_ao_intensity: None,
        sun_tint: glam::Vec3::ONE,
        sun_irradiance: environment.direct_radiance,
        ambient_sky: environment.ambient_radiance(Vec3::Z),
        ambient_ground: environment.ambient_radiance(Vec3::NEG_Z),
        exposure: environment.scene_exposure,
    }
}

fn evaluate(
    context: &GpuContext,
    entry: &str,
    frame: Option<&crate::frame::FrameUniform>,
) -> Vec<f32> {
    evaluate_with_ground(context, entry, frame, None)
}

fn evaluate_with_ground(
    context: &GpuContext,
    entry: &str,
    frame: Option<&crate::frame::FrameUniform>,
    ground: Option<&[f32]>,
) -> Vec<f32> {
    evaluate_source(context, ENVIRONMENT_WGSL, entry, frame, ground)
}

pub(crate) fn evaluate_source(
    context: &GpuContext,
    source: &str,
    entry: &str,
    frame: Option<&crate::frame::FrameUniform>,
    ground: Option<&[f32]>,
) -> Vec<f32> {
    let device = context.device();
    let source = format!("{source}\n{entry}");
    let (pipeline, uniform_binding) = math_pipeline(device, &source, frame, ground.is_some());
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let uniform = frame.as_ref().map(|frame| frame_buffer(context, frame));
    let mut entries = vec![wgpu::BindGroupEntry {
        binding: 1,
        resource: output.as_entire_binding(),
    }];
    if let Some(uniform) = &uniform {
        entries.push(wgpu::BindGroupEntry {
            binding: uniform_binding,
            resource: uniform.as_entire_binding(),
        });
    }
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    });
    let ground_group = ground.map(|values| ground_binding(context, &pipeline, values));
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        if let Some(ground) = &ground_group {
            pass.set_bind_group(1, ground, &[]);
        }
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 16);
    context.queue().submit([encoder.finish()]);
    read_values(device, &readback)
}

fn frame_buffer(context: &GpuContext, frame: &crate::frame::FrameUniform) -> wgpu::Buffer {
    let device = context.device();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: crate::frame::FRAME_UNIFORM_BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.queue().write_buffer(&buffer, 0, &frame.to_bytes());
    buffer
}

fn read_values(device: &wgpu::Device, readback: &wgpu::Buffer) -> Vec<f32> {
    let (send, receive) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let values = mapped
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    drop(mapped);
    readback.unmap();
    values
}

#[test]
fn foliage_translucency_and_contact_endpoints_are_exact() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    result[0] = vec4<f32>(scene_transmission(0.0, 1.0), scene_transmission(1.0, 1.0),
        scene_contact_weight(0.0, 2.0), scene_contact_weight(2.0, 2.0));
}",
        None,
    );
    assert_eq!(result, vec![0.0, 1.0, 1.0, 0.0]);
}

#[test]
fn foliage_coverage_endpoints_and_complement_are_exact() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let uv = vec2<f32>(0.25, 0.75);
    result[0] = vec4<f32>(f32(scene_card_covered(uv, 0.0)), f32(scene_card_covered(uv, 1.0)),
        f32(scene_card_covered(uv, 0.5)), f32(!scene_card_covered(uv, 0.5)));
}",
        None,
    );
    assert_eq!(&result[..2], &[0.0, 1.0]);
    assert_eq!((result[2] + result[3]).to_bits(), 1.0_f32.to_bits());
}

#[test]
fn folding_uses_sloped_terrain_and_preserves_intermediate_length() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let terrain = [0.0, 0.0, 1.0, 2.0, 2.0, 1.0, 2.0, 0.0, 0.0, 0.75, 0.0, 0.75];
    for (height, minimum, weight) in [
        (-1.0, 0.0, 0.0),
        (0.0, 0.0, 0.0),
        (0.5, 0.0, 0.25),
        (2.0, 0.0, 1.0),
        (4.0, 0.0, 1.0),
        (0.0, 0.25, 0.25),
        (0.5, 0.25, 0.5),
        (2.0, 0.25, 1.0),
    ] {
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let ground = scene_ground(vec3<f32>(0.2, 0.3, 0.0));
    result[0] = vec4<f32>(scene_fold_direction(vec3<f32>(0.0, 1.0, 0.0),
        vec4<f32>(ground.xyz, {height}), vec2<f32>(2.0, {minimum})), ground.w);
}} "
        );
        let result = evaluate_with_ground(&context, &entry, None, Some(&terrain));
        let expected = glam::Vec3::new(-0.6, 0.0, 0.8).lerp(glam::Vec3::Y, weight);
        for (actual, expected) in result[..3].iter().zip(expected.to_array()) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert!((result[3] - 0.15).abs() < 1e-6);
    }
}

#[test]
fn mask_coupling_and_shadowed_view_dependent_scattering_match_equations() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let shape = vec4<f32>(0.0, 4.0, 2.0, 0.0);
    let light = vec3<f32>(0.0, 0.0, 1.0);
    let mask = scene_mask_response(0.8, 0.5, 0.5, true);
    result[0] = vec4<f32>(mask,
        scene_backlight_strength(light, light, light, shape, vec3<f32>(1.0, mask.y, 1.0)),
        scene_backlight_strength(light, light, light, shape, vec3<f32>(1.0, mask.y, 0.0)));
}",
        None,
    );
    assert_eq!(result, vec![0.0, 0.2, 0.4, 0.0]);
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let light = vec3<f32>(0.0, 0.0, 1.0);
    let shape = vec4<f32>(0.0, 4.0, 2.0, 0.0);
    result[0] = vec4<f32>(scene_backlight_strength(-light, light, light, shape, vec3<f32>(1.0)),
        scene_backlight_strength(vec3<f32>(1.0, 0.0, 0.0), light, light, shape, vec3<f32>(1.0)),
        scene_mask_response(0.8, 0.5, 0.5, false));
}",
        None,
    );
    assert_eq!(result, vec![0.0, 0.0, 0.0, 0.4]);
}

#[test]
fn scattering_temperature_and_unnormalised_distortion_match_numerical_reference() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let tint = scene_scattering_tint(vec3<f32>(0.4, 0.8, 0.2), vec4<f32>(1.0, 0.3, 0.1, 6500.0), 1000.0);
    let light = vec3<f32>(0.0, 0.0, 1.0);
    let strength = scene_backlight_strength(light, light, light,
        vec4<f32>(0.5, 4.0, 2.0, 0.1), vec3<f32>(0.8, 0.5, 0.25));
    result[0] = vec4<f32>(tint, strength);
}",
        None,
    );
    let expected = [0.178_656_22, 0.360_023_17, 0.062_730_25, 0.0225];
    for (actual, expected) in result.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }
}

#[test]
fn high_temperature_and_cross_branch_warming_match_independent_values() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for (kelvin, expected) in [
        (6601, [1.0, 0.986_779, 1.0]),
        (8000, [0.867_508_4, 0.901_134_1, 1.0]),
        (20000, [0.669_426_1, 0.777_986_3, 1.0]),
        (40000, [0.594_801_5, 0.727_565_8, 1.0]),
    ] {
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0] = vec4<f32>(scene_temperature_rgb({kelvin}.0), 1.0);
}} "
        );
        let result = evaluate(&context, &entry, None);
        for (actual, expected) in result[..3].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-5,
                "{kelvin}: {actual} != {expected}"
            );
        }
    }
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    result[0] = vec4<f32>(scene_scattering_tint(vec3<f32>(0.4, 0.8, 0.2),
        vec4<f32>(1.0, 0.3, 0.0, 8000.0), 2000.0), 1.0);
}",
        None,
    );
    for (actual, expected) in result[..3]
        .iter()
        .zip([0.190_972, 0.354_928_1, 0.076_935_69])
    {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }
}

#[test]
fn sh9_frame_coefficients_match_cpu_polynomial_for_every_slot_and_axis() {
    use glam::Vec3;
    use worldrunner_render_plan::{SceneDiffuseEnvironment, SceneSh9};
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for slot in 0..9 {
        let mut coefficients = [[0; 3]; 9];
        coefficients[slot] = [250_000, -125_000, 500_000];
        let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
        frame.diffuse_environment =
            SceneDiffuseEnvironment::Sh9(SceneSh9::new(coefficients).unwrap());
        let mut polynomial = [Vec3::ZERO; 9];
        polynomial[slot] = Vec3::new(0.25, -0.125, 0.5);
        for normal in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            Vec3::NEG_Z,
            Vec3::new(1.0, 2.0, 3.0).normalize(),
        ] {
            let entry = format!(
                r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0] = vec4(scene_diffuse_environment(vec3({},{},{}), vec3(99.0), vec3(99.0)), 1.0);
}}",
                normal.x, normal.y, normal.z
            );
            let actual = evaluate(&context, &entry, Some(&frame));
            let expected = worldrunner_render_lighting::irradiance_polynomial(normal, &polynomial)
                .max(Vec3::ZERO);
            for (actual, expected) in actual.iter().zip(expected.to_array()) {
                assert!((actual - expected).abs() < 1e-6, "slot {slot}, {normal:?}");
            }
        }
    }
}

#[test]
fn selected_specular_conventions_reach_production_kernels() {
    use worldrunner_render_plan::{
        SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularOcclusion,
        SceneSpecularVisibility,
    };
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for (remap, expected) in [
        (SceneRoughnessRemap::Perceptual045, 0.045),
        (SceneRoughnessRemap::Perceptual04, 0.04),
        (SceneRoughnessRemap::LinearAlpha, 0.04),
    ] {
        let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
        frame.specular_convention = SceneSpecularConvention {
            roughness: remap,
            visibility: SceneSpecularVisibility::SchlickSmith,
            occlusion: SceneSpecularOcclusion::RoughnessAware,
            ..Default::default()
        };
        let values = evaluate(
            &context,
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    result[0] = vec4(scene_roughness(0.0), scene_roughness(0.25),
        scene_specular_ao(0.5,0.5,0.5), scene_specular_brdf(vec4(0.5,0.5,0.5,0.5),0.5,vec3(0.07)).x);
}",
            Some(&frame),
        );
        assert!((values[0] - expected).abs() < 1e-6);
        let remapped = if remap == SceneRoughnessRemap::LinearAlpha {
            0.5
        } else {
            0.25
        };
        assert!((values[1] - remapped).abs() < 1e-6);
        assert!((values[2] - 0.5).abs() < 1e-6);
        let reference = worldrunner_render_lighting::distribution_ggx(0.5, 0.5, 1e-8)
            * worldrunner_render_lighting::visibility(
                0.5,
                0.5,
                0.5,
                worldrunner_render_lighting::VisibilityAlgorithm::SchlickSmithHotness,
            )
            * 0.07;
        assert!((values[3] - reference).abs() < 1e-6);
    }
}

#[test]
fn composition_coverage_shared_lighting_in_mesh_terrain_and_water_sources() {
    use worldrunner_render_plan::{
        SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularOcclusion,
        SceneSpecularVisibility,
    };
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let water = crate::water::water_source(false);
    let sources = [
        ENVIRONMENT_WGSL,
        super::TERRAIN_WGSL,
        ENVIRONMENT_WGSL,
        &water,
    ];
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let terms = scene_surface_lighting(vec3(0.0), vec3(0.0,0.0,1.0), vec3(0.3,0.5,0.7), vec3(0.25,0.2,0.4), vec3(0.092,0.132,0.172));
    result[0] = vec4(terms.color, terms.diffuse.x + terms.specular.x);
}";
    for roughness in [
        SceneRoughnessRemap::Perceptual045,
        SceneRoughnessRemap::Perceptual04,
        SceneRoughnessRemap::LinearAlpha,
    ] {
        let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
        frame.specular_convention = SceneSpecularConvention {
            roughness,
            visibility: SceneSpecularVisibility::SchlickSmith,
            occlusion: SceneSpecularOcclusion::RoughnessAware,
            ..Default::default()
        };
        let expected = evaluate_source(&context, sources[0], entry, Some(&frame), None);
        assert!((expected[0] - expected[3]).abs() < 1e-6);
        for source in &sources[1..] {
            let actual = evaluate_source(&context, source, entry, Some(&frame), None);
            for (a, b) in actual.iter().zip(&expected) {
                assert!((a - b).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn default_specular_lighting_preserves_retired_float_bytes() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let stock = concat!(
        worldrunner_render_lighting::lighting_wgsl!(),
        worldrunner_render_post::post_wgsl!(),
        include_str!("../draw/tests/fixtures/previous_scene_lighting.wgsl"),
        include_str!("camera_rays.wgsl"),
        include_str!("unshadowed.wgsl")
    );
    let frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
    for (roughness, metalness) in [(0.0, 0.0), (0.045, 0.75), (0.5, 0.0), (1.0, 1.0)] {
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0] = vec4(surface_lighting(vec3(0.0),vec3(0.0,0.0,1.0),vec3(0.3,0.5,0.7),{roughness},{metalness},0.4),1.0);
}}"
        );
        let previous = evaluate_source(&context, stock, &entry, Some(&frame), None);
        let current = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, Some(&frame), None);
        assert_eq!(
            previous.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            current.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            "roughness={roughness}, metalness={metalness}"
        );
    }
}

fn math_pipeline(
    device: &wgpu::Device,
    source: &str,
    frame: Option<&crate::frame::FrameUniform>,
    ground: bool,
) -> (wgpu::ComputePipeline, u32) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let options = frame.map_or([0; 7], |frame| frame.specular_convention.shader_options());
    let constants = crate::renderer::specular_constants(options);
    let constants = if source.contains("override SCENE_ROUGHNESS_REMAP") {
        constants.as_slice()
    } else {
        &[]
    };
    let uniform_binding = if source.contains("struct WaterFrame") {
        3
    } else {
        0
    };
    let layout = math_layout(device, frame.map(|_| uniform_binding), ground);
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("evaluate_math"),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants,
            ..Default::default()
        },
        cache: None,
    });
    (pipeline, uniform_binding)
}

#[path = "specular_tests.rs"]
mod specular_tests;

#[test]
fn relief_normal_lighting_and_projected_depth_have_independent_oracles() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
    frame.sun_irradiance = glam::Vec3::ZERO;
    let result = evaluate(
        &context,
        r"
@group(0) @binding(1) var<storage,read_write> result:array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let normal = normalize(scene_relief_decode(vec2<f32>(0.75,0.5)));
    let position = scene_relief_offset(vec3<f32>(0.25,0.25,0.5),0.125);
    let clip = frame.view_projection * vec4<f32>(position,1.0);
    result[0] = vec4<f32>(surface_lighting(position,normal,vec3<f32>(0.5),1.0,0.0,1.0),clip.z/clip.w);
}
",
        Some(&frame),
    );
    // Sphere encoding at (3/4,1/2) gives normal Z=1/2. The independent
    // hemisphere mixture is 0.5 * (0.2/4 + 0.8*3/4) = 0.325.
    for value in &result[..3] {
        assert!((value - 0.325).abs() < 0.000_001);
    }
    assert_eq!(result[3].to_bits(), 0.375_f32.to_bits());
}

#[path = "shadow_strength_tests.rs"]
mod shadow_strength_tests;

#[test]
fn byte_fade_truncates_at_band_and_byte_edges() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for distance in [1.999, 2.0, 2.001, 2.999, 3.0, 3.001, 3.999, 4.0, 4.001] {
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0] = vec4(scene_byte_coverage(scene_distance_coverage({distance}, vec2(2.0,-4.0))), 0.0, 0.0, 0.0);
}}"
        );
        let distance: f32 = distance;
        let factor = if distance <= 2.0 {
            1.0
        } else {
            ((4.0 - distance) / 2.0).max(0.0)
        };
        let expected = (factor * 255.0).trunc() / 255.0;
        let actual = evaluate(&context, &entry, None)[0];
        assert!((actual - expected).abs() < 1e-7, "distance={distance}");
    }
}

#[test]
fn partition_byte_fade_matches_every_r8_fraction_bit_for_bit() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let expected = (0_u8..=255)
        .map(|value| format!("{}u", (f32::from(value) / 255.0).to_bits()))
        .collect::<Vec<_>>()
        .join(",");
    let entry = format!(
        r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let expected = array<u32,256>({expected});
    var failures = 0u;
    for (var value = 0u; value < 256u; value++) {{
        let factor = min(1.0, (f32(value) + 0.5) / 255.0);
        if bitcast<u32>(scene_partition_byte_coverage(factor)) != expected[value] {{ failures++; }}
    }}
    result[0] = vec4<f32>(f32(failures), 0.0, 0.0, 0.0);
}}"
    );
    assert_eq!(evaluate(&context, &entry, None), vec![0.0; 4]);
}

#[path = "fade_handoff_tests.rs"]
mod fade_handoff_tests;
