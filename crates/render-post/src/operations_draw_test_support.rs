//! Fixtures for the tests of draw operations: a 4 by 4 target, quads in clip space and a program
//! that fills a quad with a uniform color.
use super::operations_tests::{device, id};
use crate::{AttachmentLoad, contracts::*};
use wgpu::util::DeviceExt;

const SHADER: &str = "
struct Fill { color: vec4<f32> }
@group(0) @binding(0) var<uniform> fill: Fill;
@vertex fn vs(@location(0) p: vec2<f32>) -> @builtin(position) vec4<f32> { return vec4<f32>(p, 0.0, 1.0); }
@fragment fn fs() -> @location(0) vec4<f32> { return fill.color; }
@fragment fn fs_plain() -> @location(0) vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }";

pub(super) const RED: [u8; 4] = [255, 0, 0, 255];
pub(super) const BLUE: [u8; 4] = [0, 0, 255, 255];
pub(super) const GREEN: [u8; 4] = [0, 255, 0, 255];
pub(super) const BLACK: [u8; 4] = [0, 0, 0, 255];

pub(super) struct Rig {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub registry: ResourceRegistry,
    pub module: wgpu::ShaderModule,
    pub target: wgpu::Texture,
    pub target_view: CheckedView,
}

impl Rig {
    pub fn new() -> Self {
        let (device, queue) = device();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("draw fixture target"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut registry = ResourceRegistry::default();
        let key = registry
            .import(
                &target,
                ResourceLifetime::FrameLocal,
                Semantic::Unspecified,
                None,
            )
            .unwrap();
        let target_view = registry
            .view(key, TextureRange::single(0, 0, wgpu::TextureAspect::All), 1)
            .unwrap();
        Self {
            device,
            queue,
            registry,
            module,
            target,
            target_view,
        }
    }

    /// A buffer of vertices, imported as written before the frame.
    pub fn vertices(&mut self, points: &[[f32; 2]]) -> BufferView {
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("draw fixture vertices"),
                contents: &points
                    .iter()
                    .flatten()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.import(&buffer)
    }

    pub fn import(&mut self, buffer: &wgpu::Buffer) -> BufferView {
        let key = self
            .registry
            .import_buffer(buffer, ResourceLifetime::ImportedReadOnly, Some(0))
            .unwrap();
        self.registry.buffer_view(key, 0..buffer.size(), 0).unwrap()
    }

    /// A uniform group at binding 0 for the fragment stage, filled with `color`.
    pub fn fill(&mut self, color: [f32; 4], visibility: wgpu::ShaderStages) -> Group {
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("draw fixture fill"),
                contents: &color
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let view = self.import(&buffer);
        let bindings = CheckedBindings::build_with_buffers(
            &self.device,
            &self.registry,
            &[],
            &[BufferBinding {
                binding: 0,
                visibility,
                view: view.clone(),
                kind: BufferBindingKind::Uniform,
                dynamic_offset: None,
            }],
        )
        .unwrap();
        Group { bindings, view }
    }

    /// A draw of `program` with the vertex layout of one two-float stream.
    pub fn draw(
        &self,
        entry: &str,
        bindings: Vec<CheckedBindings>,
        geometry: DrawGeometry,
    ) -> Result<CheckedDraw, ContractError> {
        CheckedDraw::with_geometry(
            &self.device,
            &self.pipeline(entry, None),
            bindings,
            geometry,
        )
    }

    /// The descriptor of a pipeline that reads a stream of two-float positions.
    pub fn pipeline<'a>(
        &'a self,
        fragment: &'a str,
        layout: Option<&'a wgpu::PipelineLayout>,
    ) -> wgpu::RenderPipelineDescriptor<'a> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
        wgpu::RenderPipelineDescriptor {
            label: None,
            layout,
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some(fragment),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        }
    }

    /// The target cleared to black, with `uses` and `bindings` declared.
    pub fn pass(&self, name: &'static str) -> RasterPass {
        RasterPass {
            id: id(name),
            colors: vec![Some(ColorAttachment {
                view: self.target_view.clone(),
                before: None,
                load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
                resolve: None,
            })],
            depth: None,
            uses: Vec::new(),
            bindings: Vec::new(),
        }
    }

    /// Runs the operations in one submission and reads the target.
    pub fn run(&self, operations: &[TypedOperation]) -> Vec<u8> {
        let compiled = self
            .registry
            .compile_operations(&self.device, operations)
            .unwrap();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        compiled
            .encode(&self.registry, &mut encoder, None, |_, _| {})
            .unwrap();
        self.queue.submit([encoder.finish()]);
        crate::Readback::new(
            &self.device,
            crate::FrameConfig::new([4, 4], wgpu::TextureFormat::Rgba8Unorm, 1),
            4096,
        )
        .unwrap()
        .read(
            &self.device,
            &crate::test_authority(&self.queue),
            &self.target,
        )
        .unwrap()
    }
}

/// A uniform group with the view of its buffer, for the operation to declare.
pub(super) struct Group {
    pub bindings: CheckedBindings,
    pub view: BufferView,
}

/// A quad of two triangles over `x` columns of clip space, the full height.
pub(super) fn quad(x: [f32; 2]) -> [[f32; 2]; 6] {
    let [x0, x1] = x;
    [
        [x0, -1.0],
        [x1, -1.0],
        [x1, 1.0],
        [x0, -1.0],
        [x1, 1.0],
        [x0, 1.0],
    ]
}

pub(super) fn geometry(stream: BufferView) -> DrawGeometry {
    DrawGeometry {
        vertex_buffers: vec![stream],
        index: None,
        elements: 0..6,
        base_vertex: 0,
        instances: 0..1,
    }
}

/// The pixel at `column` and `row` of the 4 by 4 readback.
pub(super) fn pixel(image: &[u8], column: usize, row: usize) -> [u8; 4] {
    let at = (row * 4 + column) * 4;
    image[at..at + 4].try_into().unwrap()
}

/// The buffer uses a draw of these groups and streams must declare.
pub(super) fn reads(groups: &[&BufferView], streams: &[&BufferView]) -> Vec<BufferUse> {
    groups
        .iter()
        .map(|view| BufferUse::read((*view).clone(), BufferUseKind::Uniform))
        .chain(
            streams
                .iter()
                .map(|view| BufferUse::read((*view).clone(), BufferUseKind::Vertex)),
        )
        .collect()
}
