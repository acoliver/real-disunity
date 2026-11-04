//! Materials: a registered program with property values and enabled features, and the draws and
//! packets made from them.

use std::ops::Range;
use std::sync::Arc;

use crate::bind::Snapshot;
use crate::draw::{Geometry, VertexInput};
use crate::error::ProgramError;
use crate::packet::MaterialParts;
use crate::properties::{PropertySet, PropertyValue};
use crate::registry::{ProgramHandle, Registered};
use crate::runtime::ProgramRuntime;
use crate::state::RenderState;
use crate::variant::{FeatureId, FeatureSet, UnknownFeatures};

/// A material. A handle belongs to the runtime that made it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle {
    pub(crate) runtime: u64,
    pub(crate) id: u64,
}

/// A material's state. It keeps its program alive.
pub(crate) struct MaterialEntry {
    pub(crate) program: Arc<Registered>,
    pub(crate) properties: PropertySet,
    pub(crate) features: FeatureSet,
}

/// One draw of a material.
#[derive(Clone, Debug)]
pub struct MaterialDraw {
    pub material: MaterialHandle,
    pub vertices: Vec<VertexInput>,
    pub geometry: Geometry,
    /// The instances drawn; `0..1` is a single draw.
    pub instances: Range<u32>,
    /// Values for this draw alone, which win over the material's.
    pub overrides: PropertySet,
}

/// A draw resolved against its material at one moment: the program, the state, the bindings
/// and the constants the material's properties packed then. Editing or releasing the material
/// afterwards does not change it.
///
/// It holds its constant buffers' reservations, and the frame it was prepared in holds them
/// too, so their bytes stay counted until that frame is cancelled, dropped or completed on the
/// GPU, even when the packet is dropped first.
pub struct MaterialPacket {
    pub(crate) program: crate::program::GraphicsProgram,
    pub(crate) state: RenderState,
    pub(crate) vertices: Vec<VertexInput>,
    pub(crate) bindings: Vec<crate::resources::BoundResource>,
    pub(crate) geometry: Geometry,
    pub(crate) instances: Range<u32>,
    pub(crate) snapshots: Vec<Snapshot>,
    pub(crate) missing: Vec<String>,
}

impl std::fmt::Debug for MaterialPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaterialPacket")
            .field("instances", &self.instances)
            .field("missing", &self.missing)
            .finish_non_exhaustive()
    }
}

impl MaterialPacket {
    /// The properties that had no value and stayed zero in the packed constants.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    pub(crate) fn draw_packet(&self) -> crate::packet::DrawPacket<'_> {
        crate::packet::DrawPacket {
            program: &self.program,
            state: &self.state,
            vertex_inputs: &self.vertices,
            bindings: &self.bindings,
            geometry: self.geometry,
        }
    }

    pub(crate) fn parts(&self) -> MaterialParts<'_> {
        MaterialParts {
            snapshots: &self.snapshots,
            instances: self.instances.clone(),
        }
    }
}

impl ProgramRuntime {
    /// Creates a material of `program` with `properties` and no feature enabled.
    ///
    /// # Errors
    /// Fails on a handle that is not a program of this runtime.
    pub fn create_material(
        &mut self,
        program: ProgramHandle,
        properties: PropertySet,
    ) -> Result<MaterialHandle, ProgramError> {
        let program = self.registry.program(program)?.clone();
        Ok(self.registry.add_material(MaterialEntry {
            program,
            properties,
            features: FeatureSet::new(),
        }))
    }
}

impl ProgramRuntime {
    /// Enables exactly the features named, replacing the material's features. The names must
    /// select a variant of the program. A refused call leaves the material as it was.
    ///
    /// # Errors
    /// Fails on a handle that is not a live material and, naming the feature, on one the
    /// program's variants do not use. Fails with a no-match error when no variant has exactly
    /// the features that are enabled.
    pub fn set_features(
        &mut self,
        material: MaterialHandle,
        names: &[&str],
    ) -> Result<(), ProgramError> {
        let features: FeatureSet = names.iter().map(|name| FeatureId::new(*name)).collect();
        let entry = self.registry.material_mut(material)?;
        entry
            .program
            .variants
            .select(&features, UnknownFeatures::Fail)?;
        entry.features = features;
        Ok(())
    }
}

impl ProgramRuntime {
    /// Sets the property `name` on the material. Packets prepared before the call keep the
    /// values they were prepared with; the next preparation sees the new one.
    ///
    /// # Errors
    /// Fails on a handle that is not a live material.
    pub fn set_property(
        &mut self,
        material: MaterialHandle,
        name: &str,
        value: PropertyValue,
    ) -> Result<(), ProgramError> {
        self.registry
            .material_mut(material)?
            .properties
            .set(name, value);
        Ok(())
    }
}
