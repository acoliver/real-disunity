//! Bind group layouts from the parameters Unity declares and the resources a module uses.

use crate::layout::{ProgramLayout as NeutralLayout, Slot, UnusedDeclaration};

use super::module::StageProgram;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("{kind} parameters are not bound by this renderer")]
    UnsupportedParameter { kind: String },
    #[error("parameter binding {unity_binding} is not in the module's binding table")]
    UnboundResource { unity_binding: i64 },
    #[error("binding {unity_binding} is in several descriptor sets of the binding table")]
    AmbiguousBinding { unity_binding: u32 },
    #[error("constant buffer name {name_index} has a binding but no buffer record")]
    UndescribedConstantBuffer { name_index: i32 },
    #[error("the module uses {name} at set {set} binding {binding}, which no parameter declares")]
    UndeclaredResource {
        set: u32,
        binding: u32,
        name: String,
    },
    #[error("set {set} binding {binding} is declared as {declared} but the module has {found}")]
    Mismatch {
        set: u32,
        binding: u32,
        declared: &'static str,
        found: String,
    },
    #[error("set {set} binding {binding} is {first} in one stage and {second} in another")]
    Conflict {
        set: u32,
        binding: u32,
        first: String,
        second: String,
    },
    #[error("{reason}")]
    Other { reason: String },
}

/// What the neutral layout can reject once Unity's own checks have passed. The adapter builds
/// each declaration as visible to its stage, so only these cases can arise.
impl From<crate::layout::LayoutError> for LayoutError {
    fn from(error: crate::layout::LayoutError) -> Self {
        use crate::layout::LayoutError as Neutral;
        match error {
            Neutral::UndeclaredResource { set, binding, name } => {
                Self::UndeclaredResource { set, binding, name }
            }
            Neutral::Conflict {
                set,
                binding,
                first,
                second,
            } => Self::Conflict {
                set,
                binding,
                first,
                second,
            },
            other => Self::Other {
                reason: other.to_string(),
            },
        }
    }
}

/// Every binding of a program's stages, merged: the neutral layout of stages built from Unity
/// parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramLayout(NeutralLayout);

impl ProgramLayout {
    /// Merges the stages' bindings. A binding two stages share keeps one slot visible to both.
    ///
    /// # Errors
    /// Fails on a parameter kind that is not bound, a parameter outside the binding table, a
    /// resource without a parameter or of another kind than its parameter, or stages that
    /// declare one binding differently.
    pub fn build(stages: &[&StageProgram]) -> Result<Self, LayoutError> {
        let neutral = stages
            .iter()
            .map(|stage| stage.neutral())
            .collect::<Result<Vec<_>, _>>()?;
        let refs: Vec<_> = neutral.iter().collect();
        Ok(Self(NeutralLayout::build(&refs)?))
    }

    /// The slots ordered by set and binding.
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        self.0.slots()
    }

    #[must_use]
    pub fn slot(&self, set: u32, binding: u32) -> Option<&Slot> {
        self.0.slot(set, binding)
    }

    #[must_use]
    pub fn unused(&self) -> &[UnusedDeclaration] {
        self.0.unused()
    }
}
