//! GPU checks that every selectable sample count works through the HDR compositor.
use crate::compositor::FrameRenderer;
use crate::device::GpuContext;
use worldrunner_render_post::{
    AntiAliasing, FrameConfig, MsaaSamples, PostError, PostResolve, Readback, ToneMap,
};

const SIZE: u32 = 16;
const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// NDC height of the slanted edge at x = -1 and its rise across the target.
const EDGE_START: f64 = -0.6;
const EDGE_RISE: f64 = 0.45;

/// Everything below the line from (-1, -0.6) to (1, -0.15), written as white.
const EDGE_SHADER: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, -0.15),
        vec2(-1.0, -1.0), vec2(1.0, -0.15), vec2(-1.0, -0.6));
    return vec4(p[i], 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4(1.0, 1.0, 1.0, 1.0); }
";

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}

fn supported(context: &GpuContext, samples: MsaaSamples) -> bool {
    [HDR, wgpu::TextureFormat::Depth32Float]
        .into_iter()
        .all(|format| {
            context
                .adapter()
                .get_texture_format_features(format)
                .flags
                .sample_count_supported(samples.count())
        })
}

fn renderer(context: &GpuContext, samples: MsaaSamples) -> Result<FrameRenderer, PostError> {
    FrameRenderer::hdr_antialiased(
        context,
        [SIZE, SIZE],
        OUTPUT,
        AntiAliasing::new(samples, PostResolve::None),
        ToneMap::Reinhard,
        None,
    )
}

fn edge_pipeline(device: &wgpu::Device, samples: u32) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("msaa edge"),
        source: wgpu::ShaderSource::Wgsl(EDGE_SHADER.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("msaa edge"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(HDR.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

/// Standard sample positions in pixel space (origin top-left, y down) for each
/// selectable count, as fixed by the Vulkan and D3D standard sample locations.
/// Sample-rate fragments see the position of their own sample, so CPU references
/// of per-sample shading must use these rather than one count's table for all.
pub(crate) fn standard_sample_position(samples: u32, sample: u32) -> [f32; 2] {
    let table: &[[f32; 2]] = match samples {
        1 => &[[0.5, 0.5]],
        2 => &[[0.75, 0.75], [0.25, 0.25]],
        4 => &[
            [0.375, 0.125],
            [0.875, 0.375],
            [0.125, 0.625],
            [0.625, 0.875],
        ],
        8 => &[
            [0.5625, 0.3125],
            [0.4375, 0.6875],
            [0.8125, 0.5625],
            [0.3125, 0.1875],
            [0.1875, 0.8125],
            [0.0625, 0.4375],
            [0.6875, 0.9375],
            [0.9375, 0.0625],
        ],
        _ => panic!("{samples}x is not a selectable sample count"),
    };
    table[usize::try_from(sample).unwrap()]
}

/// Renders the edge through the compositor's scene attachments; returns resolved red values.
fn render_edge(context: &GpuContext, frame: &FrameRenderer) -> Vec<f32> {
    let device = context.device();
    let targets = frame.targets();
    let pipeline = edge_pipeline(device, targets.config().samples);
    let mut encoder = context.submission().begin(device).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("msaa edge"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: targets.scene(),
                resolve_target: targets.resolve(),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.draw(0..6, 0..1);
    }
    encoder.finish().submit().unwrap();
    let bytes = Readback::new(device, FrameConfig::new([SIZE, SIZE], HDR, 1), 1_000_000)
        .unwrap()
        .read(device, context.submission(), targets.texture())
        .unwrap();
    bytes
        .chunks_exact(8)
        .map(|texel| half_to_f32(u16::from_le_bytes([texel[0], texel[1]])))
        .collect()
}

fn half_to_f32(bits: u16) -> f32 {
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    let magnitude = if exponent == 0 {
        fraction * 2f32.powi(-24)
    } else {
        (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15)
    };
    if bits & 0x8000 == 0 {
        magnitude
    } else {
        -magnitude
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Region {
    Inside,
    Outside,
    Edge,
}

fn edge_height(x: f64) -> f64 {
    EDGE_START + EDGE_RISE * (x + 1.0) / 2.0
}

/// Pixel (column, row from top) against the analytic edge, in NDC.
fn classify(column: u32, row: u32) -> Region {
    let pixel = f64::from(SIZE);
    let (left, right) = (
        f64::from(column) / pixel * 2.0 - 1.0,
        f64::from(column + 1) / pixel * 2.0 - 1.0,
    );
    let (top, bottom) = (
        1.0 - f64::from(row) / pixel * 2.0,
        1.0 - f64::from(row + 1) / pixel * 2.0,
    );
    if top <= edge_height(left) {
        Region::Inside
    } else if bottom >= edge_height(right) {
        Region::Outside
    } else {
        Region::Edge
    }
}

/// Exact covered fraction of a pixel, by dense point sampling of the analytic edge.
fn area_coverage(column: u32, row: u32) -> f64 {
    const GRID: u32 = 64;
    let pixel = f64::from(SIZE);
    let mut covered = 0;
    for sy in 0..GRID {
        for sx in 0..GRID {
            let x =
                (f64::from(column) + (f64::from(sx) + 0.5) / f64::from(GRID)) / pixel * 2.0 - 1.0;
            let y = 1.0 - (f64::from(row) + (f64::from(sy) + 0.5) / f64::from(GRID)) / pixel * 2.0;
            covered += u32::from(y <= edge_height(x));
        }
    }
    f64::from(covered) / f64::from(GRID * GRID)
}

struct EdgeStats {
    /// Distinct coverage levels strictly between empty and full.
    levels: usize,
    /// Mean absolute difference from the exact covered area, over edge pixels.
    error: f64,
}

/// The sample coverage k (of `count`) a resolved value encodes, if it is a whole number of samples.
fn coverage_samples(value: f64, count: u32) -> Option<u32> {
    (0..=count).find(|&k| (value * f64::from(count) - f64::from(k)).abs() < 0.01)
}

fn is_close(value: f64, expected: f64) -> bool {
    (value - expected).abs() < 1e-6
}

fn check_image(samples: MsaaSamples, image: &[f32]) -> EdgeStats {
    let count = samples.count();
    let mut levels = Vec::new();
    let mut error = 0.0;
    let mut edge_pixels = 0.0;
    for row in 0..SIZE {
        for column in 0..SIZE {
            let value = f64::from(image[(row * SIZE + column) as usize]);
            let at = format!("{samples:?} ({column},{row}) = {value}");
            match classify(column, row) {
                Region::Inside => assert!(is_close(value, 1.0), "interior {at}"),
                Region::Outside => assert!(is_close(value, 0.0), "exterior {at}"),
                Region::Edge => {
                    let k = coverage_samples(value, count)
                        .unwrap_or_else(|| panic!("not a multiple of 1/{count}: {at}"));
                    levels.push(k);
                    error += (value - area_coverage(column, row)).abs();
                    edge_pixels += 1.0;
                }
            }
        }
    }
    levels.sort_unstable();
    levels.dedup();
    EdgeStats {
        levels: levels.iter().filter(|&&k| k != 0 && k != count).count(),
        error: error / edge_pixels,
    }
}

#[test]
fn edge_coverage_gains_resolution_with_each_sample_count_and_interiors_are_unchanged() {
    let context = context();
    let mut results = Vec::new();
    for samples in MsaaSamples::ALL {
        if !supported(&context, samples) {
            eprintln!(
                "skipping {samples:?}: adapter {} does not support {}x for Rgba16Float and Depth32Float",
                context.adapter().get_info().name,
                samples.count()
            );
            assert_eq!(
                renderer(&context, samples).err(),
                Some(PostError::SampleCount(samples.count())),
                "an unsupported count must be a typed error, never a downgrade"
            );
            continue;
        }
        let frame = renderer(&context, samples).unwrap();
        assert_eq!(frame.targets().config().samples, samples.count());
        let stats = check_image(samples, &render_edge(&context, &frame));
        eprintln!(
            "{}x: {} intermediate coverage levels, mean edge error {:.4}",
            samples.count(),
            stats.levels,
            stats.error
        );
        results.push((samples, stats));
    }
    let single = &results[0].1;
    assert_eq!(
        single.levels, 0,
        "1x produces only fully covered or empty pixels"
    );
    for pair in results.windows(2) {
        assert!(
            pair[1].1.levels >= pair[0].1.levels,
            "{:?} has fewer coverage levels than {:?}",
            pair[1].0,
            pair[0].0
        );
        assert!(
            pair[1].1.error < pair[0].1.error,
            "{:?} approximates the edge no better than {:?}",
            pair[1].0,
            pair[0].0
        );
    }
    let counts: Vec<_> = results.iter().map(|(samples, _)| samples.count()).collect();
    assert!(
        counts.starts_with(&[1]) && counts.contains(&4),
        "1x and 4x are guaranteed on every adapter, got {counts:?}"
    );
    assert!(results.iter().skip(1).all(|(_, stats)| stats.levels > 0));
}

#[test]
fn unsupported_sample_counts_fail_with_a_typed_error_before_allocation() {
    let context = context();
    for invalid in [0, 3, 5, 16] {
        let result = FrameRenderer::hdr(&context, [8, 8], OUTPUT, invalid, ToneMap::Reinhard, None);
        assert_eq!(result.err(), Some(PostError::SampleCount(invalid)));
    }
    // A device without adapter-specific format features cannot render 2x or 8x, whatever the adapter reports.
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, _) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    for samples in [2, 8] {
        assert_eq!(
            FrameConfig::new([8, 8], HDR, samples).validate_device(&device),
            Err(PostError::UnsupportedFormat(HDR)),
        );
    }
    assert_eq!(
        FrameConfig::new([8, 8], HDR, 4).validate_device(&device),
        Ok(())
    );
}

#[test]
fn fxaa_resolve_runs_at_every_supported_sample_count_and_conflicts_with_authored_fxaa() {
    let context = context();
    for samples in MsaaSamples::ALL {
        if !supported(&context, samples) {
            continue;
        }
        let aa = AntiAliasing::new(samples, PostResolve::fxaa(0.75));
        let frame = FrameRenderer::hdr_antialiased(
            &context,
            [SIZE, SIZE],
            OUTPUT,
            aa,
            ToneMap::Reinhard,
            None,
        )
        .unwrap();
        assert_eq!(frame.targets().config().samples, samples.count());
    }
    let authored = crate::post_fxaa_tests::effects(None);
    let aa = AntiAliasing::new(MsaaSamples::X4, PostResolve::fxaa(0.5));
    let result = FrameRenderer::hdr_antialiased(
        &context,
        [SIZE, SIZE],
        OUTPUT,
        aa,
        ToneMap::Reinhard,
        Some(&authored),
    );
    assert_eq!(
        result.err(),
        Some(PostError::UnsupportedProfile("duplicate FXAA"))
    );
}

/// Formats the scene tests attach at every sample count.
const TESTED_FORMATS: [wgpu::TextureFormat; 4] = [
    HDR,
    OUTPUT,
    wgpu::TextureFormat::Rgba8Unorm,
    wgpu::TextureFormat::Depth32Float,
];

/// Every selectable count the adapter supports for the formats scene tests attach.
///
/// A count the adapter lacks is logged with the reason and must be rejected with
/// the typed error, so a test never silently runs at a different count.
pub(crate) fn supported_samples(context: &GpuContext) -> Vec<u32> {
    let adapter = context.adapter();
    let mut counts = Vec::new();
    for samples in MsaaSamples::ALL {
        let count = samples.count();
        let missing = TESTED_FORMATS.into_iter().find(|&format| {
            !adapter
                .get_texture_format_features(format)
                .flags
                .sample_count_supported(count)
        });
        let Some(format) = missing else {
            counts.push(count);
            continue;
        };
        eprintln!(
            "skipping {count}x: adapter {} does not support it for {format:?}",
            adapter.get_info().name
        );
        // The frame contract always checks Depth32Float alongside its colour format.
        let color = if format.is_depth_stencil_format() {
            HDR
        } else {
            format
        };
        let expected = FrameConfig::new([SIZE, SIZE], color, count);
        assert_eq!(
            expected.validate_adapter(adapter),
            Err(PostError::SampleCount(count)),
            "{count}x is unsupported for {format:?} and must be a typed error"
        );
    }
    assert!(
        counts.starts_with(&[1]) && counts.contains(&4),
        "1x and 4x are guaranteed on every adapter, got {counts:?}"
    );
    counts
}

/// Every selectable count, for checks that never touch the adapter.
pub(crate) fn all_counts() -> [u32; 4] {
    MsaaSamples::ALL.map(MsaaSamples::count)
}
