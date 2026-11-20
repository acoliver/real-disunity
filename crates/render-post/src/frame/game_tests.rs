//! The game places that sit among the stock snapshot passes.
use super::{reading, writing};
use crate::*;

#[test]
fn a_game_pass_after_the_opaque_snapshots_reads_what_the_snapshot_passes_wrote() {
    let color = writing(PassSlot::AfterOpaqueSnapshot, 0, "opaque snapshot color");
    let mut depth = writing(PassSlot::AfterOpaqueDepth, 0, "opaque snapshot depth");
    let mut device = depth.color.clone().unwrap();
    device.resource = ResourceRef::new("opaque snapshot device depth");
    depth.additional_colors.push(device.clone());
    let place = PassSlot::Game(GamePlace::AfterOpaqueSnapshots);
    let reader = writing(place, 0, "game output");
    let mut read = reading(PassEntry::new(place), "opaque snapshot color");
    read.passes[0].reads.push(device.resource);

    assert_eq!(
        validate_frame(
            &[color.clone(), depth.clone(), reader.clone()],
            std::slice::from_ref(&read),
            &[]
        ),
        Ok(())
    );
    assert_eq!(
        validate_frame(
            &[reader, color.clone(), depth.clone()],
            std::slice::from_ref(&read),
            &[]
        ),
        Err(FrameError::MissingProducer(color.color.unwrap().resource)),
        "a reader listed before the snapshot passes has no producer"
    );
    let mut reopened = writing(
        PassSlot::ScreenSpaceAfterTransparent,
        0,
        "opaque snapshot device depth",
    );
    reopened.color.as_mut().unwrap().load = AttachmentLoad::Load;
    assert_eq!(
        validate_frame(&[depth, reopened], &[], &[]),
        Err(FrameError::AttachmentHazard(device.resource)),
        "the device depth snapshot is written once per frame"
    );
}
