//! The constants of one renderer generation that a game's passes use: neutral one-texel
//! textures, the split-sum BRDF table and the full-screen triangle. The engine owns them, so a
//! game declares none of them, and they are the same for every frame.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use worldrunner_render_gpu::census::{ResourceClass, Tracked};
use worldrunner_render_gpu::submission::{Publications, SubmissionAuthority};
use worldrunner_render_plan::SceneSpecularConvention;
use worldrunner_render_post::PrepareContext;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::{Import, ImportedGeometry};

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

/// A neutral one-texel texture, `Rgba8Unorm` and linear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnginePlaceholder {
    /// (1, 1, 1, 1): the texel the stock renderer binds where a material omits a map.
    White,
    /// (0, 0, 0, 1).
    Black,
    /// A tangent-space normal pointing straight out of the surface, (0.5, 0.5, 1, 1).
    FlatNormal,
}

impl EnginePlaceholder {
    const ALL: [Self; 3] = [Self::White, Self::Black, Self::FlatNormal];

    fn slot(self) -> usize {
        Self::ALL
            .iter()
            .position(|placeholder| *placeholder == self)
            .expect("every placeholder is listed")
    }

    fn texel(self) -> [u8; 4] {
        match self {
            Self::White => [255; 4],
            Self::Black => [0, 0, 0, 255],
            Self::FlatNormal => [128, 128, 255, 255],
        }
    }
}

/// The split-sum table of one convention, and whether the frame that integrates it has
/// submitted.
struct BrdfTable {
    convention: SceneSpecularConvention,
    texture: Tracked<wgpu::Texture>,
    /// Set when a submitted frame has written the table.
    ready: Rc<Cell<bool>>,
    /// The frame that stages the integration, while it has not submitted.
    staged: Option<Publications>,
}

#[derive(Default)]
struct State {
    placeholders: [Option<Tracked<wgpu::Texture>>; 3],
    tables: Vec<BrdfTable>,
}

/// The engine constants of one renderer generation. Cloning shares them; they are released with
/// the generation and every feature that holds a clone.
#[derive(Clone)]
pub struct EngineConstants {
    device: wgpu::Device,
    authority: SubmissionAuthority,
    state: Rc<RefCell<State>>,
}

impl EngineConstants {
    pub(crate) fn new(device: &wgpu::Device, authority: &SubmissionAuthority) -> Self {
        Self {
            device: device.clone(),
            authority: authority.clone(),
            state: Rc::default(),
        }
    }

    /// The one-texel texture `which` names, for the binding sourced from `semantic`. It is made
    /// on first use and shared after that.
    pub fn placeholder(&self, semantic: EngineSemantic, which: EnginePlaceholder) -> Import {
        let mut state = self.state.borrow_mut();
        let slot = &mut state.placeholders[which.slot()];
        let texture = slot.get_or_insert_with(|| {
            let queue = self.authority.queue();
            let texture = match which {
                EnginePlaceholder::White => crate::upload::placeholder_texture(&self.device, queue),
                _ => crate::upload::texel_texture(
                    &self.device,
                    queue,
                    "engine placeholder",
                    which.texel(),
                ),
            };
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
                    ready: Rc::default(),
                    staged: None,
                });
                state.tables.len() - 1
            });
        let table = &mut state.tables[at];
        let frame = context.publications();
        let staged_here = table
            .staged
            .as_ref()
            .is_some_and(|staged| staged.same_frame(&frame));
        if !table.ready.get() && !staged_here {
            crate::sky::preparation::stage_table(
                context,
                &table.texture,
                (convention, BRDF_TABLE_SAMPLES),
            );
            let ready = table.ready.clone();
            context.publish(move || ready.set(true));
            table.staged = Some(frame);
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
