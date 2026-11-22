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

#[path = "contour.rs"]
mod contour;
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
    ExternalSet, GraphicsProgram, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::{RasterState, RenderState};
use worldrunner_render_program::variant::{FeatureId, FeatureSet};

use super::layout::GeometryLayout;
use super::skin_source::{PALETTE_BINDING, skinned_source};
use crate::pipeline::{MATERIAL_UNIFORM_BYTES, MaskCoverage, PipelineKey};
use crate::renderer::depth::DepthMode;
use crate::skinning::PALETTE_ENTRY_BYTES;
use crate::terrain::program::{
    Family, Screen, declare, filtering, float, frame_bindings, object_shadow_bindings, read_only,
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
    /// The scene palette, bound in the programs of skinned geometry only.
    pub(crate) const SKIN_PALETTE: &str = "object.skin_palette";
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

/// The bindings of the material set of `geometry`: the static ones, and for skinned geometry
/// the scene palette, which only the vertex stage reads.
pub(crate) fn material_bindings_of(geometry: GeometryLayout) -> Vec<ResourceBinding> {
    let mut bindings = material_bindings();
    if geometry.is_skinned() {
        bindings.push(declare(
            (1, PALETTE_BINDING),
            semantic::SKIN_PALETTE,
            ShaderStages::VERTEX,
            read_only(PALETTE_ENTRY_BYTES),
        ));
    }
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

/// Everything a context's programs are built from.
#[derive(Clone)]
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
    /// Which entry points the programs use, and for the casters their depth state.
    pub entry: Entry,
}

/// The entry points a context's programs are made from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Entry {
    /// The forward pass: the shared vertex entry point and the key's fragment entry point.
    Forward,
    /// Read-only Equal-depth visible IDs, without stencil ownership or lighting.
    Contour,
    /// The shadow casters: a depth-only pass into a directional map, drawn with the caster entry
    /// points and the map's depth test and bias.
    Caster(Caster),
    /// The camera-relative celestial bodies, drawn into the sky's mesh target with the
    /// celestial entry points.
    Celestial,
}

/// The depth state of a directional map's casters.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Caster {
    pub compare: wgpu::CompareFunction,
    pub bias: wgpu::DepthBiasState,
}

/// The vertex and fragment entry points of the casters and of the celestial bodies.
const CASTER_VERTEX: &str = "vs_shadow";
const CASTER_OPAQUE: &str = "fs_depth_opaque";
const CASTER_MASKED: &str = "fs_depth_masked";
const CELESTIAL_VERTEX: &str = "vs_celestial";
const CELESTIAL_OPAQUE: &str = "fs_celestial_opaque";
const CELESTIAL_MASKED: &str = "fs_celestial_masked";

/// The key whose caster `key` draws with. A directional map tells apart only whether a key is an
/// impostor and the two lowest bits of its index (masked and double-sided): relief is flat in the
/// map, and one sample has no coverage choice. Every key casts as the first key of its family, so
/// the casters share eight pipelines among the keys.
pub(crate) fn caster_key(key: PipelineKey) -> PipelineKey {
    PipelineKey::ALL[usize::from(key.sampling.is_impostor()) * 16 + key.index() % 4]
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
    contour_layout: Option<BindGroupLayout>,
    contour: RefCell<Option<Context>>,
    contour_groups: RefCell<BTreeMap<u8, wgpu::BindGroup>>,
    vertex: RefCell<BTreeMap<GeometryLayout, StageProgram>>,
    programs: RefCell<BTreeMap<(GeometryLayout, FeatureSet), GraphicsProgram>>,
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
            contour_layout: matches!(description.entry, Entry::Contour)
                .then(|| contour::layout(&description.device)),
            contour: RefCell::default(),
            contour_groups: RefCell::default(),
            description,
            family: family.clone(),
            screen,
            vertex: RefCell::default(),
            programs: RefCell::default(),
        }))
    }

    /// Whether the context draws the shadow casters, which cast as fewer keys and materials than
    /// the forward pass draws with.
    pub(crate) fn is_caster(&self) -> bool {
        matches!(self.0.description.entry, Entry::Caster(_))
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
        if matches!(self.0.description.entry, Entry::Caster(_)) {
            return TargetSignature::depth_only(worldrunner_render_shadow::DEPTH_FORMAT, 1);
        }
        TargetSignature::attachments(
            &self.0.description.colors,
            Some(self.0.description.depth.format()),
            self.0.description.samples,
        )
    }

    /// Objects write every attachment of their pass, with no blend. The cull mode and alpha to
    /// coverage are the key's, and the depth and stencil state is the pass's [`DepthMode`] for a
    /// drawing that writes depth: the translation [`DepthMode::render_state`] that terrain and
    /// objects share, so forward depth reuse (equal depth and stencil at one sample) holds.
    pub(crate) fn state(&self, key: PipelineKey) -> RenderState {
        let description = &self.0.description;
        if let Entry::Caster(caster) = description.entry {
            return RenderState {
                raster: RasterState {
                    cull: caster_key(key).cull_mode(),
                    front_face: FrontFace::Ccw,
                    alpha_to_coverage: false,
                },
                depth: Some(worldrunner_render_program::state::DepthState {
                    test: caster.compare,
                    write: true,
                    bias: caster.bias,
                }),
                colors: Vec::new(),
                stencil: None,
            };
        }
        let mut state = description.depth.render_state(
            description.colors.len(),
            RasterState {
                cull: key.cull_mode(),
                front_face: description.front_face,
                alpha_to_coverage: key.alpha_to_coverage(description.samples),
            },
        );
        if matches!(description.entry, Entry::Contour) {
            state = contour::state(state);
        }
        state
    }

    fn external(&self) -> Vec<ExternalSet> {
        let description = &self.0.description;
        let mut sets = vec![ExternalSet {
            set: 0,
            layout: description.frame.clone(),
            bindings: object_frame_bindings(),
        }];
        if let Some(layout) = &self.0.contour_layout {
            sets.push(ExternalSet {
                set: 2,
                layout: layout.clone(),
                bindings: contour::bindings(),
            });
        }
        if let Some(shadow) = &description.shadow {
            sets.push(ExternalSet {
                set: 2,
                layout: shadow.clone(),
                bindings: object_shadow_bindings(&description.source),
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

    /// The vertex stage every key of `geometry` shares, parsed once per context.
    fn vertex(&self, geometry: GeometryLayout) -> Result<StageProgram, ProgramError> {
        if let Some(vertex) = self.0.vertex.borrow().get(&geometry) {
            return Ok(vertex.clone());
        }
        let source: Arc<str> = if geometry.is_skinned() {
            Arc::from(skinned_source(&self.0.description.source))
        } else {
            Arc::from(&*self.0.description.source)
        };
        let vertex = StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(source),
                entry_point: match self.0.description.entry {
                    Entry::Forward | Entry::Contour => VERTEX_ENTRY,
                    Entry::Caster(_) => CASTER_VERTEX,
                    Entry::Celestial => CELESTIAL_VERTEX,
                }
                .to_owned(),
                kind: StageKind::Vertex,
            },
            declarations(geometry),
        )?;
        self.0.vertex.borrow_mut().insert(geometry, vertex.clone());
        Ok(vertex)
    }

    /// The program of `key` for `geometry`, built on first use.
    ///
    /// # Errors
    /// Fails when the composed source does not parse, a declaration does not match what the
    /// entry points use, the device cannot hold the layout, or the pass writes object motion
    /// and the geometry is skinned, which has no previous-pose form yet.
    pub(crate) fn program(
        &self,
        runtime: &ProgramRuntime,
        key: PipelineKey,
        geometry: GeometryLayout,
    ) -> Result<GraphicsProgram, ProgramError> {
        let description = &self.0.description;
        let key = if matches!(description.entry, Entry::Caster(_)) {
            caster_key(key)
        } else {
            key
        };
        if geometry.is_skinned() && self.previous_transforms() {
            return Err(ProgramError::UnsupportedState {
                what: "skinned geometry in a pass that writes object motion",
            });
        }
        let features = key.features();
        if let Some(program) = self.0.programs.borrow().get(&(geometry, features.clone())) {
            return Ok(program.clone());
        }
        let vertex = self.vertex(geometry)?;
        // One parse serves the vertex stage and every fragment entry point. Only the fragment
        // stage reads the specialization constants.
        let mut constants = description.constants.clone();
        constants.extend(key.fragment_constants(description.samples));
        let entry = match (description.entry, key.masked) {
            (Entry::Caster(_), true) => CASTER_MASKED,
            (Entry::Caster(_), false) => CASTER_OPAQUE,
            (Entry::Celestial, true) => CELESTIAL_MASKED,
            (Entry::Celestial, false) => CELESTIAL_OPAQUE,
            (Entry::Forward, _) => key.fragment_entry(),
            (Entry::Contour, _) => contour::entry(key),
        };
        let fragment = vertex
            .sibling(entry, StageKind::Fragment)?
            .with_declarations(declarations(geometry))
            .with_overrides(&constants)?;
        let program = runtime.graphics_program_in(&vertex, &fragment, &self.external())?;
        self.0
            .programs
            .borrow_mut()
            .insert((geometry, features), program.clone());
        Ok(program)
    }
}

fn declarations(geometry: GeometryLayout) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(material_bindings_of(geometry)),
        constants: Vec::new(),
    }
}
