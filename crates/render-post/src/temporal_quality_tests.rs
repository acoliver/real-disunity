//! Static image quality of the temporal reconstruction, measured below the public "not yet
//! scheduled" gate on lavapipe. A WGSL pass rasterises the analytic image of
//! `temporal_quality_scene` at the internal extent and the jitter of
//! `ProjectionJitter::for_extents`, one aliased sample per pixel and no motion. The reference is
//! the same function box filtered over each output pixel with 4x4 supersampling in f64.
//!
//! Three reconstructions are compared on the interior of the output (a two pixel border is
//! excluded), as the mean absolute HDR error over the second of two full jitter cycles:
//! temporal with accumulated history, temporal with the history reset every frame (the spatial
//! Lanczos2 only) and a single unjittered internal frame through the EASU f64 reference (HDR
//! values here, so it is a filter comparison and not the display-referred production path).
use std::sync::{Arc, Mutex};
use worldrunner_render_plan::post::RenderScale;

use wgpu::util::DeviceExt;

use crate::hdr_temporal_build_tests::{public_factory, settings};
use crate::taa_gpu_test_support::{passthrough_pipeline, read};
use crate::temporal_quality_scene as scene;
use crate::upscale_tests::reference::Image;
use crate::{
    FrameConfig, FrameTargets, PostChain, PostError, ProjectionJitter, Resources, StageFactory,
    StageSet, TaaSettings, TemporalBlock, TemporalCamera, ToneMapOutput,
};

pub(crate) const OUTPUT: [u32; 2] = [160, 96];
const BORDER: usize = 2;
const EDGE_BAND: f64 = 2.0;

/// The public factory, which also keeps the reconstruction output of slot 0 so the test can read HDR.
pub(crate) struct Recording {
    pub(crate) inner: ToneMapOutput,
    pub(crate) output: Arc<Mutex<Option<wgpu::Texture>>>,
}

impl StageFactory for Recording {
    fn validate_config(&self, device: &wgpu::Device, config: FrameConfig) -> Result<(), PostError> {
        self.inner.validate_config(device, config)
    }
    fn frame_slots(&self) -> usize {
        self.inner.frame_slots()
    }
    fn needs_velocity(&self) -> bool {
        true
    }
    fn settle_build(&self, accepted: bool) {
        self.inner.settle_build(accepted);
    }
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
        slot: usize,
    ) -> Result<StageSet, PostError> {
        let set = self
            .inner
            .build_for_slot(device, targets, persistent, slot)?;
        if slot == 0 {
            *self.output.lock().unwrap() = set.resources.textures.first().cloned();
        }
        Ok(set)
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, PostError> {
        self.build_for_slot(device, targets, persistent, 0)
    }
}

pub(crate) struct Rig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    chain: PostChain<Recording>,
    hdr: wgpu::Texture,
    sink: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    render: [u32; 2],
    pub(crate) jitter: ProjectionJitter,
}

impl Rig {
    pub(crate) fn new(gpu: &(wgpu::Device, wgpu::Queue), render: [u32; 2]) -> Self {
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
        assert_eq!((hdr.width(), hdr.height()), (OUTPUT[0], OUTPUT[1]));
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("analytic scene"),
            source: wgpu::ShaderSource::Wgsl(scene::wgsl().into()),
        });
        let pipeline = passthrough_pipeline(
            &device,
            &module,
            "paint",
            Some(wgpu::TextureFormat::Rgba16Float),
        );
        let sink =
            crate::color_texture(&device, "sink", OUTPUT, wgpu::TextureFormat::Rgba8UnormSrgb);
        Self {
            device,
            queue,
            chain,
            hdr,
            sink,
            pipeline,
            render,
            jitter: ProjectionJitter::for_extents(render, OUTPUT).unwrap(),
        }
    }

    /// Rasterises the scene into the internal target for `offset`.
    fn paint(&self, encoder: &mut wgpu::CommandEncoder, offset: [f32; 2]) {
        let values = [
            f32::from(u16::try_from(self.render[0]).unwrap()),
            f32::from(u16::try_from(self.render[1]).unwrap()),
            offset[0],
            offset[1],
        ];
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
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("analytic scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.chain.targets().resolved(),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Runs frame `index` of the jitter sequence. Motion is the zero-initialised velocity target.
    fn frame(&self, index: u64, history_valid: bool) {
        self.frame_panned(index, history_valid, ([0.0; 2], None));
    }

    /// Frame `index` with the image content shifted by `pan` canvas pixels, +x right and +y down,
    /// as a camera pan would: the content at output position `p` is the scene at `p + pan`. The
    /// camera term is uploaded when given, otherwise the slot keeps the zeroed block of a stage
    /// that never heard of one.
    pub(crate) fn frame_panned(
        &self,
        index: u64,
        history_valid: bool,
        (pan, camera): ([f64; 2], Option<TemporalCamera>),
    ) {
        let offset = self.jitter.offset_pixels(index);
        let per_canvas = |axis: usize| f64::from(self.render[axis]) / scene::CANVAS[axis];
        #[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
        let painted =
            [0, 1].map(|axis| (f64::from(offset[axis]) - pan[axis] * per_canvas(axis)) as f32);
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
        self.paint(&mut encoder, painted);
        let factory = &self.chain.factory().inner;
        if let Some(camera) = camera {
            factory.prepare_temporal_camera(&mut encoder, 0, camera);
        }
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

    /// The HDR reconstruction of the last frame, mean of the three colour channels per pixel.
    pub(crate) fn image(&self) -> Vec<f64> {
        read((&self.device, &self.queue), &self.hdr)
            .iter()
            .map(|p| (p[0] + p[1] + p[2]) / 3.0)
            .collect()
    }
}

/// Box filtered truth at the output grid and the pixels that sit near a discontinuity.
pub(crate) struct Truth {
    value: Vec<f64>,
    near_edge: Vec<bool>,
}

fn truth() -> Truth {
    truth_panned([0.0; 2])
}

/// The box filtered image of the scene shifted by `pan` canvas pixels.
pub(crate) fn truth_panned(pan: [f64; 2]) -> Truth {
    const SUB: u32 = 4;
    let (mut value, mut near_edge) = (Vec::new(), Vec::new());
    for y in 0..OUTPUT[1] {
        for x in 0..OUTPUT[0] {
            let mut sum = 0.0;
            for j in 0..SUB {
                for i in 0..SUB {
                    let at = |pixel: u32, sub: u32| f64::from(pixel) + (f64::from(sub) + 0.5) / 4.0;
                    sum += scene::value([at(x, i) + pan[0], at(y, j) + pan[1]]);
                }
            }
            value.push(sum / f64::from(SUB * SUB));
            let centre = [f64::from(x) + 0.5 + pan[0], f64::from(y) + 0.5 + pan[1]];
            near_edge.push(scene::edge_distance(centre) <= EDGE_BAND);
        }
    }
    Truth { value, near_edge }
}

/// Mean absolute error over the region of interest, split by distance to an edge.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Errors {
    pub(crate) all: f64,
    pub(crate) edges: f64,
    pub(crate) interior: f64,
}

pub(crate) fn errors(truth: &Truth, image: &[f64]) -> Errors {
    let width = usize::try_from(OUTPUT[0]).unwrap();
    let height = usize::try_from(OUTPUT[1]).unwrap();
    let mut sums = [(0.0, 0_u32); 2];
    for y in BORDER..height - BORDER {
        for x in BORDER..width - BORDER {
            let i = y * width + x;
            let bucket = &mut sums[usize::from(truth.near_edge[i])];
            bucket.0 += (image[i] - truth.value[i]).abs();
            bucket.1 += 1;
        }
    }
    let mean = |(sum, count): (f64, u32)| sum / f64::from(count);
    Errors {
        all: (sums[0].0 + sums[1].0) / f64::from(sums[0].1 + sums[1].1),
        edges: mean(sums[1]),
        interior: mean(sums[0]),
    }
}

fn average(list: &[Errors]) -> Errors {
    let n = f64::from(u32::try_from(list.len()).unwrap());
    let sum = |field: fn(&Errors) -> f64| list.iter().map(field).sum::<f64>() / n;
    Errors {
        all: sum(|e| e.all),
        edges: sum(|e| e.edges),
        interior: sum(|e| e.interior),
    }
}

/// Mean error over the second of two jitter cycles, with or without accumulated history.
fn temporal(
    gpu: &(wgpu::Device, wgpu::Queue),
    truth: &Truth,
    render: [u32; 2],
    accumulate: bool,
) -> Errors {
    let rig = Rig::new(gpu, render);
    let cycle = u64::from(rig.jitter.length());
    let mut second = Vec::new();
    for index in 0..2 * cycle {
        rig.frame(index, accumulate && index > 0);
        if index >= cycle {
            second.push(errors(truth, &rig.image()));
        }
    }
    average(&second)
}

/// One unjittered internal frame through the EASU f64 reference.
fn easu(truth: &Truth, render: [u32; 2]) -> Errors {
    let size = render.map(|side| i32::try_from(side).unwrap());
    let mut pixels = Vec::new();
    for y in 0..render[1] {
        for x in 0..render[0] {
            let at = |pixel: u32, side: u32, canvas: u32| {
                (f64::from(pixel) + 0.5) / f64::from(side) * f64::from(canvas)
            };
            let v = scene::value([at(x, render[0], OUTPUT[0]), at(y, render[1], OUTPUT[1])]);
            pixels.push([v, v, v, 1.0]);
        }
    }
    let output =
        Image { size, pixels }.reconstruct(OUTPUT.map(|s| i32::try_from(s).unwrap()), true);
    let image: Vec<f64> = output.pixels.iter().map(|p| p[0]).collect();
    errors(truth, &image)
}

struct Row {
    render: [u32; 2],
    phases: u32,
    temporal: Errors,
    reset: Errors,
    easu: Errors,
}

fn measure(millionths: u32) -> Row {
    let gpu = crate::gpu_tests::gpu();
    let render = RenderScale::new(millionths)
        .unwrap()
        .extent(OUTPUT)
        .unwrap();
    let truth = truth();
    let row = Row {
        render,
        phases: ProjectionJitter::for_extents(render, OUTPUT)
            .unwrap()
            .length(),
        temporal: temporal(&gpu, &truth, render, true),
        reset: temporal(&gpu, &truth, render, false),
        easu: easu(&truth, render),
    };
    eprintln!(
        "scale {millionths} R {:?} phases {}: temporal {:?} | reset {:?} | easu {:?} | ratio all {:.4} edges {:.4} interior {:.4}",
        row.render,
        row.phases,
        row.temporal,
        row.reset,
        row.easu,
        row.temporal.all / row.reset.all,
        row.temporal.edges / row.reset.edges,
        row.temporal.interior / row.reset.interior,
    );
    row
}

/// The design's gate: the accumulated temporal error over the region of interest is at most 0.90
/// of the error of the spatial Lanczos2 alone, at every render scale. The bound is the design's.
///
/// Measured on lavapipe (error over the second of two jitter cycles, all / edges / interior):
/// scale 0.5 (R 80x48, 32 phases) ratio 0.621 / 0.618 / 0.653, scale 0.67 (R 107x64, 18 phases)
/// 0.518 / 0.483 / 1.124, scale 0.77 (R 123x74, 14 phases) 0.450 / 0.403 / 1.316. The interior ratio
/// is above one at the two larger scales but its absolute error is below 0.002, a tenth of the
/// error on the edges, so it does not decide the gate.
fn assert_gate(row: &Row) {
    for e in [row.temporal, row.reset, row.easu] {
        assert!(e.all.is_finite() && e.edges.is_finite() && e.interior.is_finite());
    }
    let ratio = row.temporal.all / row.reset.all;
    assert!(
        ratio <= 0.90,
        "temporal / reset = {ratio:.4} at {:?}",
        row.render
    );
}

#[test]
fn half_scale() {
    assert_gate(&measure(500_000));
}

#[test]
fn two_thirds_scale() {
    assert_gate(&measure(670_000));
}

#[test]
fn three_quarters_scale() {
    assert_gate(&measure(770_000));
}
