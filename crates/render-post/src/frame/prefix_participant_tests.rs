//! Which retained features the frame that completes a geometry prefix prepares again: a prefix
//! participant whose every declared entry the prefix completed is skipped, every other feature is
//! prepared as usual.
use super::{FeatureDeclaration, FeaturePass, RegisteredFeature};
use crate::{BuildContext, FrameError, FrameFeature, GamePlace, PassEntry, PassSlot};

/// A source the rule never builds.
struct Unbuilt;

impl FrameFeature for Unbuilt {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn build(
        &self,
        _context: &BuildContext<'_>,
    ) -> Result<Box<dyn crate::PreparedFeature>, FrameError> {
        unreachable!("deciding whether to prepare builds nothing")
    }
}

const FIRST: (PassSlot, u32) = (PassSlot::Game(GamePlace::AfterDepthPrepass), 0);
const SECOND: (PassSlot, u32) = (PassSlot::Game(GamePlace::AfterDepthPrepass), 1);
const LATER: (PassSlot, u32) = (PassSlot::Game(GamePlace::BeforeOpaque), 0);

fn entry((slot, occurrence): (PassSlot, u32)) -> PassEntry {
    PassEntry::nth(slot, occurrence)
}

/// A registered feature declaring `entries`, a prefix participant when `prefix` is set.
fn feature(entries: &[(PassSlot, u32)], prefix: bool) -> RegisteredFeature {
    RegisteredFeature {
        declaration: FeatureDeclaration {
            passes: entries
                .iter()
                .map(|&(slot, occurrence)| FeaturePass::nth(slot, occurrence))
                .collect(),
            effects: Vec::new(),
        },
        prepared: None,
        source: Box::new(Unbuilt),
        prefix,
    }
}

#[test]
fn a_participant_whose_every_entry_the_prefix_completed_is_not_prepared_again() {
    let completed = [entry(FIRST), entry(SECOND)];
    assert!(feature(&[FIRST, SECOND], true).completed_in(&completed));
    assert!(feature(&[SECOND], true).completed_in(&completed));
}

#[test]
fn a_participant_with_an_entry_the_prefix_did_not_complete_is_prepared_again() {
    assert!(
        !feature(&[FIRST, LATER], true).completed_in(&[entry(FIRST), entry(SECOND)]),
        "the prefix drew FIRST only; the frame draws LATER and prepares for it"
    );
    assert!(!feature(&[FIRST, SECOND], true).completed_in(&[entry(FIRST)]));
    assert!(!feature(&[FIRST], true).completed_in(&[]));
}

#[test]
fn a_participant_that_declares_no_entry_is_prepared_in_the_frame() {
    assert!(
        !feature(&[], true).completed_in(&[]),
        "a frame without a prefix prepares it"
    );
    assert!(
        !feature(&[], true).completed_in(&[entry(FIRST)]),
        "a prefix that completed other entries drew nothing of it"
    );
}

#[test]
fn an_ordinary_feature_is_prepared_whatever_the_prefix_completed() {
    assert!(!feature(&[FIRST], false).completed_in(&[entry(FIRST)]));
}
