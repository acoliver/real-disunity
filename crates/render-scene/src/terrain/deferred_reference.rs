//! The forward reference of the deferred lighting test: `scene_object_material` (the decals)
//! then `scene_finish_shading`, the forward mesh shading after decals, evaluated per block with
//! the block material's own uniform at the pixel's scene position, from the same forward object
//! source, constants and engine sets the frame's colour draws use.
use wgpu::util::DeviceExt;
use worldrunner_render_plan::SceneMaterial;

use super::{BLOCK, Block, COLUMNS, Drawn, Frame, HDR, block_values, halves, read};
use crate::device::GpuContext;
use crate::dynamic::capture_tests::SIZE;
use crate::pipeline::MaterialUniform;

const REFERENCE: &str = "
@group(1) @binding(23) var<storage, read> reference_blocks: array<vec4<f32>>;
@vertex
fn vs_reference(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment
fn fs_reference(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(pixel.xy);
    let block = u32(texel.y) / BLOCK_SIZEu * BLOCK_COLUMNSu + u32(texel.x) / BLOCK_SIZEu;
    let a = reference_blocks[block * 4u];
    let b = reference_blocks[block * 4u + 1u];
    let c = reference_blocks[block * 4u + 2u];
    let d = reference_blocks[block * 4u + 3u];
    var input: VertexOutput;
    input.world_position = scene_deferred_position(pixel.xy,
        textureLoad(scene_deferred_depth, texel, 0));
    let scattering = (u32(d.y) & 1u) != 0u;
    let surface = SceneShadingSurface(vec4<f32>(c.xyz, 1.0), a.xyz,
        vec3<f32>(a.w, select(b.w, 0.0, scattering), d.x));
    let material = scene_object_material(input, b.xyz, surface, select(0.0, b.w, scattering), c.w);
    let shaded = scene_finish_shading(input, material);
    return vec4<f32>(shaded.rgb, 1.0);
}
";

pub(super) struct ReferencePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    bytes: Vec<[u8; crate::pipeline::MATERIAL_UNIFORM_BYTES]>,
    uniforms: Vec<wgpu::Buffer>,
    camera: wgpu::Buffer,
    screen: Option<wgpu::BindGroup>,
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn buffer(ty: wgpu::BufferBindingType) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

impl ReferencePass {
    pub(super) fn new(
        context: &GpuContext,
        drawn: &mut Drawn,
        frame: Frame,
        materials: &[SceneMaterial],
    ) -> Self {
        let device = context.device();
        let forward = drawn
            .renderer
            .renderer_mut()
            .ssao
            .as_ref()
            .and_then(|screen| screen.geometry.forward.clone());
        assert_eq!(forward.is_some(), matches!(frame, Frame::Reflecting));
        let object = forward
            .as_ref()
            .map_or(&drawn.pipelines, |(pipelines, _)| pipelines)
            .object_source();
        let source = format!(
            "{}\n{}",
            crate::gbuffer::lighting::source(object),
            REFERENCE
                .replace("BLOCK_SIZE", &BLOCK.to_string())
                .replace("BLOCK_COLUMNS", &COLUMNS.to_string())
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("deferred reference"),
            entries: &[
                entry(0, buffer(wgpu::BufferBindingType::Uniform)),
                entry(
                    20,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(22, buffer(wgpu::BufferBindingType::Uniform)),
                entry(
                    23,
                    buffer(wgpu::BufferBindingType::Storage { read_only: true }),
                ),
            ],
        });
        let screen_layout = forward
            .as_ref()
            .map(|_| crate::ssr::forward::layout(device));
        let pipeline = pipeline(device, (&source, &layout), drawn, screen_layout.as_ref());
        let bytes: Vec<_> = materials
            .iter()
            .map(|material| MaterialUniform::from_material(material).to_bytes())
            .collect();
        let uniforms = bytes
            .iter()
            .map(|contents| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("deferred reference material"),
                    contents,
                    usage: wgpu::BufferUsages::UNIFORM,
                })
            })
            .collect();
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred reference camera"),
            contents: &crate::gbuffer::lighting::camera_bytes(
                drawn.frame.view_projection,
                drawn.frame.view,
            ),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        Self {
            pipeline,
            layout,
            bytes,
            uniforms,
            camera,
            screen: forward.map(|(_, group)| group),
        }
    }

    /// The forward shading of `blocks`, each lit with its material's uniform; an emissive
    /// block's material has its self-illumination flag set, as a material with a mask does.
    pub(super) fn shade(&self, context: &GpuContext, drawn: &Drawn, blocks: &[Block]) -> Vec<f32> {
        let device = context.device();
        let values = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred reference blocks"),
            contents: &block_values(blocks.iter().map(|block| block.surface)),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let depth =
            drawn
                .renderer
                .targets()
                .depth()
                .texture()
                .create_view(&wgpu::TextureViewDescriptor {
                    aspect: wgpu::TextureAspect::DepthOnly,
                    ..Default::default()
                });
        let emissive_uniforms: Vec<_> = blocks
            .iter()
            .map(|block| {
                block.emissive.then(|| {
                    let mut bytes = self.bytes[block.material];
                    let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap())
                        | crate::shader::FLAG_SELF_ILLUMINATION;
                    bytes[12..16].copy_from_slice(&flags.to_le_bytes());
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("deferred reference emissive material"),
                        contents: &bytes,
                        usage: wgpu::BufferUsages::UNIFORM,
                    })
                })
            })
            .collect();
        let groups: Vec<_> = blocks
            .iter()
            .zip(&emissive_uniforms)
            .map(|(block, emissive)| {
                let uniform = emissive.as_ref().unwrap_or(&self.uniforms[block.material]);
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("deferred reference"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 20,
                            resource: wgpu::BindingResource::TextureView(&depth),
                        },
                        wgpu::BindGroupEntry {
                            binding: 22,
                            resource: self.camera.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 23,
                            resource: values.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect();
        let target =
            worldrunner_render_post::color_texture(device, "deferred reference", [SIZE; 2], HDR);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let frame_group = drawn.gpu.drawing_frame_group_for_test();
        let mut encoder = context.submission().begin(device).unwrap();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("deferred reference"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &frame_group, &[]);
            if let Some(screen) = &self.screen {
                pass.set_bind_group(3, screen, &[]);
            }
            for (index, group) in (0..).zip(&groups) {
                pass.set_scissor_rect(
                    index % COLUMNS * BLOCK,
                    index / COLUMNS * BLOCK,
                    BLOCK,
                    BLOCK,
                );
                pass.set_bind_group(1, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        encoder.finish().submit().unwrap();
        halves(&read(context, &target, HDR))
    }
}

/// The reference pipeline: sets 0 and 1, and with reflections the forward screen-space set 3.
fn pipeline(
    device: &wgpu::Device,
    (source, layout): (&str, &wgpu::BindGroupLayout),
    drawn: &Drawn,
    screen_layout: Option<&wgpu::BindGroupLayout>,
) -> wgpu::RenderPipeline {
    let sets = [
        Some(drawn.pipelines.frame_layout()),
        Some(layout),
        None,
        screen_layout,
    ];
    let sets = if screen_layout.is_some() {
        &sets[..]
    } else {
        &sets[..2]
    };
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("deferred reference"),
        bind_group_layouts: sets,
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("deferred reference"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let mut constants =
        crate::renderer::specular_constants(drawn.frame.specular_convention.shader_options())
            .to_vec();
    constants.push(("SCENE_IMPOSTOR", 0.0));
    let options = wgpu::PipelineCompilationOptions {
        constants: &constants,
        ..Default::default()
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("deferred reference"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_reference"),
            compilation_options: options.clone(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_reference"),
            compilation_options: options,
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
