//! Scene textures a native world's sprites draw with.
pub mod mesh_instances;
use crate::SceneTexture;
use std::collections::BTreeMap;

/// Content identity to scene texture table. A sprite names its texture by this identity, and the
/// bytes follow the ordinary scene texture contract.
#[derive(Clone, Debug, Default)]
pub struct SpriteTextures {
    pub textures: BTreeMap<u64, SceneTexture>,
}
