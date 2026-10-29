//! End-to-end TAA behaviour through the `FrameRenderer`: real jittered frames, the scene
//! rasterised into the frame targets, the TAA resolve, and a read-back of the tone-mapped output.
//!
//! The scene is a far wall with a thin bright line, and a nearer slanted triangle in front of it.
//! Every comparison is against a supersampled reference: the same scene rasterised at 4x the
//! width and height without jitter, box-averaged in linear HDR and tone mapped by the CPU curve,
//! which equals the renderer's output stage for the fixed exposure of 1 used here, then encoded
//! with the sRGB transfer function. Errors are measured on the 8-bit sRGB bytes the output
//! stage writes (scaled to 0..=1), so they carry a quantisation floor of about 0.001.
use crate::{
    compositor::FrameRenderer,
    device::GpuContext,
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use glam::Mat4;
use wgpu::util::DeviceExt;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_post::{
    AntiAliasing, Cmaa2Preset, FrameConfig, FrameError, MsaaSamples, PostResolve, Readback,
    SmaaPreset, TaaSettings, ToneMap,
};

const SIZE: u32 = 64;
const SCALE: u32 = 4;
const WIDTH: usize = 64;
const FACTOR: usize = 4;
const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Frames submitted before the converged image is read.
const FRAMES: usize = 32;
/// Camera travel per frame, in millimetres: about 0.27 px on the triangle (8 m away), 0.11 px on the wall.
const STEP_MM: i64 = 41;
const SLOTS: usize = 3;

type Image = Vec<[f64; 3]>;

/// World space is x right, y forward, z up. The wall is 20 m from the camera, the triangle 8 m.
const SCENE_SHADER: &str = "
struct Camera { vp: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Out { @builtin(position) position: vec4<f32>, @location(0) colour: vec3<f32> }

@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    var world = array<vec3<f32>, 15>(
        vec3(-30.0, 10.0, -20.0), vec3(30.0, 10.0, -20.0), vec3(30.0, 10.0, 25.0),
        vec3(-30.0, 10.0, -20.0), vec3(30.0, 10.0, 25.0), vec3(-30.0, 10.0, 25.0),
        vec3(-9.06, 9.95, -5.0), vec3(-8.94, 9.95, -5.0), vec3(-8.94, 9.95, 6.0),
        vec3(-9.06, 9.95, -5.0), vec3(-8.94, 9.95, 6.0), vec3(-9.06, 9.95, 6.0),
        vec3(-1.2, -2.0, 0.7), vec3(1.2, -2.0, 0.7), vec3(-1.2, -2.0, 2.7));
    var colour = array<vec3<f32>, 3>(
        vec3(0.08, 0.10, 0.30), vec3(1.5, 1.5, 1.5), vec3(0.9, 0.6, 0.1));
    var out: Out;
    out.position = camera.vp * vec4(world[i], 1.0);
    out.colour = colour[i / 6u];
    return out;
}
@fragment fn fs(in: Out) -> @location(0) vec4<f32> { return vec4(in.colour, 1.0); }
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

/// A camera at `eye_x` looking along +y; the target moves with the eye so the view only translates.
fn frame(eye_x: i64, size: u32) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -10_000, 1_700],
            target_millimeters: [eye_x, 0, 1_700],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: size,
            height: size,
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

struct Scene {
    pipeline: wgpu::RenderPipeline,
}

struct Target<'a> {
    color: &'a wgpu::TextureView,
    resolve: Option<&'a wgpu::TextureView>,
    depth: &'a wgpu::TextureView,
}

impl Scene {
    fn new(device: &wgpu::Device, samples: u32) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("taa end to end scene"),
            source: wgpu::ShaderSource::Wgsl(SCENE_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("taa end to end scene"),
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline }
    }

    fn draw(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &Target<'_>,
        view_projection: Mat4,
    ) {
        let contents: Vec<u8> = view_projection
            .to_cols_array()
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("taa end to end camera"),
            contents: &contents,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("taa end to end camera"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("taa end to end scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.color,
                resolve_target: target.resolve,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: target.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..15, 0..1);
    }
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

fn decode_output(bytes: &[u8]) -> Image {
    bytes
        .chunks_exact(4)
        .map(|texel| [0, 1, 2].map(|lane| f64::from(texel[lane]) / 255.0))
        .collect()
}

fn decode_hdr(bytes: &[u8]) -> Image {
    bytes
        .chunks_exact(8)
        .map(|texel| {
            [0, 2, 4].map(|lane| {
                f64::from(half_to_f32(u16::from_le_bytes([
                    texel[lane],
                    texel[lane + 1],
                ])))
            })
        })
        .collect()
}

fn texture(device: &wgpu::Device, size: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("taa end to end texture"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// One renderer with its scene pipeline and a display-linear read-back target.
struct Rig<'a> {
    context: &'a GpuContext,
    renderer: FrameRenderer,
    scene: Scene,
    output: wgpu::Texture,
    readback: Readback,
    submitted: usize,
}

impl<'a> Rig<'a> {
    fn new(context: &'a GpuContext, samples: MsaaSamples, resolve: PostResolve) -> Self {
        let renderer = FrameRenderer::hdr_antialiased(
            context,
            [SIZE, SIZE],
            OUTPUT,
            AntiAliasing::new(samples, resolve),
            ToneMap::Reinhard,
            None,
        )
        .unwrap();
        let device = context.device();
        Self {
            context,
            renderer,
            scene: Scene::new(device, samples.count()),
            output: texture(device, SIZE, OUTPUT),
            readback: Readback::new(device, FrameConfig::new([SIZE, SIZE], OUTPUT, 1), 1 << 20)
                .unwrap(),
            submitted: 0,
        }
    }

    /// Submits one frame as a host would (rotating frame slots) and reads the output back.
    fn render(&mut self, eye_x: i64) -> Image {
        let device = self.context.device();
        self.renderer.select_frame_slot(self.submitted % SLOTS);
        self.submitted += 1;
        let prepared = self.renderer.prepare_uniform(frame(eye_x, SIZE)).unwrap();
        let view = self
            .output
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (renderer, scene) = (&mut self.renderer, &self.scene);
        let transaction = self
            .context
            .submission()
            .prepare(device, |encoder, _| {
                let targets = renderer.targets();
                let target = Target {
                    color: targets.scene(),
                    resolve: None,
                    depth: targets.depth(),
                };
                scene.draw(device, encoder, &target, prepared.view_projection);
                renderer.encode_frame(device, encoder, (&[], &[]), &mut [], (prepared, None))?;
                renderer.encode_output(encoder, &view).unwrap();
                Ok::<_, FrameError>(())
            })
            .unwrap();
        transaction.submit().unwrap();
        let bytes = self
            .readback
            .read(device, self.context.submission(), &self.output)
            .unwrap();
        decode_output(&bytes)
    }

    fn run(&mut self, cameras: impl IntoIterator<Item = i64>) -> Image {
        cameras
            .into_iter()
            .map(|eye_x| self.render(eye_x))
            .last()
            .expect("at least one frame")
    }
}

fn taa() -> PostResolve {
    PostResolve::taa_with(TaaSettings::DEFAULT)
}

/// The output stage in the sRGB bytes' scale: Reinhard at exposure 1, then the sRGB transfer.
fn display(radiance: f64) -> f64 {
    let linear = ToneMap::Reinhard.evaluate(radiance, 1.0);
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn tone_map(image: Image) -> Image {
    image.into_iter().map(|rgb| rgb.map(display)).collect()
}

/// The scene at `SCALE` times the resolution, unjittered, box-averaged in HDR and tone mapped.
fn supersampled(context: &GpuContext, eye_x: i64) -> Image {
    let device = context.device();
    let big = SIZE * SCALE;
    let color = texture(device, big, HDR);
    let depth = texture(device, big, wgpu::TextureFormat::Depth32Float);
    let scene = Scene::new(device, 1);
    let mut encoder = context.submission().begin(device).unwrap();
    scene.draw(
        device,
        &mut encoder,
        &Target {
            color: &color.create_view(&wgpu::TextureViewDescriptor::default()),
            resolve: None,
            depth: &depth.create_view(&wgpu::TextureViewDescriptor::default()),
        },
        frame(eye_x, big).view_projection,
    );
    encoder.finish().submit().unwrap();
    let bytes = Readback::new(device, FrameConfig::new([big, big], HDR, 1), 1 << 24)
        .unwrap()
        .read(device, context.submission(), &color)
        .unwrap();
    let fine = decode_hdr(&bytes);
    let mut average = vec![[0.0; 3]; WIDTH * WIDTH];
    for (index, pixel) in average.iter_mut().enumerate() {
        let (column, row) = (index % WIDTH, index / WIDTH);
        for sy in 0..FACTOR {
            for sx in 0..FACTOR {
                let sample = fine[(row * FACTOR + sy) * WIDTH * FACTOR + column * FACTOR + sx];
                for (lane, value) in sample.into_iter().enumerate() {
                    pixel[lane] += value / count(FACTOR * FACTOR);
                }
            }
        }
    }
    tone_map(average)
}

fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap())
}

fn mean_abs_error(a: &Image, b: &Image) -> f64 {
    assert_eq!(a.len(), b.len());
    let total: f64 = a
        .iter()
        .zip(b)
        .flat_map(|(x, y)| x.iter().zip(y))
        .map(|(x, y)| (x - y).abs())
        .sum();
    total / count(3 * a.len())
}

/// Sub-pixel column of the wall to triangle edge in rows 32..=36, from the red channel's coverage.
fn edge_column(image: &Image) -> f64 {
    const FIRST: usize = 10;
    const LAST: usize = 25;
    let rows = 32..=36_usize;
    let mut total = 0.0;
    for row in rows.clone() {
        let red = |column: usize| image[row * WIDTH + column][0];
        let (wall, triangle) = (red(FIRST), red(LAST));
        let covered: f64 = (FIRST..=LAST)
            .map(|column| ((red(column) - wall) / (triangle - wall)).clamp(0.0, 1.0))
            .sum();
        total += count(LAST + 1) - covered;
    }
    total / count(rows.count())
}

fn cameras(count: usize) -> Vec<i64> {
    (0..count)
        .map(|index| i64::try_from(index).unwrap() * STEP_MM)
        .collect()
}

fn converged(samples: MsaaSamples) {
    let context = context();
    if !supported(&context, samples) {
        return;
    }
    let reference = supersampled(&context, 0);
    let single = Rig::new(&context, MsaaSamples::X1, PostResolve::None).render(0);
    let mut rig = Rig::new(&context, samples, taa());
    let accumulated = rig.run(std::iter::repeat_n(0, FRAMES));
    assert!(rig.renderer.has_temporal_history());
    let (taa_error, single_error) = (
        mean_abs_error(&accumulated, &reference),
        mean_abs_error(&single, &reference),
    );
    assert!(taa_error < TAA_BOUND, "{samples:?} TAA error {taa_error}");
    assert!(
        taa_error < single_error * TAA_MARGIN,
        "{samples:?} TAA {taa_error} vs single frame {single_error}"
    );
}

/// Mean absolute display error of the converged image against the supersampled reference.
const TAA_BOUND: f64 = 0.0035;
/// The converged image must beat one unjittered, unantialiased 1x frame by this factor.
const TAA_MARGIN: f64 = 0.75;

#[test]
fn taa_converges_to_the_supersampled_reference_without_msaa() {
    converged(MsaaSamples::X1);
}

#[test]
fn taa_converges_to_the_supersampled_reference_with_four_msaa_samples() {
    converged(MsaaSamples::X4);
}

fn translation(samples: MsaaSamples) {
    let context = context();
    if !supported(&context, samples) {
        return;
    }
    let path = cameras(16);
    let last = *path.last().unwrap();
    let reference = supersampled(&context, last);
    let first_reference = supersampled(&context, 0);
    let still = Rig::new(&context, samples, PostResolve::None).render(last);
    let accumulated = Rig::new(&context, samples, taa()).run(path.clone());
    let mut cut = Rig::new(&context, samples, taa());
    let mut cut_output = Vec::new();
    for &eye_x in &path {
        cut.renderer.reset_temporal_history();
        cut_output = cut.render(eye_x);
    }
    let errors = [
        mean_abs_error(&accumulated, &reference),
        mean_abs_error(&accumulated, &first_reference),
        mean_abs_error(&accumulated, &still),
        mean_abs_error(&still, &reference),
        mean_abs_error(&cut_output, &reference),
    ];
    let edges = [&reference, &accumulated, &cut_output, &first_reference].map(edge_column);
    let [moving, stale, no_history, single, reset] = errors;
    assert!(moving < MOVING_BOUND, "moving TAA error {moving}");
    assert!(
        no_history < NO_HISTORY_BOUND,
        "TAA vs unjittered frame {no_history}"
    );
    assert!(
        moving < stale * 0.5,
        "output follows the final camera: {moving} vs {stale}"
    );
    assert!(
        (edges[3] - edges[0]).abs() > 2.0,
        "the camera really moved {edges:?}"
    );
    let drift = (edges[1] - edges[0]).abs();
    assert!(drift < EDGE_BOUND, "edge position drift {drift} {edges:?}");
    // The run that discards history before every frame shows what a single frame looks like at
    // this bound: it passes the bounds too, so the smear check above is what needs history.
    assert!(reset < NO_HISTORY_BOUND, "reset {reset}");
    assert!(
        (edges[2] - edges[0]).abs() < EDGE_BOUND + 0.25,
        "reset edge {edges:?}"
    );
    assert!(reset > single * 0.5, "reset {reset} single {single}");
}

/// Edge position bound in pixels: the TAA edge may trail the true edge by at most half a pixel.
const EDGE_BOUND: f64 = 0.5;
/// Error to the supersampled reference after 16 frames of 0.25 px motion on the near object.
const MOVING_BOUND: f64 = 0.006;
/// Error to an unjittered render of the same camera; a smear trail of several pixels would exceed it.
const NO_HISTORY_BOUND: f64 = 0.006;

#[test]
fn a_translating_camera_leaves_no_smear_trail_without_msaa() {
    translation(MsaaSamples::X1);
}

#[test]
fn a_translating_camera_leaves_no_smear_trail_with_four_msaa_samples() {
    translation(MsaaSamples::X4);
}

#[test]
fn a_reset_makes_the_next_frames_equal_the_first_frames_of_a_fresh_renderer() {
    let context = context();
    let mut fresh = Rig::new(&context, MsaaSamples::X1, taa());
    let expected = [fresh.render(0), fresh.render(STEP_MM)];
    let mut steady = Rig::new(&context, MsaaSamples::X1, taa());
    steady.run(std::iter::repeat_n(0, 3 * 4));
    assert!(steady.renderer.has_temporal_history());
    steady.renderer.reset_temporal_history();
    assert!(!steady.renderer.has_temporal_history());
    // Twelve frames were submitted, so the next one uses slot 0 like a fresh renderer's first.
    let actual = [steady.render(0), steady.render(STEP_MM)];
    // The resolve ignores the stale history texture when the block says there is none and the
    // jitter sequence restarts, so the result is the same computation on the same inputs.
    assert_eq!(actual[0], expected[0], "first frame after the reset");
    assert_eq!(actual[1], expected[1], "second frame after the reset");
    assert_ne!(
        steady.run(std::iter::repeat_n(STEP_MM, 4)),
        expected[0],
        "the test distinguishes accumulated frames from a reset one"
    );
}

#[test]
fn renderers_without_taa_do_not_jitter_or_accumulate() {
    let context = context();
    let resolves = [
        PostResolve::None,
        PostResolve::fxaa(0.5),
        PostResolve::smaa(SmaaPreset::Medium),
        PostResolve::cmaa2(Cmaa2Preset::Medium),
    ];
    let wall = [0.08, 0.10, 0.30].map(display);
    for resolve in resolves {
        let mut rig = Rig::new(&context, MsaaSamples::X1, resolve);
        assert_eq!(rig.renderer.projection_jitter(), None, "{resolve:?}");
        let frames: Vec<Image> = (0..4).map(|_| rig.render(0)).collect();
        assert!(!rig.renderer.has_temporal_history(), "{resolve:?}");
        assert!(
            frames.windows(2).all(|pair| pair[0] == pair[1]),
            "{resolve:?}"
        );
        let corner = frames[0][WIDTH + 1];
        for (lane, expected) in wall.into_iter().enumerate() {
            assert!(
                (corner[lane] - expected).abs() < 3e-3,
                "{resolve:?} {corner:?}"
            );
        }
    }
}
