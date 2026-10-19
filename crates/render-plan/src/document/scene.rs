//! Strictly ordered authored scene components.
//!
//! Scene components are wholly identified by caller-authored identifiers and
//! content-addressed asset URIs; there is no path, dataset, or profile
//! knowledge here.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::spatial::TransformMillimeters;
use crate::document::validate::{require_identifier, require_sorted_unique, require_uri};

/// One authored static mesh component.
///
/// The component binds exactly one asset URI and places it with one exact
/// transform under the document coordinate basis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshComponent {
    /// Unique scene component identifier.
    pub component_id: String,
    /// Structural URI of the mesh asset this component renders.
    pub asset_uri: String,
    /// Exact placement under the document basis.
    pub transform: TransformMillimeters,
}

impl MeshComponent {
    /// Returns the unique scene component identifier.
    #[must_use]
    pub fn component_id(&self) -> &str {
        &self.component_id
    }

    /// Returns the structural URI of the bound mesh asset.
    #[must_use]
    pub fn asset_uri(&self) -> &str {
        &self.asset_uri
    }

    /// Returns the exact placement of this component.
    #[must_use]
    pub fn transform(&self) -> &TransformMillimeters {
        &self.transform
    }

    /// Validates the identifier, URI, and transform invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when any child field is malformed.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_identifier("mesh component_id", &self.component_id)?;
        require_uri("mesh asset URI", &self.asset_uri)?;
        self.transform.validate()
    }
}

/// Authored scene description.
///
/// Mesh components are held in strictly ascending, duplicate-free
/// `component_id` order so scene traversal is deterministic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDescription {
    meshes: Vec<MeshComponent>,
}

impl SceneDescription {
    /// Validates and adopts the mesh components in authored order.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when components are unsorted,
    /// duplicated, or individually malformed.
    pub fn new(meshes: Vec<MeshComponent>) -> Result<Self, ContractDocumentError> {
        let scene = Self { meshes };
        scene.validate()?;
        Ok(scene)
    }

    /// Returns the strictly ordered mesh components.
    #[must_use]
    pub fn meshes(&self) -> &[MeshComponent] {
        &self.meshes
    }

    /// Validates ordering, uniqueness, and every component.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_sorted_unique(
            "scene meshes",
            self.meshes.iter().map(|mesh| mesh.component_id.as_str()),
        )?;
        for mesh in &self.meshes {
            mesh.validate()?;
        }
        Ok(())
    }
}
