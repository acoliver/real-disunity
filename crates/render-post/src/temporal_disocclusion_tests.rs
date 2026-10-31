//! Measurement of the temporal reconstruction on a disocclusion, below the public "not yet
//! scheduled" gate on lavapipe. It records what the PR 2 path does today so that the internal
//! motion and depth reconstruction can be held to a stated reduction of the same numbers.
//!
//! `temporal_disocclusion_scene` rasterises a near bar that sweeps over a checkerboard of a
//! similar colour, with the analytic backward motion and device depth of every surface, next to a
//! static plane of stripes at a grazing depth ramp. The reference is the same function box
//! filtered over each output pixel with 4x4 supersampling in f64. Two paths run the same frames:
//! temporal reconstruction with accumulated history (the path under measurement) and the same
//! stage with the history reset every frame (the spatial Lanczos2 only).
//!
//! The measured region is the strip the bar uncovered during the last `UNCOVERED_FRAMES` frames
//! (pixel centres at least one pixel from the bar's edges and rows, so only background shows),
//! plus the static plane. Both are averaged over the last `MEASURED` frames of the sequence.
//! Errors are mean absolute HDR error against the reference; flicker is the mean absolute
//! difference of the same pixels between consecutive frames.
use crate::hdr_temporal_build_tests::{public_factory, settings};
use crate::taa_gpu_test_support::read;
use crate::temporal_disocclusion_scene as scene;
use crate::temporal_quality_tests::Recording;
use crate::{
    FrameConfig, PostChain, ProjectionJitter, RenderScale, Resources, TaaSettings, TemporalBlock,
};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

const OUTPUT: [u32; 2] = [160, 96];
/// Frames rendered: the bar travels `FRAMES * SPEED` pixels, 96, and stays on the canvas.
const FRAMES: u64 = 48;
/// The last frames over which the metrics are averaged.
const MEASURED: u64 = 8;
/// Frames of vacated background that count as newly uncovered.
const UNCOVERED_FRAMES: u64 = 8;

struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    chain: PostChain<Recording>,
    hdr: wgpu::Texture,
    sink: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    render: [u32; 2],
    jitter: ProjectionJitter,
}

fn pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("disocclusion scene"),
        source: wgpu::ShaderSource::Wgsl(scene::wgsl().into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("disocclusion scene"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("paint"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[
                Some(wgpu::TextureFormat::Rgba16Float.into()),
                Some(crate::VELOCITY_FORMAT.into()),
            ],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Rig {
    fn new(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2]) -> Self {
        let (device, queue) = gpu.clone();
        let config = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let output = Arc::new(Mutex::new(None));
        let factory = Recording {
            inner: public_factory(&device, &settings()),
            output: output.clone(),
        };
        let chain = PostChain::new(&device, config, Resources::default(), factory).unwrap();
        let hdr = output.lock().unwrap().take().expect("slot 0 was built");
        let sink =
            crate::color_texture(&device, "sink", OUTPUT, wgpu::TextureFormat::Rgba8UnormSrgb);
        Self {
            pipeline: pipeline(&device),
            device,
            queue,
            chain,
            hdr,
            sink,
            render,
            jitter: ProjectionJitter::for_extents(render, OUTPUT).unwrap(),
        }
    }

    /// Rasterises the scene of `frame` into colour, velocity and depth at the internal extent.
    fn paint(&self, encoder: &mut wgpu::CommandEncoder, offset: [f32; 2], frame: u64) {
        let [rw, rh] = self
            .render
            .map(|side| f32::from(u16::try_from(side).unwrap()));
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the bar's left edge is a small canvas coordinate"
        )]
        let left = scene::bar_left(frame) as f32;
        let values = [rw, rh, offset[0], offset[1], left, 0.0, 0.0, 0.0];
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene frame"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let targets = self.chain.targets();
        let clear = |color| wgpu::Operations {
            load: wgpu::LoadOp::Clear(color),
            store: wgpu::StoreOp::Store,
        };
        let far = wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        };
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: clear(wgpu::Color::BLACK),
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("disocclusion scene"),
            color_attachments: &[
                attachment(targets.resolved()),
                attachment(targets.velocity().expect("temporal needs velocity")),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: targets.depth(),
                depth_ops: Some(far),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn frame(&self, index: u64, history_valid: bool) {
        let offset = self.jitter.offset_pixels(index);
        let block = TemporalBlock {
            render: self.render,
            output: OUTPUT,
            jitter: offset,
            phase_count: self.jitter.length(),
            settings: TaaSettings::new(0.9),
            history_valid,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.paint(&mut encoder, offset, index);
        let factory = &self.chain.factory().inner;
        factory.prepare_temporal(&mut encoder, 0, &block).unwrap();
        factory
            .prepare_effects(&mut encoder, 0, 0.031, true)
            .unwrap();
        let view = self
            .sink
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.chain.encode(&mut encoder, &view);
        self.queue.submit([encoder.finish()]);
    }

    /// The HDR reconstruction of the last frame, mean of the colour channels, row major.
    fn image(&self) -> Vec<f64> {
        read((&self.device, &self.queue), &self.hdr)
            .iter()
            .map(|p| (p[0] + p[1] + p[2]) / 3.0)
            .collect()
    }
}

type Pixel = [u32; 2];

fn centre(pixel: Pixel) -> [f64; 2] {
    pixel.map(|v| f64::from(v) + 0.5)
}

/// Pixels whose centre lies in the plane, a pixel in from its border.
fn plane_pixels() -> Vec<Pixel> {
    let (x, y) = (scene::PLANE_LEFT + 1.0, scene::PLANE_TOP + 1.0);
    let (w, h) = (scene::PLANE_WIDTH - 2.0, scene::PLANE_HEIGHT - 2.0);
    region(x..x + w, y..y + h)
}

/// Pixels the bar vacated during the last `UNCOVERED_FRAMES` frames before `frame`: centres
/// from a pixel inside the bar's left edge `UNCOVERED_FRAMES` ago to a pixel before its
/// current left edge, and inside its rows by a pixel.
fn strip_pixels(frame: u64) -> Vec<Pixel> {
    let from = scene::bar_left(frame - UNCOVERED_FRAMES) + 1.0;
    let to = scene::bar_left(frame) - 1.0;
    region(
        from..to,
        scene::BAR_TOP + 1.0..scene::BAR_TOP + scene::BAR_SIZE[1] - 1.0,
    )
}

fn region(x: std::ops::Range<f64>, y: std::ops::Range<f64>) -> Vec<Pixel> {
    let mut pixels = Vec::new();
    for py in 0..OUTPUT[1] {
        for px in 0..OUTPUT[0] {
            let c = centre([px, py]);
            if x.contains(&c[0]) && y.contains(&c[1]) {
                pixels.push([px, py]);
            }
        }
    }
    pixels
}

/// Box filtered truth of `pixel` for the bar at `left`, 4x4 samples.
fn truth(pixel: Pixel, left: f64) -> f64 {
    const SUB: u32 = 4;
    let at = |v: u32, sub: u32| f64::from(v) + (f64::from(sub) + 0.5) / f64::from(SUB);
    let mut sum = 0.0;
    for j in 0..SUB {
        for i in 0..SUB {
            sum += scene::value([at(pixel[0], i), at(pixel[1], j)], left);
        }
    }
    sum / f64::from(SUB * SUB)
}

fn index(pixel: Pixel) -> usize {
    usize::try_from(pixel[1] * OUTPUT[0] + pixel[0]).unwrap()
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0_u32), |(s, n), v| (s + v, n + 1));
    sum / f64::from(count)
}

fn error(image: &[f64], pixels: &[Pixel], left: f64) -> f64 {
    mean(
        pixels
            .iter()
            .map(|&p| (image[index(p)] - truth(p, left)).abs()),
    )
}

fn flicker(image: &[f64], previous: &[f64], pixels: &[Pixel]) -> f64 {
    mean(
        pixels
            .iter()
            .map(|&p| (image[index(p)] - previous[index(p)]).abs()),
    )
}

/// Metrics averaged over the last `MEASURED` frames.
#[derive(Clone, Copy, Debug)]
struct Metrics {
    strip_error: f64,
    plane_error: f64,
    plane_flicker: f64,
}

fn run(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2], accumulate: bool) -> Metrics {
    let rig = Rig::new(gpu, render);
    let plane = plane_pixels();
    let mut previous = Vec::new();
    let (mut strip, mut plane_error, mut plane_flicker) = (Vec::new(), Vec::new(), Vec::new());
    for frame in 0..FRAMES {
        rig.frame(frame, accumulate && frame > 0);
        if frame + MEASURED < FRAMES - 1 {
            continue;
        }
        let image = rig.image();
        if frame + MEASURED >= FRAMES {
            let left = scene::bar_left(frame);
            strip.push(error(&image, &strip_pixels(frame), left));
            plane_error.push(error(&image, &plane, left));
            plane_flicker.push(flicker(&image, &previous, &plane));
        }
        previous = image;
    }
    Metrics {
        strip_error: mean(strip.into_iter()),
        plane_error: mean(plane_error.into_iter()),
        plane_flicker: mean(plane_flicker.into_iter()),
    }
}

fn measure(millionths: u32) -> (Metrics, Metrics) {
    let gpu = crate::gpu_tests::gpu();
    let render = RenderScale::new(millionths)
        .unwrap()
        .extent(OUTPUT)
        .unwrap();
    let temporal = run(&gpu, render, true);
    let reset = run(&gpu, render, false);
    eprintln!(
        "scale {millionths} R {render:?}: temporal {temporal:?} | reset {reset:?} | strip ratio {:.4}",
        temporal.strip_error / reset.strip_error
    );
    (temporal, reset)
}

/// The baseline of the PR 2 path, measured on lavapipe (mean absolute HDR error over the last
/// eight of 48 frames; bar 24 x 48 pixels moving 2 pixels per frame, 12 pixel checker cells).
///
/// | scale | R | strip error, temporal | strip error, reset | plane error, temporal / reset | plane flicker, temporal / reset |
/// |---|---|---|---|---|---|
/// | 0.50 | 80x48 | 0.0621 | 0.0430 | 0.0714 / 0.1080 | 0.0109 / 0.1639 |
/// | 0.67 | 107x64 | 0.0488 | 0.0319 | 0.0383 / 0.0828 | 0.0086 / 0.1319 |
/// | 0.77 | 123x74 | 0.0482 | 0.0303 | 0.0336 / 0.0725 | 0.0074 / 0.1220 |
///
/// The history leaves a trail in the uncovered strip: temporal error is 1.45, 1.53 and 1.59
/// times the reset error. The reset error is the spatial reconstruction floor of the strip
/// (checker edges inside it), 69%, 65% and 63% of the temporal error. A later step that
/// rejects history exactly where the surface changed can therefore reach the reset error but
/// not go below it, and a cut of the total strip error by 50% (0.0310, 0.0244, 0.0241) is below
/// that floor at 0.5 (0.0430) and at 0.67 and 0.77. The reduction to assert is on the excess of
/// the strip error over the reset error: 0.0191, 0.0169 and 0.0179 today.
///
/// Only properties that hold today are asserted. Every number is finite and positive; the trail
/// exists with more margin than the 8 to 10% platform spread (temporal strip error at least
/// 1.15 times the reset one); and the static plane is stable under accumulation (flicker at most
/// a quarter of the reset flicker, measured at less than a tenth).
fn assert_baseline((temporal, reset): (Metrics, Metrics)) {
    for m in [temporal, reset] {
        for v in [m.strip_error, m.plane_error, m.plane_flicker] {
            assert!(v.is_finite() && v > 0.0, "{m:?}");
        }
    }
    assert!(
        temporal.strip_error >= 1.15 * reset.strip_error,
        "{temporal:?} {reset:?}"
    );
    assert!(
        temporal.plane_flicker <= 0.25 * reset.plane_flicker,
        "{temporal:?} {reset:?}"
    );
}

#[test]
fn half_scale_disocclusion() {
    assert_baseline(measure(500_000));
}

#[test]
fn two_thirds_scale_disocclusion() {
    assert_baseline(measure(670_000));
}

#[test]
fn three_quarters_scale_disocclusion() {
    assert_baseline(measure(770_000));
}
