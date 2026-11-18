//! A group that fails abandons its frame; the frames after it read only what their own groups
//! write.
use super::*;

/// The operations of `Reads::new` writing `color` to the texture the frame hosts, as a retry
/// after a failed group would: another group of the same name over the same exported texture.
fn rewriting(frame: &Reads, color: wgpu::Color) -> Ops {
    let mut ops = frame.rig.ops("renewed");
    ops.texture = frame.written.clone();
    ops.color = color;
    ops.outputs = vec![OperationsOutput::new(ResourceRef::new("ops"), FORMAT, SIZE)];
    ops
}

fn failing_group(frame: &Reads) -> Ops {
    let mut ops = rewriting(frame, wgpu::Color::BLUE);
    ops.name = "failing";
    ops.fails = true;
    ops
}

#[test]
fn a_retry_after_a_failed_group_samples_what_its_own_group_wrote_not_the_earlier_frames() {
    let (mut frame, ops) = Reads::new();
    let scope = frame
        .rig
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let second = frame.rig.entries[1];
    let (passes, views) = (frame.rig.passes.clone(), frame.views(true));
    let first_frame = frame.run(&passes, &views, &[before(second, &ops)]).unwrap();
    assert!(
        first_frame
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]),
        "the first frame's group left red in the texture"
    );

    let failing = failing_group(&frame);
    assert_eq!(
        frame.run(&passes, &views, &[before(second, &failing)]),
        Err(FrameError::FeaturePreparation {
            feature: "test operations",
            reason: "failing".to_owned(),
        }),
        "the group that cannot encode abandons the frame"
    );

    let renewed = rewriting(&frame, wgpu::Color::GREEN);
    let retry = frame
        .run(&passes, &views, &[before(second, &renewed)])
        .unwrap();
    assert!(
        retry.chunks_exact(4).all(|pixel| pixel == [0, 255, 0, 255]),
        "the retry samples the green its group wrote, not the red of the first frame"
    );
    assert_eq!(
        frame.rig.logged(),
        ["a", "ops", "a", "failing", "a", "renewed"]
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn a_frame_without_the_group_after_a_failed_one_has_no_producer_and_reads_nothing_stale() {
    let (mut frame, ops) = Reads::new();
    let second = frame.rig.entries[1];
    let passes = frame.rig.passes.clone();
    frame
        .run(&passes, &frame.views(true), &[before(second, &ops)])
        .unwrap();

    let failing = failing_group(&frame);
    assert!(
        frame
            .run(&passes, &frame.views(true), &[before(second, &failing)])
            .is_err()
    );
    let logged = frame.rig.logged();

    // The texture still holds the first frame's red. The frames that follow the failure do not
    // inherit a producer from the frames before it: the host hands the view of an export only
    // with its group, and a group that follows its reader produces nothing for it.
    let missing = Err(FrameError::MissingProducer(ResourceRef::new("ops")));
    assert_eq!(
        frame.run(&passes, &frame.views(false), &[]),
        missing,
        "no group and no view"
    );
    assert_eq!(
        frame.run(&passes, &frame.views(true), &[after_all(&ops)]),
        missing,
        "the group follows the reader"
    );
    assert_eq!(
        frame.rig.logged(),
        logged,
        "the refused frames encoded nothing"
    );
}
