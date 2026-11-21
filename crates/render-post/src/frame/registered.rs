//! A retained feature as the chain holds it: its source, its build for the frame and the
//! declaration it was last planned with.
use super::{FeatureDeclaration, FrameFeature, PassEntry, PreparedFeature};

pub(crate) struct RegisteredFeature {
    pub declaration: FeatureDeclaration,
    pub prepared: Option<Box<dyn PreparedFeature>>,
    pub source: Box<dyn FrameFeature>,
    /// Takes part in geometry prefixes as well as in the frames that complete them.
    pub prefix: bool,
}

impl RegisteredFeature {
    /// Whether a geometry prefix already drew everything this prefix participant declares, so
    /// the frame that completes the prefix neither prepares nor draws it again. A participant
    /// that declares no pass drew nothing in any prefix: the frame prepares it as usual.
    pub(crate) fn completed_in(&self, completed: &[PassEntry]) -> bool {
        let passes = &self.declaration.passes;
        self.prefix
            && !passes.is_empty()
            && passes.iter().all(|pass| completed.contains(&pass.entry()))
    }
}
