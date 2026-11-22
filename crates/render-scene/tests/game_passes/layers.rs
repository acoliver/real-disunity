//! A game with a persistent three-layer array. Producer passes fill chosen layers with a uniform
//! colour that the frame uploads; a reader pass samples the whole array and writes the frame's
//! colour, so a layer filled in one frame must read back unchanged in a later one.
use super::common::{self, Counts, array_binding, graphics, uniform_binding};
use std::sync::{Arc, Mutex};
use wgpu::TextureFormat;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{EngineSemantic, ResourceLayout};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, PreparedProgramDraw, ProgramRuntime,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameCameraHandle, GameBuild, GameFeatureFactory, GamePassSpec, GameTargets,
        PassTarget, TargetDeclaration, TargetExtent,
    },
    wgpu,
};

const FILL_WGSL: &str = "
struct Fill { colour: vec4<f32> }
@group(0) @binding(0) var<uniform> fill: Fill;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return fill.colour; }";

const READ_WGSL: &str = "
@group(0) @binding(0) var layers: texture_2d_array<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let a = textureLoad(layers, vec2<i32>(0, 0), 0, 0);
    let b = textureLoad(layers, vec2<i32>(0, 0), 1, 0);
    let c = textureLoad(layers, vec2<i32>(0, 0), 2, 0);
    return vec4<f32>(a.r, b.g, c.b, 1.0);
}";

pub const ARRAY: &str = "layers";
/// A persistent target that follows the frame's size, for the resize case.
pub const HISTORY: &str = "history";
pub const FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
pub const LAYERS: u32 = 3;
pub const READER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 10);
/// The producer of the history target's only layer, listed in `Knobs::produce` like the array's.
pub const HISTORY_PRODUCER: u32 = 20;

/// The target and layer a producer fills.
fn produced(producer: u32) -> ResourceRef {
    ResourceRef {
        name: if producer == HISTORY_PRODUCER {
            HISTORY
        } else {
            ARRAY
        },
        version: 0,
        mip: 0,
        layer: if producer == HISTORY_PRODUCER {
            0
        } else {
            producer
        },
    }
}

/// The constants buffer of a producer.
fn constants_slot(producer: u32) -> usize {
    if producer == HISTORY_PRODUCER {
        LAYERS as usize
    } else {
        producer as usize
    }
}

fn producer(layer: u32) -> PassEntry {
    PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), layer)
}

#[derive(Default)]
pub struct Knobs {
    /// The layers the producers fill this frame and the colour each uploads.
    pub produce: Vec<(u32, [f32; 4])>,
    /// Plan the reader, which samples the whole array.
    pub read: bool,
    /// Refuse to build the feature, as a factory whose resources cannot be made does.
    pub refuse_build: bool,
    pub counts: Counts,
    /// The first producer's constants buffer, which the test reads back.
    pub constants: Option<wgpu::Buffer>,
    pub stats: Option<worldrunner_render_program::runtime::RuntimeStats>,
}

pub type Shared = Arc<Mutex<Knobs>>;

pub fn knobs() -> Shared {
    Arc::new(Mutex::new(Knobs {
        read: true,
        ..Knobs::default()
    }))
}

pub struct LayersGame {
    pub knobs: Shared,
}

impl GameFeatureFactory for LayersGame {
    fn name(&self) -> &'static str {
        "layers game"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            TargetDeclaration::new(ARRAY, FORMAT, TargetExtent::Fixed([4, 4]))
                .layers(LAYERS)
                .persistent()
                .with_usage(wgpu::TextureUsages::COPY_SRC),
            TargetDeclaration::new(HISTORY, FORMAT, TargetExtent::Frame).persistent(),
        ]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        {
            let mut knobs = self.knobs.lock().unwrap();
            knobs.counts.builds += 1;
            if knobs.refuse_build {
                return Err(FrameError::FeaturePreparation {
                    feature: "layers",
                    reason: "the factory refuses this generation".to_owned(),
                });
            }
        }
        let device = context.context.device();
        let runtime = ProgramRuntime::new(
            device.clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let fill = graphics(
            &runtime,
            FILL_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: ResourceLayout::new(vec![uniform_binding(0, "layers.fill")]),
                constants: Vec::new(),
            },
        );
        let read = graphics(
            &runtime,
            READ_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: ResourceLayout::new(vec![array_binding(0, "layers.array")]),
                constants: Vec::new(),
            },
        );
        let constants: Vec<wgpu::Buffer> = (0..=LAYERS)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("layer fill constants"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                })
            })
            .collect();
        self.knobs.lock().unwrap().constants = Some(constants[0].clone());
        Ok(Box::new(Layers {
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            knobs: self.knobs.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                fill,
                read,
                constants,
                fills: Vec::new(),
                reader: None,
            })),
        }))
    }
}

struct Parts {
    runtime: ProgramRuntime,
    fill: GraphicsProgram,
    read: GraphicsProgram,
    constants: Vec<wgpu::Buffer>,
    fills: Vec<(u32, PreparedProgramDraw)>,
    reader: Option<PreparedProgramDraw>,
}

struct Layers {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for Layers {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let knobs = self.knobs.lock().unwrap();
        let mut declared = Vec::new();
        for (producer, _) in &knobs.produce {
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, *producer)
                .writing([ColorOutput::Target(produced(*producer))]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        if knobs.read {
            let all = (0..LAYERS).map(|layer| ResourceRef {
                name: ARRAY,
                version: 0,
                mip: 0,
                layer,
            });
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 10)
                .writing([ColorOutput::Frame])
                .reading(all);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(LayersFrame {
            targets: self.targets.clone(),
            camera: self.camera.clone(),
            knobs: self.knobs.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct LayersFrame {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl LayersFrame {
    fn raster(&self) -> Result<RasterState, FrameError> {
        Ok(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        })
    }

    /// Prepares the draw of each producer and of the reader the first time their entries appear.
    fn prepare_draws(&self, context: &PrepareContext<'_>) -> Result<(), FrameError> {
        let produce = self.knobs.lock().unwrap().produce.clone();
        let raster = self.raster()?;
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        for (layer, _) in produce {
            if parts.fills.iter().any(|(known, _)| *known == layer) {
                continue;
            }
            let target = PassTarget::of(context, producer(layer))?;
            let state = target.render_state(raster);
            let imports = [Import::buffer(
                EngineSemantic::named("layers.fill"),
                parts.constants[constants_slot(layer)].clone(),
            )];
            let draw = parts
                .runtime
                .prepare_imported_draw(
                    &ImportedDraw {
                        program: &parts.fill,
                        state: &state,
                        vertex_inputs: &[],
                        bindings: &[],
                        imports: &imports,
                        sets: &[],
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                        instances: 0..1,
                        owners: &[],
                    },
                    target.signature().unwrap(),
                )
                .map_err(|error| common::program_error("layers", &error))?;
            target.admit("layers", &draw)?;
            parts.fills.push((layer, draw));
        }
        if parts.reader.is_none() && self.knobs.lock().unwrap().read {
            let target = PassTarget::of(context, READER)?;
            let texture = self.targets.texture(ARRAY)?;
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let imports = [Import::texture_array(
                EngineSemantic::named("layers.array"),
                texture,
                view,
            )];
            let state = target.render_state(raster);
            let draw = parts
                .runtime
                .prepare_imported_draw(
                    &ImportedDraw {
                        program: &parts.read,
                        state: &state,
                        vertex_inputs: &[],
                        bindings: &[],
                        imports: &imports,
                        sets: &[],
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                        instances: 0..1,
                        owners: &[],
                    },
                    target.signature().unwrap(),
                )
                .map_err(|error| common::program_error("layers", &error))?;
            target.admit("layers", &draw)?;
            parts.reader = Some(draw);
        }
        Ok(())
    }
}

impl PreparedFeature for LayersFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.knobs.lock().unwrap().counts.prepares += 1;
        self.prepare_draws(context)?;
        let produce = self.knobs.lock().unwrap().produce.clone();
        let parts = self.parts.lock().unwrap();
        for (layer, colour) in produce {
            let bytes: Vec<u8> = colour
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            context.upload_buffer(&parts.constants[constants_slot(layer)], 0, &bytes);
        }
        self.knobs.lock().unwrap().stats = Some(parts.runtime.stats());
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        self.knobs
            .lock()
            .unwrap()
            .counts
            .recorded
            .push(context.entry);
        if context.entry == READER {
            parts.reader.as_ref().expect("prepared").record(pass);
        } else {
            let layer = context.entry.occurrence();
            let (_, draw) = parts
                .fills
                .iter()
                .find(|(known, _)| *known == layer)
                .expect("prepared");
            draw.record(pass);
        }
    }
}
