use super::*;
use crate::{
    AdaptiveExposure, Bloom, BufferReadback, HdrPostSettings, Readback, ToneMap, ToneMapOutput,
    adapt_exposure, color_texture, histogram_bin, target_exposure,
};

pub(super) fn adaptive() -> AdaptiveExposure {
    AdaptiveExposure {
        log_range: [-12.0, 12.0],
        percentiles: [0.1, 0.9],
        key: 0.18,
        exposure_range: [0.01, 64.0],
        speeds: [3.0, 1.0],
        fixed_delta: Some(0.25),
        fixed_exposure: None,
    }
}
pub(super) fn chain(
    device: &wgpu::Device,
    settings: HdrPostSettings,
    size: [u32; 2],
) -> PostChain<ToneMapOutput> {
    PostChain::new(
        device,
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
            .unwrap()
            .with_effects(device, settings)
            .unwrap(),
    )
    .unwrap()
}
pub(super) fn paint(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    body: &str,
) {
    let shader = format!(
        "@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{return vec4<f32>(f32((i<<1u)&2u)*2.0-1.0,f32(i&2u)*2.0-1.0,0.0,1.0);}} @fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{{body}}}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let pipeline = crate::fullscreen_pipeline(
        device,
        crate::FullscreenDescriptor {
            label: None,
            layout: &layout,
            module: &module,
            vertex_entry: "vs",
            fragment_entry: "fs",
            format: wgpu::TextureFormat::Rgba16Float,
            constants: &[],
        },
    );
    crate::fullscreen_pass(
        encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[],
    );
}
pub(super) fn output(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    color_texture(
        device,
        "post fixture",
        size,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
}
pub(super) fn pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    Readback::new(
        device,
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(device, &crate::test_authority(queue), texture)
    .unwrap()
}
pub(super) fn buffer(device: &wgpu::Device, queue: &wgpu::Queue, source: &wgpu::Buffer) -> Vec<u8> {
    let mut capture = BufferReadback::new(device, source.size(), 4096).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    capture.encode(&mut encoder, source).unwrap();
    let submission = queue.submit([encoder.finish()]);
    capture.read_encoded(device, Some(submission)).unwrap()
}
pub(super) fn step(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<ToneMapOutput>,
    slot: usize,
    body: &str,
    reset: bool,
) -> wgpu::Texture {
    let out = output(device, chain.targets().config().size);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    paint(device, &mut encoder, chain.targets().resolved(), body);
    chain
        .factory()
        .prepare_effects(&mut encoder, slot, 0.031, reset)
        .unwrap();
    chain.encode(
        &mut encoder,
        &out.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Poll).unwrap();
    out
}
#[test]
fn histogram_percentile_reference_and_asymmetric_convergence() {
    let a = adaptive();
    assert_eq!(histogram_bin(0.0, a.log_range), 0);
    assert_eq!(histogram_bin(f32::INFINITY, a.log_range), 0);
    assert_eq!(histogram_bin(1e10, a.log_range), 255);
    let mut bins = [0; 256];
    bins[0] = 10;
    bins[128] = 80;
    bins[255] = 10;
    let target = target_exposure(&bins, a);
    assert!((target - 0.18 / 2.0_f64.powf(0.046_875)).abs() < 1e-8);
    for (start, end, speed) in [(1.0, 4.0, 3.0), (4.0, 1.0, 1.0)] {
        let mut value = start;
        for _ in 0..8 {
            value = adapt_exposure(value, end, 0.25, a.speeds);
        }
        let expected = end + (start - end) * (-speed * 2.0_f32).exp();
        assert!((value - expected).abs() < 1e-6);
    }
}
#[test]
fn gpu_histogram_matches_cpu_bins_and_trimmed_exposure() {
    let (device, queue) = crate::gpu_tests::gpu();
    let a = adaptive();
    let c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(a),
            ..HdrPostSettings::default()
        },
        [16, 8],
    );
    step(
        &device,
        &queue,
        &c,
        0,
        "let v=array<f32,4>(0.0,0.1875,0.75,3.25)[u32(p.x)%4u]; return vec4<f32>(v,v,v,1.0);",
        false,
    );
    let raw = buffer(&device, &queue, &c.configured[0].resources.buffers[0]);
    let actual: Vec<_> = raw
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let mut expected = [0; 256];
    for v in [0.0, 0.1875, 0.75, 3.25] {
        expected[histogram_bin(v, a.log_range)] += 32;
    }
    assert_eq!(actual, expected);
    let target = target_exposure(&expected, a);
    let expected = 1.0
        + (target - 1.0) * (1.0 - (-f64::from(a.speeds[usize::from(target < 1.0)]) * 0.25).exp());
    let raw = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
    let observed = f32::from_le_bytes(raw[..4].try_into().unwrap());
    assert!((f64::from(observed) - expected).abs() < 1e-6);
}
#[test]
fn gpu_slots_overlap_without_history_races_and_capture_reset_is_deterministic() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(adaptive()),
            ..HdrPostSettings::default()
        },
        [8, 8],
    );
    let dark = "return vec4<f32>(0.03125,0.03125,0.03125,1.0);";
    let bright = "return vec4<f32>(4.0,4.0,4.0,1.0);";
    let first = step(&device, &queue, &c, 0, dark, false);
    c.select_frame_slot(1);
    step(&device, &queue, &c, 1, bright, false);
    let a = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
    let b = buffer(&device, &queue, c.factory().exposure_buffer(1).unwrap());
    assert!(f32::from_le_bytes(a[..4].try_into().unwrap()) > 1.0);
    assert!(
        f32::from_le_bytes(b[..4].try_into().unwrap())
            < f32::from_le_bytes(a[..4].try_into().unwrap())
    );
    c.select_frame_slot(0);
    let reset = step(&device, &queue, &c, 0, dark, true);
    assert_eq!(
        pixels(&device, &queue, &first),
        pixels(&device, &queue, &reset)
    );
    assert_eq!(
        a,
        buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap())
    );
    let old = c.factory().exposure_buffer(0).unwrap().clone();
    c.resize(&device, [9, 7]).unwrap();
    assert_eq!(&old, c.factory().exposure_buffer(0).unwrap());
}
#[test]
fn gpu_fixed_capture_exposure_ignores_metered_scene() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut a = adaptive();
    a.fixed_exposure = Some(0.5);
    let c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(a),
            ..HdrPostSettings::default()
        },
        [3, 3],
    );
    for body in ["return vec4<f32>(0.01);", "return vec4<f32>(64.0);"] {
        step(&device, &queue, &c, 0, body, false);
        let raw = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
        assert_eq!(&raw[..4], &0.5_f32.to_le_bytes());
    }
}
#[test]
fn gpu_bloom_spreads_energy_and_excludes_nonfinite_fireflies() {
    let (device, queue) = crate::gpu_tests::gpu();
    let settings = HdrPostSettings {
        bloom: Some(Bloom {
            threshold: 0.0,
            knee: 0.5,
            intensity: 1.0,
            levels: 3,
        }),
        ..HdrPostSettings::default()
    };
    let c = chain(&device, settings, [64, 64]);
    step(
        &device,
        &queue,
        &c,
        0,
        "let v=select(0.0,16.0,u32(p.x)==32u && u32(p.y)==32u);return vec4<f32>(v,v,v,1.0);",
        false,
    );
    // Last intermediate is the full-resolution bloom. Normalized filters preserve
    // impulse energy away from boundaries; half storage rounds each of five stages.
    let bloom = c.configured[0].resources.textures.last().unwrap();
    let raw = Readback::new(
        &device,
        FrameConfig::new([64, 64], bloom.format(), 1),
        65536,
    )
    .unwrap()
    .read(&device, &crate::test_authority(&queue), bloom)
    .unwrap();
    let values: Vec<_> = raw
        .chunks_exact(8)
        .map(|p| crate::tone_map_gpu_tests::half(&p[..2]))
        .collect();
    assert!(values.iter().filter(|&&v| v > 0.0).count() > 40);
    let energy: f64 = values.iter().sum();
    assert!((energy - 16.0).abs() < 16.0 * 5.0 / 1024.0, "{energy}");
    step(
        &device,
        &queue,
        &c,
        0,
        "let v=select(0.0,bitcast<f32>(0x7f800000u),u32(p.x)==32u && u32(p.y)==32u);return vec4<f32>(v,v,v,1.0);",
        false,
    );
    let raw = Readback::new(
        &device,
        FrameConfig::new([64, 64], bloom.format(), 1),
        65536,
    )
    .unwrap()
    .read(&device, &crate::test_authority(&queue), bloom)
    .unwrap();
    assert!(raw.chunks_exact(8).all(|p| p[..6] == [0; 6]));
}
#[test]
fn gpu_fxaa_smooths_diagonal_preserves_flat_and_disabled_bytes() {
    let (device, queue) = crate::gpu_tests::gpu();
    let plain = chain(&device, HdrPostSettings::default(), [32, 32]);
    let filtered = chain(
        &device,
        HdrPostSettings {
            fxaa: Some(1.0),
            ..HdrPostSettings::default()
        },
        [32, 32],
    );
    let edge = "let v=select(0.0,3.25,p.x>p.y*0.6+4.0);return vec4<f32>(v,v,v,1.0);";
    let a = step(&device, &queue, &plain, 0, edge, false);
    let b = step(&device, &queue, &filtered, 0, edge, false);
    let a = pixels(&device, &queue, &a);
    let b = pixels(&device, &queue, &b);
    assert!(
        a.chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
            > 20
    );
    assert_eq!(&a[..8], &b[..8]);
    let empty = chain(&device, HdrPostSettings::default(), [32, 32]);
    let b = step(&device, &queue, &empty, 0, edge, false);
    assert_eq!(a, pixels(&device, &queue, &b));
    let flat = "return vec4<f32>(0.75,0.75,0.75,1.0);";
    let a = step(&device, &queue, &plain, 0, flat, false);
    let b = step(&device, &queue, &filtered, 0, flat, false);
    assert_eq!(pixels(&device, &queue, &a), pixels(&device, &queue, &b));
}

#[test]
fn gpu_adaptation_converges_at_fixed_asymmetric_rates() {
    let (device, queue) = crate::gpu_tests::gpu();
    let a = adaptive();
    let c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(a),
            ..HdrPostSettings::default()
        },
        [4, 4],
    );
    for (radiance, speed) in [(0.046_875_f32, 3.0_f64), (3.25, 1.0)] {
        let body = format!("return vec4<f32>({radiance},{radiance},{radiance},1.0);");
        let mut bins = [0; 256];
        bins[histogram_bin(radiance, a.log_range)] = 16;
        let target = target_exposure(&bins, a);
        for frame in 1..=8 {
            step(&device, &queue, &c, 0, &body, frame == 1);
            let raw = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
            let observed = f64::from(f32::from_le_bytes(raw[..4].try_into().unwrap()));
            let expected = target + (1.0 - target) * (-speed * f64::from(frame) * 0.25).exp();
            assert!(
                (observed - expected).abs() < 0.000_01,
                "{observed} != {expected}"
            );
        }
    }
}

#[test]
fn gpu_nonfinite_inputs_cannot_poison_histogram_or_bloom() {
    let (device, queue) = crate::gpu_tests::gpu();
    let c = chain(
        &device,
        HdrPostSettings {
            adaptive: Some(adaptive()),
            bloom: Some(Bloom {
                threshold: 0.0,
                knee: 0.5,
                intensity: 1.0,
                levels: 3,
            }),
            fxaa: None,
        },
        [9, 7],
    );
    for bits in [0x7f80_0000_u32, 0xff80_0000, 0x7fc0_0000] {
        let body = format!(
            "let v=select(0.0,bitcast<f32>({bits}u),u32(p.x)==4u && u32(p.y)==3u);return vec4<f32>(v,v,v,1.0);"
        );
        step(&device, &queue, &c, 0, &body, true);
        let bins = buffer(&device, &queue, &c.configured[0].resources.buffers[0]);
        assert_eq!(&bins[..4], &63_u32.to_le_bytes());
        assert!(bins[4..].iter().all(|&byte| byte == 0));
        let history = buffer(&device, &queue, c.factory().exposure_buffer(0).unwrap());
        assert!(f32::from_le_bytes(history[..4].try_into().unwrap()).is_finite());
        let bloom = c.configured[0].resources.textures.last().unwrap();
        let raw = Readback::new(&device, FrameConfig::new([9, 7], bloom.format(), 1), 4096)
            .unwrap()
            .read(&device, &crate::test_authority(&queue), bloom)
            .unwrap();
        assert!(raw.chunks_exact(8).all(|p| p[..6] == [0; 6]));
    }
}

#[test]
fn gpu_bloom_threshold_and_quadratic_knee_match_exact_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    let c = chain(
        &device,
        HdrPostSettings {
            bloom: Some(Bloom {
                threshold: 1.0,
                knee: 0.5,
                intensity: 1.0,
                levels: 1,
            }),
            ..HdrPostSettings::default()
        },
        [4, 1],
    );
    step(
        &device,
        &queue,
        &c,
        0,
        "let v=array<f32,4>(0.25,0.75,1.0,2.0)[u32(p.x)];return vec4<f32>(v,v,v,1.0);",
        false,
    );
    let bloom = &c.configured[0].resources.textures[0];
    let raw = Readback::new(&device, FrameConfig::new([4, 1], bloom.format(), 1), 4096)
        .unwrap()
        .read(&device, &crate::test_authority(&queue), bloom)
        .unwrap();
    for (pixel, expected) in raw.chunks_exact(8).zip([0_u16, 0x2800, 0x3000, 0x3c00]) {
        for channel in pixel[..6].chunks_exact(2) {
            assert_eq!(u16::from_le_bytes(channel.try_into().unwrap()), expected);
        }
    }
}
