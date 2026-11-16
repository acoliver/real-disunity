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
    /// The one admitted experiment: allocate and clear the compact deferred G-buffer
    /// before Opaque and expose it to captures, while the visible frame stays forward.
    /// Producers write terrain material values into it; nothing consumes it yet.
    pub const GBUFFER_OBSERVATION: Self = Self::experiment(
        SceneOpaqueMode::Observe,
        SceneOpaqueFamily::Terrain,
        SceneOpaqueStorage::Compact,
        SceneOpaqueVisibility::MaterialOwnedDepth,
    );

    /// Render the opaque frame from the compact deferred G-buffer: terrain, meshes and
    /// impostor cards store their materials before Opaque, one full-screen pass lights them
    /// into the scene colour, and Opaque draws only what has no G-buffer producer (sky,
    /// celestial bodies, dynamic and player draws) before water and transparency.
    pub const GBUFFER_RENDER: Self = Self::experiment(
        SceneOpaqueMode::Render,
        SceneOpaqueFamily::AllStock,
        SceneOpaqueStorage::Compact,
        SceneOpaqueVisibility::MaterialOwnedDepth,
    );

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
    /// The forward path, [`SceneOpaquePath::GBUFFER_OBSERVATION`] and
    /// [`SceneOpaquePath::GBUFFER_RENDER`] are implemented.
    /// Every other experiment tuple fails explicitly until its rendering path is implemented.
    pub fn with_opaque_path(mut self, path: SceneOpaquePath) -> Result<Self, SceneValidationError> {
        self.opaque_path = match path {
            SceneOpaquePath::Forward
            | SceneOpaquePath::GBUFFER_OBSERVATION
            | SceneOpaquePath::GBUFFER_RENDER => path,
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
