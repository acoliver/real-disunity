//! The TAA resolve and history passes on synthetic images, compared with the scalar reference.
//!
//! Bound: the GPU computes in f32 and stores `Rgba16Float`, so each channel of the result is
//! compared with the f64 reference in luminance-compressed space (`c / (1 + luma)`, values
//! below 1) within 1e-3 absolute, which is 1e-3 relative to the compressed full scale. Alpha is
//! raw and compared within 1e-3 relative (absolute below 1). Inputs
//! are the f32 values the GPU receives; the history is quantised to f16 first, exactly as
//! stored. This does not establish behavior with a multisampled depth texture or against a
//! real camera's matrices.
use wgpu::util::DeviceExt;

use crate::cas_reference::Image;
use crate::taa_gpu::{self, Inputs};
use crate::taa_reference::{Depth, Frame, IDENTITY, Mat4, compress};
use crate::{
    FrameConfig, FrameTargets, PostChain, PostError, Readback, Resources, Stage, StageFactory,
    StageId, StageSet, StageTarget, TaaSettings, TaaUniform, color_texture,
};

const BOUND: f64 = 1e-3;

/// Test-only passes: a depth upload (`Depth32Float` cannot be a copy destination) and an
/// identity pass to the chain output, which a stage set requires as its last stage.
const SHADER: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
struct Depth { @builtin(frag_depth) value: f32 }
@fragment fn upload_depth(@builtin(position) p: vec4<f32>) -> Depth {
    return Depth(textureLoad(source, vec2<i32>(p.xy), 0).r);
}
struct SampleDepth { @builtin(frag_depth) value: f32 }
// Runs once per sample. Plane `s` of the source is laid out in columns x * count + s.
@fragment fn upload_sample_depth(
    @builtin(position) p: vec4<f32>,
    @builtin(sample_index) s: u32,
) -> SampleDepth {
    return SampleDepth(textureLoad(source, vec2<i32>(i32(p.x) * 4 + i32(s), i32(p.y)), 0).r);
}
@fragment fn identity(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(p.xy), 0);
}
";

struct Case {
    color: Image,
    depth: Depth,
    history: Image,
    reprojection: Mat4,
    jitter: [f32; 2],
    valid: bool,
    settings: TaaSettings,
    /// Per-sample depth planes of a multisampled depth texture. `depth` is then their
    /// per-pixel minimum, which is what the resolve must see.
    depth_samples: Vec<Depth>,
}
impl Case {
    fn new(color: Image, history: Image) -> Self {
        let depth = Depth::from_fn(color.size, |_, _| 0.5);
        Self {
            color,
            depth,
            history,
            reprojection: IDENTITY,
            jitter: [0.0; 2],
            valid: true,
            settings: TaaSettings::DEFAULT,
            depth_samples: Vec::new(),
        }
    }

    fn with_depth_samples(mut self, samples: Vec<Depth>) -> Self {
        self.depth = Depth::minimum(&samples);
        self.depth_samples = samples;
        self
    }

    fn size(&self) -> [u32; 2] {
        self.color.size.map(|v| u32::try_from(v).unwrap())
    }

    fn quantised_history(&self) -> Image {
        Image {
            size: self.history.size,
            pixels: self
                .history
                .pixels
                .iter()
                .map(|p| p.map(|v| f16_to_f32(f16_bits(v))))
                .collect(),
        }
    }

    fn expected(&self) -> Vec<[f64; 4]> {
        let history = self.quantised_history();
        Frame {
            color: &self.color,
            depth: &self.depth,
            history: &history,
            reprojection: self.reprojection,
            jitter: self.jitter.map(f64::from),
            history_valid: self.valid,
            settings: self.settings,
            samples: u32::try_from(self.depth_samples.len().max(1)).unwrap(),
        }
        .resolve()
    }
}

/// Round-to-nearest f32 to f16 bits for zero and normal values within the f16 range.
fn f16_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = u16::try_from((bits >> 16) & 0x8000).unwrap();
    let exponent = i32::try_from((bits >> 23) & 0xff).unwrap() - 127 + 15;
    assert!(exponent < 31, "{v} is outside the f16 range");
    if exponent <= 0 {
        return sign;
    }
    let rounded = ((bits & 0x007f_ffff) + 0x1000) >> 13;
    sign | u16::try_from((u32::try_from(exponent).unwrap() << 10) + rounded).unwrap()
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f32::from(bits & 0x3ff);
    if exponent == 0 {
        sign * mantissa * 2f32.powi(-24)
    } else {
        sign * (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15)
    }
}

fn write_texture(queue: &wgpu::Queue, texture: &wgpu::Texture, bytes: &[u8], pixel_bytes: u32) {
    let size = texture.size();
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.width * pixel_bytes),
            rows_per_image: Some(size.height),
        },
        size,
    );
}

fn sampled(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn passthrough_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    fragment: &str,
    format: Option<wgpu::TextureFormat>,
) -> wgpu::RenderPipeline {
    passthrough_pipeline_with(device, module, fragment, format, 1)
}

fn passthrough_pipeline_with(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    fragment: &str,
    format: Option<wgpu::TextureFormat>,
    samples: u32,
) -> wgpu::RenderPipeline {
    let depth = format.is_none();
    let targets: Vec<_> = format.map(|f| Some(f.into())).into_iter().collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth.then(|| wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

/// Renders `depth` into a `Depth32Float` texture through a fragment depth write.
fn upload_depth(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    depth: &Depth,
) -> wgpu::TextureView {
    let size = depth.size.map(|v| u32::try_from(v).unwrap());
    let source = sampled(
        device,
        "TAA test depth source",
        size,
        wgpu::TextureFormat::R32Float,
    );
    let bytes: Vec<u8> = depth.values.iter().flat_map(|v| v.to_le_bytes()).collect();
    write_texture(queue, &source, &bytes, 4);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("TAA test depth"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("TAA test passes"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = passthrough_pipeline(device, &module, "upload_depth", None);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view(&source)),
        }],
    });
    draw_depth((device, queue), &target, &pipeline, &group)
}

/// Draws one fullscreen triangle into `target`, clearing its depth to 1 first, and returns the
/// depth-only view.
fn draw_depth(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    target: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) -> wgpu::TextureView {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let depth_view = view(target);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("TAA test depth upload"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    target.create_view(&wgpu::TextureViewDescriptor {
        aspect: wgpu::TextureAspect::DepthOnly,
        ..Default::default()
    })
}

/// Renders each plane of `planes` (four) into one sample of a 4x `Depth32Float` texture
/// through a per-sample fragment depth write.
fn upload_multisampled_depth(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    planes: &[Depth],
) -> wgpu::TextureView {
    assert_eq!(planes.len(), 4);
    let size = planes[0].size.map(|v| u32::try_from(v).unwrap());
    let source = sampled(
        device,
        "TAA test sample depth source",
        [size[0] * 4, size[1]],
        wgpu::TextureFormat::R32Float,
    );
    let bytes: Vec<u8> = (0..planes[0].values.len())
        .flat_map(|i| planes.iter().map(move |plane| plane.values[i]))
        .flat_map(f32::to_le_bytes)
        .collect();
    write_texture(queue, &source, &bytes, 4);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("TAA test multisampled depth"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 4,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("TAA test passes"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = passthrough_pipeline_with(device, &module, "upload_sample_depth", None, 4);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view(&source)),
        }],
    });
    draw_depth((device, queue), &target, &pipeline, &group)
}

struct Resolve {
    settings: TaaSettings,
    depth_samples: u32,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    uniform: wgpu::Buffer,
    history: wgpu::TextureView,
    output: wgpu::TextureView,
}
impl StageFactory for Resolve {
    fn build(
        &self,
        device: &wgpu::Device,
        _: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set = StageSet::default();
        taa_gpu::append(
            device,
            &mut set,
            &Inputs {
                color: &self.color,
                depth: &self.depth,
                depth_samples: self.depth_samples,
                uniform: &self.uniform,
                history: &self.history,
                output: &self.output,
            },
            self.settings,
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("TAA test passes"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = passthrough_pipeline(device, &module, "identity", Some(taa_gpu::FORMAT));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&self.output),
            }],
        });
        set.stages.push(Stage::fullscreen(
            StageId::new("taa test output", 0),
            &pipeline,
            &[&group],
            StageTarget::Output,
        ));
        Ok(set)
    }
}

fn read((device, queue): (&wgpu::Device, &wgpu::Queue), texture: &wgpu::Texture) -> Vec<[f64; 4]> {
    let size = [texture.width(), texture.height()];
    let raw = Readback::new(
        device,
        FrameConfig::new(size, texture.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(device, &crate::test_authority(queue), texture)
    .unwrap();
    raw.chunks_exact(8)
        .map(|p| {
            std::array::from_fn(|c| {
                let bits = u16::from_le_bytes([p[c * 2], p[c * 2 + 1]]);
                f64::from(f16_to_f32(bits))
            })
        })
        .collect()
}

fn uniform_buffer(device: &wgpu::Device, case: &Case) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("TAA test uniform"),
        contents: &TaaUniform {
            reprojection: case.reprojection,
            jitter_pixels: case.jitter,
            history_valid: case.valid,
        }
        .to_bytes(),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

/// Runs the two passes and returns the resolve output and the history afterwards.
fn run(case: &Case) -> (Vec<[f64; 4]>, Vec<[f64; 4]>) {
    let (device, queue) = crate::gpu_tests::gpu();
    let size = case.size();
    let color = sampled(
        &device,
        "TAA test color",
        size,
        wgpu::TextureFormat::Rgba32Float,
    );
    let bytes: Vec<u8> = case
        .color
        .pixels
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    write_texture(&queue, &color, &bytes, 16);
    let targets = taa_gpu::allocate(&device, size);
    let bytes: Vec<u8> = case
        .history
        .pixels
        .iter()
        .flatten()
        .flat_map(|v| f16_bits(*v).to_le_bytes())
        .collect();
    write_texture(&queue, &targets.history, &bytes, 8);
    let depth = if case.depth_samples.is_empty() {
        upload_depth((&device, &queue), &case.depth)
    } else {
        upload_multisampled_depth((&device, &queue), &case.depth_samples)
    };
    let uniform = uniform_buffer(&device, case);
    let chain = PostChain::new(
        &device,
        FrameConfig::new(size, taa_gpu::FORMAT, 1),
        Resources::default(),
        Resolve {
            settings: case.settings,
            depth_samples: u32::try_from(case.depth_samples.len().max(1)).unwrap(),
            color: view(&color),
            depth,
            uniform,
            history: view(&targets.history),
            output: view(&targets.output),
        },
    )
    .unwrap();
    let sink = color_texture(&device, "TAA test sink", size, taa_gpu::FORMAT);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(&mut encoder, &view(&sink));
    queue.submit([encoder.finish()]);
    let gpu = (&device, &queue);
    (read(gpu, &targets.output), read(gpu, &targets.history))
}

fn assert_matches_reference(case: &Case, context: &str) -> Vec<[f64; 4]> {
    let (actual, _) = run(case);
    let expected = case.expected();
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (index, (a, e)) in actual.iter().zip(&expected).enumerate() {
        let rgb = |p: &[f64; 4]| compress([p[0], p[1], p[2]]);
        let (ca, ce) = (rgb(a), rgb(e));
        for c in 0..3 {
            assert!(
                (ca[c] - ce[c]).abs() <= BOUND,
                "{context}: pixel {index} channel {c}: gpu {a:?} reference {e:?}"
            );
        }
        assert!(
            (a[3] - e[3]).abs() <= BOUND * e[3].max(1.0),
            "{context}: alpha {index}: gpu {} reference {}",
            a[3],
            e[3]
        );
    }
    actual
}

fn gray(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

/// Deterministic value in 0..=1 that varies per pixel and channel.
fn noise(x: i32, y: i32, channel: u32) -> f32 {
    let mut h = u32::try_from(x).unwrap().wrapping_mul(0x9E37_79B1)
        ^ u32::try_from(y).unwrap().wrapping_mul(0x85EB_CA6B)
        ^ channel.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    f32::from(u16::try_from(h & 0xffff).unwrap()) / 65535.0 * 4.0
}

fn noisy(size: [i32; 2], channel_offset: u32) -> Image {
    Image::from_fn(size, |x, y| {
        [0, 1, 2, 3]
            .map(|c| noise(x, y, c + channel_offset))
            .map(|v| v.min(8.0))
    })
}

/// A gray ramp along one axis, 0.2 plus 0.1 per pixel.
fn ramp(size: [i32; 2], along_y: bool, shift: i32) -> Image {
    Image::from_fn(size, |x, y| {
        let t = if along_y { y } else { x } - shift;
        gray(0.25 + 0.125 * f32::from(i16::try_from(t).unwrap()))
    })
}

fn translation(ndc: [f64; 2]) -> Mat4 {
    let mut m = IDENTITY;
    m[3] = [ndc[0], ndc[1], 0.0, 1.0];
    m
}

fn top_row(size: [i32; 2]) -> Image {
    Image::from_fn(size, |_, y| gray(if y == 1 { 1.0 } else { 0.0 }))
}

#[test]
fn a_constant_image_with_valid_identity_history_is_a_fixed_point() {
    let image = Image::from_fn([9, 7], |_, _| [0.5, 0.25, 0.125, 0.75]);
    let mut case = Case::new(image.clone(), image.clone());
    case.jitter = [0.25, -0.5];
    let out = assert_matches_reference(&case, "constant");
    for pixel in out {
        assert!((pixel[0] - 0.5).abs() < 1e-3 && (pixel[2] - 0.125).abs() < 1e-3);
    }
}

#[test]
fn invalid_history_passes_the_reconstructed_current_through() {
    let mut case = Case::new(noisy([11, 6], 0), noisy([11, 6], 9));
    case.valid = false;
    case.jitter = [0.375, 0.125];
    assert_matches_reference(&case, "history_valid = 0");
}

#[test]
fn the_history_lookup_follows_a_translation_reprojection() {
    for (along_y, shift, ndc) in [
        (false, 2, [4.0 / 9.0, 0.0]),
        (false, -1, [-2.0 / 9.0, 0.0]),
        (true, 2, [0.0, -4.0 / 9.0]),
    ] {
        let mut case = Case::new(ramp([9, 9], along_y, 0), ramp([9, 9], along_y, shift));
        case.reprojection = translation(ndc);
        let out = assert_matches_reference(&case, &format!("axis y {along_y} shift {shift}"));
        // Every interior pixel's history lines up with the current value, so it is kept.
        let centre = out[4 * 9 + 4][0];
        assert!((centre - 0.75).abs() < 1e-3, "{centre}");
    }
}

#[test]
fn history_far_outside_the_neighbourhood_box_is_clipped() {
    let history = Image::from_fn([7, 7], |_, _| gray(100.0));
    let mut case = Case::new(top_row([7, 7]), history);
    case.settings = TaaSettings::new(0.5);
    let out = assert_matches_reference(&case, "clip");
    assert!(out[3 * 7 + 3][0] < 5.0, "unclipped history would dominate");
}

#[test]
fn a_lookup_outside_the_image_ignores_history() {
    let mut case = Case::new(noisy([8, 5], 0), Image::from_fn([8, 5], |_, _| gray(4.0)));
    // Two pixels to the right: the last two columns fall outside the history image.
    case.reprojection = translation([4.0 / 8.0, 0.0]);
    let out = assert_matches_reference(&case, "out of image");
    let mut current = Case::new(noisy([8, 5], 0), Image::from_fn([8, 5], |_, _| gray(4.0)));
    current.valid = false;
    let reconstructed = current.expected();
    for y in 0..5 {
        for x in [6, 7] {
            let i = y * 8 + x;
            assert!((out[i][0] - reconstructed[i][0]).abs() < 2e-3, "{x},{y}");
        }
    }
}

#[test]
fn odd_extents_with_jitter_depth_and_motion_match_the_reference() {
    for size in [[1, 1], [2, 1], [1, 5], [3, 3], [17, 9], [31, 13]] {
        let mut case = Case::new(noisy(size, 0), noisy(size, 5));
        case.jitter = [-0.25, 0.375];
        case.depth = Depth::from_fn(size, |x, y| 0.1 + 0.8 * noise(x, y, 7) / 4.0);
        case.reprojection = translation([2.0 / f64::from(size[0]), -2.0 / f64::from(size[1])]);
        case.reprojection[2] = [0.25, 0.0, 1.0, 0.0];
        assert_matches_reference(&case, &format!("{size:?}"));
    }
}

#[test]
fn the_history_pass_copies_the_resolve_output_into_the_history() {
    let mut case = Case::new(noisy([13, 7], 0), noisy([13, 7], 3));
    case.settings = TaaSettings::new(0.5);
    let (output, history) = run(&case);
    assert_eq!(output, history);
    assert!(output.iter().any(|p| p[0] > 0.0));
}

/// Four sample planes whose minimum differs from every single plane at most pixels.
fn sample_planes(size: [i32; 2]) -> Vec<Depth> {
    (0..4)
        .map(|plane| Depth::from_fn(size, |x, y| 0.1 + 0.8 * noise(x, y, 20 + plane) / 4.0))
        .collect()
}

#[test]
fn a_multisampled_depth_uses_the_minimum_over_samples() {
    for size in [[1, 1], [3, 3], [17, 9], [31, 13]] {
        let mut case =
            Case::new(noisy(size, 0), noisy(size, 5)).with_depth_samples(sample_planes(size));
        case.jitter = [-0.25, 0.375];
        case.reprojection = translation([2.0 / f64::from(size[0]), -2.0 / f64::from(size[1])]);
        case.reprojection[2] = [0.25, 0.0, 1.0, 0.0];
        assert_matches_reference(&case, &format!("4x depth {size:?}"));
    }
}

#[test]
fn a_multisampled_depth_differs_from_its_first_sample() {
    let size = [17, 9];
    let planes = sample_planes(size);
    // Parallax small enough that the multisampled path still keeps some history weight.
    let parallax = [0.003, 0.0, 1.0, 0.0];
    let mut minimum = Case::new(noisy(size, 0), noisy(size, 5)).with_depth_samples(planes.clone());
    minimum.reprojection[2] = parallax;
    let mut first =
        Case::new(noisy(size, 0), noisy(size, 5))
            .with_depth_samples(vec![planes[0].clone(); planes.len()]);
    first.reprojection[2] = parallax;
    let (a, b) = (minimum.expected(), first.expected());
    assert!(a.iter().zip(&b).any(|(x, y)| (x[0] - y[0]).abs() > 1e-3));
}

/// History of one colour. Hardware filtering may compute bilinear weights with as few as
/// four sub-texel bits, so a varying history fetched at an arbitrary sub-pixel offset differs
/// from the exact f64 reference by more than the comparison bound on some GPUs. A uniform
/// history filters exactly at any offset, so this test checks the motion-dependent history
/// weight alone; the integer and half-pixel lookups are covered by the other tests.
fn uniform_history(size: [i32; 2]) -> Image {
    Image::from_fn(size, |_, _| [0.75, 1.5, 0.375, 1.0])
}

#[test]
fn sub_pixel_motion_lowers_the_history_weight_like_the_reference() {
    let size = [17, 9];
    for shift in [0.05, 0.3, 0.9] {
        let mut case = Case::new(noisy(size, 0), uniform_history(size));
        case.settings = TaaSettings::new(0.5);
        case.jitter = [0.25, -0.125];
        case.reprojection = translation([2.0 * shift / f64::from(size[0]), 0.0]);
        assert_matches_reference(&case, &format!("1x shift {shift}"));
        let planes = sample_planes(size);
        let mut msaa = Case::new(noisy(size, 0), uniform_history(size)).with_depth_samples(planes);
        msaa.settings = TaaSettings::new(0.5);
        msaa.reprojection = case.reprojection;
        assert_matches_reference(&msaa, &format!("4x shift {shift}"));
    }
}

#[test]
fn motion_changes_the_output_by_the_weight_scale_on_each_path() {
    let size = [17, 9];
    let moving = |depth_samples: Option<Vec<Depth>>, shift: f64| {
        let mut case = Case::new(noisy(size, 0), noisy(size, 5));
        if let Some(planes) = depth_samples {
            case = case.with_depth_samples(planes);
        }
        case.reprojection = translation([2.0 * shift / f64::from(size[0]), 0.0]);
        case.expected()
    };
    let (still, slow, fast) = (moving(None, 0.0), moving(None, 0.3), moving(None, 0.9));
    let msaa_still = moving(Some(sample_planes(size)), 0.0);
    let msaa_slow = moving(Some(sample_planes(size)), 0.3);
    let spread = |a: &[[f64; 4]], b: &[[f64; 4]]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x[0] - y[0]).abs())
            .sum::<f64>()
    };
    assert!(spread(&still, &slow) > 0.0 && spread(&still, &fast) > 0.0);
    // Above 1 / 16 pixel the multisampled path uses the current frame alone.
    let current = Case::new(noisy(size, 0), noisy(size, 5));
    let mut invalid = current;
    invalid.valid = false;
    let expected = invalid.with_depth_samples(sample_planes(size)).expected();
    assert!(spread(&msaa_slow, &expected) < 1e-9);
    assert!(spread(&msaa_still, &expected) > 1e-3);
}
