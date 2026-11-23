//! The constants of one renderer generation that a game's passes use: neutral one-texel
//! textures, the split-sum BRDF table and the full-screen triangle. The engine owns them, so a
//! game declares none of them, and they are the same for every frame.
mod fill;
mod placeholders;
mod scene_sources;
mod terrain_arrays;

use self::fill::Fill;
use super::lent::Lent;
use super::resources::GameResources;
use std::cell::RefCell;
use std::rc::Rc;
use worldrunner_render_gpu::census::{ResourceClass, Tracked};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_plan::SceneSpecularConvention;
use worldrunner_render_post::PrepareContext;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::{Import, ImportedGeometry};

pub(crate) use scene_sources::SceneSources;
pub use terrain_arrays::{LayerShape, TerrainMaterialArrayError, TerrainMaterialChannel};

/// The vertex stage helpers of the full-screen triangle, to put in front of a program's source:
/// `fullscreen_position(index)` is the clip position and `fullscreen_uv(index)` the texture
/// coordinate (origin at the top left of the frame) of vertex `index` in `0..3`. Draw it with
/// [`EngineConstants::fullscreen_triangle`], which names no vertex buffer.
pub const FULLSCREEN_TRIANGLE_WGSL: &str = "
fn fullscreen_corner(index: u32) -> vec2<f32> {
    return vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
}
fn fullscreen_position(index: u32) -> vec4<f32> {
    return vec4<f32>(fullscreen_corner(index) * 2.0 - 1.0, 0.0, 1.0);
}
fn fullscreen_uv(index: u32) -> vec2<f32> {
    let corner = fullscreen_corner(index);
    return vec2<f32>(corner.x, 1.0 - corner.y);
}
";

/// Texels along each side of the split-sum table.
pub const BRDF_TABLE_SIZE: u32 = 64;
/// Importance samples the engine integrates each texel of the table with.
pub const BRDF_TABLE_SAMPLES: u32 = 256;

/// A neutral one-texel texture. The first three are `Rgba8Unorm` and linear. The stock renderer
/// has a neutral only for a missing material map (white), so the zero placeholders hold zero,
/// which reads as no coverage, no height, no motion or no density in the formats a game binds
/// them as. A signed pair stored in `Rg8Unorm` reads (0, 0), not its midpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnginePlaceholder {
    /// (1, 1, 1, 1): the texel the stock renderer binds where a material omits a map.
    White,
    /// (0, 0, 0, 1).
    Black,
    /// A tangent-space normal pointing straight out of the surface, (0.5, 0.5, 1, 1).
    FlatNormal,
    /// A one-texel `R16Float` texture holding 0.
    ZeroR16Float,
    /// A one-texel `R8Unorm` texture holding 0.
    ZeroR8,
    /// A one-texel `Rg8Unorm` texture holding (0, 0).
    ZeroRg8,
    /// A 1x1x1 `Rg8Unorm` 3D texture holding (0, 0).
    ZeroRg8Volume,
}

impl EnginePlaceholder {
    const ALL: [Self; 7] = [
        Self::White,
        Self::Black,
        Self::FlatNormal,
        Self::ZeroR16Float,
        Self::ZeroR8,
        Self::ZeroRg8,
        Self::ZeroRg8Volume,
    ];

    fn slot(self) -> usize {
        Self::ALL
            .iter()
            .position(|placeholder| *placeholder == self)
            .expect("every placeholder is listed")
    }
}

/// The split-sum table of one convention, and the frame that integrates it.
struct BrdfTable {
    convention: SceneSpecularConvention,
    texture: Tracked<wgpu::Texture>,
    integration: Fill,
}

#[derive(Default)]
struct State {
    placeholders: [Option<Tracked<wgpu::Texture>>; EnginePlaceholder::ALL.len()],
    terrain: scene_sources::TerrainProducts,
    tables: Vec<BrdfTable>,
}

/// The engine constants of one renderer generation. Cloning shares them; they are released with
/// the generation and every feature that holds a clone.
#[derive(Clone)]
pub struct EngineConstants {
    device: wgpu::Device,
    authority: SubmissionAuthority,
    state: Rc<RefCell<State>>,
    /// The scene the frame in flight lends, which terrain products are built from.
    scene: Lent<SceneSources>,
    /// The factory whose runtime ledger and budget the terrain material arrays are charged to.
    charge: Option<GameResources>,
}

impl EngineConstants {
    pub(crate) fn new(device: &wgpu::Device, authority: &SubmissionAuthority) -> Self {
        Self {
            device: device.clone(),
            authority: authority.clone(),
            state: Rc::default(),
            scene: Lent::default(),
            charge: None,
        }
    }

    /// These constants as one factory's feature sees them: the same cache and scene, with the
    /// arrays it asks for charged to `resources`, the factory's own ledger and budget.
    pub(crate) fn charged_to(&self, resources: &GameResources) -> Self {
        Self {
            charge: Some(resources.clone()),
            ..self.clone()
        }
    }

    /// Records the scene the frame in flight draws, and drops the terrain products built from a
    /// scene that is no longer the one the renderer draws.
    pub(crate) fn publish_scene(&self, sources: SceneSources) {
        self.state.borrow_mut().terrain.retain_for(&sources);
        self.scene.publish(sources);
    }

    /// The one-texel texture `which` names, for the binding sourced from `semantic`. It is made
    /// on first use and shared after that.
    pub fn placeholder(&self, semantic: EngineSemantic, which: EnginePlaceholder) -> Import {
        let mut state = self.state.borrow_mut();
        let slot = &mut state.placeholders[which.slot()];
        let texture = slot.get_or_insert_with(|| {
            let texture = which.create(&self.device, self.authority.queue());
            self.authority
                .census()
                .track_texture(ResourceClass::Texture, texture)
        });
        Import::texture(
            semantic,
            (**texture).clone(),
            texture.create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    /// The integrated split-sum BRDF table the stock environment consumers sample, for the
    /// binding sourced from `semantic`: [`BRDF_TABLE_SIZE`] texels square, view cosine along x
    /// and roughness along y, with the scale and bias of the specular colour in red and green.
    /// It is the table the stock environment preparation integrates (default intensity,
    /// [`BRDF_TABLE_SAMPLES`] samples) under `convention`, which is the convention the frame's
    /// scene selects.
    ///
    /// The table of a convention is integrated in the first frame that asks for it, before that
    /// frame's passes draw, and kept for the generation. A frame that is cancelled integrates it
    /// again in the next frame that asks.
    pub fn brdf_table(
        &self,
        context: &mut PrepareContext<'_>,
        semantic: EngineSemantic,
        convention: SceneSpecularConvention,
    ) -> Import {
        let mut state = self.state.borrow_mut();
        let at = state
            .tables
            .iter()
            .position(|table| table.convention == convention)
            .unwrap_or_else(|| {
                state.tables.push(BrdfTable {
                    convention,
                    texture: self.authority.census().track_texture(
                        ResourceClass::Texture,
                        crate::sky::preparation::table_texture(&self.device, BRDF_TABLE_SIZE),
                    ),
                    integration: Fill::default(),
                });
                state.tables.len() - 1
            });
        let table = &mut state.tables[at];
        let frame = context.publications();
        if table.integration.is_needed_in(&frame) {
            crate::sky::preparation::stage_table(
                context,
                &table.texture,
                (convention, BRDF_TABLE_SAMPLES),
            );
            table.integration.stage_in(frame);
        }
        Import::texture(
            semantic,
            (*table.texture).clone(),
            table
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    /// Three vertices that cover the frame: a draw over no vertex buffer, whose vertex stage
    /// reads `@builtin(vertex_index)`. See [`FULLSCREEN_TRIANGLE_WGSL`].
    pub fn fullscreen_triangle() -> ImportedGeometry {
        ImportedGeometry::Vertices { first: 0, count: 3 }
    }
}
