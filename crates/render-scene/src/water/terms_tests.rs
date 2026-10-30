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
    let disk = body.material.sun_disk.is_some();
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
    if disk {
        frame.unlit = false;
        frame.sun_direction = glam::Vec3::new(0.0, 0.6, 0.8);
        frame.sun_irradiance = glam::Vec3::ONE;
    }
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
fn lighting_strength_and_minified_normals_match_independent_cpu_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
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
    for samples in crate::msaa_tests::supported_samples(&context) {
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
        zero_frequency: worldrunner_render_plan::SceneWaterZeroFrequency::default(),
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
fn zero_frequency_modes_match_constant_half_or_skip_oracle() {
    use worldrunner_render_plan::SceneWaterZeroFrequency;
    let context = pollster::block_on(GpuContext::request()).unwrap();
    assert_eq!(
        SceneWaterZeroFrequency::default(),
        SceneWaterZeroFrequency::Skip
    );
    for (mode, flag) in [
        (SceneWaterZeroFrequency::Skip, 1.0),
        (SceneWaterZeroFrequency::ConstantHalf, 2.0),
    ] {
        let mut body = settings();
        body.material.environment = Some(SceneWaterEnvironment {
            amplitude_micro: 750_000,
            edge_frequency_micro: 0,
            zero_frequency: mode,
            depth_self_occlusion: false,
        });
        body.validate().unwrap();
        let encoded = terms::uniforms(&body, 0);
        assert_eq!(&encoded[4..8], &[0.75, 0.0, 0.0, flag]);
        let equation = equation_source().replace(
            "vec4<f32>(0.75, 1.0, 1.0, 1.0)",
            &format!(
                "vec4<f32>({}, {}, {}, {})",
                encoded[4], encoded[5], encoded[6], encoded[7]
            ),
        );
        let source = format!(
            "{equation}\n{}",
            r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let depth = array<f32, 5>(0.0, 0.01, 0.025, 0.05, 1.0)[id.x];
    output[id.x * 2u] = vec4<f32>(water_environment_gain(vec2<f32>(17.0, -31.0), depth), 0.0, 0.0, 0.0);
}"
        );
        let actual = optics_tests::compute(&context, &source);
        for (i, depth) in [0.0f64, 0.01, 0.025, 0.05, 1.0].into_iter().enumerate() {
            let edge = 1.0 - (2.0 * depth.powf(0.2) - 0.5).clamp(0.0, 1.0);
            let expected = if mode == SceneWaterZeroFrequency::Skip {
                0.75
            } else {
                0.75 * (1.0 - 0.5 * edge)
            };
            assert!((f64::from(actual[i * 8]) - expected).abs() < 0.00001);
            if mode == SceneWaterZeroFrequency::Skip {
                assert_eq!(actual[i * 8].to_bits(), 0.75f32.to_bits());
            }
        }
    }
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
        "{}\n{}\n{}",
        worldrunner_render_lighting::lighting_wgsl!(),
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
    for radius_micro in [2700, 17_453] {
        let radius = f64::from(radius_micro) / 1_000_000.0;
        let source = format!(
            "{}\n{}",
            equation_source(),
            format_args!(
                r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let angle = array<f32, 5>(0.0, 0.001, 0.05, 1.0, 3.0)[id.x];
    let ray = normalize(vec3<f32>(sin(angle), 0.0, cos(angle)));
    output[id.x * 2u] = vec4<f32>(scene_disk_direction(ray, vec3<f32>(0.0, 0.0, 1.0), {radius}), 0.0);
}}"
            )
        );
        let values = optics_tests::compute(&context, &source);
        for (i, angle) in [0.0f64, 0.001, 0.05, 1.0, 3.0].into_iter().enumerate() {
            let selected = angle.min(radius);
            let expected = [selected.sin(), 0.0, selected.cos()];
            // WGSL sin/cos absolute error is at most 2^-11. Propagate input
            // normalization and the cosine boundary uncertainty to direction.
            let trig = 2.0f64.powi(-11);
            let error = 2.0f64.sqrt() * trig;
            let bound = (error / (1.0 - error)).asin() + (radius.cos() - trig).acos() - radius
                + 2.0 * error / (1.0 - error)
                + 64.0 * f64::from(f32::EPSILON);
            for (actual, expected) in values[i * 8..i * 8 + 3].iter().zip(expected) {
                assert!((f64::from(*actual) - expected).abs() <= bound);
            }
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
    for samples in crate::msaa_tests::supported_samples(&context) {
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
fn zero_wind_has_identical_pixels_and_receiver_bytes_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
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
fn opposing_wind_and_tangent_normal_use_flat_limit_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
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

#[test]
fn default_disk_uniform_and_rendered_bytes_match_fixed_radius() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut body = settings();
    body.material.sun_disk = Some(worldrunner_render_plan::SceneWaterSunDisk::default());
    let mut expected_uniforms = [0.0f32; 32];
    expected_uniforms[11] = 1.0;
    expected_uniforms[12] = 1.0;
    expected_uniforms[13] = 1.0;
    assert_eq!(
        terms::uniforms(&body, 0).map(f32::to_bits),
        expected_uniforms.map(f32::to_bits)
    );
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut renderer = render(&context, body.clone(), samples, false);
        draw_water_slot(&context, &renderer, 0);
        let expected = read(&context, renderer.targets().texture());
        let fixed = crate::shader::surface_policy_tests::fixed_disk_source(&water_source(false));
        let water = renderer.water.as_mut().unwrap();
        let fixed_pipeline = pipeline_with_source(
            context.device(),
            samples,
            &water.layout,
            false,
            [0, 0, 0, 0, 32],
            &fixed,
        );
        water
            .specializations
            .borrow_mut()
            .insert([0, 0, 0, 0, 32], [fixed_pipeline.clone(), fixed_pipeline]);
        draw_water_slot(&context, &renderer, 0);
        // The fixed-radius source folds cos(0.0027) and sin(0.0027) like the
        // default branch of the production source, so the two are mathematically
        // equal. Separate shader compiles may contract or order the f32
        // operations differently, which moves a result by a few f32 ULP, far
        // below one float16 step: after the identical float16 store the words
        // are equal or adjacent. Measured on RADV: 1 of 4096 words, by one step.
        let actual = read(&context, renderer.targets().texture());
        assert!(
            crate::tolerance_tests::max_half_steps(&actual, &expected) <= 1,
            "samples={samples}"
        );
        let mut dark_body = body.clone();
        dark_body
            .material
            .sun_disk
            .as_mut()
            .unwrap()
            .amplitude_micro = 0;
        let dark = render(&context, dark_body, samples, false);
        assert_ne!(read(&context, dark.targets().texture()), expected);
    }
    body.material
        .sun_disk
        .as_mut()
        .unwrap()
        .angular_radius_microradians = 17_453;
    assert_eq!(
        terms::uniforms(&body, 0)[14].to_bits(),
        17_453.0f32.to_bits()
    );
}

#[test]
fn water_call_site_uses_each_bodys_radius() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for radius in [2700, 17_453] {
        let mut body = settings();
        body.material.sun_disk = Some(worldrunner_render_plan::SceneWaterSunDisk {
            angular_radius_microradians: radius,
            ..Default::default()
        });
        let encoded = terms::uniforms(&body, 0)[14];
        let source = equation_source().replace(
            "vec4<f32>(1.0, 1.0, 0.0, 0.0)",
            &format!("vec4<f32>(1.0, 1.0, {encoded}, 0.0)"),
        );
        // Observe selection before BRDF evaluation, preserving the production call site.
        let start = source.find("    let halfway =").unwrap();
        let end = start + source[start..].find("\n}").unwrap();
        let mut source = source;
        source.replace_range(start..end, "    return selected;");
        source.push_str(r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
 let view=normalize(vec3<f32>(-0.2,0.0,1.0));
 output[id.x*2u]=vec4<f32>(water_disk_highlight(vec3<f32>(0.0,0.0,1.0),view,vec3<f32>(0.0,0.0,1.0),vec3<f32>(0.04),0.1,1.0),0.0);
}");
        let actual = optics_tests::compute(&context, &source);
        let radians = f64::from(radius) / 1_000_000.0;
        // sin/cos each have absolute error <= 2^-11, plus rim normalization.
        for (value, expected) in actual[..3].iter().zip([radians.sin(), 0.0, radians.cos()]) {
            assert!((f64::from(*value) - expected).abs() < 4.0 * 2.0f64.powi(-11));
        }
    }
}
