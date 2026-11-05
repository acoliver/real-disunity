//! Main-camera terrain coverage and replacement work, frozen on submission.
use crate::terrain::streaming::{TerrainChunk, TerrainSelectionObservation};

/// Nonstreaming terrain has one permanently resident authored mesh and no leaf policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainGeometryStatistics {
    Static,
    Streaming(TerrainStreamingStatistics),
}

/// FNV-1a-128 of sorted sample windows (minimum x/y, maximum x/y, stride),
/// encoded as little-endian u32s. Includes every drawn leaf, without truncation.
/// This is a deterministic comparison fingerprint, not a cryptographic content hash.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainLeafIdentity {
    pub count: usize,
    pub fingerprint: u128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainStreamingStatistics {
    /// Desired terminal leaves from the streaming selector, including merge hysteresis.
    pub selected_leaves: usize,
    /// Drawn leaves at selected detail or finer, including fine coverage awaiting a merge.
    pub drawn_at_detail: usize,
    /// Drawn resident ancestors of selected leaves, not their desired descendants.
    pub drawn_coarse: usize,
    /// Resident leaves used by this frame, including uploads ordered before its draws.
    /// Retired allocations are excluded.
    pub resident_leaves: usize,
    /// Selected leaves not yet built/uploaded, including budget-blocked merges.
    /// Work is synchronous and budgeted per frame; this is not a worker queue size.
    pub unresolved_leaves: usize,
    /// Every selected leaf is admitted; no split or merge replacement remains unresolved.
    pub complete: bool,
    pub drawn: TerrainLeafIdentity,
}

pub(crate) fn measure(
    drawn: &[TerrainChunk],
    observation: TerrainSelectionObservation,
) -> TerrainStreamingStatistics {
    TerrainStreamingStatistics {
        selected_leaves: observation.selected_leaves,
        drawn_at_detail: observation.drawn_at_detail,
        drawn_coarse: observation.drawn_coarse,
        resident_leaves: drawn.len(),
        unresolved_leaves: observation.unresolved_leaves,
        complete: observation.unresolved_leaves == 0,
        drawn: identity(drawn),
    }
}

fn identity(drawn: &[TerrainChunk]) -> TerrainLeafIdentity {
    let mut sorted = drawn.to_vec();
    sorted.sort_unstable();
    let mut fingerprint = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for leaf in sorted {
        for value in [
            leaf.minimum[0],
            leaf.minimum[1],
            leaf.maximum[0],
            leaf.maximum[1],
            leaf.stride,
        ] {
            for byte in value.to_le_bytes() {
                fingerprint ^= u128::from(byte);
                fingerprint = fingerprint.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
            }
        }
    }
    TerrainLeafIdentity {
        count: drawn.len(),
        fingerprint,
    }
}

impl super::SceneGpu {
    pub(super) fn drawing_geometry_statistics(&self) -> Option<TerrainGeometryStatistics> {
        self.terrain.as_ref().map(|terrain| {
            terrain
                .streaming
                .as_ref()
                .map_or(TerrainGeometryStatistics::Static, |streaming| {
                    TerrainGeometryStatistics::Streaming(streaming.drawing_statistics())
                })
        })
    }
}
