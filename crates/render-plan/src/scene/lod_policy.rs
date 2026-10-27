//! Terrain-normal sampling policy. Existing constructors retain defaults.
/// Terrain normal lookup location, before wind and independent of fold height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneFoldSampling {
    InstanceOrigin,
    Vertex,
}
