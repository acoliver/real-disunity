//! The terrain's shadow caster on the program runtime.
//!
//! A shadow map has a depth attachment and no colour attachment, and terrain needs no fragment
//! stage to write depth, so the caster is a vertex-only program (`vs_terrain_depth` of the shadow
//! depth source) drawn into a depth-only signature. Its state is the fixed caster's: back faces
//! culled, the configuration's depth test with writes, and its depth bias.
//!
//! The cascade's light frame is the program's only binding, set 0, and is lent as a buffer: the
//! runtime builds one set for each light frame buffer the scene draws a cascade with, once, and
//! records the same chunk geometry the forward draw does. Recording is then the commands the fixed
//! caster recorded: the pipeline and the frame group once, then per chunk two vertex buffers, the
//! index buffer and the draw.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wgpu::{Buffer, Device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportTable, Keepalive, PipelineDraw, PreparedPipeline,
    ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::{DepthState, RasterState, RenderState};
use worldrunner_render_shadow::DirectionalShadowConfig;

use crate::terrain::prepared::vertex_layout;
use crate::terrain::program::Family;

/// The engine semantic the cascade's light frame fills.
const FRAME: &str = "scene.frame";
const ENTRY: &str = "vs_terrain_depth";

/// The pipelines a terrain caster has made, one for each light frame buffer it draws with.
#[derive(Default)]
struct Made {
    program: Option<GraphicsProgram>,
    cascades: Vec<(Buffer, Rc<PreparedPipeline>)>,
}

/// The terrain's depth-only draw for one caster configuration.
pub(crate) struct TerrainCaster {
    family: Family,
    device: Device,
    authority: SubmissionAuthority,
    state: RenderState,
    made: RefCell<Made>,
}

impl std::fmt::Debug for TerrainCaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerrainCaster")
            .field("cascades", &self.made.borrow().cascades.len())
            .finish_non_exhaustive()
    }
}

/// The render state of a terrain caster: back faces culled, the configuration's depth test with
/// writes and its bias, and no colour.
pub(crate) fn state(configuration: &DirectionalShadowConfig) -> RenderState {
    RenderState {
        raster: RasterState {
            cull: Some(wgpu::Face::Back),
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        },
        depth: Some(DepthState {
            test: configuration.caster_compare,
            write: true,
            bias: configuration.caster_bias,
        }),
        colors: Vec::new(),
        stencil: None,
    }
}

impl TerrainCaster {
    /// A caster for `configuration` that makes its pipelines on the runtime of `family`, from
    /// `authority`, when a cascade first needs one.
    pub(crate) fn new(
        configuration: &DirectionalShadowConfig,
        family: &Family,
        (device, authority): (&Device, &SubmissionAuthority),
    ) -> Self {
        Self {
            family: family.clone(),
            device: device.clone(),
            authority: authority.clone(),
            state: state(configuration),
            made: RefCell::default(),
        }
    }

    /// Makes the pipeline of every light frame buffer in `frames` that has none, before the pass
    /// that draws with it opens.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program, the set or the pipeline.
    pub(crate) fn prepare<'a>(
        &self,
        frames: impl IntoIterator<Item = &'a Buffer>,
    ) -> Result<(), ProgramError> {
        for frame in frames {
            self.pipeline(frame)?;
        }
        Ok(())
    }

    /// The pipeline that draws a cascade whose light frame is `frame`, made on first use and
    /// found in a list afterwards.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program, the set or the pipeline.
    pub(crate) fn pipeline(&self, frame: &Buffer) -> Result<Rc<PreparedPipeline>, ProgramError> {
        let known = self
            .made
            .borrow()
            .cascades
            .iter()
            .find(|(buffer, _)| buffer == frame)
            .map(|(_, pipeline)| pipeline.clone());
        if let Some(pipeline) = known {
            return Ok(pipeline);
        }
        let made = Rc::new(
            self.family
                .with((&self.device, &self.authority), |runtime| {
                    self.make(runtime, frame)
                })?,
        );
        self.made
            .borrow_mut()
            .cascades
            .push((frame.clone(), made.clone()));
        Ok(made)
    }

    fn make(
        &self,
        runtime: &mut ProgramRuntime,
        frame: &Buffer,
    ) -> Result<PreparedPipeline, ProgramError> {
        let program = self.program(runtime)?;
        let owners: Vec<Keepalive> = vec![Arc::new(frame.clone())];
        let table =
            ImportTable::new().with(Import::buffer(EngineSemantic::named(FRAME), frame.clone()));
        let set = runtime.prepare_set(&program, 0, &table, owners.clone())?;
        runtime.prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &self.state,
                layout: &vertex_layout(),
                bindings: &[],
                imports: &[],
                sets: &[&set],
                owners: &owners,
            },
            TargetSignature::depth_only(worldrunner_render_shadow::DEPTH_FORMAT, 1)?,
        )
    }

    /// The vertex-only program, built on first use.
    fn program(&self, runtime: &ProgramRuntime) -> Result<GraphicsProgram, ProgramError> {
        if let Some(program) = self.made.borrow().program.clone() {
            return Ok(program);
        }
        let source: Arc<str> = super::resources::depth_source().into();
        let frame = ResourceBinding {
            at: BindingAddress { set: 0, binding: 0 },
            source: BindingSource::Engine(EngineSemantic::named(FRAME)),
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            kind: ResourceKind::Uniform {
                min_size: crate::frame::FRAME_UNIFORM_BYTES as u64,
            },
        };
        let vertex = StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(source),
                entry_point: ENTRY.to_owned(),
                kind: StageKind::Vertex,
            },
            StageDeclarations {
                resources: ResourceLayout::new(vec![frame]),
                constants: Vec::new(),
            },
        )?;
        let program = runtime.vertex_only_program(&vertex)?;
        self.made.borrow_mut().program = Some(program.clone());
        Ok(program)
    }
}
