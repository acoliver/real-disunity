use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum Phase {
    Initialize,
    FixedUpdate,
    FixedCoroutines,
    Contacts,
    Update,
    UpdateCoroutines,
    LateUpdate,
    EndOfFrame,
    Cleanup,
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
pub(crate) enum Stage {
    Phase(Phase),
    Idle,
    Commit,
    End,
    Closed,
}
impl Phase {
    pub(super) fn next(self, remaining: u32) -> Stage {
        match self {
            Self::Initialize => Stage::Idle,
            Self::FixedUpdate => Stage::Phase(Self::FixedCoroutines),
            Self::FixedCoroutines => Stage::Commit,
            Self::Contacts if remaining > 0 => Stage::Phase(Self::FixedUpdate),
            Self::Contacts => Stage::Phase(Self::Update),
            Self::Update => Stage::Phase(Self::UpdateCoroutines),
            Self::UpdateCoroutines => Stage::Phase(Self::LateUpdate),
            Self::LateUpdate => Stage::Phase(Self::EndOfFrame),
            Self::EndOfFrame => Stage::End,
            Self::Cleanup => Stage::Closed,
        }
    }
    pub(super) fn can_issue(self, stage: Stage) -> bool {
        if self == Self::Cleanup {
            matches!(stage, Stage::Idle | Stage::Phase(Self::Initialize))
        } else {
            stage == Stage::Phase(self)
        }
    }
    pub(super) fn uses_fixed_delta(self) -> bool {
        self.is_fixed() || self == Self::Contacts
    }
    pub(super) fn is_fixed(self) -> bool {
        matches!(self, Self::FixedUpdate | Self::FixedCoroutines)
    }
}
pub(crate) fn phases() -> [Phase; 9] {
    [
        Phase::Initialize,
        Phase::FixedUpdate,
        Phase::FixedCoroutines,
        Phase::Contacts,
        Phase::Update,
        Phase::UpdateCoroutines,
        Phase::LateUpdate,
        Phase::EndOfFrame,
        Phase::Cleanup,
    ]
}
