//! Deferred lighting of the G-buffer observation.
//!
//! One full-screen pass in [`PassSlot::OpaqueLighting`], after Opaque has completed the scene
//! depth, lights every pixel a producer wrote with the frame's own forward lighting: the
//! program is the frame's forward object source with [`LIGHTING_WGSL`] appended, so every
//! shadow, ambient-occlusion and reflection variant and every specialisation constant is the
//! one the forward draws use. It writes a diagnostic HDR colour target, and with reflections
//! the environment contribution and replacement factor the forward Opaque pass writes for SSR.
//! In observation nothing reads these except captures, so the visible frame stays forward.
use std::cell::RefCell;
use std::rc::Rc;

use wgpu::util::DeviceExt;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeaturePass, FrameError,
    PassDeclaration, PassSlot, ResourceRef,
};

use super::records::{LIGHTING_RECORD_BYTES, LightingRecordTable};
use super::{Gbuffer, GbufferTarget};

/// The lighting module appended to the forward object source with the G-buffer module.
pub const LIGHTING_WGSL: &str = include_str!("../shader/gbuffer_lighting.wgsl");

const VERTEX_ENTRY: &str = "vs_scene_deferred_lighting";
const FRAGMENT_ENTRY: &str = "fs_scene_deferred_lighting";

/// Bytes of the camera block at (1, 22): `scene_from_clip` and the padded `scene_from_view`.
const CAMERA_BYTES: usize = 112;

/// One output of the lighting pass, in colour-location order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightingTarget {
    /// Location 0: exposed HDR radiance after fog, alpha 1 where a producer wrote the pixel.
    Colour,
    /// Location 1, with reflections: the replaceable environment contribution SSR reads.
    Environment,
    /// Location 2, with reflections: the environment replacement factor SSR reads.
    Factor,
}

impl LightingTarget {
    /// The resource name the lighting pass declares the target under.
    #[must_use]
    pub const fn resource_name(self) -> &'static str {
        match self {
            Self::Colour => "deferred lighting colour",
            Self::Environment => "deferred lighting environment",
            Self::Factor => "deferred lighting factor",
        }
    }
}

/// The lighting pass's targets, at the scene extent in the scene colour format.
pub(crate) struct LightingTargets {
    textures: Vec<(LightingTarget, wgpu::Texture)>,
}

impl LightingTargets {
    /// The colour target, and with `reflections` the two reflection products.
    pub(crate) fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        reflections: bool,
    ) -> Self {
        let outputs: &[LightingTarget] = if reflections {
            &[
                LightingTarget::Colour,
                LightingTarget::Environment,
                LightingTarget::Factor,
            ]
        } else {
            &[LightingTarget::Colour]
        };
        Self {
            textures: outputs
                .iter()
                .map(|&target| {
                    let texture = worldrunner_render_post::color_texture(
                        device,
                        target.resource_name(),
                        size,
                        format,
                    );
                    (target, texture)
                })
                .collect(),
        }
    }

    pub(crate) fn texture(&self, target: LightingTarget) -> Option<&wgpu::Texture> {
        self.textures
            .iter()
            .find(|(candidate, _)| *candidate == target)
            .map(|(_, texture)| texture)
    }

    pub(crate) fn textures(&self) -> impl Iterator<Item = &wgpu::Texture> {
        self.textures.iter().map(|(_, texture)| texture)
    }

    pub(crate) fn format(&self) -> wgpu::TextureFormat {
        self.textures[0].1.format()
    }

    pub(crate) fn reflections(&self) -> bool {
        self.textures.len() == 3
    }

    /// Adds the lighting pass, which clears its targets to zero.
    pub(crate) fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        let mut pass = PassDeclaration::new(PassSlot::OpaqueLighting);
        for (target, texture) in &self.textures {
            let view = AttachmentView::new(ResourceRef::new(target.resource_name()), texture)?;
            let attachment = AttachmentDeclaration {
                resource: view.resource(),
                format: view.format(),
                size: view.size(),
                samples: 1,
                load: AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
                read_only: false,
            };
            if pass.color.is_none() {
                pass.color = Some(attachment);
            } else {
                pass.additional_colors.push(attachment);
            }
            views.push(view);
        }
        passes.push(pass);
        Ok(())
    }
}

/// The lighting program over `object`, a frame's forward object source.
///
/// The forward reflection normal reads the material's fold setting to decide whether to
/// normalise; the stored shading normal is already unit length, so both of its branches give
/// that normal and the lighting pass takes it directly. With the receiver payload the far
/// side's visibility and the payload pixel come from the receiver consumer, as forward.
pub(crate) fn source(object: &str) -> String {
    const MATERIAL_NORMAL: &str = "fn scene_environment_normal(normal: vec3<f32>) -> vec3<f32> { if material.contact_surface.z > 0.0 { return reflection_normal(normal); } return normal; }";
    let object = object.replace(
        MATERIAL_NORMAL,
        "fn scene_environment_normal(normal: vec3<f32>) -> vec3<f32> { return normal; }",
    );
    let receiver = object.contains("var<private> scene_receiver_pixel");
    let far = if object.contains("fn scene_receiver_back_visibility(") {
        "scene_receiver_back_visibility(position, normal)"
    } else {
        "sunlight_visibility(position, normal)"
    };
    let lighting = LIGHTING_WGSL.replace(
        "// SCENE_DEFERRED_PIXEL",
        if receiver {
            "scene_receiver_pixel = vec2<i32>(pixel.xy);"
        } else {
            ""
        },
    );
    let entry = if object.contains("fn reflection_output(") {
        "struct SceneDeferredReflection {
    @location(0) colour: vec4<f32>,
    @location(1) environment: vec4<f32>,
    @location(2) factor: vec4<f32>,
}
@fragment
fn fs_scene_deferred_lighting(@builtin(position) pixel: vec4<f32>) -> SceneDeferredReflection {
    let colour = scene_deferred_shade(pixel);
    let products = reflection_output(colour);
    let covered = colour.a != 0.0;
    return SceneDeferredReflection(colour, select(vec4<f32>(0.0), products.environment, covered),
        select(vec4<f32>(0.0), products.factor, covered));
}"
    } else {
        "@fragment
fn fs_scene_deferred_lighting(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    return scene_deferred_shade(pixel);
}"
    };
    format!(
        "{object}\n{}\n{lighting}\nfn scene_deferred_far_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {{ return {far}; }}\n{entry}\n",
        super::GBUFFER_WGSL
    )
}

/// What a lighting program is built from: the forward sibling's source, override constants
/// and engine sets 0, 2 and 3, and the scene colour format.
pub(crate) struct ProgramParts<'a> {
    pub source: &'a str,
    pub constants: &'a [(&'static str, f64)],
    pub format: wgpu::TextureFormat,
    pub frame: &'a wgpu::BindGroupLayout,
    pub shadow: Option<&'a wgpu::BindGroupLayout>,
    pub screen: Option<&'a wgpu::BindGroupLayout>,
}

/// The full-screen lighting pipeline of one forward sibling and the layout of its set 1.
#[derive(Debug)]
pub(crate) struct LightingProgram {
    pipeline: wgpu::RenderPipeline,
    inputs: wgpu::BindGroupLayout,
    reflections: bool,
}

impl LightingProgram {
    pub(crate) fn new(device: &wgpu::Device, parts: &ProgramParts<'_>) -> Self {
        let source = source(parts.source);
        let reflections = parts.source.contains("fn reflection_output(");
        let inputs = inputs_layout(device);
        let mut sets = vec![Some(parts.frame), Some(&inputs)];
        if parts.shadow.is_some() || parts.screen.is_some() {
            sets.push(parts.shadow);
        }
        if parts.screen.is_some() {
            sets.push(parts.screen);
        }
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("deferred lighting"),
            bind_group_layouts: &sets,
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deferred lighting"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let target = Some(wgpu::ColorTargetState {
            format: parts.format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
        let targets = vec![target; if reflections { 3 } else { 1 }];
        let options = wgpu::PipelineCompilationOptions {
            constants: parts.constants,
            ..Default::default()
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("deferred lighting"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(VERTEX_ENTRY),
                compilation_options: options.clone(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(FRAGMENT_ENTRY),
                compilation_options: options,
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            inputs,
            reflections,
        }
    }

    /// Whether the program writes the two reflection products after the colour.
    pub(crate) const fn reflections(&self) -> bool {
        self.reflections
    }

    /// Records the pass. The caller has bound sets 0, 2 and 3 as for the forward draws.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, inputs: &LightingInputs) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &inputs.group, &[]);
        pass.draw(0..3, 0..1);
    }

    pub(crate) const fn inputs_layout(&self) -> &wgpu::BindGroupLayout {
        &self.inputs
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn buffer_entry(
    binding: u32,
    ty: wgpu::BufferBindingType,
    size: usize,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size as u64),
        },
        count: None,
    }
}

/// Set 1 of the lighting pass: the four targets, the scene depth, the record table and the
/// camera block, at bindings 16 to 22 so that they never alias the material's set 1.
fn inputs_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let float = wgpu::TextureSampleType::Float { filterable: false };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("deferred lighting inputs"),
        entries: &[
            texture_entry(16, float),
            texture_entry(17, float),
            texture_entry(18, float),
            texture_entry(19, wgpu::TextureSampleType::Uint),
            texture_entry(20, wgpu::TextureSampleType::Depth),
            buffer_entry(
                21,
                wgpu::BufferBindingType::Storage { read_only: true },
                LIGHTING_RECORD_BYTES,
            ),
            buffer_entry(22, wgpu::BufferBindingType::Uniform, CAMERA_BYTES),
        ],
    })
}

/// The camera block for a frame's `view_projection` and right-handed `view`: the inverse of
/// the projection the producers rasterised with, and the inverse of the view rotation they
/// encoded normals with.
pub(crate) fn camera_bytes(view_projection: glam::Mat4, view: glam::Mat4) -> [u8; CAMERA_BYTES] {
    let scene_from_clip = view_projection.inverse();
    let scene_from_view = glam::Mat3::from_mat4(view).transpose();
    let mut values = scene_from_clip.to_cols_array().to_vec();
    for column in [
        scene_from_view.x_axis,
        scene_from_view.y_axis,
        scene_from_view.z_axis,
    ] {
        values.extend(column.extend(0.0).to_array());
    }
    let mut bytes = [0; CAMERA_BYTES];
    for (chunk, value) in bytes.chunks_exact_mut(4).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// One frame's set 1 of the lighting pass.
#[derive(Debug)]
pub(crate) struct LightingInputs {
    group: wgpu::BindGroup,
}

impl LightingInputs {
    pub(crate) fn new(
        device: &wgpu::Device,
        program: &LightingProgram,
        (gbuffer, depth): (&Gbuffer, &wgpu::Texture),
        records: &wgpu::Buffer,
        frame: &crate::frame::FrameUniform,
    ) -> Self {
        let views = GbufferTarget::ALL.map(|target| {
            gbuffer
                .texture(target)
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
        let depth = depth.create_view(&wgpu::TextureViewDescriptor {
            aspect: wgpu::TextureAspect::DepthOnly,
            ..Default::default()
        });
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred lighting camera"),
            contents: &camera_bytes(frame.view_projection, frame.view),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries: Vec<_> = views
            .iter()
            .chain(std::iter::once(&depth))
            .zip(16..)
            .map(|(view, binding)| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 21,
            resource: records.as_entire_binding(),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 22,
            resource: camera.as_entire_binding(),
        });
        Self {
            group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("deferred lighting inputs"),
                layout: program.inputs_layout(),
                entries: &entries,
            }),
        }
    }
}

/// The feature pass a stock scene feature declares to draw the lighting pass: it reads the
/// four targets and the completed scene `depth`, and writes the program's outputs.
pub(crate) fn feature_pass(
    depth: ResourceRef,
    format: wgpu::TextureFormat,
    reflections: bool,
) -> FeaturePass {
    let mut pass = FeaturePass::new(PassSlot::OpaqueLighting);
    pass.reads = GbufferTarget::ALL
        .map(|target| ResourceRef::new(target.resource_name()))
        .to_vec();
    pass.reads.push(depth);
    pass.color_format = Some(format);
    if reflections {
        pass.additional_color_formats = vec![format; 2];
    }
    pass
}

/// The lighting records of one resident scene and the table buffer the lighting pass binds,
/// rebuilt when an append adds records.
#[derive(Debug)]
pub(crate) struct SceneRecords {
    table: LightingRecordTable,
    buffer: RefCell<Option<Rc<wgpu::Buffer>>>,
}

impl SceneRecords {
    pub(crate) fn new(table: LightingRecordTable) -> Self {
        Self {
            table,
            buffer: RefCell::new(None),
        }
    }

    pub(crate) const fn table(&self) -> &LightingRecordTable {
        &self.table
    }

    pub(crate) fn replace(&mut self, table: LightingRecordTable) {
        if table != self.table {
            self.table = table;
            self.buffer = RefCell::new(None);
        }
    }

    /// The table buffer, after admitting the table for the eight-bit G1.a index.
    pub(crate) fn buffer(&self, device: &wgpu::Device) -> Result<Rc<wgpu::Buffer>, FrameError> {
        self.table
            .admit()
            .map_err(|error| FrameError::FeaturePreparation {
                feature: "opaque G-buffer",
                reason: error.to_string(),
            })?;
        Ok(self
            .buffer
            .borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("lighting records"),
                        contents: &self.table.to_bytes(),
                        usage: wgpu::BufferUsages::STORAGE,
                    }),
                )
            })
            .clone())
    }
}

#[cfg(test)]
#[path = "lighting_tests.rs"]
pub(crate) mod tests;
