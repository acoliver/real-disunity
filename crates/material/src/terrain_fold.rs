use serde::{Deserialize, Serialize};

/// Application-authored selection for blending an object's shading normals
/// toward its supporting terrain. This does not select a material capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TerrainRequestOverride {
    ForceOff,
    ForceOn,
    Inherit,
}

impl TerrainRequestOverride {
    #[must_use]
    pub const fn resolve(self, object_request: Option<bool>) -> Option<bool> {
        match self {
            Self::ForceOff => Some(false),
            Self::ForceOn => Some(true),
            Self::Inherit => object_request,
        }
    }
}

/// A material's authored support for terrain-normal blending and its request
/// policy. Capability alone never enables blending; an inherited absent request
/// remains absent so the scene composer can reject missing application policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainFoldSource {
    pub provided_capability: bool,
    pub request_override: TerrainRequestOverride,
}
