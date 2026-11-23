//! The UI frame: the runs of UI draws that a game's shader draws, as offscreen work.
//!
//! A run (see `UiLayers`) is drawn into its rectangle of the layer atlas by the game's own
//! program, after the frame's own work, and the viewer's overlay shows that rectangle at the sort
//! position of the run's first draw. The work is the world's own offscreen language, so the
//! adapter maps it with the code that maps every other frame: the shader, the material's values
//! and the built-in values such as the time are the ones a world frame has.
//!
//! The atlas is a render texture of the frame with an identity no world render texture has. Each
//! run is one mesh of the frame's host assets with a sub-mesh for each span of consecutive draws
//! that share a material and a texture. The mesh has the position, colour and first texture
//! coordinate channels of an engine mesh, so a UI shader reads them as it reads a canvas's.
//! A draw is a quad of four vertices, or a triangle of three.

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};
use worldrunner_disunity_runtime::{MeshChannel, MeshChannels, MeshRange};
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, OffscreenWork, RenderTextureId, ShaderProperties, TargetRef, TextureRef,
    TextureSlot, WorldRenderTexture, WorldSampler, dimension,
};
use worldrunner_world_simulation::ui::{
    UiTexture,
    presentation::{UiDraw, UiLayers, UiPresentation, UiRun},
};

use super::assets::{AssetError, Assets};
use crate::player_textures::FIRST_PLAYER_TEXTURE;
use worldrunner_disunity_runtime::{PlayerTexture, TextureSampler};

/// The slot a world never gives a render texture, with the generation 1 of a first one.
const ATLAS_BITS: u64 = (1 << 32) | 0xFFFF_FFFF;
/// The identity of the first run's mesh. Engine meshes are named from `1 << 41` up.
const FIRST_MESH: u64 = 1 << 62;
/// The `GraphicsFormat` of 8-bit RGBA in sRGB, the format of every screen.
const ATLAS_FORMAT: u32 = 4;
/// The vertex channel numbers of the colour and of the first texture coordinate set.
const COLOUR: u32 = 3;
const TEXTURE_COORDINATES: u32 = 4;
/// The property a UI shader samples its draw's image through.
const MAIN_TEXTURE: &str = "_MainTex";

/// The render texture the runs are drawn into.
#[must_use]
pub fn atlas_id() -> RenderTextureId {
    RenderTextureId::from_bits(ATLAS_BITS)
}

/// The work of one frame's runs, and the meshes it draws.
#[derive(Clone, Debug)]
pub struct UiFrame {
    work: Vec<OffscreenWork>,
    atlas: WorldRenderTexture,
    meshes: BTreeMap<u64, MeshChannels>,
}

impl UiFrame {
    /// The work that draws the runs of `layers`, or `None` for a presentation with none.
    #[must_use]
    pub fn new(plan: &UiPresentation, layers: &UiLayers) -> Option<Self> {
        if layers.runs.is_empty() {
            return None;
        }
        let [width, height] = layers.atlas_size;
        let mut work = vec![
            OffscreenWork::SetRenderTarget {
                color: vec![TargetRef::Render(atlas_id())],
                depth: None,
                mip: 0,
                face: -1,
                slice: -1,
            },
            OffscreenWork::ClearRenderTarget {
                color: true,
                depth: false,
                stencil: false,
                value: [0.0; 4],
                depth_value: 1.0,
                stencil_value: 0,
            },
        ];
        let mut meshes = BTreeMap::new();
        for (number, run) in (0_u64..).zip(&layers.runs) {
            let identity = FIRST_MESH + number;
            meshes.insert(identity, mesh_of(plan, run));
            work.extend(run_work(plan, run, identity));
        }
        Some(Self {
            work,
            atlas: WorldRenderTexture {
                width,
                height,
                format: ATLAS_FORMAT,
                depth_bits: 0,
                mip_count: 1,
                auto_generate_mips: false,
                srgb: true,
                dimension: dimension::TEX_2D,
                volume_depth: 1,
                sampler: WorldSampler::default(),
                generation: 0,
            },
            meshes,
        })
    }

    /// The size of the atlas the work draws into, in pixels.
    #[must_use]
    pub fn atlas_size(&self) -> [u32; 2] {
        [self.atlas.width, self.atlas.height]
    }

    /// Adds the atlas and the work to `snapshot`, after the work the game recorded.
    pub fn join(&self, snapshot: &mut GraphicsSnapshot) {
        snapshot.render_textures.insert(atlas_id(), self.atlas);
        snapshot.work.extend(self.work.iter().cloned());
    }

    /// The assets of the frame: the run meshes, and `host`'s for everything else.
    #[must_use]
    pub fn assets<'a>(&'a self, host: &'a dyn Assets) -> UiAssets<'a> {
        UiAssets {
            host,
            meshes: &self.meshes,
        }
    }
}

/// The records of one run: the view that maps its rectangle of the viewport to its rectangle of
/// the atlas, and one draw for each span.
fn run_work(plan: &UiPresentation, run: &UiRun, mesh: u64) -> Vec<OffscreenWork> {
    let [x, y, width, height] = glam::UVec4::from(run.bounds).as_vec4().to_array();
    let projection = Mat4::from_translation(Vec3::new(-1.0, -1.0, 0.0))
        * Mat4::from_scale(Vec3::new(2.0 / width, 2.0 / height, -1.0))
        * Mat4::from_translation(Vec3::new(-x, -y, 0.0));
    let mut work = vec![
        OffscreenWork::SetViewProjection {
            view: Mat4::IDENTITY.to_cols_array(),
            projection: projection.to_cols_array(),
        },
        OffscreenWork::SetViewport {
            rect: glam::UVec4::from(run.atlas).as_vec4().to_array(),
        },
    ];
    for (submesh, span) in (0_u32..).zip(&run.spans) {
        work.push(OffscreenWork::DrawMesh {
            mesh,
            matrix: Mat4::IDENTITY.to_cols_array(),
            material: span.material.id,
            submesh,
            pass: -1,
            block: main_texture(plan.draws[span.draws.start].texture).map(Box::new),
            instances: 1,
        });
    }
    work
}

/// The block that gives a draw its image, when it has one the player stored.
fn main_texture(texture: Option<UiTexture>) -> Option<ShaderProperties> {
    let Some(UiTexture::Image(identity)) = texture else {
        return None;
    };
    let identity = Some(identity).filter(|identity| *identity >= FIRST_PLAYER_TEXTURE)?;
    Some(ShaderProperties {
        textures: vec![(
            MAIN_TEXTURE.to_owned(),
            TextureSlot {
                texture: TextureRef::Asset(identity),
                ..TextureSlot::default()
            },
        )],
        ..ShaderProperties::default()
    })
}

/// The mesh of a run: the draws of each span in the order the plan has them, a sub-mesh for each
/// span. A UI quad winds clockwise from its lower left corner, the engine's front face.
fn mesh_of(plan: &UiPresentation, run: &UiRun) -> MeshChannels {
    let mut vertices = Vertices::default();
    let mut indices = Vec::new();
    let mut ranges = Vec::new();
    for span in &run.spans {
        let start = u32::try_from(indices.len()).unwrap_or(u32::MAX);
        for draw in &plan.draws[span.draws.clone()] {
            vertices.add(draw, &mut indices);
        }
        ranges.push(MeshRange {
            start,
            count: u32::try_from(indices.len()).unwrap_or(u32::MAX) - start,
            base_vertex: 0,
            topology: 0,
        });
    }
    MeshChannels {
        generation: vertices.generation(&indices),
        vertex_count: vertices.count,
        channels: vec![
            MeshChannel {
                number: 0,
                dimension: 3,
                data: vertices.positions,
            },
            MeshChannel {
                number: COLOUR,
                dimension: 4,
                data: vertices.colours,
            },
            MeshChannel {
                number: TEXTURE_COORDINATES,
                dimension: 2,
                data: vertices.coordinates,
            },
        ],
        indices,
        ranges,
    }
}

#[derive(Default)]
struct Vertices {
    count: u32,
    positions: Vec<f32>,
    colours: Vec<f32>,
    coordinates: Vec<f32>,
}

impl Vertices {
    /// Adds the vertices of `draw` and the indices that wind its triangles clockwise.
    fn add(&mut self, draw: &UiDraw, indices: &mut Vec<u32>) {
        let first = self.count;
        if let Some(uv) = draw.triangle_uv {
            for corner in 0..3 {
                let colour = draw
                    .corner_colors
                    .map_or(draw.color, |colours| modulate(draw.color, colours[corner]));
                self.push(draw.corners[corner], colour, uv[corner]);
            }
            indices.extend([first, first + 1, first + 2]);
            return;
        }
        let [u0, v0, u1, v1] = draw.uv_rect;
        // Corners run lower left, lower right, upper right, upper left; the rectangle's
        // v counts from the top row.
        let uv = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
        for corner in 0..4 {
            let colour = draw
                .corner_colors
                .map_or(draw.color, |colours| modulate(draw.color, colours[corner]));
            self.push(draw.corners[corner], colour, uv[corner]);
        }
        // Lower left, upper left, upper right; upper right, lower right, lower left.
        indices.extend([first, first + 3, first + 2, first + 2, first + 1, first]);
    }

    /// One vertex. The texture coordinates of the plan count from the top row; the engine's
    /// count from the bottom row.
    fn push(&mut self, position: [f32; 3], colour: [f32; 4], uv: [f32; 2]) {
        self.count += 1;
        self.positions.extend(position);
        self.colours.extend(colour);
        self.coordinates.extend([uv[0], 1.0 - uv[1]]);
    }

    /// A number that differs whenever the mesh does, so that the adapter uploads the mesh again
    /// when its draws change and keeps the upload while they do not. Never zero, which is the
    /// generation of a mesh no frame has seen.
    fn generation(&self, indices: &[u32]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for value in self
            .positions
            .iter()
            .chain(&self.colours)
            .chain(&self.coordinates)
        {
            value.to_bits().hash(&mut hasher);
        }
        indices.hash(&mut hasher);
        hasher.finish().max(1)
    }
}

fn modulate(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|channel| left[channel] * right[channel])
}

/// The assets of a UI frame: its run meshes, and the host's meshes and textures for the rest.
pub struct UiAssets<'a> {
    host: &'a dyn Assets,
    meshes: &'a BTreeMap<u64, MeshChannels>,
}

impl Assets for UiAssets<'_> {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        match self.meshes.get(&identity) {
            Some(mesh) => Ok((mesh.generation != known).then(|| mesh.clone())),
            None => self.host.mesh(identity, known),
        }
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        self.host.texture(identity, known)
    }

    fn sampler(&self, identity: u64) -> Result<Option<TextureSampler>, AssetError> {
        self.host.sampler(identity)
    }
}
