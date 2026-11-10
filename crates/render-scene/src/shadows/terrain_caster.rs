//! The terrain's shadow caster on the program runtime.
//!
//! A shadow map has a depth attachment and no colour attachment, and terrain needs no fragment
//! stage to write depth, so the caster is a vertex-only program (`vs_terrain_depth` of the shadow
//! depth source) drawn into a depth-only signature. Its state is the fixed caster's: back faces
//! culled, the configuration's depth test with writes, and its depth bias.
//!
//! The cascade's light frame is set 0, lent by the pass: one pipeline serves every cascade, and
//! the pass binds the cascade's frame group after the pipeline. Recording is then the commands
//! the fixed caster recorded: the pipeline and the frame group once, then per chunk two vertex
//! buffers, the index buffer and the draw.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wgpu::{BindGroupLayout, Device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    ExternalSet, PipelineDraw, PreparedPipeline, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::{DepthState, RasterState, RenderState};
use worldrunner_render_shadow::DirectionalShadowConfig;

use crate::terrain::prepared::vertex_layout;
use crate::terrain::program::Family;

const ENTRY: &str = "vs_terrain_depth";

/// The terrain's depth-only draw for one caster configuration.
pub(crate) struct TerrainCaster {
    family: Family,
    device: Device,
    authority: SubmissionAuthority,
    frame: BindGroupLayout,
    state: RenderState,
    made: RefCell<Option<Rc<PreparedPipeline>>>,
}

impl std::fmt::Debug for TerrainCaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerrainCaster")
            .field("made", &self.made.borrow().is_some())
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
    /// A caster for `configuration` that makes its pipeline on the runtime of `family`, from
    /// `authority`, when a cascade first needs it. The cascades' frame groups are built against
    /// `frame`, the scene's frame layout, which the program lends as set 0.
    pub(crate) fn new(
        configuration: &DirectionalShadowConfig,
        family: &Family,
        (device, authority): (&Device, &SubmissionAuthority),
        frame: &BindGroupLayout,
    ) -> Self {
        Self {
            family: family.clone(),
            device: device.clone(),
            authority: authority.clone(),
            frame: frame.clone(),
            state: state(configuration),
            made: RefCell::default(),
        }
    }

    /// The one pipeline every cascade draws with, made on first use. The pass binds the
    /// cascade's frame group at set 0 after it.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program or the pipeline.
    pub(crate) fn pipeline(&self) -> Result<Rc<PreparedPipeline>, ProgramError> {
        if let Some(pipeline) = self.made.borrow().clone() {
            return Ok(pipeline);
        }
        let made = Rc::new(
            self.family
                .with((&self.device, &self.authority), |runtime| {
                    self.make(runtime)
                })?,
        );
        *self.made.borrow_mut() = Some(made.clone());
        Ok(made)
    }

    fn make(&self, runtime: &mut ProgramRuntime) -> Result<PreparedPipeline, ProgramError> {
        let source: Arc<str> = super::resources::depth_source().into();
        let vertex = StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(source),
                entry_point: ENTRY.to_owned(),
                kind: StageKind::Vertex,
            },
            StageDeclarations::default(),
        )?;
        let frame = ExternalSet {
            set: 0,
            layout: self.frame.clone(),
            bindings: crate::terrain::program::frame_bindings(),
        };
        let program = runtime.vertex_only_program_in(&vertex, &[frame])?;
        runtime.prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &self.state,
                layout: &vertex_layout(),
                bindings: &[],
                imports: &[],
                sets: &[],
                owners: &[],
            },
            TargetSignature::depth_only(worldrunner_render_shadow::DEPTH_FORMAT, 1)?,
        )
    }
}
