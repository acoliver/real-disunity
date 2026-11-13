//! GPU forms of the scene skinning contract.
//!
//! A [`SceneSkinPose`](worldrunner_render_plan::SceneSkinPose) palette entry is the whole
//! skinning transform of one joint: the rest pose is all identity and draws the bind mesh, so
//! the inverse binds of the skeleton are already folded in and the GPU never reads them.

use worldrunner_render_plan::{SceneAffineMatrix, SceneSkinInfluence};

use crate::geometry::{micro_to_unit, millimeter_to_meter};

/// Bytes of one palette entry on the GPU: three `vec4<f32>` rows.
pub(crate) const PALETTE_ENTRY_BYTES: u64 = 48;

/// One vertex's influences as the `Uint32x2` skin word.
///
/// The four joint indices fill `x` and the four normalized weights fill `y`, one byte per lane
/// in lane order, exactly as the contract stores them. A `y` of zero marks an explicitly
/// unweighted vertex, which keeps its bind position.
pub(crate) fn skin_word(influence: SceneSkinInfluence) -> [u32; 2] {
    [
        u32::from_le_bytes(influence.joints),
        u32::from_le_bytes(influence.weights),
    ]
}

/// One palette entry as three rows of a row-major 3x4 matrix in metres.
pub(crate) fn palette_rows(matrix: &SceneAffineMatrix) -> [[f32; 4]; 3] {
    std::array::from_fn(|row| {
        let linear = matrix.linear_micro[row];
        [
            micro_to_unit(i64::from(linear[0])),
            micro_to_unit(i64::from(linear[1])),
            micro_to_unit(i64::from(linear[2])),
            millimeter_to_meter(matrix.translation_millimeters[row]),
        ]
    })
}

/// Appends one palette's entries to a GPU byte stream.
pub(crate) fn extend_palette(bytes: &mut Vec<u8>, palette: &[SceneAffineMatrix]) {
    for matrix in palette {
        for value in palette_rows(matrix).as_flattened() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_word_keeps_each_lane_in_its_own_byte() {
        let word = skin_word(SceneSkinInfluence {
            joints: [1, 2, 3, 250],
            weights: [200, 50, 5, 0],
        });
        assert_eq!(word, [0xfa03_0201, 0x0005_32c8]);
        for lane in 0..4 {
            assert_eq!((word[0] >> (8 * lane)) & 0xff, [1, 2, 3, 250][lane]);
            assert_eq!((word[1] >> (8 * lane)) & 0xff, [200, 50, 5, 0][lane]);
        }
    }

    #[test]
    fn unweighted_vertex_has_an_empty_weight_word() {
        let word = skin_word(SceneSkinInfluence {
            joints: [7; 4],
            weights: [0; 4],
        });
        assert_eq!(word[1], 0);
    }

    #[test]
    fn identity_palette_entry_is_the_identity_rows() {
        assert_eq!(
            palette_rows(&SceneAffineMatrix::IDENTITY),
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0]
            ]
        );
    }

    #[test]
    fn palette_rows_convert_micro_units_and_millimetres() {
        let matrix = SceneAffineMatrix {
            linear_micro: [[0, -1_000_000, 0], [1_000_000, 0, 0], [0, 0, 500_000]],
            translation_millimeters: [1_500, -250, 0],
        };
        assert_eq!(
            palette_rows(&matrix),
            [
                [0.0, -1.0, 0.0, 1.5],
                [1.0, 0.0, 0.0, -0.25],
                [0.0, 0.0, 0.5, 0.0]
            ]
        );
    }

    #[test]
    fn extended_palette_is_forty_eight_bytes_per_joint() {
        let mut bytes = Vec::new();
        extend_palette(&mut bytes, &[SceneAffineMatrix::IDENTITY; 3]);
        assert_eq!(bytes.len() as u64, 3 * PALETTE_ENTRY_BYTES);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(bytes[60..64].try_into().unwrap()), 0.0);
    }
}
