//! World-schema vocabulary and the single Y-up to Z-up scene boundary.
//!
//! Producers quantize metres to millimetres before this boundary. The basis
//! change is a proper rotation: `(x, y, z) -> (x, -z, y)`. Apply it to both
//! mesh vectors and placements, never to just one side of a placed mesh.

pub use worldrunner_world_schema::{
    EnvironmentInstance, EnvironmentOrientation, EnvironmentPlacement, TerrainDocument,
    TerrainMaterialSlot, WorldCoordinateFrame,
};

use crate::{SceneRotation, SceneTransform, SceneValidationError, SceneVertex};

/// Converts a position or vector from schema XYZ to scene east/north/elevation.
/// Fails if negating the south component cannot be represented.
pub fn scene_vector_from_world(
    frame: WorldCoordinateFrame,
    [x, y, z]: [i64; 3],
) -> Result<[i64; 3], SceneValidationError> {
    match frame {
        WorldCoordinateFrame::RightHandedYUp => Ok([
            x,
            z.checked_neg()
                .ok_or(SceneValidationError::ArithmeticOverflow {
                    part: "world-to-scene basis",
                })?,
            y,
        ]),
    }
}

impl SceneTransform {
    /// Converts a quantized schema-basis placement to the scene basis.
    /// Quaternion XYZ transforms as an axial vector under this proper rotation;
    /// W and uniform scale are unchanged. Inputs use millimetres and micro-units.
    pub fn from_world_schema(
        frame: WorldCoordinateFrame,
        position_xyz_millimeters: [i64; 3],
        rotation_xyzw_micro: [i64; 4],
        scale_millionths: u64,
    ) -> Result<Self, SceneValidationError> {
        let [x, y, z, w] = rotation_xyzw_micro;
        let [x, y, z] = scene_vector_from_world(frame, [x, y, z])?;
        let transform = Self {
            position_millimeters: scene_vector_from_world(frame, position_xyz_millimeters)?,
            rotation: SceneRotation { x, y, z, w },
            scale_millionths,
        };
        transform.validate()?;
        Ok(transform)
    }
}

impl SceneVertex {
    /// Converts both position and normal of a quantized schema-basis vertex.
    /// UVs are independent of the world basis and remain unchanged.
    pub fn from_world_schema(
        frame: WorldCoordinateFrame,
        position_xyz_millimeters: [i64; 3],
        normal_xyz_micro: [i64; 3],
        texcoord_micro: [i64; 2],
    ) -> Result<Self, SceneValidationError> {
        Ok(Self {
            position_millimeters: scene_vector_from_world(frame, position_xyz_millimeters)?,
            normal_micro: scene_vector_from_world(frame, normal_xyz_micro)?,
            texcoord_micro,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: WorldCoordinateFrame = WorldCoordinateFrame::RightHandedYUp;

    #[test]
    fn schema_axes_preserve_handedness_and_elevation() {
        assert_eq!(
            scene_vector_from_world(FRAME, [1, 0, 0]).unwrap(),
            [1, 0, 0]
        );
        assert_eq!(
            scene_vector_from_world(FRAME, [0, 1, 0]).unwrap(),
            [0, 0, 1]
        );
        assert_eq!(
            scene_vector_from_world(FRAME, [0, 0, 1]).unwrap(),
            [0, -1, 0]
        );
        assert!(scene_vector_from_world(FRAME, [0, 0, i64::MIN]).is_err());
    }

    #[test]
    fn placement_and_mesh_share_the_same_boundary() {
        let placement = SceneTransform::from_world_schema(
            FRAME,
            [2_000, 3_000, -5_000],
            [0, 707_107, 0, 707_107],
            2_000_000,
        )
        .unwrap();
        assert_eq!(placement.position_millimeters, [2_000, 5_000, 3_000]);
        assert_eq!(
            placement.rotation,
            SceneRotation {
                x: 0,
                y: 0,
                z: 707_107,
                w: 707_107
            }
        );
        assert_eq!(placement.scale_millionths, 2_000_000);
        let vertex = SceneVertex::from_world_schema(
            FRAME,
            [1_000, 0, 0],
            [0, 1_000_000, 0],
            [250_000, 750_000],
        )
        .unwrap();
        assert_eq!(vertex.position_millimeters, [1_000, 0, 0]);
        assert_eq!(vertex.normal_micro, [0, 0, 1_000_000]);
        assert_eq!(vertex.texcoord_micro, [250_000, 750_000]);
        // A positive schema Y yaw becomes positive scene Z yaw, so east turns north.
        let q = placement.rotation;
        let rotated_east = [q.w * q.w - q.z * q.z, 2 * q.w * q.z, 0];
        assert_eq!(rotated_east[0], 0);
        assert!(rotated_east[1] > 0);
    }

    #[test]
    fn converted_placements_still_validate_rotation_and_scale() {
        assert!(SceneTransform::from_world_schema(FRAME, [0; 3], [0; 4], 1_000_000).is_err());
        assert!(SceneTransform::from_world_schema(FRAME, [0; 3], [0, 0, 0, 1_000_000], 0).is_err());
    }

    #[test]
    fn extreme_rotation_norms_return_overflow_errors() {
        for rotation in [
            [i64::MAX, i64::MAX, 0, i64::MAX],
            [i64::MAX; 4],
            [i64::MIN, i64::MIN, 0, 0],
        ] {
            assert_eq!(
                SceneTransform::from_world_schema(FRAME, [0; 3], rotation, 1_000_000),
                Err(SceneValidationError::ArithmeticOverflow {
                    part: "scene rotation",
                })
            );
        }
        assert_eq!(
            SceneTransform::from_world_schema(FRAME, [0; 3], [0, 0, 0, i64::MIN], 1_000_000),
            Err(SceneValidationError::RotationNotNormalized {
                part: "scene rotation",
            })
        );
    }
}
