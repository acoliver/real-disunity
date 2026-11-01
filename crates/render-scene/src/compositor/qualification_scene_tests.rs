//! The held-out composite scene of the temporal upscaler qualification matrix, at any output
//! extent. It is public synthetic geometry rasterised by one pipeline: a checkered far wall, a
//! 2.5 cm vertical post and a 2.5 cm horizontal bar (thin features), a slanted bar, a striped
//! floor at a grazing angle and a bright card in front of the wall. Nothing here is tuned
//! against; the rig scenes of the render-post crate are the tuning set.
use crate::{
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use glam::Mat4;
use wgpu::util::DeviceExt;
use worldrunner_render_plan::SceneCamera;

pub(super) const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const VERTICES: u32 = 36;
/// Millimetres of camera travel per output pixel at the wall's distance is derived from this
/// distance and the vertical field of view of `frame`.
const WALL_DISTANCE_MM: f64 = 20_000.0;
const VERTICAL_FOV_MILLIDEGREES: u32 = 60_000;

/// World space is x right, y forward, z up. The camera is 10 m in front of the origin plane;
/// the wall is 20 m from it. Quads are six vertices each: wall, post, bar, slanted bar, floor, card.
const SHADER: &str = "
struct Camera { vp: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) kind: u32,
}

@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    var w = array<vec3<f32>, 36>(
        vec3(-30.0, 10.0, -20.0), vec3(30.0, 10.0, -20.0), vec3(30.0, 10.0, 25.0),
        vec3(-30.0, 10.0, -20.0), vec3(30.0, 10.0, 25.0), vec3(-30.0, 10.0, 25.0),
        vec3(-9.0125, 9.95, -5.0), vec3(-8.9875, 9.95, -5.0), vec3(-8.9875, 9.95, 6.0),
        vec3(-9.0125, 9.95, -5.0), vec3(-8.9875, 9.95, 6.0), vec3(-9.0125, 9.95, 6.0),
        vec3(-14.0, 9.95, 3.9875), vec3(-4.0, 9.95, 3.9875), vec3(-4.0, 9.95, 4.0125),
        vec3(-14.0, 9.95, 3.9875), vec3(-4.0, 9.95, 4.0125), vec3(-14.0, 9.95, 4.0125),
        vec3(4.0, 9.95, -3.0), vec3(4.0225, 9.95, -3.0), vec3(12.0225, 9.95, 5.0),
        vec3(4.0, 9.95, -3.0), vec3(12.0225, 9.95, 5.0), vec3(12.0, 9.95, 5.0),
        vec3(-30.0, -10.0, 0.0), vec3(30.0, -10.0, 0.0), vec3(30.0, 10.0, 0.0),
        vec3(-30.0, -10.0, 0.0), vec3(30.0, 10.0, 0.0), vec3(-30.0, 10.0, 0.0),
        vec3(-1.2, -2.0, 0.7), vec3(1.2, -2.0, 0.7), vec3(1.2, -2.0, 2.7),
        vec3(-1.2, -2.0, 0.7), vec3(1.2, -2.0, 2.7), vec3(-1.2, -2.0, 2.7));
    var out: Out;
    out.world = w[i];
    out.position = camera.vp * vec4(w[i], 1.0);
    out.kind = i / 6u;
    return out;
}

@fragment fn fs(in: Out) -> @location(0) vec4<f32> {
    let cell = vec2<i32>(floor(in.world.xz / 0.05));
    let checker = f32((cell.x + cell.y) & 1);
    let stripe = f32(i32(floor(in.world.x / 0.1)) & 1);
    var colours = array<vec3<f32>, 6>(
        vec3(0.05, 0.08, 0.25) + vec3(0.12) * checker, vec3(1.8, 1.8, 1.8),
        vec3(1.6, 1.4, 0.2), vec3(1.2, 0.2, 0.2),
        vec3(0.06, 0.14, 0.05) + vec3(0.35) * stripe, vec3(0.9, 0.6, 0.1));
    return vec4(colours[in.kind], 1.0);
}
";

/// A camera at `eye_x` millimetres looking along +y; the target moves with the eye so the view
/// only translates. The viewport is the extent the frame is rasterised at.
pub(super) fn frame(eye_x: i64, extent: [u32; 2]) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -10_000, 1_700],
            target_millimeters: [eye_x, 0, 1_700],
        },
        SceneCamera {
            vertical_fov_millidegrees: VERTICAL_FOV_MILLIDEGREES,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: extent[0],
            height: extent[1],
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

/// Camera travel in millimetres for `pixels` output pixels at the wall, `frames` frames in.
pub(super) fn eye_travel_mm(pixels_per_frame: f64, frames: usize, output_height: u32) -> i64 {
    let half_fov = f64::from(VERTICAL_FOV_MILLIDEGREES) / 2000.0;
    let wall_pixel_mm =
        2.0 * WALL_DISTANCE_MM * half_fov.to_radians().tan() / f64::from(output_height);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a few thousand millimetres"
    )]
    #[expect(clippy::cast_precision_loss, reason = "frame counts are tiny")]
    let travel = (pixels_per_frame * frames as f64 * wall_pixel_mm).round() as i64;
    travel
}

pub(super) struct CompositeScene {
    pipeline: wgpu::RenderPipeline,
}

impl CompositeScene {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("qualification composite scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("qualification composite scene"),
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
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline }
    }

    /// Rasterises the scene into `color` and `depth` with the given view-projection.
    pub(super) fn draw(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        attachments: (&wgpu::TextureView, &wgpu::TextureView),
        view_projection: Mat4,
    ) {
        let contents: Vec<u8> = view_projection
            .to_cols_array()
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("qualification camera"),
            contents: &contents,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("qualification camera"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("qualification composite scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: attachments.0,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: attachments.1,
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
        pass.draw(0..VERTICES, 0..1);
    }
}

/// A render-attachment texture of the given extent that can also be read back.
pub(super) fn target(
    device: &wgpu::Device,
    extent: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("qualification texture"),
        size: wgpu::Extent3d {
            width: extent[0],
            height: extent[1],
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

pub(super) fn depth_target(device: &wgpu::Device, extent: [u32; 2]) -> wgpu::Texture {
    target(device, extent, DEPTH)
}
