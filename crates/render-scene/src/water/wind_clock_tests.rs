use super::*;

fn scene_band_source() -> &'static str {
    let source = include_str!("../shader/foliage.wgsl");
    let start = source.find("fn scene_band(").unwrap();
    let end = source[start..].find("\n}\n").unwrap() + start + 2;
    &source[start..end]
}

#[test]
fn integer_wind_clock_matches_f64_oracle_and_scene_wind() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for clocks in [
        [0u32, 1, 124, 125, 126],
        [6282, 6283, 6284, 4_000_000_000, 4_000_000_001],
        [u32::MAX - 2, u32::MAX - 1, u32::MAX, 0, 1],
    ] {
        let source = format!(
            "{}\n{}\n{}\n{}",
            terms_tests::equation_source(),
            "const PI: f32 = 3.141592653589793;",
            scene_band_source(),
            format_args!(
                r"
@group(0) @binding(0) var<storage, read_write> output: array<vec4<f32>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let clock = array<u32, 5>({}u, {}u, {}u, {}u, {}u)[id.x];
    let position = vec3<f32>(0.25, -0.5, 0.0);
    let gain = water_wind_scale(position, clock, vec3<f32>(1.0, 0.5, 2.0));
    let band = vec4<u32>(0u, bitcast<u32>(0.5), bitcast<u32>(2.0), bitcast<u32>(1.0));
    let scene_gain = scene_band(position, band, clock, 0.0);
    let seed = water_wind_seed(vec2<f32>(0.25, -0.75), vec2<f32>(1.0, 0.5) * gain);
    output[id.x * 2u] = vec4<f32>(gain, scene_gain, seed);
}}",
                clocks[0], clocks[1], clocks[2], clocks[3], clocks[4]
            )
        );
        let actual = optics_tests::compute(&context, &source);
        for (index, clock) in clocks.into_iter().enumerate() {
            check_clock(&actual[index * 8..index * 8 + 4], clock);
        }
    }
}

fn check_clock(actual: &[f32], clock: u32) {
    let period =
        u32::try_from(std::time::Duration::from_secs_f64(std::f64::consts::TAU).as_millis())
            .unwrap();
    let angle = f64::from(clock % period) / 1000.0 - 0.5;
    let gain = 1.0 + angle.sin() * 0.5;
    let strength = gain * 1.5 * 0.25;
    let expected = [
        gain,
        gain,
        (0.25 * strength.clamp(0.2, 1.0) - strength) * 2.0 / 1.25f64.sqrt(),
        (-0.75 * strength.clamp(0.2, 1.0) - strength) / 1.25f64.sqrt(),
    ];
    // Reduced sine has a 2^-11 absolute bound; seed sensitivity is below four.
    let bound = 4.0 * 2.0f64.powi(-11) + 128.0 * f64::from(f32::EPSILON);
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (f64::from(*actual) - expected).abs() < bound,
            "clock {clock}: {actual} vs {expected}"
        );
    }
    assert!((actual[0] - actual[1]).abs() < 4.0 * f32::EPSILON);
}

#[test]
fn default_water_frame_descriptor_is_fragment_only() {
    let entries = receivers::environment_entries();
    let frame = entries.iter().find(|entry| entry.binding == 3).unwrap();
    assert_eq!(frame.visibility, wgpu::ShaderStages::FRAGMENT);
    assert!(matches!(frame.ty, wgpu::BindingType::Buffer { .. }));
}

#[test]
fn water_vertex_clock_uses_the_scene_integer_lane() {
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let mut body = settings();
    body.material.wind = Some(worldrunner_render_plan::SceneWaterWind {
        field: worldrunner_render_plan::ScenePlanarWindField::new(
            worldrunner_render_plan::TextureHandle::from_raw(0),
            [0; 2],
            [1000; 2],
            [None; 2],
        )
        .unwrap(),
        band: worldrunner_render_plan::SceneWindBand::new(1_000_000, 500_000, 2_000_000).unwrap(),
        amplitude_micro: 1_000_000,
    });
    for time in [0, 1, 4_000_000_000, u32::MAX - 2, u32::MAX - 1, u32::MAX] {
        frame.animation_time_milliseconds = time;
        assert_eq!(&frame.to_bytes()[152..156], &time.to_le_bytes());
        let bytes: Vec<_> = uniform_data(&frame, 0.0, &body)
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(&bytes[548..552], &time.to_le_bytes());
        assert_eq!(
            terms::uniforms(&settings(), time).map(f32::to_bits),
            [0; 32]
        );
        let mut disabled = body.clone();
        disabled.material.wind.as_mut().unwrap().amplitude_micro = 0;
        assert_eq!(terms::uniforms(&disabled, time)[29].to_bits(), 0);
    }
    let vertex = include_str!("water.wgsl")
        .split("fn water_camera_transmittance")
        .next()
        .unwrap();
    assert!(vertex.contains("water_wind_scale(world, bitcast<u32>(water.wind_flags.y)"));
    assert!(!vertex.contains("frame.grading"));
}
