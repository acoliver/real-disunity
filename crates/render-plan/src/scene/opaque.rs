use super::{SceneEffects, SceneValidationError};

/// Which operation the opaque-path experiment requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneOpaqueMode {
    /// Produce admission diagnostics only.
    Observe,
    /// Replace opaque rendering.
    Render,
}

/// The opaque surface family selected by an experiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneOpaqueFamily {
    /// Terrain only.
    Terrain,
    /// Ordinary and masked meshes.
    Meshes,
    /// Every stock opaque surface family.
    AllStock,
}

/// Material storage representation selected by an experiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneOpaqueStorage {
    /// Four wider attachments, 32 bytes per sample.
    Wide,
    /// Four compact attachments, 14 bytes per sample.
    Compact,
}

/// Depth ownership policy selected by an experiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneOpaqueVisibility {
    /// The material producer owns depth.
    MaterialOwnedDepth,
    /// A coverage prefix owns depth before material production.
    CoveragePrefix,
}

/// Choices carried by a not-yet-admitted opaque experiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneOpaqueExperiment {
    mode: SceneOpaqueMode,
    family: SceneOpaqueFamily,
    storage: SceneOpaqueStorage,
    visibility: SceneOpaqueVisibility,
}

/// Explicit opaque rendering contract. Experiments are validated by `SceneEffects`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneOpaquePath {
    /// Existing renderer path, selected by default.
    #[default]
    Forward,
    /// A closed experiment choice that is refused until implemented.
    Experiment(SceneOpaqueExperiment),
}

impl SceneOpaquePath {
    /// Constructs an experiment request; `SceneEffects::with_opaque_path` admits supported tuples.
    #[must_use]
    pub const fn experiment(
        mode: SceneOpaqueMode,
        family: SceneOpaqueFamily,
        storage: SceneOpaqueStorage,
        visibility: SceneOpaqueVisibility,
    ) -> Self {
        Self::Experiment(SceneOpaqueExperiment {
            mode,
            family,
            storage,
            visibility,
        })
    }
}

impl SceneEffects {
    /// Selects the validated opaque rendering contract.
    ///
    /// The existing forward path is implemented. Unsupported experiment tuples
    /// fail explicitly until their rendering path is implemented.
    pub fn with_opaque_path(mut self, path: SceneOpaquePath) -> Result<Self, SceneValidationError> {
        self.opaque_path = match path {
            SceneOpaquePath::Forward => SceneOpaquePath::Forward,
            SceneOpaquePath::Experiment(_) => {
                return Err(SceneValidationError::UnsupportedOpaquePath);
            }
        };
        Ok(self)
    }

    /// Returns the selected opaque rendering contract.
    #[must_use]
    pub const fn opaque_path(&self) -> SceneOpaquePath {
        self.opaque_path
    }
}
