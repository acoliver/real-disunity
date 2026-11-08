use super::ReceiverLayout;

/// Maximum receiver displacement storage admitted by a renderer configuration.
/// Backing is allocated at this maximum on first use by shadowed, paired
/// single-sample metadata, and retained until that metadata generation is replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverGradientReservation {
    /// No paired shadow receivers are admitted.
    None,
    /// Six f32 components, 24 bytes per pixel. Normal-biased transmission is excluded.
    Front,
    /// Independently biased front and back receivers, at most 48 bytes per pixel.
    Opposing,
}
impl ReceiverGradientReservation {
    pub(crate) const fn layout(self) -> Option<ReceiverLayout> {
        match self {
            Self::None => None,
            Self::Front => Some(ReceiverLayout::Front),
            Self::Opposing => Some(ReceiverLayout::Opposing),
        }
    }
    pub(crate) fn admits(self, layout: ReceiverLayout) -> bool {
        self.layout()
            .is_some_and(|reserved| reserved.sides() >= layout.sides())
    }
}
