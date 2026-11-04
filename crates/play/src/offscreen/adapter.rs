//! The adapter a host calls each frame: it loads and uploads what a snapshot needs, maps the
//! snapshot onto an [`OffscreenFrame`] and builds the frame's operations.
//!
//! Shaders, meshes and stored textures are registered with the [`OffscreenState`] once and again
//! when they change. A shader is registered again when its world shader's source changes, and a
//! mesh or texture when the managed engine's generation of it rises.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use worldrunner_render_program::offscreen::frame::{
    ChannelFormat, Mesh, MeshChannel, MeshIndices, OffscreenFrame, SubMesh,
};
use worldrunner_render_program::offscreen::{OffscreenError, OffscreenOperations, OffscreenState};
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, ShaderId as WorldShaderId, ShaderSource, TextureRef,
};

use super::assets::{Assets, mesh_of, texture_of};
use super::frame::{Adapted, Available, BLIT_MESH, Settings, ShaderInfo, adapt};
use super::ids;
use super::shaders::ShaderLoader;
use super::snapshot::GraphicsSnapshot;

/// What the adapter keeps between frames.
pub struct OffscreenAdapter {
    loader: ShaderLoader,
    settings: Settings,
    shaders: BTreeMap<WorldShaderId, (ShaderSource, ShaderInfo)>,
    meshes: BTreeMap<u64, Stored>,
    textures: BTreeMap<u64, Stored>,
    blit_mesh: bool,
}

/// A stored mesh or texture the adapter has read: the generation, and whether it can be drawn.
#[derive(Clone, Copy)]
struct Stored {
    generation: u64,
    usable: bool,
}

impl OffscreenAdapter {
    /// An adapter that loads shaders from the player's `data_directory`.
    #[must_use]
    pub fn new(data_directory: impl Into<PathBuf>, settings: Settings) -> Self {
        Self {
            loader: ShaderLoader::new(data_directory),
            settings,
            shaders: BTreeMap::new(),
            meshes: BTreeMap::new(),
            textures: BTreeMap::new(),
            blit_mesh: false,
        }
    }

    /// Registers what the snapshot needs with `state`, then maps it. What cannot be loaded or
    /// uploaded is counted in the report's failures and left out of the frame.
    pub fn adapt(
        &mut self,
        snapshot: &GraphicsSnapshot,
        assets: &dyn Assets,
        state: &mut OffscreenState,
        runtime: &mut ProgramRuntime,
    ) -> Adapted {
        let mut failures = BTreeMap::new();
        let mut available = Available::default();
        self.register_shaders(snapshot, state, &mut available, &mut failures);
        let renderers = snapshot.renderers.iter().map(|renderer| renderer.mesh);
        self.register_meshes(renderers.collect(), assets, (state, runtime), &mut failures);
        let wanted = stored_textures(snapshot);
        self.register_textures(&wanted, assets, (state, runtime), &mut failures);
        available.meshes = usable(&self.meshes);
        available.textures = usable(&self.textures);
        if !self.blit_mesh {
            let blit = state.set_mesh(runtime, BLIT_MESH, &blit_mesh());
            self.blit_mesh = blit.is_ok();
        }
        let mut adapted = adapt(snapshot, &available, &self.settings);
        adapted.report.failures.extend(failures);
        adapted
    }

    fn register_shaders(
        &mut self,
        snapshot: &GraphicsSnapshot,
        state: &mut OffscreenState,
        available: &mut Available,
        failures: &mut BTreeMap<String, usize>,
    ) {
        for (id, shader) in &snapshot.shaders {
            let Some(source) = &shader.source else {
                *failures.entry("shader:NoSource".to_owned()).or_default() += 1;
                continue;
            };
            if let Some((held, info)) = self.shaders.get(id)
                && held == source
            {
                available.shaders.insert(*id, *info);
                continue;
            }
            match self.loader.load(source) {
                Ok(loaded) => {
                    let info = ShaderInfo {
                        passes: loaded.passes,
                        queue: loaded.queue,
                    };
                    state.add_shader(ids::shader(*id), loaded.program);
                    self.shaders.insert(*id, (source.clone(), info));
                    available.shaders.insert(*id, info);
                }
                Err(error) => {
                    let name = format!("shader:{}", error.name());
                    *failures.entry(name).or_default() += 1;
                }
            }
        }
    }

    fn register_meshes(
        &mut self,
        identities: BTreeSet<u64>,
        assets: &dyn Assets,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        failures: &mut BTreeMap<String, usize>,
    ) {
        for identity in identities {
            let known = self.meshes.get(&identity).map_or(0, |held| held.generation);
            let stored = match assets.mesh(identity, known) {
                Ok(Some(stored)) => stored,
                Ok(None) => continue,
                Err(_) => {
                    *failures.entry("mesh:Read".to_owned()).or_default() += 1;
                    continue;
                }
            };
            let uploaded = match mesh_of(&stored) {
                Ok(mesh) => state
                    .set_mesh(runtime, ids::mesh(identity), &mesh)
                    .map_err(|error| format!("mesh:{}", variant_name(&error))),
                Err(error) => Err(format!("mesh:{}", variant_name(&error))),
            };
            if let Err(name) = &uploaded {
                *failures.entry(name.clone()).or_default() += 1;
            }
            self.meshes.insert(
                identity,
                Stored {
                    generation: stored.generation,
                    usable: uploaded.is_ok(),
                },
            );
        }
    }

    fn register_textures(
        &mut self,
        identities: &BTreeSet<u64>,
        assets: &dyn Assets,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        failures: &mut BTreeMap<String, usize>,
    ) {
        for &identity in identities {
            let Some(id) = ids::stored_texture(identity) else {
                *failures
                    .entry("texture:IdentityTooLarge".to_owned())
                    .or_default() += 1;
                continue;
            };
            let known = self
                .textures
                .get(&identity)
                .map_or(0, |held| held.generation);
            let stored = match assets.texture(identity, known) {
                Ok(Some(stored)) => stored,
                Ok(None) => continue,
                Err(_) => {
                    *failures.entry("texture:Read".to_owned()).or_default() += 1;
                    continue;
                }
            };
            let uploaded = match texture_of(id, &stored) {
                Ok(texture) => state
                    .set_texture(runtime, &texture)
                    .map_err(|error| format!("texture:{}", variant_name(&error))),
                Err(error) => Err(format!("texture:{}", variant_name(&error))),
            };
            if let Err(name) = &uploaded {
                *failures.entry(name.clone()).or_default() += 1;
            }
            self.textures.insert(
                identity,
                Stored {
                    generation: stored.generation,
                    usable: uploaded.is_ok(),
                },
            );
        }
    }
}

fn usable(stored: &BTreeMap<u64, Stored>) -> BTreeSet<u64> {
    stored
        .iter()
        .filter(|(_, held)| held.usable)
        .map(|(identity, _)| *identity)
        .collect()
}

/// The identities of the stored textures the snapshot's properties and records name.
fn stored_textures(snapshot: &GraphicsSnapshot) -> BTreeSet<u64> {
    let mut found = BTreeSet::new();
    let mut note = |texture: TextureRef| {
        if let TextureRef::Asset(identity) = texture {
            found.insert(identity);
        }
    };
    let properties = snapshot
        .materials
        .values()
        .map(|material| &material.properties)
        .chain(
            snapshot
                .renderers
                .iter()
                .filter_map(|r| r.property_block.as_ref()),
        )
        .chain(snapshot.global.iter().map(|global| &global.properties));
    for properties in properties {
        for (_, slot) in &properties.textures {
            note(slot.texture);
        }
    }
    for work in &snapshot.work {
        match *work {
            OffscreenWork::Blit { source, .. } | OffscreenWork::CopyTexture { source, .. } => {
                note(source);
            }
            OffscreenWork::CameraRender { .. } => {}
        }
    }
    found
}

/// The mesh a blit draws: a quad over clip space as two triangles with no indices, whose texture
/// coordinates are the image's own (row 0 is clip y of -1).
fn blit_mesh() -> Mesh {
    let corners: [(f32, f32); 6] = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (1.0, 1.0),
    ];
    let bytes =
        |values: Vec<f32>| -> Vec<u8> { values.into_iter().flat_map(f32::to_le_bytes).collect() };
    let positions = bytes(corners.iter().flat_map(|&(x, y)| [x, y, 0.0]).collect());
    let coordinates = bytes(
        corners
            .iter()
            .flat_map(|&(x, y)| [f32::midpoint(x, 1.0), f32::midpoint(y, 1.0)])
            .collect(),
    );
    Mesh {
        vertex_count: 6,
        channels: vec![
            MeshChannel {
                source: super::assets::POSITION,
                format: ChannelFormat::Float32x3,
                bytes: positions,
            },
            MeshChannel {
                source: super::assets::TEXTURE_COORDINATES,
                format: ChannelFormat::Float32x2,
                bytes: coordinates,
            },
        ],
        indices: MeshIndices::None,
        sub_meshes: vec![SubMesh {
            first: 0,
            count: 6,
            base_vertex: 0,
        }],
    }
}

/// The name of an error's variant: the leading identifier of its `Debug` form.
fn variant_name(error: &dyn std::fmt::Debug) -> String {
    let text = format!("{error:?}");
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Builds the frame's operations. When the frame as a whole is refused, each record is built
/// alone and the ones that are refused are left out and counted in the report by the name of
/// their error; the rest are built together.
///
/// A record that builds alone can still fail with the records before it, and then the frame's
/// error is returned.
///
/// # Errors
/// Fails with the error of the frame that is left once the records refused alone are removed.
pub fn build(
    state: &mut OffscreenState,
    runtime: &mut ProgramRuntime,
    arena: &ProgramArena,
    adapted: &mut Adapted,
) -> Result<OffscreenOperations, OffscreenError> {
    if let Ok(operations) = state.build(runtime, arena, &adapted.frame) {
        return Ok(operations);
    }
    let records = std::mem::take(&mut adapted.frame.records);
    let mut kept = Vec::new();
    for record in records {
        let alone = OffscreenFrame {
            records: vec![record.clone()],
            ..adapted.frame.clone()
        };
        match state.build(runtime, arena, &alone) {
            Ok(_) => kept.push(record),
            Err(error) => {
                let name = format!("build:{}", variant_name(&error));
                *adapted.report.failures.entry(name).or_default() += 1;
                adapted.report.built = adapted.report.built.saturating_sub(1);
            }
        }
    }
    adapted.frame.records = kept;
    state.build(runtime, arena, &adapted.frame)
}
