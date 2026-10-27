//! Validated geometry streaming bounds for terrain.
use super::SceneValidationError;

/// Geometry selection and allocation limits. Chunks have at most
/// `(chunk_intervals + 1)^2` surface vertices. Every edge carries a skirt down
/// below the heightfield's minimum elevation, covering gaps between unequal
/// LODs. Skirts are double-sided geometry, including at the terrain perimeter.
/// Selection bounds projected cell-axis footprints in screen pixels, with a
/// 1.2 split/merge hysteresis ratio. It does not measure height interpolation residuals.
/// Streaming currently supports forward rendering only; other terrain consumers
/// must reject it until they share resident geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainStreamingPolicy {
    chunk_intervals: u32,
    maximum_resident_chunks: u32,
    uploads_per_frame: u32,
    split_error_millipixels: u32,
}

impl TerrainStreamingPolicy {
    /// Budgets count complete chunks, including skirts. The upload budget must
    /// admit four siblings atomically so refinement never leaves a hole.
    pub fn new(
        chunk_intervals: u32,
        maximum_resident_chunks: u32,
        uploads_per_frame: u32,
        split_error_millipixels: u32,
    ) -> Result<Self, SceneValidationError> {
        if !chunk_intervals.is_power_of_two() || chunk_intervals > 128 {
            return Err(SceneValidationError::UnsupportedInput {
                part: "terrain streaming chunk intervals must be a power of two at most 128",
            });
        }
        if maximum_resident_chunks < 5 || uploads_per_frame < 4 || split_error_millipixels == 0 {
            return Err(SceneValidationError::UnsupportedInput {
                part: "terrain streaming requires five resident slots, four uploads and positive error",
            });
        }
        Ok(Self {
            chunk_intervals,
            maximum_resident_chunks,
            uploads_per_frame,
            split_error_millipixels,
        })
    }
    pub const fn chunk_intervals(self) -> u32 {
        self.chunk_intervals
    }
    pub const fn maximum_resident_chunks(self) -> u32 {
        self.maximum_resident_chunks
    }
    pub const fn uploads_per_frame(self) -> u32 {
        self.uploads_per_frame
    }
    pub const fn split_error_millipixels(self) -> u32 {
        self.split_error_millipixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_rejects_unbounded_or_nonprogressing_configuration() {
        for args in [
            (0, 5, 4, 1),
            (3, 5, 4, 1),
            (256, 5, 4, 1),
            (4, 4, 4, 1),
            (4, 5, 3, 1),
            (4, 5, 4, 0),
        ] {
            assert!(TerrainStreamingPolicy::new(args.0, args.1, args.2, args.3).is_err());
        }
        assert!(TerrainStreamingPolicy::new(16, 64, 8, 2000).is_ok());
    }
}
