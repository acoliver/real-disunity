//! Objects on the program runtime: lit surfaces, foliage and impostors.
//!
//! One program per pass context and [`PipelineKey`]. A context is what a `ScenePipelines` fixes:
//! the colour attachments and sample count of its pass, the object source it composes, the
//! winding and specular constants of the frame, and the bind group layouts the engine owns at
//! sets 0, 2 and 3. A key is a feature set (`MASKED`, `DOUBLE_SIDED`, `COVERAGE_MULTISAMPLE`,
//! `IMPOSTOR`, `RELIEF`), which picks the fragment entry point and the override constants and
//! decides the cull mode and alpha to coverage of the state.
//!
//! The engine sets are lent, as terrain lends them: the pass owner binds the frame group once for
//! the pass and the shadow and screen groups before the draws, and the runtime builds none of
//! them ([`ExternalSet`]). Set 1 is the material's own. One
//! [`PreparedSet`](worldrunner_render_program::runtime::PreparedSet) per scene material is built
//! from the resources the scene holds and bound with the pipeline the material draws with.
//!
//! The objects declare no constant layout. The 576-byte `Material` block is packed by
//! [`MaterialUniform`](crate::pipeline::MaterialUniform) and lent as a buffer, so there is one
//! source of truth for its offsets.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use wgpu::{BindGroupLayout, Device, FrontFace, ShaderStages, TextureFormat};
use worldrunner_render_program::binding::{ResourceBinding, ResourceKind, ResourceLayout};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    ExternalSet, GraphicsProgram, ProgramRuntime, TargetSignature, VertexBufferSpec,
    VertexLayoutSpec,
};
use worldrunner_render_program::state::{RasterState, RenderState};
use worldrunner_render_program::variant::{FeatureId, FeatureSet};

use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE};
use crate::geometry::{instance_attributes, previous_instance_attributes, vertex_attributes};
use crate::pipeline::{MATERIAL_UNIFORM_BYTES, MaskCoverage, PipelineKey};
use crate::renderer::depth::DepthMode;
use crate::terrain::program::{
    Family, Screen, declare, filtering, float, frame_bindings, read_only, shadow_bindings,
};

/// The engine semantics the material's bindings are filled from.
pub(crate) mod semantic {
    pub(crate) const MATERIAL: &str = "object.material";
    pub(crate) const ALBEDO: &str = "object.albedo";
    pub(crate) const NORMAL: &str = "object.normal";
    pub(crate) const ROUGH_METAL: &str = "object.rough_metal";
    pub(crate) const AMBIENT_OCCLUSION: &str = "object.ambient_occlusion";
    pub(crate) const SELF_ILLUMINATION: &str = "object.self_illumination";
    pub(crate) const SAMPLER: &str = "object.sampler";
    pub(crate) const CONTACT_HEIGHTS: &str = "object.contact_heights";
    pub(crate) const IMPOSTOR_DEPTH: &str = "object.impostor_depth";
    pub(crate) const IMPOSTOR_BORDERS: &str = "object.impostor_borders";
    pub(crate) const WIND_FIELD: &str = "object.wind_field";
    /// Declared by the frame set in the contexts that write the object-motion residual.
    pub(crate) const PREVIOUS_FRAME: &str = "scene.previous_frame";
}

const VERTEX_ENTRY: &str = crate::shader::VERTEX_ENTRY_POINT;

/// Bytes of the previous-frame block at binding 6 of the frame set.
const PREVIOUS_FRAME_BYTES: u64 = 80;

/// Bytes of the contact-height block's two header vectors, which the runtime array follows.
const CONTACT_HEADER_BYTES: u64 = 32;

impl PipelineKey {
    /// The features whose variant is this key.
    pub(crate) fn features(self) -> FeatureSet {
        let mut features = FeatureSet::new();
        for (set, name) in [
            (self.masked, "MASKED"),
            (self.double_sided, "DOUBLE_SIDED"),
            (
                matches!(self.coverage, MaskCoverage::Multisample),
                "COVERAGE_MULTISAMPLE",
            ),
            (self.sampling.is_impostor(), "IMPOSTOR"),
            (self.relief, "RELIEF"),
        ] {
            if set {
                features.insert(FeatureId::new(name));
            }
        }
        features
    }

    /// The fragment entry point the key draws with.
    pub(crate) const fn fragment_entry(self) -> &'static str {
        match (self.relief, self.masked) {
            (true, true) => "fs_relief_masked",
            (true, false) => "fs_relief_opaque",
            (false, masked) => crate::shader::fragment_entry_point(masked),
        }
    }
}

/// The bindings of the object's own set, the set 1 the material owns. Only the fragment stage
/// reads the surface maps and the relief depth; the vertex stage reads the impostor borders and
/// the wind field, and both read the material block, the sampler and the contact heights.
pub(crate) fn material_bindings() -> Vec<ResourceBinding> {
    use wgpu::TextureViewDimension::D2;
    let (fragment, vertex, both) = (
        ShaderStages::FRAGMENT,
        ShaderStages::VERTEX,
        ShaderStages::VERTEX_FRAGMENT,
    );
    let mut bindings = vec![declare(
        (1, 0),
        semantic::MATERIAL,
        both,
        ResourceKind::Uniform {
            min_size: MATERIAL_UNIFORM_BYTES as u64,
        },
    )];
    for (binding, name) in [
        (1, semantic::ALBEDO),
        (2, semantic::NORMAL),
        (3, semantic::ROUGH_METAL),
        (4, semantic::AMBIENT_OCCLUSION),
        (5, semantic::SELF_ILLUMINATION),
    ] {
        bindings.push(declare((1, binding), name, fragment, float(D2)));
    }
    bindings.push(declare((1, 6), semantic::SAMPLER, both, filtering()));
    bindings.push(declare(
        (1, 7),
        semantic::CONTACT_HEIGHTS,
        both,
        read_only(CONTACT_HEADER_BYTES),
    ));
    bindings.push(declare(
        (1, 8),
        semantic::IMPOSTOR_DEPTH,
        fragment,
        float(D2),
    ));
    bindings.push(declare(
        (1, 9),
        semantic::IMPOSTOR_BORDERS,
        vertex,
        float(D2),
    ));
    bindings.push(declare((1, 10), semantic::WIND_FIELD, vertex, float(D2)));
    bindings
}

/// What the shader reads from the frame group, set 0: terrain's bindings and the previous-frame
/// block that the velocity contexts' vertex stage reads.
fn object_frame_bindings() -> ResourceLayout {
    let mut bindings = frame_bindings().bindings;
    bindings.push(declare(
        (0, 6),
        semantic::PREVIOUS_FRAME,
        ShaderStages::VERTEX_FRAGMENT,
        ResourceKind::Uniform {
            min_size: PREVIOUS_FRAME_BYTES,
        },
    ));
    ResourceLayout::new(bindings)
}

/// How the pipelines read the mesh, instance and, in the velocity contexts, previous-instance
/// buffers, which are the fixed path's.
pub(crate) fn vertex_layout(previous_transforms: bool) -> VertexLayoutSpec {
    let mut buffers = vec![
        VertexBufferSpec {
            stride: VERTEX_STRIDE,
            step: wgpu::VertexStepMode::Vertex,
            attributes: vertex_attributes().to_vec(),
        },
        VertexBufferSpec {
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: instance_attributes().to_vec(),
        },
    ];
    if previous_transforms {
        buffers.push(VertexBufferSpec {
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: previous_instance_attributes().to_vec(),
        });
    }
    VertexLayoutSpec::new(buffers)
}

/// Everything a context's programs are built from.
pub(crate) struct Description {
    pub device: Device,
    pub source: Rc<str>,
    pub colors: Vec<TextureFormat>,
    pub samples: u32,
    pub front_face: FrontFace,
    /// How the pass shares its depth with the prepass: the representation, test and stencil
    /// state of every object pipeline.
    pub depth: DepthMode,
    /// Whether the pass writes the object-motion residual, which adds the third vertex buffer.
    pub previous_transforms: bool,
    /// The SCENE_* override constants of the fragment stage.
    pub constants: Vec<(&'static str, f64)>,
    pub frame: BindGroupLayout,
    pub shadow: Option<BindGroupLayout>,
    pub screen: Option<BindGroupLayout>,
}

static CONTEXTS: AtomicU64 = AtomicU64::new(0);

/// The object programs of one `ScenePipelines`: made on first use, one per pipeline key.
#[derive(Clone)]
pub(crate) struct Context(Rc<ContextState>);

struct ContextState {
    id: u64,
    description: Description,
    family: Family,
    screen: Option<Screen>,
    vertex: RefCell<Option<StageProgram>>,
    programs: RefCell<BTreeMap<FeatureSet, GraphicsProgram>>,
}

/// A context that does not keep its programs alive.
#[derive(Clone, Debug)]
pub(crate) struct WeakContext(std::rc::Weak<ContextState>);

impl WeakContext {
    pub(crate) fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("id", &self.0.id)
            .field("programs", &self.0.programs.borrow().len())
            .finish_non_exhaustive()
    }
}

impl Context {
    pub(crate) fn new(description: Description, family: &Family) -> Self {
        let screen = description
            .screen
            .as_ref()
            .and_then(|_| Screen::of(&description.source));
        Self(Rc::new(ContextState {
            id: CONTEXTS.fetch_add(1, Ordering::Relaxed),
            description,
            family: family.clone(),
            screen,
            vertex: RefCell::default(),
            programs: RefCell::default(),
        }))
    }

    pub(crate) fn id(&self) -> u64 {
        self.0.id
    }

    pub(crate) fn downgrade(&self) -> WeakContext {
        WeakContext(Rc::downgrade(&self.0))
    }

    pub(crate) fn family(&self) -> &Family {
        &self.0.family
    }

    pub(crate) fn device(&self) -> &Device {
        &self.0.description.device
    }

    /// Whether the pass the context draws into writes the object-motion residual, so that its
    /// pipelines read the previous-instance buffer.
    pub(crate) fn previous_transforms(&self) -> bool {
        self.0.description.previous_transforms
    }

    /// The pass the context draws into.
    ///
    /// # Errors
    /// Fails when the attachments are not ones the runtime draws into.
    pub(crate) fn signature(&self) -> Result<TargetSignature, ProgramError> {
        TargetSignature::attachments(
            &self.0.description.colors,
            Some(self.0.description.depth.format()),
            self.0.description.samples,
        )
    }

    /// Objects write every attachment of their pass, with no blend. The cull mode and alpha to
    /// coverage are the key's, and the depth and stencil state is the pass's [`DepthMode`] for a
    /// drawing that writes depth: the same function the fixed pipelines use, so forward depth
    /// reuse (equal depth and stencil at one sample) is unchanged.
    pub(crate) fn state(&self, key: PipelineKey) -> RenderState {
        let description = &self.0.description;
        description.depth.render_state(
            description.colors.len(),
            RasterState {
                cull: key.cull_mode(),
                front_face: description.front_face,
                alpha_to_coverage: key.alpha_to_coverage(description.samples),
            },
        )
    }

    fn external(&self) -> Vec<ExternalSet> {
        let description = &self.0.description;
        let mut sets = vec![ExternalSet {
            set: 0,
            layout: description.frame.clone(),
            bindings: object_frame_bindings(),
        }];
        if let Some(shadow) = &description.shadow {
            sets.push(ExternalSet {
                set: 2,
                layout: shadow.clone(),
                bindings: shadow_bindings(description.source.contains("fn scene_receiver_load(")),
            });
        }
        if let (Some(layout), Some(screen)) = (&description.screen, self.0.screen) {
            sets.push(ExternalSet {
                set: 3,
                layout: layout.clone(),
                bindings: screen.bindings(),
            });
        }
        sets
    }

    /// The vertex stage every key shares, parsed once per context.
    fn vertex(&self) -> Result<StageProgram, ProgramError> {
        if let Some(vertex) = self.0.vertex.borrow().as_ref() {
            return Ok(vertex.clone());
        }
        let source: Arc<str> = Arc::from(&*self.0.description.source);
        let vertex = StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(source),
                entry_point: VERTEX_ENTRY.to_owned(),
                kind: StageKind::Vertex,
            },
            declarations(),
        )?;
        *self.0.vertex.borrow_mut() = Some(vertex.clone());
        Ok(vertex)
    }

    /// The program of `key`, built on first use.
    ///
    /// # Errors
    /// Fails when the composed source does not parse, a declaration does not match what the
    /// entry points use, or the device cannot hold the layout.
    pub(crate) fn program(
        &self,
        runtime: &ProgramRuntime,
        key: PipelineKey,
    ) -> Result<GraphicsProgram, ProgramError> {
        let features = key.features();
        if let Some(program) = self.0.programs.borrow().get(&features) {
            return Ok(program.clone());
        }
        let description = &self.0.description;
        let vertex = self.vertex()?;
        // One parse serves the vertex stage and every fragment entry point. Only the fragment
        // stage reads the specialization constants, as the frozen reference does.
        let mut constants = description.constants.clone();
        constants.extend(key.fragment_constants(description.samples));
        let fragment = vertex
            .sibling(key.fragment_entry(), StageKind::Fragment)?
            .with_declarations(declarations())
            .with_overrides(&constants)?;
        let program = runtime.graphics_program_in(&vertex, &fragment, &self.external())?;
        self.0
            .programs
            .borrow_mut()
            .insert(features, program.clone());
        Ok(program)
    }
}

fn declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(material_bindings()),
        constants: Vec::new(),
    }
}
