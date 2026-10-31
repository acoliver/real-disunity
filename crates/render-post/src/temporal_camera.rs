//! Uniform block of the camera term of the temporal reconstruction stage.
use crate::TaaUniform;

/// Size in bytes of the [`TemporalCamera`] block, a `mat4x4<f32>` and a `vec4<f32>`.
pub const TEMPORAL_CAMERA_BYTES: usize = 80;

/// The camera motion between two frames, the second uniform of `taa_upscale.wgsl`. It is a
/// separate buffer from the [`crate::TemporalBlock`], so that block keeps its layout. Layout,
/// little-endian f32, matching the `Camera` struct in `taa_upscale.wgsl`:
///
/// | bytes   | content |
/// |---------|---------|
/// | 0..64   | `reprojection`, column-major `mat4x4<f32>` |
/// | 64..68  | `valid`: 1.0 when the camera term applies, 0.0 for no camera motion |
/// | 68..72  | depth convention: 0.0 is standard Z (device depth 0 near, 1 far), the only one so far |
/// | 72..80  | `depth_terms` `a`, `b`: linear view depth of device depth `z` is `b / (z - a)` |
///
/// The reprojection is the one native TAA uses: previous unjittered view-projection times the
/// inverse of the current one, in f64 on the CPU, mapping (NDC x, NDC y, device depth, 1) of the
/// current frame to the previous clip space. An all-zero block, which is what a slot holds until
/// its first upload, has `valid` 0.0: the stage then applies the object residual alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalCamera {
    pub reprojection: [[f64; 4]; 4],
    pub depth_terms: [f64; 2],
    pub valid: bool,
}
impl TemporalCamera {
    /// No camera motion: the residual alone is the total motion.
    pub const NONE: Self = Self {
        reprojection: [[0.0; 4]; 4],
        depth_terms: [0.0; 2],
        valid: false,
    };

    /// The block as bytes. A matrix or depth term that is not finite after the f32 narrowing
    /// uploads as [`Self::NONE`], never as NaN.
    pub fn to_bytes(self) -> [u8; TEMPORAL_CAMERA_BYTES] {
        let matrix = self.reprojection.iter().flatten().map(|v| narrow(*v));
        let tail = [
            1.0,
            0.0,
            narrow(self.depth_terms[0]),
            narrow(self.depth_terms[1]),
        ];
        let values: Vec<f32> = matrix.chain(tail).collect();
        let usable = self.valid && values.iter().all(|v| v.is_finite());
        let mut bytes = [0; TEMPORAL_CAMERA_BYTES];
        if usable {
            for (chunk, value) in bytes.chunks_exact_mut(4).zip(values) {
                chunk.copy_from_slice(&value.to_le_bytes());
            }
        }
        bytes
    }
}

/// The camera term of a native TAA block: the same reprojection, depth terms and validity, so
/// the temporal stage reuses the producer of the native resolve and keeps no second camera clock.
impl From<&TaaUniform> for TemporalCamera {
    fn from(uniform: &TaaUniform) -> Self {
        Self {
            reprojection: uniform.reprojection,
            depth_terms: uniform.depth_terms,
            valid: uniform.history_valid,
        }
    }
}

#[expect(clippy::cast_possible_truncation, reason = "the GPU block is f32")]
fn narrow(v: f64) -> f32 {
    v as f32
}
