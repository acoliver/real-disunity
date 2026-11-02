//! Installed pass resources and receiver bindings of one submitted frame.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PassInstallation {
    #[default]
    Absent,
    Installed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReceiverBinding {
    #[default]
    Unbound,
    Bound,
}
impl ReceiverBinding {
    pub(crate) fn from_bound(bound: bool) -> Self {
        if bound { Self::Bound } else { Self::Unbound }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PassExecution {
    #[default]
    Skipped,
    Ran,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectEnabled {
    #[default]
    Disabled,
    Enabled,
}
impl EffectEnabled {
    pub(crate) fn from_enabled(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

/// Bindings describe receiver pipelines, independently of pixel coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReceiverBindings {
    pub meshes: ReceiverBinding,
    pub terrain: ReceiverBinding,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShadowStatistics {
    pub installation: PassInstallation,
    pub receivers: ReceiverBindings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SsaoStatistics {
    pub installation: PassInstallation,
    /// Installed AO with nonzero intensity. Neutral reflection metadata is disabled.
    pub enabled: EffectEnabled,
    pub prepass: PassExecution,
    pub receivers: ReceiverBindings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStatistics {
    pub shadows: ShadowStatistics,
    pub ssao: SsaoStatistics,
}

impl ReceiverBindings {
    pub(crate) fn new(meshes: bool, terrain: bool) -> Self {
        Self {
            meshes: ReceiverBinding::from_bound(meshes),
            terrain: ReceiverBinding::from_bound(terrain),
        }
    }
}
