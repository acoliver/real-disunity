//! A runtime, an arena, offscreen state and the helpers that build frames and read results.

use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_program::ids::{MeshId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{
    OffscreenFrame, RenderDimension, RenderTextureSpec, SourceTexture,
};
use worldrunner_render_program::unity::offscreen::{OffscreenError, OffscreenState};
use worldrunner_render_program::unity::runtime::{ProgramArena, ProgramRuntime};

use crate::common::Gpu;
pub use worldrunner_render_program::unity::synthetic::shaders::{blit_quad, quad};

pub const RGBA8: u32 = 8;
pub const BUDGET: u64 = 64 << 20;

pub const RED: [u8; 4] = [255, 0, 0, 255];
pub const GREEN: [u8; 4] = [0, 255, 0, 255];
pub const BLUE: [u8; 4] = [0, 0, 255, 255];
pub const CYAN: [u8; 4] = [0, 255, 255, 255];
pub const ZERO: [u8; 4] = [0, 0, 0, 0];
pub const CLEAR: [u8; 4] = [255, 255, 0, 255];

pub struct Rig {
    pub gpu: Gpu,
    pub runtime: ProgramRuntime,
    pub arena: ProgramArena,
    pub state: OffscreenState,
}

impl Rig {
    pub fn new() -> Self {
        let gpu = Gpu::new();
        let runtime = gpu.runtime_with(AllocationBudget(BUDGET));
        let limits = gpu.device.limits();
        let mut renderer = Configuration::new(
            &gpu.authority,
            (),
            AllocationSet::default(),
            &limits,
            AllocationBudget(1 << 30),
        )
        .unwrap();
        let reservation = renderer
            .reserve::<()>(&limits, ProgramRuntime::arena_allocations(BUDGET))
            .unwrap();
        let arena = runtime.open_arena(reservation).unwrap();
        let state = OffscreenState::new(&runtime);
        Self {
            gpu,
            runtime,
            arena,
            state,
        }
    }

    pub fn build(
        &mut self,
        frame: &OffscreenFrame,
    ) -> Result<worldrunner_render_program::unity::offscreen::OffscreenOperations, OffscreenError>
    {
        self.state.build(&mut self.runtime, &self.arena, frame)
    }

    /// Builds the frame, reads back `reads` after its operations, runs it as the one submission
    /// of a frame and returns the bytes of each read.
    pub fn run(
        &mut self,
        frame: &OffscreenFrame,
        reads: &[(TextureId, u32, u32)],
    ) -> Result<Vec<Vec<u8>>, OffscreenError> {
        let mut operations = self.build(frame)?;
        let pending: Vec<_> = reads
            .iter()
            .map(|&(texture, mip, layer)| operations.read_back(texture, mip, layer))
            .collect::<Result<_, _>>()?;
        let authority = self.gpu.authority.clone();
        let mut encoder = authority.begin(&self.gpu.device).unwrap();
        let admitted = operations.admit(&encoder)?;
        admitted.encode(None, &mut encoder)?;
        encoder.finish().submit().unwrap();
        Ok(pending
            .into_iter()
            .map(|image| image.bytes(&self.gpu.device))
            .collect())
    }
}

pub fn texture(id: u64, size: [u32; 2]) -> RenderTextureSpec {
    RenderTextureSpec {
        id: TextureId(id),
        size,
        dimension: RenderDimension::D2,
        graphics_format: RGBA8,
        depth_bits: 0,
        mips: 1,
        srgb: false,
        generation: 0,
    }
}

/// The texel at column `x`, row `y` of `bytes`, a tightly packed RGBA8 image `width` across.
pub fn pixel(bytes: &[u8], width: u32, (x, y): (u32, u32)) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    bytes[at..at + 4].try_into().unwrap()
}

pub fn vector(value: [f32; 4]) -> PropertyValue {
    PropertyValue::Vector(value)
}

pub fn layer(values: &[(&str, [f32; 4])]) -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    for (name, value) in values {
        layer.values.set(name, vector(*value));
    }
    layer
}

pub const QUAD: MeshId = MeshId(1);
pub const BLIT_QUAD: MeshId = MeshId(2);
pub const NEAR_QUAD: MeshId = MeshId(3);

/// A `w` by `h` RGBA8 image whose texel at (x, y) is `color(x, y)`.
pub fn image(size: [u32; 2], color: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    (0..size[1])
        .flat_map(|y| (0..size[0]).flat_map(move |x| [(x, y)]))
        .flat_map(|(x, y)| color(x, y))
        .collect()
}

pub fn source(id: u64, size: [u32; 2], bytes: Vec<u8>) -> SourceTexture {
    SourceTexture {
        id: TextureId(id),
        size,
        graphics_format: RGBA8,
        bytes,
        sampler: None,
    }
}
