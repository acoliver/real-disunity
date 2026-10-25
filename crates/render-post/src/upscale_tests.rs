use crate::{
    FrameConfig, PostChain, Readback, Resources, SpatialUpscale, UpscaleMode, UpscaleSettings,
    color_texture,
};
#[path = "upscale_reference.rs"]
pub(super) mod reference;
use reference::Image;

pub(super) fn fixture(size: [i32; 2], pattern: u32) -> Image {
    let mut pixels = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let v = match pattern {
                0 => 0.375,
                1 => {
                    if x < size[0] / 2 {
                        0.125
                    } else {
                        0.875
                    }
                }
                2 => {
                    if x < y + (size[0] - size[1]) / 2 {
                        0.125
                    } else {
                        0.875
                    }
                }
                4 => f64::from((x + y) % 2),
                5 => {
                    if x * size[1] < y * size[0] {
                        0.125
                    } else {
                        0.875
                    }
                }
                _ => {
                    if (x + y) % 2 == 0 {
                        0.125
                    } else {
                        0.875
                    }
                }
            };
            pixels.push([v, v * 0.5, 1.0 - v, 0.75]);
        }
    }
    Image { size, pixels }
}
pub(super) fn paint(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    pattern: u32,
) {
    let shader = format!(
        "@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{return vec4<f32>(f32((i<<1u)&2u)*2.0-1.0,f32(i&2u)*2.0-1.0,0.0,1.0);}} @fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{{let x=i32(p.x);let y=i32(p.y);var v=0.375;switch {pattern}u {{case 1u:{{v=select(0.875,0.125,x<{});}}case 2u:{{v=select(0.875,0.125,x<y+{});}}case 3u:{{v=select(0.875,0.125,(x+y)%2==0);}}case 4u:{{v=f32((x+y)%2);}}case 5u:{{v=select(0.875,0.125,x*{}<y*{});}}default:{{}}}}return vec4<f32>(v,v*0.5,1.0-v,0.75);}}",
        texture.width() / 2,
        (texture.width() - texture.height()) / 2,
        texture.height(),
        texture.width()
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
            format: texture.format(),
            constants: &[],
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    crate::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &view,
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
    queue.submit([encoder.finish()]);
}
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<SpatialUpscale>,
) -> Vec<[f64; 4]> {
    let size = chain.targets().config().output_extent();
    let output = color_texture(
        device,
        "upscale numerical output",
        size,
        wgpu::TextureFormat::Rgba32Float,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let raw = Readback::new(
        device,
        FrameConfig::new(size, output.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(device, &crate::test_authority(queue), &output)
    .unwrap();
    raw.chunks_exact(16)
        .map(|p| {
            std::array::from_fn(|c| {
                f64::from(f32::from_le_bytes(p[c * 4..c * 4 + 4].try_into().unwrap()))
            })
        })
        .collect()
}
fn chain(
    device: &wgpu::Device,
    input: [u32; 2],
    output: [u32; 2],
    settings: UpscaleSettings,
) -> PostChain<SpatialUpscale> {
    PostChain::new(
        device,
        FrameConfig::new(output, wgpu::TextureFormat::Rgba32Float, 1).with_render_extent(input),
        Resources::default(),
        SpatialUpscale {
            settings,
            output_format: wgpu::TextureFormat::Rgba32Float,
        },
    )
    .unwrap()
}
fn compare(actual: &[[f64; 4]], expected: &Image) {
    // Absolute per-channel tolerance 1e-4 covers f32 coordinate error amplified by RCAS.
    assert_eq!(actual.len(), expected.pixels.len());
    for (index, (a, e)) in actual.iter().zip(&expected.pixels).enumerate() {
        for c in 0..4 {
            assert!(
                (a[c] - e[c]).abs() < 0.000_1,
                "pixel {index} channel {c}: {} != {}",
                a[c],
                e[c]
            );
            assert!((0.0..=1.0).contains(&a[c]));
        }
    }
}
#[test]
fn gpu_spatial_kernels_match_scalar_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    for (input, output) in [([1, 1], [2, 2]), ([2, 2], [4, 4]), ([33, 17], [50, 26])] {
        for pattern in 0..6 {
            let image = fixture(input, pattern);
            for mode in [UpscaleMode::Bilinear, UpscaleMode::Easu] {
                verify_reconstruction(&device, &queue, &image, output, pattern, mode);
            }
        }
    }
}
#[test]
fn gpu_resize_rebinds_both_extents_and_rejects_invalid_replacement() {
    let (device, queue) = crate::gpu_tests::gpu();
    let settings = UpscaleSettings {
        mode: UpscaleMode::Easu,
        sharpness: None,
    };
    let mut chain = chain(&device, [2, 2], [4, 4], settings);
    paint(&device, &queue, chain.targets().texture(), 1);
    render(&device, &queue, &chain);
    chain.resize_extents(&device, [33, 17], [50, 26]).unwrap();
    let image = fixture([33, 17], 0);
    paint(&device, &queue, chain.targets().texture(), 0);
    compare(
        &render(&device, &queue, &chain),
        &image.reconstruct([50, 26], true),
    );
    assert!(chain.resize_extents(&device, [51, 17], [50, 26]).is_err());
    assert_eq!(chain.targets().config().size, [33, 17]);
    compare(
        &render(&device, &queue, &chain),
        &image.reconstruct([50, 26], true),
    );
    chain
        .reconfigure(
            &device,
            SpatialUpscale {
                settings: UpscaleSettings {
                    mode: UpscaleMode::Bilinear,
                    sharpness: Some(0.0),
                },
                output_format: wgpu::TextureFormat::Rgba32Float,
            },
        )
        .unwrap();
    paint(&device, &queue, chain.targets().texture(), 1);
    compare(
        &render(&device, &queue, &chain),
        &fixture([33, 17], 1)
            .reconstruct([50, 26], false)
            .sharpen(0.0),
    );
}
#[test]
fn gpu_rcas_strength_increases_edge_contrast() {
    let (device, queue) = crate::gpu_tests::gpu();
    let image = fixture([8, 4], 1);
    let mut results = Vec::new();
    for sharpness in [None, Some(2.0), Some(0.0)] {
        let chain = chain(
            &device,
            [8, 4],
            [8, 4],
            UpscaleSettings {
                mode: UpscaleMode::Bilinear,
                sharpness,
            },
        );
        paint(&device, &queue, chain.targets().texture(), 1);
        let actual = render(&device, &queue, &chain);
        let expected = sharpness.map_or_else(|| image.clone(), |s| image.sharpen(f64::from(s)));
        compare(&actual, &expected);
        if sharpness.is_none() {
            assert_eq!(actual, image.pixels);
        }
        results.push(actual[12][0] - actual[11][0]);
    }
    assert!(results[2] > results[1] && results[1] > results[0]);
}

fn verify_reconstruction(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &Image,
    output: [i32; 2],
    pattern: u32,
    mode: UpscaleMode,
) {
    let expected = image.reconstruct(output, mode == UpscaleMode::Easu);
    for sharpness in [None, Some(0.0), Some(2.0)] {
        let chain = chain(
            device,
            image.size.map(|v| u32::try_from(v).unwrap()),
            output.map(|v| u32::try_from(v).unwrap()),
            UpscaleSettings { mode, sharpness },
        );
        paint(device, queue, chain.targets().texture(), pattern);
        let reference =
            sharpness.map_or_else(|| expected.clone(), |s| expected.sharpen(f64::from(s)));
        eprintln!(
            "reference input={:?} pattern={pattern} mode={mode:?} sharpness={sharpness:?}",
            image.size
        );
        let actual = render(device, queue, &chain);
        compare(&actual, &reference);
        if sharpness.is_none() && matches!(pattern, 1 | 2) {
            for row in actual.chunks_exact(usize::try_from(output[0]).unwrap()) {
                assert!(row.windows(2).all(|p| p[0][0] <= p[1][0] + 0.000_03));
            }
        }
    }
}
