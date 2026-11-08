//! Terrain on the program runtime.
//!
//! One program per pass context and surface source. A context is what a `ScenePipelines`
//! fixes: the colour attachments and sample count of its pass, the terrain source it composes,
//! the winding and specular constants of the frame, and the bind group layouts the engine owns
//! at sets 0, 2 and 3. A surface source is a feature set (`TERRAIN_PAGES`, `TERRAIN_LAYERED`),
//! which selects the composition in [`source`] and the bindings the material set holds.
//!
//! The engine sets are lent: the pass owner binds the frame group once for the pass, the shadow
//! group before the terrain and the screen group before the draws, and the runtime builds none
//! of them ([`ExternalSet`]). The material set is the terrain's own. It is built once from the
//! resources the scene retains ([`PreparedSet`](worldrunner_render_program::runtime::PreparedSet))
//! and shared by every draw. The SCENE_* constants of the frame's specular convention and
//! winding are override constants of the fragment stage.
//!
//! The terrain declares no constant layout. Its 8224-byte block is packed from quantized plan
//! data by [`TerrainUniformState`](crate::terrain_uniform::TerrainUniformState) and lent as a
//! buffer, so there is one source of truth for its offsets.

pub(crate) mod source;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use wgpu::{BindGroupLayout, Device, FrontFace, ShaderStages, TextureFormat};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
    StorageAccess,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    ExternalSet, GraphicsProgram, ProgramRuntime, RuntimeStats, TargetSignature,
};
use worldrunner_render_program::state::{
    ColorState, DepthState, RasterState, RenderState, StencilState,
};
use worldrunner_render_program::variant::{FeatureId, FeatureSet};

use crate::renderer::depth::{DepthMode, STENCIL_REFERENCE};
use crate::terrain_uniform::TERRAIN_UNIFORM_BYTES;

/// The engine semantics terrain's bindings are filled from.
pub(crate) mod semantic {
    pub(crate) const FRAME: &str = "scene.frame";
    pub(crate) const DECALS: &str = "scene.decals";
    pub(crate) const DECAL_ALBEDO: &str = "scene.decals.albedo";
    pub(crate) const DECAL_SAMPLER: &str = "scene.decals.sampler";
    pub(crate) const DECAL_NORMAL: &str = "scene.decals.normal";
    pub(crate) const DECAL_SURFACE: &str = "scene.decals.surface";
    pub(crate) const SHADOW: &str = "scene.shadow";
    pub(crate) const SHADOW_DEPTH: &str = "scene.shadow.depth";
    pub(crate) const SHADOW_SAMPLER: &str = "scene.shadow.sampler";
    pub(crate) const SCREEN: &str = "scene.screen";
    pub(crate) const AMBIENT: &str = "scene.ao";
    pub(crate) const AMBIENT_RECEIVER: &str = "scene.ao.receiver";
    pub(crate) const ENVIRONMENT_SPECULAR: &str = "scene.environment.specular";
    pub(crate) const ENVIRONMENT_DIFFUSE: &str = "scene.environment.diffuse";
    pub(crate) const ENVIRONMENT_SAMPLER: &str = "scene.environment.sampler";
    pub(crate) const ENVIRONMENT_INTENSITY: &str = "scene.environment.intensity";

    pub(crate) const BLOCK: &str = "terrain.block";
    pub(crate) const TYPE_GRID: &str = "terrain.type_grid";
    pub(crate) const PALETTE_ALBEDO: &str = "terrain.palette.albedo";
    pub(crate) const PALETTE_NORMAL: &str = "terrain.palette.normal";
    pub(crate) const PALETTE_ROUGH_METAL: &str = "terrain.palette.rough_metal";
    pub(crate) const SAMPLER: &str = "terrain.sampler";
    pub(crate) const PAGES: &str = "terrain.pages";
    pub(crate) const PAGE_TABLE: &str = "terrain.page_table";
    pub(crate) const LAYERED_SOURCE: &str = "terrain.layered.source";
    pub(crate) const LAYERED_GRID: &str = "terrain.layered.grid";
    pub(crate) const LAYERED_IMAGES: &str = "terrain.layered.images";
    pub(crate) const LAYERED_REPEAT: &str = "terrain.layered.repeat";
    pub(crate) const LAYERED_CLAMP: &str = "terrain.layered.clamp";
    pub(crate) const PLANES: &str = "terrain.planes";

    /// The semantic of the `index`th layered source array.
    pub(crate) fn layered_array(index: u32) -> String {
        format!("terrain.layered.array.{index}")
    }
}

const VERTEX_ENTRY: &str = crate::shader::TERRAIN_VERTEX_ENTRY_POINT;
const FRAGMENT_ENTRY: &str = crate::shader::TERRAIN_FRAGMENT_ENTRY_POINT;

/// Where the surface colour comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Surface {
    /// Palette arrays blended through the type grid.
    Plain,
    /// Baked material pages.
    Pages,
    /// Layered source composition.
    Layered,
    /// Baked pages of layered sources.
    LayeredPages,
}

impl Surface {
    pub(crate) const fn new(layered: bool, pages: bool) -> Self {
        match (layered, pages) {
            (false, false) => Self::Plain,
            (false, true) => Self::Pages,
            (true, false) => Self::Layered,
            (true, true) => Self::LayeredPages,
        }
    }

    pub(crate) const fn layered(self) -> bool {
        matches!(self, Self::Layered | Self::LayeredPages)
    }

    pub(crate) const fn pages(self) -> bool {
        matches!(self, Self::Pages | Self::LayeredPages)
    }

    /// The features whose variant is this surface.
    pub(crate) fn features(self) -> FeatureSet {
        let mut features = FeatureSet::new();
        if self.pages() {
            features.insert(FeatureId::new("TERRAIN_PAGES"));
        }
        if self.layered() {
            features.insert(FeatureId::new("TERRAIN_LAYERED"));
        }
        features
    }
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

fn texture(dimension: wgpu::TextureViewDimension, sample: wgpu::TextureSampleType) -> ResourceKind {
    ResourceKind::Texture {
        dimension,
        sample,
        multisampled: false,
    }
}

fn float(dimension: wgpu::TextureViewDimension) -> ResourceKind {
    texture(
        dimension,
        wgpu::TextureSampleType::Float { filterable: true },
    )
}

fn read_only(min_size: u64) -> ResourceKind {
    ResourceKind::Storage {
        access: StorageAccess::ReadOnly,
        min_size,
    }
}

fn filtering() -> ResourceKind {
    ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering)
}

/// The bindings of the terrain's own set, the set 1 the material owns. The vertex stage reads
/// the triangle source; everything else is fragment-stage.
pub(crate) fn material_bindings(surface: Surface) -> Vec<ResourceBinding> {
    use wgpu::TextureViewDimension::D2;
    let (fragment, both) = (ShaderStages::FRAGMENT, ShaderStages::VERTEX_FRAGMENT);
    let mut bindings = vec![
        declare(
            (1, 0),
            semantic::BLOCK,
            fragment,
            ResourceKind::Uniform {
                min_size: TERRAIN_UNIFORM_BYTES as u64,
            },
        ),
        declare(
            (1, crate::terrain::planes::BINDING),
            semantic::PLANES,
            both,
            read_only(16),
        ),
    ];
    if surface.pages() {
        bindings.extend(page_bindings());
        return bindings;
    }
    bindings.push(declare(
        (1, 1),
        semantic::TYPE_GRID,
        fragment,
        texture(D2, wgpu::TextureSampleType::Uint),
    ));
    bindings.extend(if surface.layered() {
        layered_bindings()
    } else {
        palette_bindings()
    });
    bindings
}

/// The baked pages and the table that finds them.
fn page_bindings() -> [ResourceBinding; 2] {
    let fragment = ShaderStages::FRAGMENT;
    [
        declare(
            (1, 6),
            semantic::PAGES,
            fragment,
            texture(
                wgpu::TextureViewDimension::D2Array,
                wgpu::TextureSampleType::Uint,
            ),
        ),
        declare((1, 7), semantic::PAGE_TABLE, fragment, read_only(48)),
    ]
}

/// The layered source: its tables, its arrays and its two samplers.
fn layered_bindings() -> Vec<ResourceBinding> {
    let fragment = ShaderStages::FRAGMENT;
    let mut bindings: Vec<_> = [
        (8, semantic::LAYERED_SOURCE),
        (9, semantic::LAYERED_GRID),
        (10, semantic::LAYERED_IMAGES),
    ]
    .into_iter()
    .map(|(binding, name)| declare((1, binding), name, fragment, read_only(4)))
    .collect();
    for index in 0..crate::terrain::pages::layered_gpu::textures::ARRAY_LIMIT {
        bindings.push(declare(
            (1, 11 + index),
            &semantic::layered_array(index),
            fragment,
            float(wgpu::TextureViewDimension::D2Array),
        ));
    }
    bindings.push(declare(
        (1, 21),
        semantic::LAYERED_REPEAT,
        fragment,
        filtering(),
    ));
    bindings.push(declare(
        (1, 22),
        semantic::LAYERED_CLAMP,
        fragment,
        filtering(),
    ));
    bindings
}

/// The palette arrays and their sampler.
fn palette_bindings() -> Vec<ResourceBinding> {
    let fragment = ShaderStages::FRAGMENT;
    let mut bindings: Vec<_> = [
        (2, semantic::PALETTE_ALBEDO),
        (3, semantic::PALETTE_NORMAL),
        (4, semantic::PALETTE_ROUGH_METAL),
    ]
    .into_iter()
    .map(|(binding, name)| {
        declare(
            (1, binding),
            name,
            fragment,
            float(wgpu::TextureViewDimension::D2Array),
        )
    })
    .collect();
    bindings.push(declare((1, 5), semantic::SAMPLER, fragment, filtering()));
    bindings
}

/// What the shader reads from the frame group, set 0.
fn frame_bindings() -> ResourceLayout {
    use wgpu::TextureViewDimension::D2Array;
    let (fragment, both) = (ShaderStages::FRAGMENT, ShaderStages::VERTEX_FRAGMENT);
    ResourceLayout::new(vec![
        declare(
            (0, 0),
            semantic::FRAME,
            both,
            ResourceKind::Uniform {
                min_size: crate::frame::FRAME_UNIFORM_BYTES as u64,
            },
        ),
        declare(
            (0, 1),
            semantic::DECALS,
            fragment,
            ResourceKind::Uniform {
                min_size: crate::draw::decals::BYTES as u64,
            },
        ),
        declare((0, 2), semantic::DECAL_ALBEDO, fragment, float(D2Array)),
        declare((0, 3), semantic::DECAL_SAMPLER, fragment, filtering()),
        declare((0, 4), semantic::DECAL_NORMAL, fragment, float(D2Array)),
        declare((0, 5), semantic::DECAL_SURFACE, fragment, float(D2Array)),
    ])
}

/// What the shader reads from the shadow map group, set 2.
fn shadow_bindings() -> ResourceLayout {
    let fragment = ShaderStages::FRAGMENT;
    ResourceLayout::new(vec![
        declare(
            (2, 0),
            semantic::SHADOW,
            fragment,
            ResourceKind::Uniform {
                min_size: crate::shadows::SHADOW_UNIFORM_BYTES as u64,
            },
        ),
        declare(
            (2, 1),
            semantic::SHADOW_DEPTH,
            fragment,
            texture(
                wgpu::TextureViewDimension::D2Array,
                wgpu::TextureSampleType::Depth,
            ),
        ),
        declare(
            (2, 2),
            semantic::SHADOW_SAMPLER,
            fragment,
            ResourceKind::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
    ])
}

/// How the prepass terrain source declares its camera uniform, which `ssao/camera.wgsl` owns.
const CAMERA_DECLARATION: &str = "var<uniform> screen: ScreenCamera;";

/// The view, projection and inverse projection matrices and the two settings vectors of the
/// camera uniform: `ScreenCamera` in `ssao/camera.wgsl`.
const CAMERA_BYTES: u64 = 3 * 64 + 2 * 16;

/// What the screen group at set 3 holds in a context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    /// The filtered ambient occlusion, read at the fragment's own texel.
    Ambient,
    /// The ambient occlusion with the depth-aware gather's view matrix.
    AmbientGather,
    /// Ambient occlusion and the reflection environment.
    Reflection,
    /// The reflection environment and the gather's view matrix.
    ReflectionGather,
    /// The camera uniform the metadata prepass reads to write view-space normals.
    Camera,
}

impl Screen {
    /// The screen of a context whose terrain source is `source`, or `None` for a source no
    /// context makes.
    pub(crate) fn of(source: &str) -> Option<Self> {
        let gather = source.contains("fn receiver_gather(");
        if source.contains("struct ReflectionOutput") || source.contains("fn forward_reflection(") {
            Some(if gather {
                Self::ReflectionGather
            } else {
                Self::Reflection
            })
        } else if source.contains("fn screen_ao(") {
            Some(if gather {
                Self::AmbientGather
            } else {
                Self::Ambient
            })
        } else if source.contains(CAMERA_DECLARATION) {
            Some(Self::Camera)
        } else {
            None
        }
    }

    fn bindings(self) -> ResourceLayout {
        use wgpu::TextureViewDimension::{Cube, D2};
        let fragment = ShaderStages::FRAGMENT;
        if self == Self::Camera {
            return ResourceLayout::new(vec![declare(
                (3, 0),
                semantic::SCREEN,
                fragment,
                ResourceKind::Uniform {
                    min_size: CAMERA_BYTES,
                },
            )]);
        }
        let ambient = declare(
            (3, 0),
            semantic::AMBIENT,
            fragment,
            texture(D2, wgpu::TextureSampleType::Float { filterable: false }),
        );
        let receiver = declare(
            (3, 5),
            semantic::AMBIENT_RECEIVER,
            fragment,
            ResourceKind::Uniform { min_size: 64 },
        );
        let mut bindings = vec![ambient];
        if matches!(self, Self::Reflection | Self::ReflectionGather) {
            bindings.extend([
                declare(
                    (3, 1),
                    semantic::ENVIRONMENT_SPECULAR,
                    fragment,
                    float(Cube),
                ),
                declare((3, 2), semantic::ENVIRONMENT_DIFFUSE, fragment, float(D2)),
                declare((3, 3), semantic::ENVIRONMENT_SAMPLER, fragment, filtering()),
                declare(
                    (3, 4),
                    semantic::ENVIRONMENT_INTENSITY,
                    fragment,
                    ResourceKind::Uniform { min_size: 16 },
                ),
            ]);
        }
        if matches!(self, Self::AmbientGather | Self::ReflectionGather) {
            bindings.push(receiver);
        }
        ResourceLayout::new(bindings)
    }
}

/// Everything a context's programs are built from.
pub(crate) struct Description {
    pub device: Device,
    pub terrain_source: Rc<str>,
    pub colors: Vec<TextureFormat>,
    pub samples: u32,
    pub front_face: FrontFace,
    /// How the pass shares its depth with the prepass: the representation, test and stencil
    /// state of every terrain pipeline.
    pub depth: DepthMode,
    /// The SCENE_* override constants of the fragment stage.
    pub constants: Vec<(&'static str, f64)>,
    pub frame: BindGroupLayout,
    pub shadow: Option<BindGroupLayout>,
    pub screen: Option<BindGroupLayout>,
}

/// The program runtime of a family of pipelines: the pipelines a scene was built with and the
/// siblings it specializes from them. Terrain pipelines compile once per context however many
/// scenes draw with it. It is made when the first terrain is drawn, from that scene's
/// submission authority, and counts nothing against a budget: the resources it draws with are
/// lent by the scene, which has its own accounting.
#[derive(Clone, Default)]
pub(crate) struct Family(Rc<FamilyState>);

struct FamilyState {
    id: u64,
    runtime: RefCell<Option<ProgramRuntime>>,
}

impl Default for FamilyState {
    fn default() -> Self {
        Self {
            id: FAMILIES.fetch_add(1, Ordering::Relaxed),
            runtime: RefCell::default(),
        }
    }
}

/// A family that does not keep its runtime alive, for a draw's caches to tell when the pipelines
/// they were made for are gone.
#[derive(Clone, Debug)]
pub(crate) struct WeakFamily(std::rc::Weak<FamilyState>);

impl WeakFamily {
    pub(crate) fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

/// A context that does not keep its programs alive.
#[derive(Clone, Debug)]
pub(crate) struct WeakContext(std::rc::Weak<ContextState>);

impl WeakContext {
    pub(crate) fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

impl std::fmt::Debug for Family {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Family")
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl Family {
    /// Names the runtime, which no other family and no later one shares. A prepared set belongs
    /// to the runtime that built it, so anything that keeps sets keys them by this.
    pub(crate) fn id(&self) -> u64 {
        self.0.id
    }

    pub(crate) fn downgrade(&self) -> WeakFamily {
        WeakFamily(Rc::downgrade(&self.0))
    }

    /// What the runtime has built so far.
    pub(crate) fn stats(&self) -> RuntimeStats {
        self.0
            .runtime
            .borrow()
            .as_ref()
            .map_or_else(RuntimeStats::default, ProgramRuntime::stats)
    }

    /// Runs `build` against the runtime, making it from `authority` first if no scene has.
    pub(crate) fn with<R>(
        &self,
        (device, authority): (&Device, &SubmissionAuthority),
        build: impl FnOnce(&mut ProgramRuntime) -> R,
    ) -> R {
        let mut runtime = self.0.runtime.borrow_mut();
        let runtime = runtime.get_or_insert_with(|| {
            ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(0))
        });
        build(runtime)
    }
}

static CONTEXTS: AtomicU64 = AtomicU64::new(0);
static FAMILIES: AtomicU64 = AtomicU64::new(0);

/// The terrain programs of one `ScenePipelines`: made on first use, one per surface source.
#[derive(Clone)]
pub(crate) struct Context(Rc<ContextState>);

struct ContextState {
    id: u64,
    description: Description,
    family: Family,
    screen: Option<Screen>,
    programs: RefCell<BTreeMap<FeatureSet, GraphicsProgram>>,
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
            .and_then(|_| Screen::of(&description.terrain_source));
        Self(Rc::new(ContextState {
            id: CONTEXTS.fetch_add(1, Ordering::Relaxed),
            description,
            family: family.clone(),
            screen,
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

    /// Whether the context's screen group is one the program lends. A group of any other shape is
    /// drawn with the fixed pipelines.
    pub(crate) fn on_runtime(&self) -> bool {
        self.0.description.screen.is_none() || self.0.screen.is_some()
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

    /// Terrain writes every attachment of its pass, with no blend and back faces culled. Its
    /// depth and stencil state is the pass's [`DepthMode`] for a drawing that writes depth: the
    /// same function the fixed pipelines use, so the two cannot disagree.
    pub(crate) fn state(&self) -> RenderState {
        let depth = self.0.description.depth.state(true);
        RenderState {
            raster: RasterState {
                cull: Some(wgpu::Face::Back),
                front_face: self.0.description.front_face,
                alpha_to_coverage: false,
            },
            depth: Some(DepthState {
                test: depth
                    .depth_compare
                    .expect("a depth mode always names its test"),
                write: depth
                    .depth_write_enabled
                    .expect("a depth mode always names its write"),
                bias: depth.bias,
            }),
            colors: vec![
                Some(ColorState {
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                });
                self.0.description.colors.len()
            ],
            stencil: (depth.stencil != wgpu::StencilState::default()).then_some(StencilState {
                ops: depth.stencil,
                reference: STENCIL_REFERENCE,
            }),
        }
    }

    fn external(&self) -> Vec<ExternalSet> {
        let description = &self.0.description;
        let mut sets = vec![ExternalSet {
            set: 0,
            layout: description.frame.clone(),
            bindings: frame_bindings(),
        }];
        if let Some(shadow) = &description.shadow {
            sets.push(ExternalSet {
                set: 2,
                layout: shadow.clone(),
                bindings: shadow_bindings(),
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

    /// The program of `surface`, built on first use.
    ///
    /// # Errors
    /// Fails when the composed source does not parse, a declaration does not match what the
    /// entry points use, or the device cannot hold the layout.
    pub(crate) fn program(
        &self,
        runtime: &ProgramRuntime,
        surface: Surface,
    ) -> Result<GraphicsProgram, ProgramError> {
        let features = surface.features();
        if let Some(program) = self.0.programs.borrow().get(&features) {
            return Ok(program.clone());
        }
        let description = &self.0.description;
        let composed: std::sync::Arc<str> =
            source::compose(surface, &description.terrain_source).into();
        let declarations = || StageDeclarations {
            resources: ResourceLayout::new(material_bindings(surface)),
            constants: Vec::new(),
        };
        let vertex = StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(composed),
                entry_point: VERTEX_ENTRY.to_owned(),
                kind: StageKind::Vertex,
            },
            declarations(),
        )?;
        // One parse serves both stages. Only the fragment stage reads the specialization
        // constants, as in the fixed pipelines.
        let fragment = vertex
            .sibling(FRAGMENT_ENTRY, StageKind::Fragment)?
            .with_declarations(declarations())
            .with_overrides(&description.constants)?;
        let program = runtime.graphics_program_in(&vertex, &fragment, &self.external())?;
        self.0
            .programs
            .borrow_mut()
            .insert(features, program.clone());
        Ok(program)
    }
}
