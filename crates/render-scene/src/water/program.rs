//! The water body draw on the program runtime.
//!
//! Water owns a runtime of its own, made from the submission authority the renderer installs it
//! with, and counts nothing against a budget: every buffer and texture a body draws with belongs
//! to [`Water`](super::Water) and is lent to the runtime, as the dynamic unlit material lends its
//! quads. There is one program for each pipeline key, `([u32; 5], shadows)`, built on first use
//! from the text the fixed pipeline builder composed (`water::fixed` keeps a frozen copy). The
//! four text bits of `options[4]` (endpoint 256, vertex seeds 512, shadow response 1024 through
//! its filter 2048) and `shadows` choose the text, and the specular options and term bits are
//! override constants of both stages, as the fixed pipelines set them.
//!
//! Set 0 is water's own and is built from imports for each body and frame slot, as the fixed
//! path builds a bind group per body for each feature. Set 1 is the shadow group, lent: the pass
//! owner binds it once before the bodies, and the runtime builds none ([`ExternalSet`]).
//!
//! A draw is prepared, and retained by the frame's encoder, when the frame's water feature is
//! made, before the pass opens. Recording it in the pass only sets state.

use std::collections::BTreeMap;
use std::sync::Arc;

use wgpu::{Device, ShaderStages, TextureFormat, TextureViewDimension};
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
    ExternalSet, GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedVertexInput,
    Keepalive, PreparedProgramDraw, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::RasterState;

use crate::renderer::depth::DepthMode;

/// The engine semantics water lends to set 0, by the binding each fills.
mod semantic {
    pub(super) const BODY: &str = "water.body";
    pub(super) const SNAPSHOT_COLOR: &str = "water.snapshot_color";
    pub(super) const SNAPSHOT_DEPTH: &str = "water.snapshot_depth";
    pub(super) const FRAME: &str = "water.frame";
    pub(super) const ENVIRONMENT: &str = "water.environment";
    pub(super) const BRDF: &str = "water.brdf";
    pub(super) const SAMPLER: &str = "water.sampler";
    pub(super) const NORMAL: &str = "water.normal";
    pub(super) const FLOW: &str = "water.flow";
    pub(super) const WIND: &str = "water.wind";
    pub(super) const SURFACE_SAMPLER: &str = "water.surface_sampler";
    pub(super) const SHADOW: &str = "water.shadow";
    pub(super) const SHADOW_DEPTH: &str = "water.shadow_depth";
    pub(super) const SHADOW_SAMPLER: &str = "water.shadow_sampler";
}

/// The byte range of the per-body block and of the frame block in a body's uniform buffer.
pub(super) const BODY_RANGE: (u64, u64) = (0, 560);
pub(super) const FRAME_RANGE: (u64, u64) = (768, crate::frame::FRAME_UNIFORM_BYTES as u64);

const VERTEX_ENTRY: &str = "vs_water";
const FRAGMENT_ENTRY: &str = "fs_water";

/// The pipeline options of a body and whether the frame has a shadow group.
pub(super) type Key = ([u32; 5], bool);

/// Everything one body lends to its draw for one frame slot.
pub(super) struct BodyInputs<'a> {
    pub uniform: &'a wgpu::Buffer,
    pub snapshot_color: &'a wgpu::Texture,
    pub snapshot_depth: &'a wgpu::Texture,
    pub environment: &'a [wgpu::TextureView; 2],
    pub sampler: &'a wgpu::Sampler,
    /// Normal, flow and wind fields; a missing one reads the BRDF table, as the fixed group does.
    pub surface: [Option<&'a wgpu::TextureView>; 3],
    pub surface_sampler: &'a wgpu::Sampler,
    pub vertices: (&'a wgpu::Buffer, u32),
    /// The endpoint coordinates, drawn as a second vertex buffer when the key's text reads them.
    pub seed: Option<&'a wgpu::Buffer>,
}

/// The runtime water draws with and the programs it has built, by key.
pub(crate) struct Programs {
    runtime: ProgramRuntime,
    shadow_layout: wgpu::BindGroupLayout,
    by_key: BTreeMap<Key, GraphicsProgram>,
    /// Text, and the options it is compiled with, that replace those of a key. Only tests that
    /// check an equation by substituting shader text put any here.
    sources: BTreeMap<Key, (String, [u32; 5])>,
}

impl std::fmt::Debug for Programs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Programs")
            .field("programs", &self.by_key.len())
            .finish_non_exhaustive()
    }
}

impl Programs {
    pub(crate) fn new(device: &Device, authority: &SubmissionAuthority) -> Self {
        Self {
            runtime: ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(0)),
            shadow_layout: crate::shadows::water_layout(device),
            by_key: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }

    pub(crate) fn runtime(&self) -> &ProgramRuntime {
        &self.runtime
    }

    /// How many programs have been built, one for each key a feature has drawn with.
    #[cfg(test)]
    pub(crate) fn built(&self) -> usize {
        self.by_key.len()
    }

    /// Draws bodies of `key` with `source` compiled for `options` from now on, in place of the
    /// composed text: the substitution the equation tests make.
    #[cfg(test)]
    pub(crate) fn substitute(&mut self, key: Key, source: &str, options: [u32; 5]) {
        self.by_key.remove(&key);
        self.sources.insert(key, (source.to_owned(), options));
    }

    /// The program of `key`, built on first use.
    pub(super) fn program(&mut self, key: Key) -> Result<GraphicsProgram, ProgramError> {
        if let Some(program) = self.by_key.get(&key) {
            return Ok(program.clone());
        }
        let (source, options) = self.source(key);
        let program = build(&self.runtime, &self.shadow_layout, (options, key.1), source)?;
        self.by_key.insert(key, program.clone());
        Ok(program)
    }

    /// The text of `key` and the options it is compiled with: what the fixed pipeline builder
    /// built its module from (`water::fixed::pipelines` for the frozen copy).
    fn source(&self, (options, shadows): Key) -> (Arc<str>, [u32; 5]) {
        if let Some((source, options)) = self.sources.get(&(options, shadows)) {
            return (text(source, *options, shadows), *options);
        }
        (
            text(&super::water_source(shadows), options, shadows),
            options,
        )
    }

    /// The draw of one body for passes with `depth` and `samples`, retained by `encoder`'s frame.
    ///
    /// # Errors
    /// Fails when the program cannot be built, a lent resource does not fit its binding, or
    /// `encoder` is not a frame of this runtime's authority (`ForeignAuthority`).
    pub(super) fn prepare(
        &mut self,
        (key, depth, samples): (Key, DepthMode, u32),
        inputs: &BodyInputs<'_>,
        owner: &Keepalive,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<PreparedProgramDraw, ProgramError> {
        let program = self.program(key)?;
        let signature = TargetSignature::attachments(
            &[TextureFormat::Rgba16Float; 4],
            Some(depth.format()),
            samples,
        )?;
        // The fixed pipeline's primitive state is wgpu's default: no culling, counter-clockwise
        // fronts.
        let state = depth.render_state(
            4,
            RasterState {
                cull: None,
                front_face: wgpu::FrontFace::Ccw,
                alpha_to_coverage: false,
            },
        );
        let endpoint = key.0[4] & super::endpoint::SPECIALIZATION != 0;
        let vertex_inputs = vertex_inputs((inputs.vertices.0, inputs.seed), endpoint)?;
        // The runtime gives a binding the text never reads no slot and refuses an import for
        // it. Water's own text reads all of set 0, so every input is lent and an unread one is
        // the runtime's `UnusedImport`. A substituted test text may read fewer, and only there
        // is what the program does not read held back.
        let substituted = self.sources.contains_key(&key);
        let imports: Vec<_> = imports(inputs)
            .into_iter()
            .filter(|import| {
                let source = BindingSource::Engine(import.semantic.clone());
                !substituted
                    || program
                        .layout()
                        .slots()
                        .iter()
                        .any(|slot| slot.source == source)
            })
            .collect();
        let draw = self.runtime.prepare_imported_draw(
            &ImportedDraw {
                program: &program,
                state: &state,
                vertex_inputs: &vertex_inputs,
                bindings: &[],
                imports: &imports,
                sets: &[],
                geometry: &ImportedGeometry::Vertices {
                    first: 0,
                    count: inputs.vertices.1,
                },
                instances: 0..1,
                owners: std::slice::from_ref(owner),
            },
            signature,
        )?;
        draw.retain(encoder)?;
        Ok(draw)
    }
}

/// `source` specialised for `options` as the fixed pipeline builder does: the shadow response
/// filter first, then the vertex seeds.
fn text(source: &str, options: [u32; 5], shadows: bool) -> Arc<str> {
    let source = super::shadow_response::specialize(source, options[4], shadows);
    Arc::from(super::vertex_seeds::specialize(&source, options[4]).as_ref())
}

fn declare(
    (set, binding): (u32, u32),
    name: &str,
    visibility: ShaderStages,
    kind: ResourceKind,
) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress { set, binding },
        source: BindingSource::Engine(EngineSemantic::named(name)),
        visibility,
        kind,
    }
}

fn texture(dimension: TextureViewDimension, filterable: bool) -> ResourceKind {
    ResourceKind::Texture {
        dimension,
        sample: wgpu::TextureSampleType::Float { filterable },
        multisampled: false,
    }
}

/// Set 0 as `input_layout` declares it.
pub(super) fn bindings() -> Vec<ResourceBinding> {
    use TextureViewDimension::{Cube, D2};
    let (fragment, both) = (ShaderStages::FRAGMENT, ShaderStages::VERTEX_FRAGMENT);
    let filtering = ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering);
    vec![
        declare(
            (0, 0),
            semantic::BODY,
            both,
            ResourceKind::Uniform { min_size: 0 },
        ),
        declare(
            (0, 1),
            semantic::SNAPSHOT_COLOR,
            fragment,
            texture(D2, false),
        ),
        declare(
            (0, 2),
            semantic::SNAPSHOT_DEPTH,
            fragment,
            texture(D2, false),
        ),
        declare(
            (0, 3),
            semantic::FRAME,
            fragment,
            ResourceKind::Uniform {
                min_size: FRAME_RANGE.1,
            },
        ),
        declare((0, 4), semantic::ENVIRONMENT, fragment, texture(Cube, true)),
        declare((0, 5), semantic::BRDF, fragment, texture(D2, true)),
        declare((0, 6), semantic::SAMPLER, fragment, filtering),
        declare((0, 7), semantic::NORMAL, both, texture(D2, true)),
        declare((0, 8), semantic::FLOW, both, texture(D2, true)),
        declare((0, 10), semantic::WIND, both, texture(D2, true)),
        declare((0, 9), semantic::SURFACE_SAMPLER, both, filtering),
    ]
}

/// The shadow group as `shadows::water_layout` declares it, at set 1.
fn shadow_bindings() -> ResourceLayout {
    let fragment = ShaderStages::FRAGMENT;
    ResourceLayout::new(vec![
        declare(
            (1, 0),
            semantic::SHADOW,
            fragment,
            ResourceKind::Uniform {
                min_size: crate::shadows::SHADOW_UNIFORM_BYTES as u64,
            },
        ),
        declare(
            (1, 1),
            semantic::SHADOW_DEPTH,
            fragment,
            ResourceKind::Texture {
                dimension: TextureViewDimension::D2Array,
                sample: wgpu::TextureSampleType::Depth,
                multisampled: false,
            },
        ),
        declare(
            (1, 2),
            semantic::SHADOW_SAMPLER,
            fragment,
            ResourceKind::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
    ])
}

fn build(
    runtime: &ProgramRuntime,
    shadow_layout: &wgpu::BindGroupLayout,
    (options, shadows): Key,
    source: Arc<str>,
) -> Result<GraphicsProgram, ProgramError> {
    let declarations = || StageDeclarations {
        resources: ResourceLayout::new(bindings()),
        constants: Vec::new(),
    };
    let vertex = StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(source),
            entry_point: VERTEX_ENTRY.to_owned(),
            kind: StageKind::Vertex,
        },
        declarations(),
    )?;
    let fragment = vertex
        .sibling(FRAGMENT_ENTRY, StageKind::Fragment)?
        .with_declarations(declarations())
        .with_overrides(&super::terms::constants(options))?;
    let vertex = vertex.with_overrides(&super::terms::vertex_constants(options))?;
    let external = if shadows {
        vec![ExternalSet {
            set: 1,
            layout: shadow_layout.clone(),
            bindings: shadow_bindings(),
        }]
    } else {
        Vec::new()
    };
    runtime.graphics_program_in(&vertex, &fragment, &external)
}

/// The body's vertices, and the endpoint coordinates when the text reads them, in the layouts
/// `geometry::layouts` gives the fixed pipeline.
///
/// # Errors
/// `GeometryLayoutMismatch` when a vertex buffer slot has no layout, and `VertexBufferCount`
/// when the text reads endpoint coordinates the body does not have.
pub(super) fn vertex_inputs(
    (vertices, seed): (&wgpu::Buffer, Option<&wgpu::Buffer>),
    endpoint: bool,
) -> Result<Vec<ImportedVertexInput>, ProgramError> {
    let buffers = [Some(vertices), seed.filter(|_| endpoint)];
    let layouts = super::geometry::layouts(endpoint);
    let expected = layouts.len();
    let found = buffers.iter().take(expected).flatten().count();
    layouts
        .into_iter()
        .zip(buffers)
        .map(|(layout, buffer)| {
            let layout = layout.ok_or(ProgramError::GeometryLayoutMismatch)?;
            let buffer = buffer.ok_or(ProgramError::VertexBufferCount { expected, found })?;
            Ok(ImportedVertexInput {
                buffer: buffer.clone(),
                stride: layout.array_stride,
                step: layout.step_mode,
                attributes: layout.attributes.to_vec(),
            })
        })
        .collect()
}

/// What `snapshot_group` binds, as imports.
fn imports(inputs: &BodyInputs<'_>) -> Vec<Import> {
    let named = |name: &str| EngineSemantic::named(name);
    let view =
        |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
    let lent = |name: &str, view: &wgpu::TextureView, dimension| {
        Import::texture_view(named(name), view.texture().clone(), view.clone(), dimension)
    };
    let brdf = &inputs.environment[1];
    let [normal, flow, wind] = inputs.surface.map(|input| input.unwrap_or(brdf));
    vec![
        Import::buffer_range(
            named(semantic::BODY),
            inputs.uniform.clone(),
            BODY_RANGE.0,
            BODY_RANGE.1,
        ),
        Import::texture(
            named(semantic::SNAPSHOT_COLOR),
            inputs.snapshot_color.clone(),
            view(inputs.snapshot_color),
        ),
        Import::texture(
            named(semantic::SNAPSHOT_DEPTH),
            inputs.snapshot_depth.clone(),
            view(inputs.snapshot_depth),
        ),
        Import::buffer_range(
            named(semantic::FRAME),
            inputs.uniform.clone(),
            FRAME_RANGE.0,
            FRAME_RANGE.1,
        ),
        lent(
            semantic::ENVIRONMENT,
            &inputs.environment[0],
            TextureViewDimension::Cube,
        ),
        lent(semantic::BRDF, brdf, TextureViewDimension::D2),
        Import::sampler(named(semantic::SAMPLER), inputs.sampler.clone()),
        lent(semantic::NORMAL, normal, TextureViewDimension::D2),
        lent(semantic::FLOW, flow, TextureViewDimension::D2),
        lent(semantic::WIND, wind, TextureViewDimension::D2),
        Import::sampler(
            named(semantic::SURFACE_SAMPLER),
            inputs.surface_sampler.clone(),
        ),
    ]
}
