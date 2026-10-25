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
    pub fn world_position(&self, object: ObjectHandle) -> Result<Vec3, WorldError> {
        Ok(self.world_matrix(object)?.w_axis.truncate())
    }

    pub fn local_position_from_world(
        &self,
        object: ObjectHandle,
        position: Vec3,
    ) -> Result<Vec3, WorldError> {
        self.live_object(object)?;
        if !position.is_finite() {
            return Err(WorldError::InvalidComponent(
                crate::ComponentError::InvalidTransform,
            ));
        }
        let Some(parent) = self.parent(object)? else {
            return Ok(position);
        };
        let matrix = self.world_matrix(parent)?;
        if matrix.determinant() == 0.0 {
            return Err(WorldError::SingularParent);
        }
        let inverse = matrix.inverse();
        if !inverse.is_finite() {
            return Err(WorldError::SingularParent);
        }
        Ok(inverse.transform_point3(position))
    }

    pub fn set_world_position(
        &mut self,
        object: ObjectHandle,
        position: Vec3,
    ) -> Result<(), WorldError> {
        let translation = self.local_position_from_world(object, position)?;
        let local = LocalTransform {
            translation,
            ..self.local_transform(object)?
        };
        self.set_local_transform(object, local)
    }

    pub fn world_rotation(&self, object: ObjectHandle) -> Result<Quat, WorldError> {
        let mut rotation = Quat::IDENTITY;
        let mut current = Some(object);
        while let Some(handle) = current {
            rotation = self.local_transform(handle)?.rotation * rotation;
            current = self.parent(handle)?;
        }
        Ok(rotation.normalize())
    }

    pub fn set_world_rotation(
        &mut self,
        object: ObjectHandle,
        rotation: Quat,
    ) -> Result<(), WorldError> {
        let mut local = self.local_transform(object)?;
        let parent = self
            .parent(object)?
            .map(|p| self.world_rotation(p))
            .transpose()?
            .unwrap_or(Quat::IDENTITY);
        local.rotation = parent.inverse() * rotation;
        self.set_local_transform(object, local)
    }

    pub fn world_matrix(&self, object: ObjectHandle) -> Result<Mat4, WorldError> {
        let mut matrix = Mat4::IDENTITY;
        let mut current = Some(object);
        while let Some(handle) = current {
            matrix = self.local_transform(handle)?.matrix() * matrix;
            current = self.parent(handle)?;
        }
        Ok(matrix)
    }
}
