use crate::{LocalTransform, ObjectHandle, World, WorldError};
use glam::{Mat3, Mat4, Quat, Vec3};

pub(crate) fn decompose(matrix: Mat4, orientation: Quat) -> Result<LocalTransform, WorldError> {
    if !matrix.is_finite() {
        return Err(WorldError::NonRepresentableLocalTransform);
    }
    let value = if matrix.determinant() == 0.0 {
        singular_trs(matrix, orientation)
    } else {
        let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
        LocalTransform {
            translation,
            rotation,
            scale,
        }
    };
    if value.validate().is_err() || !value.matrix().abs_diff_eq(matrix, 1e-4) {
        return Err(WorldError::NonRepresentableLocalTransform);
    }
    Ok(value)
}

fn singular_trs(matrix: Mat4, orientation: Quat) -> LocalTransform {
    let columns = [
        matrix.x_axis.truncate(),
        matrix.y_axis.truncate(),
        matrix.z_axis.truncate(),
    ];
    let basis = Mat3::from_quat(orientation);
    let mut axes = [basis.x_axis, basis.y_axis, basis.z_axis];
    let mut scale = Vec3::ZERO;
    let mut present = [false; 3];
    for index in 0..3 {
        let length = columns[index].length();
        if length != 0.0 {
            present[index] = true;
            scale[index] = if columns[index].dot(axes[index]) < 0.0 {
                -length
            } else {
                length
            };
            axes[index] = columns[index] / scale[index];
        }
    }
    complete_basis(&mut axes, present, orientation);
    LocalTransform {
        translation: matrix.w_axis.truncate(),
        rotation: Quat::from_mat3(&Mat3::from_cols(axes[0], axes[1], axes[2])).normalize(),
        scale,
    }
}

fn complete_basis(axes: &mut [Vec3; 3], present: [bool; 3], orientation: Quat) {
    match present {
        [true, true, false] => axes[2] = axes[0].cross(axes[1]),
        [true, false, true] => axes[1] = axes[2].cross(axes[0]),
        [false, true, true] => axes[0] = axes[1].cross(axes[2]),
        [true, false, false] => orient_single_axis(axes, 0, orientation),
        [false, true, false] => orient_single_axis(axes, 1, orientation),
        [false, false, true] => orient_single_axis(axes, 2, orientation),
        _ => {}
    }
}

fn orient_single_axis(axes: &mut [Vec3; 3], index: usize, orientation: Quat) {
    let basis = Mat3::from_quat(orientation);
    let hinted = [basis.x_axis, basis.y_axis, basis.z_axis];
    // Retain the existing twist around the only observable scale axis.
    let rotation = Quat::from_rotation_arc(hinted[index], axes[index]) * orientation;
    let basis = Mat3::from_quat(rotation.normalize());
    *axes = [basis.x_axis, basis.y_axis, basis.z_axis];
}

impl World {
    pub(crate) fn world_rotation(&self, object: ObjectHandle) -> Result<Quat, WorldError> {
        let mut rotation = Quat::IDENTITY;
        let mut current = Some(object);
        while let Some(handle) = current {
            rotation = self.local_transform(handle)?.rotation * rotation;
            current = self.parent(handle)?;
        }
        Ok(rotation.normalize())
    }

    pub fn world_matrix(&self, object: ObjectHandle) -> Result<Mat4, WorldError> {
        let mut chain = Vec::new();
        let mut current = Some(object);
        while let Some(handle) = current {
            chain.push(self.local_transform(handle)?.matrix());
            current = self.parent(handle)?;
        }
        Ok(chain
            .into_iter()
            .rev()
            .fold(Mat4::IDENTITY, |world, local| world * local))
    }
}
