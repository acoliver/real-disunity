//! The dynamic unlit-alpha material on the program runtime.
//!
//! Dynamic quads, native sprites and tilemaps, and UI glyphs routed into the world draw with
//! the scene shader's `vs_unlit` and `fs_unlit`. The shader source is the scene's own and is not
//! changed. The runtime builds the pipeline and the bind groups, and the quads' buffers, their
//! texture and sampler and the scene's frame uniform are lent to it rather than copied. The
//! runtime counts none of them against a budget: the scene owns them and has no allocation
//! accounting for dynamic geometry, so this adds none.
//!
//! Preparation happens before the pass, where it can fail and allocate: staging a contribution
//! prepares a draw for each frame slot for the pass the staging pipelines describe, and a frame
//! that draws into another pass prepares its own before it opens the pass. Recording is then only
//! state setting, at the position the fixed pipeline's draw had.

use std::sync::Arc;

use wgpu::{Buffer, Device, Sampler, Texture, TextureView};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedIndices, ImportedVertexInput,
    Keepalive, PreparedProgramDraw, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::{ColorState, DepthState, RasterState, RenderState};

use super::DynamicError;
use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};

/// The engine semantics the scene supplies to the material, by the binding each fills.
mod semantic {
    pub(super) const FRAME: &str = "scene.frame";
    pub(super) const MATERIAL: &str = "scene.material";
    pub(super) const ALBEDO: &str = "scene.albedo";
    pub(super) const SAMPLER: &str = "scene.sampler";
}

/// Entry points of the scene shader's unlit material.
const VERTEX_ENTRY: &str = "vs_unlit";
const FRAGMENT_ENTRY: &str = "fs_unlit";

/// Bytes of the shader's frame block, which the runtime checks every lent frame buffer holds.
const FRAME_BYTES: u64 = crate::frame::FRAME_UNIFORM_BYTES as u64;

/// The buffers, texture and sampler of one instanced draw of quads. The draws hold this as the
/// owner of what they borrowed, so it lives until every frame that records them has completed.
#[derive(Debug)]
pub(super) struct QuadResources {
    pub(super) vertices: Buffer,
    pub(super) instance: Buffer,
    pub(super) tint: Buffer,
    pub(super) material: Buffer,
    pub(super) albedo: Texture,
    pub(super) albedo_view: TextureView,
    pub(super) sampler: Sampler,
    pub(super) indices: Buffer,
    /// Six indices over a quad's four corners, or three over a triangle's first three.
    pub(super) index_count: u32,
}

/// The runtime that draws dynamic quads, made when the first quads are staged, and the programs
/// it has registered for the shader sources the scene's pipelines carry.
#[derive(Default)]
pub(crate) struct UnlitRuntime {
    runtime: Option<ProgramRuntime>,
    programs: Vec<(Arc<str>, GraphicsProgram)>,
}

impl std::fmt::Debug for UnlitRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlitRuntime")
            .field("programs", &self.programs.len())
            .finish_non_exhaustive()
    }
}

impl UnlitRuntime {
    /// The runtime and the program for `source`, registered on first use. The runtime is made
    /// when the first quads are staged, from `context`; a later preparation passes none and
    /// uses the runtime that staging made.
    ///
    /// # Errors
    /// Fails when naga rejects the source, its declarations disagree with the shader, or there
    /// is no runtime yet and no context to make one.
    pub(super) fn program(
        &mut self,
        context: Option<(&Device, &SubmissionAuthority)>,
        source: &str,
    ) -> Result<(&mut ProgramRuntime, GraphicsProgram), DynamicError> {
        if self.runtime.is_none() {
            let (device, authority) = context.ok_or(DynamicError::NotPrepared)?;
            self.runtime = Some(ProgramRuntime::new(
                device.clone(),
                authority.clone(),
                AllocationBudget(0),
            ));
        }
        let runtime = self.runtime.as_mut().expect("the runtime was just made");
        let known = self
            .programs
            .iter()
            .find(|(registered, _)| registered.as_ref() == source);
        let program = if let Some((_, program)) = known {
            program.clone()
        } else {
            let text: Arc<str> = Arc::from(source);
            let program = register(runtime, &text)?;
            self.programs.push((text, program.clone()));
            program
        };
        Ok((runtime, program))
    }
}

fn semantic_source(name: &str) -> BindingSource {
    BindingSource::Engine(EngineSemantic::named(name))
}

fn declaration(
    (set, binding): (u32, u32),
    name: &str,
    visibility: wgpu::ShaderStages,
    kind: ResourceKind,
) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress { set, binding },
        source: semantic_source(name),
        visibility,
        kind,
    }
}

fn stage(
    source: &Arc<str>,
    entry: &str,
    kind: StageKind,
    resources: Vec<ResourceBinding>,
) -> Result<StageProgram, ProgramError> {
    Ok(StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(source.clone()),
            entry_point: entry.to_owned(),
            kind,
        },
        StageDeclarations {
            resources: ResourceLayout::new(resources),
            constants: Vec::new(),
        },
    )?)
}

/// The shader's own bindings: the frame block at group zero, and at group one the material
/// block, the albedo texture and the material sampler. The other bindings of the scene's frame
/// and material layouts are not reachable from these entry points, so the runtime gives them no
/// slot.
fn register(runtime: &ProgramRuntime, source: &Arc<str>) -> Result<GraphicsProgram, ProgramError> {
    let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
    let frame = declaration(
        (0, 0),
        semantic::FRAME,
        both,
        ResourceKind::Uniform {
            min_size: FRAME_BYTES,
        },
    );
    let vertex = stage(source, VERTEX_ENTRY, StageKind::Vertex, vec![frame.clone()])?;
    let fragment = stage(
        source,
        FRAGMENT_ENTRY,
        StageKind::Fragment,
        vec![
            frame,
            declaration(
                (1, 0),
                semantic::MATERIAL,
                wgpu::ShaderStages::FRAGMENT,
                ResourceKind::Uniform {
                    min_size: crate::pipeline::MATERIAL_UNIFORM_BYTES as u64,
                },
            ),
            declaration(
                (1, 1),
                semantic::ALBEDO,
                wgpu::ShaderStages::FRAGMENT,
                ResourceKind::Texture {
                    dimension: wgpu::TextureViewDimension::D2,
                    sample: wgpu::TextureSampleType::Float { filterable: true },
                    multisampled: false,
                },
            ),
            declaration(
                (1, 6),
                semantic::SAMPLER,
                wgpu::ShaderStages::FRAGMENT,
                ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ],
    )?;
    runtime.graphics_program(&vertex, &fragment)
}

/// Straight alpha over the first target, `LessEqual` depth without a depth write, no culling and
/// counter-clockwise fronts. The attachments after the first belong to other materials, so their
/// write masks are empty and the fragment stage, which writes location zero only, leaves them as
/// they were.
fn state(attachments: usize) -> RenderState {
    let mut colors = vec![Some(ColorState {
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    colors.resize(
        attachments,
        Some(ColorState {
            blend: None,
            write_mask: wgpu::ColorWrites::empty(),
        }),
    );
    RenderState {
        raster: RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        },
        depth: Some(DepthState {
            test: wgpu::CompareFunction::LessEqual,
            write: false,
            bias: wgpu::DepthBiasState::default(),
        }),
        colors,
        stencil: None,
    }
}

fn inputs(resources: &QuadResources) -> [ImportedVertexInput; 3] {
    let tint = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 9,
    }];
    [
        ImportedVertexInput {
            buffer: resources.vertices.clone(),
            stride: VERTEX_STRIDE,
            step: wgpu::VertexStepMode::Vertex,
            attributes: vertex_attributes().to_vec(),
        },
        ImportedVertexInput {
            buffer: resources.instance.clone(),
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: instance_attributes().to_vec(),
        },
        ImportedVertexInput {
            buffer: resources.tint.clone(),
            stride: 16,
            step: wgpu::VertexStepMode::Instance,
            attributes: tint.to_vec(),
        },
    ]
}

/// One instanced draw of quads for each frame slot, each binding that slot's frame uniform.
///
/// # Errors
/// Fails when the runtime refuses the draw: a lent buffer that is too small, an instance range
/// past the instance buffer, a state the signature does not describe or a GPU validation error.
pub(super) fn prepare_quads(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    signature: TargetSignature,
    frames: &[Buffer],
    (resources, instances): (&Arc<QuadResources>, u32),
) -> Result<Vec<PreparedProgramDraw>, ProgramError> {
    let state = state(signature.color_count());
    let inputs = inputs(resources);
    let owner: Keepalive = resources.clone();
    frames
        .iter()
        .map(|frame| {
            let imports = [
                Import::buffer(EngineSemantic::named(semantic::FRAME), frame.clone()),
                Import::buffer(
                    EngineSemantic::named(semantic::MATERIAL),
                    resources.material.clone(),
                ),
                Import::texture(
                    EngineSemantic::named(semantic::ALBEDO),
                    resources.albedo.clone(),
                    resources.albedo_view.clone(),
                ),
                Import::sampler(
                    EngineSemantic::named(semantic::SAMPLER),
                    resources.sampler.clone(),
                ),
            ];
            runtime.prepare_imported_draw(
                &ImportedDraw {
                    program,
                    state: &state,
                    vertex_inputs: &inputs,
                    bindings: &[],
                    imports: &imports,
                    geometry: &ImportedGeometry::Indexed(ImportedIndices {
                        buffer: resources.indices.clone(),
                        format: wgpu::IndexFormat::Uint32,
                        first: 0,
                        count: resources.index_count,
                        base_vertex: 0,
                    }),
                    instances: 0..instances,
                    owners: std::slice::from_ref(&owner),
                },
                signature,
            )
        })
        .collect()
}
