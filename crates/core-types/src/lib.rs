use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldId(Uuid);

impl WorldId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for WorldId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WorldId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldSeed(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GenerationVersion(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetCatalogVersion(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldStateHash(pub [u8; 32]);

impl WorldStateHash {
    pub const ZERO: Self = Self([0; 32]);
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WorldDimensions {
    pub width_meters: u32,
    pub depth_meters: u32,
}

impl WorldDimensions {
    pub const fn new(width_meters: u32, depth_meters: u32) -> Self {
        Self {
            width_meters,
            depth_meters,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width_meters == 0 || self.depth_meters == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldPosition {
    pub east_meters: f64,
    pub elevation_meters: f64,
    pub north_meters: f64,
}

impl WorldPosition {
    pub const ORIGIN: Self = Self {
        east_meters: 0.0,
        elevation_meters: 0.0,
        north_meters: 0.0,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_dimensions_are_empty() {
        assert!(WorldDimensions::new(0, 10).is_empty());
        assert!(WorldDimensions::new(10, 0).is_empty());
        assert!(!WorldDimensions::new(10, 10).is_empty());
    }

    #[test]
    fn world_ids_round_trip_through_uuid() {
        let id = WorldId::new();
        assert_eq!(WorldId::from_uuid(id.as_uuid()), id);
    }
}
