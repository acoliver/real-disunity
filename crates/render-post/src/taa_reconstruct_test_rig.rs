//! Rig of the reconstruction stages on lavapipe. A scene function in WGSL gives device depth and
//! backward motion in internal texels (+x right, +y down) for every texel: `scene(p)` returns
//! `(depth, motion x, motion y)`. A raster pass writes them into a depth attachment and a
//! velocity target, the two stages of `taa_reconstruct_gpu` run, and the dilated depth, dilated
//! motion and scatter buffer are read back.
use crate::taa_reconstruct_gpu::{self, Inputs};
use crate::{Operation, Resources, StageSet, TemporalCamera};
use wgpu::util::DeviceExt;

/// A static camera with a standard Z perspective of near 0.1 and far 10: the identity
/// reprojection and the depth terms `a = far / (far - near)`, `b = -near * far / (far - near)`.
pub(crate) fn static_camera() -> TemporalCamera {
    let (near, far) = (0.1_f64, 10.0_f64);
    let mut reprojection = [[0.0; 4]; 4];
    for (i, column) in reprojection.iter_mut().enumerate() {
        column[i] = 1.0;
    }
    TemporalCamera {
        reprojection,
        depth_terms: [far / (far - near), -near * far / (far - near)],
        valid: true,
    }
}

pub(crate) struct Frame {
    pub(crate) size: [usize; 2],
    pub(crate) depth: Vec<f32>,
    pub(crate) motion: Vec<[f32; 2]>,
    pub(crate) scatter: Vec<u32>,
    pub(crate) confidence: Vec<f32>,
}

impl Frame {
    pub(crate) fn at(&self, x: usize, y: usize) -> usize {
        y * self.size[0] + x
    }
}

fn scene_source(size: [u32; 2], body: &str) -> String {
    format!(
        "const SIZE = vec2<f32>({}.0, {}.0);
fn scene(p: vec2<i32>) -> vec3<f32> {{ {body} }}
struct Out {{ @location(0) motion: vec2<f32>, @builtin(frag_depth) depth: f32 }}
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}}
@fragment fn paint(@builtin(position) pos: vec4<f32>) -> Out {{
    let s = scene(vec2<i32>(floor(pos.xy)));
    return Out(vec2<f32>(s.y, s.z) / SIZE, s.x);
}}",
        size[0], size[1]
    )
}

fn scene_pipeline(device: &wgpu::Device, source: String) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("reconstruct scene"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("reconstruct scene"),
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
            targets: &[Some(crate::VELOCITY_FORMAT.into())],
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

fn texture(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reconstruct input"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn draw_scene(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    (motion, depth): (&wgpu::TextureView, &wgpu::TextureView),
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("reconstruct scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: motion,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    pass.draw(0..3, 0..1);
}

fn dispatch(encoder: &mut wgpu::CommandEncoder, operation: &Operation) {
    let Operation::Compute {
        pipeline,
        groups,
        dispatch,
    } = operation
    else {
        panic!("the reconstruction stages are compute passes");
    };
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &groups[0], &[]);
    pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
}

fn staging(device: &wgpu::Device, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("reconstruct readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// Row pitch of a texture copy: tight rows rounded up to the copy alignment.
fn pitch(width: u32, texel_bytes: u32) -> u32 {
    (width * texel_bytes).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
}

fn copy_texture(
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    buffer: &wgpu::Buffer,
    texel_bytes: u32,
) {
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch(texture.width(), texel_bytes)),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
}

fn mapped(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    receive.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Rows of `width` texels of `texel_words` words, with the copy padding of each row removed.
fn tight(bytes: &[u8], (width, height): (usize, usize), texel_words: usize) -> Vec<u32> {
    let row = pitch(
        u32::try_from(width).unwrap(),
        u32::try_from(texel_words * 4).unwrap(),
    );
    let all = words(bytes);
    let stride = usize::try_from(row).unwrap() / 4;
    (0..height)
        .flat_map(|y| all[y * stride..y * stride + width * texel_words].to_vec())
        .collect()
}

fn camera_block(device: &wgpu::Device, camera: TemporalCamera) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("reconstruct camera"),
        contents: &camera.to_bytes(),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

fn floats(words: Vec<u32>) -> Vec<f32> {
    words.into_iter().map(f32::from_bits).collect()
}

/// Runs the stages on the scene `body` of extent `size` with `camera`.
pub(crate) fn run(
    gpu: &(wgpu::Device, wgpu::Queue),
    size: [u32; 2],
    body: &str,
    camera: TemporalCamera,
) -> Frame {
    let (device, queue) = gpu;
    let depth = texture(device, size, wgpu::TextureFormat::Depth32Float);
    let motion = texture(device, size, crate::VELOCITY_FORMAT);
    let (depth_view, motion_view) = (
        depth.create_view(&wgpu::TextureViewDescriptor::default()),
        motion.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    let block = camera_block(device, camera);
    let mut set = StageSet {
        stages: Vec::new(),
        resources: Resources::default(),
    };
    let outputs = taa_reconstruct_gpu::append(
        device,
        &mut set,
        &Inputs {
            depth: &depth_view,
            motion: &motion_view,
            camera: &block,
            size,
        },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    draw_scene(
        &mut encoder,
        &scene_pipeline(device, scene_source(size, body)),
        (&motion_view, &depth_view),
    );
    for stage in &set.stages {
        dispatch(&mut encoder, &stage.operation);
    }
    let cells = u64::from(size[0]) * u64::from(size[1]);
    let rows = u64::from(size[1]);
    let buffers = [
        staging(device, u64::from(pitch(size[0], 4)) * rows),
        staging(device, u64::from(pitch(size[0], 8)) * rows),
        staging(device, cells * 4),
        staging(device, u64::from(pitch(size[0], 4)) * rows),
    ];
    copy_texture(&mut encoder, &outputs.depth, &buffers[0], 4);
    copy_texture(&mut encoder, &outputs.motion, &buffers[1], 8);
    encoder.copy_buffer_to_buffer(&outputs.scatter, 0, &buffers[2], 0, cells * 4);
    copy_texture(&mut encoder, &outputs.confidence, &buffers[3], 4);
    queue.submit([encoder.finish()]);
    let extent = (
        usize::try_from(size[0]).unwrap(),
        usize::try_from(size[1]).unwrap(),
    );
    let depth = floats(tight(&mapped(device, &buffers[0]), extent, 1));
    let flow = floats(tight(&mapped(device, &buffers[1]), extent, 2));
    Frame {
        size: [extent.0, extent.1],
        depth,
        motion: flow.chunks_exact(2).map(|p| [p[0], p[1]]).collect(),
        scatter: words(&mapped(device, &buffers[2])),
        confidence: floats(tight(&mapped(device, &buffers[3]), extent, 1)),
    }
}
