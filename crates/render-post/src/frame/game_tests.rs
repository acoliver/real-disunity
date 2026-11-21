//! The game places that sit among the stock snapshot passes, and the place before post.
use super::{reading, writing};
use crate::frame::validate_stock_order;
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

/// A frozen plan lists the game's passes before post after every other colour writer, stock or
/// game: listed earlier, a later writer would draw over what they drew.
#[test]
fn a_frozen_plan_lists_the_games_passes_before_post_after_every_other_role() {
    let loading = |slot, occurrence| {
        let mut pass = writing(slot, occurrence, "scene color");
        pass.color.as_mut().unwrap().load = AttachmentLoad::Load;
        pass
    };
    let before_post = PassSlot::Game(GamePlace::BeforePost);
    let opaque = writing(PassSlot::Opaque, 0, "scene color");
    let transparent = loading(PassSlot::Game(GamePlace::AfterTransparent), 0);
    let screen = loading(PassSlot::ScreenSpaceAfterTransparent, 0);
    let first = loading(before_post, 0);
    let second = loading(before_post, 1);

    let ordered = [
        opaque.clone(),
        transparent.clone(),
        screen.clone(),
        first.clone(),
        second.clone(),
    ];
    assert_eq!(validate_frame(&ordered, &[], &[]), Ok(()));
    assert_eq!(validate_stock_order(&ordered, &[]), Ok(()));

    for late in [&screen, &transparent] {
        let mut passes = vec![opaque.clone(), first.clone()];
        passes.push(late.clone());
        assert_eq!(
            validate_frame(&passes, &[], &[]),
            Ok(()),
            "resource validation alone accepts {}",
            late.entry()
        );
        assert_eq!(
            validate_stock_order(&passes, &[]),
            Err(FrameError::StockOrder {
                pass: PassEntry::nth(before_post, 0),
                needs: late.entry(),
            }),
            "{} listed after the pass before post",
            late.entry()
        );
    }
    assert_eq!(
        validate_stock_order(&[opaque, second, first], &[]),
        Ok(()),
        "occurrences in one place keep the order the plan lists them in"
    );
}
