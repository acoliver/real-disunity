//! The texture views of one build: draws that sample a texture at one version share one view,
//! and a write of the texture gives the draws after it a view of their own.

use worldrunner_render_program::ids::{CameraId, RendererId, TextureId};
use worldrunner_render_program::unity::offscreen::frame::{OffscreenFrame, Record};

use super::camera::{CAMERA, TARGET, camera};
use super::mips::camera_then_blit;
use super::rig::{BLUE, CLEAR, RED, Rig, pixel, texture};
use super::sampler::shifted;

/// The frame of the sampler fixture with `count` renderers, each a copy of its one renderer, so
/// `count` draws sample the same albedo texture in one pass.
fn sampling(rig: &mut Rig, count: u64) -> OffscreenFrame {
    let mut frame = shifted(rig, None);
    let renderer = frame.renderers[&RendererId(1)].clone();
    for id in 0..count {
        frame
            .renderers
            .insert(RendererId(100 + id), renderer.clone());
    }
    frame.renderers.remove(&RendererId(1));
    frame
}

#[test]
fn draws_that_sample_one_texture_share_one_view_of_it() {
    let mut rig = Rig::new();
    let single = sampling(&mut rig, 1);
    let twenty = sampling(&mut rig, 20);
    let one = rig.build(&single).unwrap();
    let many = rig.build(&twenty).unwrap();
    assert_eq!(one.planned_draws(), 1);
    assert_eq!(many.planned_draws(), 20);
    assert_eq!(
        many.view_counts().created,
        one.view_counts().created,
        "twenty draws of one texture make no more views than one draw"
    );
    assert!(
        many.view_counts().reused >= one.view_counts().reused + 19,
        "{:?} against {:?}",
        many.view_counts(),
        one.view_counts()
    );
}

#[test]
fn the_views_of_one_build_are_not_kept_for_the_next() {
    let mut rig = Rig::new();
    let frame = sampling(&mut rig, 5);
    let first = rig.build(&frame).unwrap().view_counts();
    let second = rig.build(&frame).unwrap().view_counts();
    assert!(first.created > 0);
    assert_eq!(second, first);
}

#[test]
fn a_draw_after_a_write_of_the_texture_it_samples_reads_the_written_contents() {
    let mut rig = Rig::new();
    let mut frame = camera_then_blit(&mut rig, 1);
    // A second camera draws the right half into the target after the first blit read it, and a
    // second blit then reads the target again into a texture of its own.
    frame.render_textures.push(texture(12, [8, 8]));
    let mut second = camera(1 << 2);
    second.clear_color = None;
    second.clear_depth = false;
    frame.cameras.insert(CameraId(2), second);
    frame.records.push(Record::CameraRender {
        camera: CameraId(2),
        target: TARGET,
    });
    let blit = frame.records[1].clone();
    let Record::Blit {
        source,
        pass,
        material,
        ..
    } = blit
    else {
        panic!("the fixture's second record is its blit");
    };
    frame.records.push(Record::Blit {
        source,
        target: TextureId(12),
        material,
        pass,
    });
    assert_eq!(CAMERA, CameraId(1));
    let images = rig
        .run(&frame, &[(TextureId(11), 0, 0), (TextureId(12), 0, 0)])
        .unwrap();
    assert_eq!(pixel(&images[0], 8, (1, 4)), RED);
    assert_eq!(pixel(&images[0], 8, (6, 4)), CLEAR);
    assert_eq!(pixel(&images[1], 8, (1, 4)), RED);
    assert_eq!(pixel(&images[1], 8, (6, 4)), BLUE);
}

/// The planned draws of every record of `frame`, one build each, against the bound the frame
/// gives before any of them is built.
fn bounds_and_planned(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<(usize, usize)> {
    frame
        .records
        .iter()
        .map(|record| {
            let mut alone = frame.clone();
            alone.records = vec![record.clone()];
            (
                frame.planned_draw_bound(record),
                rig.build(&alone).unwrap().planned_draws(),
            )
        })
        .collect()
}

#[test]
fn the_bound_on_the_draws_of_a_record_holds_for_cameras_and_lists() {
    use super::camera::fixture;
    use super::lists::{item, list, whole};

    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, (1 << 1) | (1 << 2));
    frame.records.push(list(
        whole(TARGET),
        vec![item(1, None), item(2, None), item(1, None)],
    ));
    // The camera draws both renderers, and the list its three items.
    assert_eq!(bounds_and_planned(&mut rig, &frame), [(2, 2), (3, 3)]);
    // A camera whose mask names one renderer's layer is bound by that renderer alone.
    frame.cameras.get_mut(&CAMERA).unwrap().culling_mask = 1 << 1;
    let measured = bounds_and_planned(&mut rig, &frame);
    assert_eq!(measured[0], (1, 1));
}
