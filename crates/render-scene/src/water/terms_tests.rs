use super::*;
use worldrunner_render_plan::{SceneWaterEnvironment, SceneWaterFlowCoordinates, TextureHandle};

fn half(bytes: &[u8]) -> f32 {
    let bits = u16::from_le_bytes(bytes.try_into().unwrap());
    numerical::positive_half(&(bits & 0x7fff).to_le_bytes())
        * if bits & 0x8000 == 0 { 1.0 } else { -1.0 }
}

fn render(
    context: &GpuContext,
    body: SceneWaterBody,
    samples: u32,
    mip: bool,
) -> crate::compositor::FrameRenderer {
    let mut renderer = regressions::renderer(context, samples);
    renderer.install_water(context.device(), body).unwrap();
    install_maps(context, &mut renderer, mip);
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
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    frame.sun_irradiance = glam::Vec3::ZERO;
    renderer.set_frame_time(2.0).unwrap();
    encode_fixture(context, &mut renderer, (&color, &depth), &frame, 0.5)
        .finish()
        .submit()
        .unwrap();
    renderer
}

fn install_maps(context: &GpuContext, renderer: &mut crate::compositor::FrameRenderer, mip: bool) {
    install_encoded_maps(context, renderer, mip, [204, 128]);
}

fn install_encoded_maps(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    mip: bool,
    base: [u8; 2],
) {
    let width = if mip { 128 } else { 1 };
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("water term normals"),
        size: wgpu::Extent3d {
            width,
            height: width,
            depth_or_array_layers: 1,
        },
        mip_level_count: if mip { 2 } else { 1 },
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for level in 0..texture.mip_level_count() {
        let side = width >> level;
        let encoded = if level == 0 { base } else { [128, 204] };
        context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: level,
                ..texture.as_image_copy()
            },
            &encoded.repeat(usize::try_from(side * side).unwrap()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 2),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer.water.as_mut().unwrap().surface_inputs[0] =
        [Some(view.clone()), Some(view.clone()), Some(view)];
}

fn receiver_normal(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    sample: u32,
) -> [f32; 3] {
    let bytes = lighting_tests::environment_tests::sample_read(
        context,
        &renderer.water.as_ref().unwrap().receivers[0][0],
        sample,
    );
    let center = (16 * 32 + 16) * 8;
    std::array::from_fn(|channel| half(&bytes[center + channel * 2..center + channel * 2 + 2]))
}

#[test]
fn lighting_strength_and_minified_normals_match_independent_cpu_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        for (strength, mip, phased) in [
            (0, false, false),
            (500_000, false, false),
            (2_000_000, false, true),
            (1_000_000, true, false),
            (1_000_000, true, true),
        ] {
            let mut body = settings();
            body.material.normal = Some(TextureHandle::from_raw(0));
            body.material.screen_refraction_micro = Some(0);
            body.material.lighting_normal_micro = Some(strength);
            body.material.normal_mips = mip;
            body.material.tile_millimeters = [1; 2];
            body.material.flow_period_milliseconds = phased.then_some(4000);
            let renderer = render(&context, body, samples, mip);
            let encoded = if mip {
                [128.0f64, 204.0]
            } else {
                [204.0f64, 128.0]
            };
            let xy = encoded.map(|v| v / 255.0 * 2.0 - 1.0);
            let gain = f64::from(strength) / 1_000_000.0;
            let expected = glam::DVec3::new(
                xy[0] * gain,
                xy[1] * gain,
                (1.0 - xy[0] * xy[0] - xy[1] * xy[1]).sqrt(),
            )
            .normalize();
            let frame = FrameUniform::new(&fixture_view(), None).unwrap();
            let expected = frame.view.transform_vector3(expected.as_vec3()).to_array();
            for sample in 0..samples {
                for (actual, expected) in receiver_normal(&context, &renderer, sample)
                    .into_iter()
                    .zip(expected)
                {
                    assert!(
                        (actual - expected).abs() < 0.001,
                        "{actual} vs {expected}, {samples}x, mip={mip}, phase={phased}"
                    );
                }
            }
        }
    }
}

#[test]
fn metallic_diffuse_and_direct_composition_match_cpu_without_absorption() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        for metal in [0, 100_000, 1_000_000] {
            let mut body = settings();
            body.color_micro = [250_000, 500_000, 750_000];
            body.absorption_micro = [0; 3];
            body.material.direct_surface = true;
            body.material.metallic_micro = Some(metal);
            let renderer = render(&context, body, samples, false);
            let bytes = read(&context, renderer.targets().texture());
            for (channel, color) in [0.25f64, 0.5, 0.75].into_iter().enumerate() {
                let base = (16 * 32 + 16) * 8 + channel * 2;
                let expected = color * (1.0 - f64::from(metal) / 1_000_000.0);
                assert!((f64::from(half(&bytes[base..base + 2])) - expected).abs() < 0.0005);
            }
        }
    }
}

#[test]
fn term_descriptors_validate_ranges_and_disabled_uniform_bytes() {
    let mut body = settings();
    assert_eq!(terms::uniforms(&body, 0).map(f32::to_bits), [0; 32]);
    assert_eq!(terms::mask(&[body.clone()]), 0);
    body.material.lighting_normal_micro = Some(10_000_001);
    assert!(body.validate().is_err());
    body.material.lighting_normal_micro = None;
    body.material.metallic_micro = Some(1_000_001);
    assert!(body.validate().is_err());
    body.material.metallic_micro = None;
    body.material.normal_mips = true;
    assert!(body.validate().is_err());
    body.material.normal_mips = false;
    body.material.environment = Some(SceneWaterEnvironment {
        amplitude_micro: 10_000_001,
        edge_frequency_micro: 0,
        depth_self_occlusion: false,
    });
    assert!(body.validate().is_err());
    body.material.environment = None;
    body.material.flow_coordinates = Some(SceneWaterFlowCoordinates {
        origin_millimeters: [0; 2],
        tile_millimeters: [0; 2],
    });
    assert!(body.validate().is_err());
}

#[test]
fn edge_noise_and_environment_depth_terms_match_f64_oracle() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = format!(
        "{}\n{}",
        equation_source(),
        r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let depth = array<f32, 5>(0.0, 0.01, 0.025, 0.05, 1.0)[id.x];
    let point = vec2<f32>(f32(id.x) * 0.125, -0.25);
    output[id.x * 2u] = vec4<f32>(water_edge_noise(point), water_environment_gain(point, depth), 0.0, 0.0);
}"
    );
    let values = optics_tests::compute(&context, &source);
    for (i, depth) in [0.0f64, 0.01, 0.025, 0.05, 1.0].into_iter().enumerate() {
        let noise = noise_oracle([f64::from(u32::try_from(i).unwrap()) * 0.125, -0.25]);
        let edge = 1.0 - (depth.powf(0.2) * 2.0 - 0.5).clamp(0.0, 1.0);
        let gain =
            0.75 * (1.0 - ((noise + 1.0) * 0.5).min(0.6) * edge) * (depth * 20.0).clamp(0.0, 1.0);
        assert!((f64::from(values[i * 8]) - noise).abs() < 0.00001);
        assert!((f64::from(values[i * 8 + 1]) - gain).abs() < 0.00001);
    }
}

pub(super) fn equation_source() -> String {
    format!(
        "{}\n{}",
        r"
struct TestWater { environment_terms: vec4<f32>, disk: vec4<f32>, sun_radiance: vec4<f32> }
const SCENE_ROUGHNESS_REMAP: u32 = 0u;
const water = TestWater(vec4<f32>(0.75, 1.0, 1.0, 1.0), vec4<f32>(1.0, 1.0, 0.0, 0.0), vec4<f32>(1.0));
fn fresnel_schlick(c: f32, f: vec3<f32>) -> vec3<f32> { return f; }
fn scene_environment_roughness(r: f32) -> f32 { return r; }
fn scene_specular_brdf(d: vec4<f32>, r: f32, f: vec3<f32>) -> vec3<f32> { return f; }
",
        include_str!("terms.wgsl")
    )
}

fn noise_oracle(point: [f64; 2]) -> f64 {
    let skew = (3.0f64.sqrt() - 1.0) / 2.0;
    let unskew = (3.0 - 3.0f64.sqrt()) / 6.0;
    let sum = (point[0] + point[1]) * skew;
    let cell = point.map(|v| (v + sum).floor());
    let local = [
        point[0] - cell[0] + (cell[0] + cell[1]) * unskew,
        point[1] - cell[1] + (cell[0] + cell[1]) * unskew,
    ];
    let step = if local[0] > local[1] {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    };
    let mut result = 0.0;
    for offset in [[0.0, 0.0], step, [1.0, 1.0]] {
        let delta = [
            local[0] - offset[0] + (offset[0] + offset[1]) * unskew,
            local[1] - offset[1] + (offset[0] + offset[1]) * unskew,
        ];
        let permute = |v: f64| (v * (34.0 * v + 1.0)).rem_euclid(289.0);
        let hash = permute(
            permute((cell[1] + offset[1]).rem_euclid(289.0))
                + (cell[0] + offset[0]).rem_euclid(289.0),
        );
        let x = 2.0 * (hash / 41.0).fract() - 1.0;
        let gradient = [x - (x + 0.5).floor(), x.abs() - 0.5];
        let distance = delta[0] * delta[0] + delta[1] * delta[1];
        let norm = 1.792_842_914_001_59
            - 0.853_734_720_953_14 * (gradient[0] * gradient[0] + gradient[1] * gradient[1]);
        result += (0.5 - distance).max(0.0).powi(4)
            * norm
            * (gradient[0] * delta[0] + gradient[1] * delta[1]);
    }
    result * 130.0
}

#[test]
fn finite_disk_selects_closest_direction_with_wgsl_trigonometric_bounds() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = format!(
        "{}\n{}",
        equation_source(),
        r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let angle = array<f32, 5>(0.0, 0.001, 0.01, 1.0, 3.0)[id.x];
    let ray = normalize(vec3<f32>(sin(angle), 0.0, cos(angle)));
    output[id.x * 2u] = vec4<f32>(water_disk_direction(ray, vec3<f32>(0.0, 0.0, 1.0)), 0.0);
}"
    );
    let values = optics_tests::compute(&context, &source);
    for (i, angle) in [0.0f64, 0.001, 0.01, 1.0, 3.0].into_iter().enumerate() {
        let selected = angle.min(0.0027);
        let expected = [selected.sin(), 0.0, selected.cos()];
        // Trig perturbs the unit input by at most sqrt(2)*2^-11. Normalizing
        // changes its direction by at most asin(e/(1-e)). The cosine disk test
        // can select an interior ray up to acos(cos(radius)-2^-11) from center.
        // Include that boundary uncertainty and normalization of the rim vector.
        let trig = 2.0f64.powi(-11);
        let error = 2.0f64.sqrt() * trig;
        let normalized_angle = (error / (1.0 - error)).asin();
        let selection_angle = (0.0027f64.cos() - trig).acos() - 0.0027;
        let bound = normalized_angle
            + selection_angle
            + 2.0 * error / (1.0 - error)
            + 64.0 * f64::from(f32::EPSILON);
        for (actual, expected) in values[i * 8..i * 8 + 3].iter().zip(expected) {
            assert!((f64::from(*actual) - expected).abs() <= bound);
        }
    }
}

#[test]
fn finite_highlight_peak_and_zero_amplitudes_match_cpu() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for (input, peak, amplitude) in [
        ([0.000_001f64; 3], 1.0, 1.0),
        ([0.25; 3], 1.0, 1.0),
        ([1.0; 3], 1.0, 1.0),
        ([2.0, 1.0, 0.5], 1.0, 1.0),
        ([0.000_001; 3], 0.0, 1.0),
        ([2.0, 1.0, 0.5], 1.0, 0.0),
    ] {
        let helpers = equation_source().replace(
            "vec4<f32>(1.0, 1.0, 0.0, 0.0)",
            &format!("vec4<f32>({amplitude}, {peak}, 0.0, 0.0)"),
        );
        let source = format!(
            "{helpers}\n{}",
            format_args!(
                r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let value = water_disk_highlight(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, 1.0), vec3<f32>({}, {}, {}), 0.1, 1.0);
    output[id.x * 2u] = vec4<f32>(value, 0.0);
}}",
                input[0], input[1], input[2]
            )
        );
        let actual = optics_tests::compute(&context, &source);
        let luminance = input[0] * 0.2126 + input[1] * 0.7152 + input[2] * 0.0722;
        let scale = if luminance > peak {
            peak / luminance
        } else {
            1.0
        };
        for (value, input) in actual[..3].iter().zip(input) {
            let expected = input * scale * amplitude;
            assert!((f64::from(*value) - expected).abs() <= expected.abs() * 0.000_001 + 1e-13);
        }
    }
}

#[test]
fn metallic_receiver_keeps_rgb_f0_and_oct_normal_with_no_extra_textures() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut body = settings();
        body.color_micro = [250_000, 500_000, 750_000];
        body.material.metallic_micro = Some(100_000);
        let renderer = render(&context, body, samples, false);
        for sample in 0..samples {
            let water = renderer.water.as_ref().unwrap();
            let normal = lighting_tests::environment_tests::sample_read(
                &context,
                &water.receivers[0][0],
                sample,
            );
            let material = lighting_tests::environment_tests::sample_read(
                &context,
                &water.receivers[0][1],
                sample,
            );
            let offset = (16 * 32 + 16) * 8;
            let actual = [
                half(&normal[offset + 4..offset + 6]),
                half(&material[offset + 4..offset + 6]),
                -half(&normal[offset + 6..offset + 8]) - 1.0,
            ];
            for (value, color) in actual.into_iter().zip([0.25f64, 0.5, 0.75]) {
                let expected = 0.04 * 0.9 + color * 0.1;
                assert!((f64::from(value) - expected).abs() < 0.001);
            }
        }
    }
}

#[test]
fn octahedral_receivers_decode_normal_and_coloured_f0() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = format!(
        "{}\n{}\n{}",
        equation_source(),
        include_str!("../ssr/water_receiver.wgsl"),
        r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let normals = array<vec3<f32>, 5>(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 2.0, -3.0), vec3<f32>(-2.0, 1.0, 3.0), vec3<f32>(0.0, 0.0, -1.0), vec3<f32>(-1.0, -2.0, -3.0));
    let packed = vec4<f32>(water_pack_normal(normalize(normals[id.x])), 0.06, -1.11);
    output[id.x * 2u] = water_receiver_normal(packed);
    output[id.x * 2u + 1u] = vec4<f32>(water_receiver_f0(packed, vec4<f32>(0.0, 0.0, 0.08, 0.0)), 0.0);
}"
    );
    let actual = optics_tests::compute(&context, &source);
    for (i, normal) in [
        [0.0f64, 0.0, 1.0],
        [1.0, 2.0, -3.0],
        [-2.0, 1.0, 3.0],
        [0.0, 0.0, -1.0],
        [-1.0, -2.0, -3.0],
    ]
    .into_iter()
    .enumerate()
    {
        let length = normal.into_iter().map(|v| v * v).sum::<f64>().sqrt();
        for (value, expected) in actual[i * 8..i * 8 + 3]
            .iter()
            .zip(normal.map(|v| v / length))
        {
            assert!((f64::from(*value) - expected).abs() < 0.000_001);
        }
        for (value, expected) in actual[i * 8 + 4..i * 8 + 7]
            .iter()
            .zip([0.06f64, 0.08, 0.11])
        {
            assert!((f64::from(*value) - expected).abs() < 0.000_001);
        }
    }
}

#[test]
fn zero_wind_has_identical_pixels_and_receiver_bytes_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut body = settings();
        body.material.normal = Some(TextureHandle::from_raw(0));
        body.material.flow = Some(TextureHandle::from_raw(0));
        body.material.flow_period_milliseconds = Some(4000);
        let original = render(&context, body.clone(), samples, false);
        body.material.wind = Some(worldrunner_render_plan::SceneWaterWind {
            field: worldrunner_render_plan::ScenePlanarWindField::new(
                TextureHandle::from_raw(0),
                [0; 2],
                [1000; 2],
                [None; 2],
            )
            .unwrap(),
            band: worldrunner_render_plan::SceneWindBand::new(1_000_000, 500_000, 2_000_000)
                .unwrap(),
            amplitude_micro: 0,
        });
        let disabled = render(&context, body, samples, false);
        assert_eq!(
            read(&context, original.targets().texture()),
            read(&context, disabled.targets().texture())
        );
        for sample in 0..samples {
            for receiver in 0..3 {
                let a = &original.water.as_ref().unwrap().receivers[0][receiver];
                let b = &disabled.water.as_ref().unwrap().receivers[0][receiver];
                assert_eq!(
                    lighting_tests::environment_tests::sample_read(&context, a, sample),
                    lighting_tests::environment_tests::sample_read(&context, b, sample)
                );
            }
        }
    }
}

#[test]
fn opposing_wind_and_tangent_normal_use_flat_limit_at_1x_and_4x() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let mut body = settings();
        body.material.normal = Some(TextureHandle::from_raw(0));
        body.material.flow = Some(TextureHandle::from_raw(0));
        body.material.flow_period_milliseconds = Some(4000);
        body.material.wind = Some(worldrunner_render_plan::SceneWaterWind {
            field: worldrunner_render_plan::ScenePlanarWindField::new(
                TextureHandle::from_raw(0),
                [0; 2],
                [1000; 2],
                [None; 2],
            )
            .unwrap(),
            band: worldrunner_render_plan::SceneWindBand::new(1_000_000, 0, 0).unwrap(),
            amplitude_micro: 1_000_000,
        });
        let mut renderer = render(&context, body, samples, false);
        install_encoded_maps(&context, &mut renderer, false, [255, 0]);
        draw_water_slot(&context, &renderer, 0);
        assert_finite_half(&read(&context, renderer.targets().texture()));
        let expected = FrameUniform::new(&fixture_view(), None)
            .unwrap()
            .view
            .transform_vector3(glam::Vec3::Z);
        for sample in 0..samples {
            for receiver in &renderer.water.as_ref().unwrap().receivers[0] {
                assert_finite_half(&lighting_tests::environment_tests::sample_read(
                    &context, receiver, sample,
                ));
            }
            for (actual, expected) in receiver_normal(&context, &renderer, sample)
                .into_iter()
                .zip(expected.to_array())
            {
                assert!((actual - expected).abs() < 0.001);
            }
        }
    }
}

fn assert_finite_half(bytes: &[u8]) {
    assert!(
        bytes
            .chunks_exact(2)
            .all(|v| { u16::from_le_bytes(v.try_into().unwrap()) & 0x7c00 != 0x7c00 })
    );
}
