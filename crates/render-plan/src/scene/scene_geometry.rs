use super::{SceneMesh, SceneObject, SceneValidationError};

/// Mesh table plus ordered objects of one scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneGeometry {
    pub(super) meshes: std::sync::Arc<[SceneMesh]>,
    pub(super) objects: Box<[SceneObject]>,
}

impl SceneGeometry {
    /// Adopts the mesh table and ordered objects.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when a mesh chain or an object
    /// transform violates its own closure.
    pub fn new(
        meshes: Vec<SceneMesh>,
        objects: Vec<SceneObject>,
    ) -> Result<Self, SceneValidationError> {
        let geometry = Self {
            meshes: meshes.into(),
            objects: objects.into_boxed_slice(),
        };
        geometry.validate()?;
        Ok(geometry)
    }

    /// Returns the owned mesh table.
    #[must_use]
    pub fn meshes(&self) -> &[SceneMesh] {
        &self.meshes
    }

    /// Returns the ordered objects in authored draw order.
    #[must_use]
    pub fn objects(&self) -> &[SceneObject] {
        &self.objects
    }

    /// Validates every mesh chain and object transform.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for mesh in self.meshes.iter() {
            mesh.validate()?;
        }
        for object in &self.objects {
            object.transform().validate()?;
        }
        Ok(())
    }
}
