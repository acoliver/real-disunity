//! Read-only world views and entity handles.
//!
//! The engine implements [`WorldView`]; scripts see it only through
//! generation-stamped handles, so a stale handle is a caught Lua error
//! rather than undefined behavior.

/// A script-visible entity reference. `generation` matches the world
/// generation that produced it; the host rejects handles whose generation is
/// not the live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityHandle {
    pub id: u64,
    pub generation: u64,
}

/// Read-only world queries available to scripts. No mutation surface exists
/// in v0: scripts observe, the engine acts.
pub trait WorldView: Send + Sync {
    /// Terrain height at a world position, if the position is on the map.
    fn terrain_height(&self, east: f64, north: f64) -> Option<f64>;
    /// Entity handles for every entity in a family.
    fn entities_by_family(&self, family: &str) -> Vec<EntityHandle>;
    /// The family an entity belongs to, if the handle is still live.
    fn entity_family(&self, handle: EntityHandle) -> Option<String>;
    /// The current world generation; handles from older generations are dead.
    fn generation(&self) -> u64;
}
