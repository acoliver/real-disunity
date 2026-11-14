//! GPU forms of the scene skinning contract.
//!
//! A [`SceneSkinPose`](worldrunner_render_plan::SceneSkinPose) palette entry is the whole
//! skinning transform of one joint: the rest pose is all identity and draws the bind mesh, so
//! the inverse binds of the skeleton are already folded in and the GPU never reads them.

use worldrunner_render_plan::{SceneAffineMatrix, SceneMeshLod, SceneSkinInfluence};

use crate::geometry::{
    GeometryError, VERTEX_BYTES, micro_to_unit, millimeter_to_meter, pack_vertices,
};

mod palette;

pub(crate) use palette::{PaletteOverflow, PoseRange, ScenePalette};

/// Bytes of one palette entry on the GPU: three `vec4<f32>` rows.
pub(crate) const PALETTE_ENTRY_BYTES: u64 = 48;

/// Bytes of one skin word, appended after the static vertex at location 9.
pub(crate) const SKIN_WORD_BYTES: usize = 8;

/// Bytes of one skinned vertex: the unchanged static vertex followed by its skin word.
pub(crate) const SKINNED_VERTEX_BYTES: usize = VERTEX_BYTES + SKIN_WORD_BYTES;

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

/// Packs a skinned LOD: each static vertex, byte for byte, followed by its skin word.
///
/// Returns `None` for a LOD without influences, whose vertices use the static packing alone.
pub(crate) fn pack_skinned_vertices(lod: &SceneMeshLod) -> Result<Option<Vec<u8>>, GeometryError> {
    let Some(influences) = lod.skin_influences() else {
        return Ok(None);
    };
    let static_bytes = pack_vertices(lod)?;
    let mut bytes = Vec::with_capacity(influences.len() * SKINNED_VERTEX_BYTES);
    for (vertex, influence) in static_bytes.chunks_exact(VERTEX_BYTES).zip(influences) {
        bytes.extend_from_slice(vertex);
        for lane in skin_word(*influence) {
            bytes.extend_from_slice(&lane.to_le_bytes());
        }
    }
    Ok(Some(bytes))
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

    fn triangle() -> SceneMeshLod {
        let vertex = |x: i64| worldrunner_render_plan::SceneVertex {
            tangent_micro: None,
            position_millimeters: [x, 0, 0],
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: [0; 2],
        };
        SceneMeshLod::new(
            0,
            vec![vertex(0), vertex(1_000), vertex(2_000)],
            vec![0, 1, 2],
            None,
        )
        .expect("synthetic triangle validates")
    }

    #[test]
    fn rigid_lod_has_no_skinned_packing() {
        assert_eq!(pack_skinned_vertices(&triangle()).unwrap(), None);
    }

    #[test]
    fn skinned_vertex_keeps_the_static_bytes_then_appends_its_word() {
        let lod = triangle();
        let static_bytes = pack_vertices(&lod).unwrap();
        let influences =
            [[255, 0, 0, 0], [128, 127, 0, 0], [0; 4]].map(|weights| SceneSkinInfluence {
                joints: [0, 1, 2, 3],
                weights,
            });
        let skinned = lod
            .with_skin_influences(
                worldrunner_render_plan::SkeletonHandle::from_raw(0),
                influences.to_vec(),
            )
            .unwrap();
        let bytes = pack_skinned_vertices(&skinned).unwrap().unwrap();
        assert_eq!(bytes.len(), 3 * SKINNED_VERTEX_BYTES);
        for (index, vertex) in bytes.chunks_exact(SKINNED_VERTEX_BYTES).enumerate() {
            let range = index * VERTEX_BYTES..(index + 1) * VERTEX_BYTES;
            assert_eq!(&vertex[..VERTEX_BYTES], &static_bytes[range]);
            let word = skin_word(influences[index]);
            assert_eq!(
                &vertex[VERTEX_BYTES..VERTEX_BYTES + 4],
                &word[0].to_le_bytes()
            );
            assert_eq!(&vertex[VERTEX_BYTES + 4..], &word[1].to_le_bytes());
        }
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
