//! Test-only accessors of this module's types.

impl super::super::SceneGpu {
    /// The scene's celestial meshes, for the captures that switch how they record.
    pub(crate) fn celestial_meshes(&self) -> Option<&crate::sky::meshes::Meshes> {
        self.celestial.as_ref()
    }
}
