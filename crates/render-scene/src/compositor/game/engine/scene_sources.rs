//! What the engine builds terrain resources from: the scene's resident textures and its terrain,
//! as the frame in flight lends them, and the products kept until either changes.
use super::terrain_arrays::MaterialArray;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use worldrunner_render_plan::SceneTerrain;

/// The resident scene a frame lends the engine. Both parts are shared snapshots: the texture
/// table is replaced when a scene append or texture replacement changes it, and the terrain is
/// the scene's own shared terrain.
#[derive(Clone, Debug)]
pub(crate) struct SceneSources {
    pub(crate) textures: Rc<[wgpu::Texture]>,
    pub(crate) terrain: Option<Arc<SceneTerrain>>,
}

/// Which snapshots the cached products were built from. It holds them weakly, so a replaced
/// scene is not kept alive by the cache, and a dead snapshot's address is never reused while
/// the key names it.
struct SourceKey {
    textures: Weak<[wgpu::Texture]>,
    terrain: Option<std::sync::Weak<SceneTerrain>>,
}

impl SourceKey {
    fn of(sources: &SceneSources) -> Self {
        Self {
            textures: Rc::downgrade(&sources.textures),
            terrain: sources.terrain.as_ref().map(Arc::downgrade),
        }
    }

    fn same_textures(&self, sources: &SceneSources) -> bool {
        Weak::ptr_eq(&self.textures, &Rc::downgrade(&sources.textures))
    }

    fn same_terrain(&self, sources: &SceneSources) -> bool {
        match (&self.terrain, &sources.terrain) {
            (None, None) => true,
            (Some(key), Some(terrain)) => std::sync::Weak::ptr_eq(key, &Arc::downgrade(terrain)),
            _ => false,
        }
    }
}

/// The products built from the scene's terrain, shared by every feature of the generation. A
/// material array depends on the terrain's surface table and the texture table.
#[derive(Default)]
pub(super) struct TerrainProducts {
    key: Option<SourceKey>,
    pub(super) arrays: Vec<MaterialArray>,
}

impl TerrainProducts {
    /// Drops what `sources` no longer match, so its resident bytes leave the census with the
    /// scene that they were built from.
    pub(super) fn retain_for(&mut self, sources: &SceneSources) {
        let Some(key) = &self.key else {
            self.key = Some(SourceKey::of(sources));
            return;
        };
        if key.same_terrain(sources) && key.same_textures(sources) {
            return;
        }
        self.arrays.clear();
        self.key = Some(SourceKey::of(sources));
    }
}
