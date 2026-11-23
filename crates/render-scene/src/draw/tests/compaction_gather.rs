//! The compute gather writes byte for byte what plain buffer copies of the selected ranges write.
//!
//! The scene places 48 cards in three batches (materials 0, 1 and 2), alternating between two
//! levels, with every fifth card hidden and some outside the view, so every batch selects many
//! short ranges with holes between them. The reference copies each selected range from its batch
//! buffer with `copy_buffer_to_buffer` on the same device, after the frame that gathered it.
use crate::test_support::upload_scene;
use std::collections::BTreeSet;

use super::compaction::{mesh, placement, pose_at, unlit};
use super::*;
use crate::compositor::FrameRenderer;
use crate::offscreen::OffscreenTarget;
use worldrunner_render_post::BufferReadback;

const CARDS: i64 = 48;

/// Card `index % 8` of the compaction scene, one row further back for every eight cards.
pub(super) fn object(index: i64) -> SceneObject {
    let mut transform = *placement(index % 8).transform();
    transform.position_millimeters[1] += 2_000 * (index / 8);
    let material = u32::try_from(index % 3).expect("three materials");
    SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(material),
    )
}

pub(super) fn fragmented() -> RenderScene {
    let source = super::lod_material::scene(true);
    source
        .with_resources(
            source.resources().clone(),
            SceneGeometry::new(vec![mesh()], (0..CARDS).map(object).collect()).unwrap(),
            None,
        )
        .unwrap()
}

pub(super) fn hide_every_fifth(gpu: &mut SceneGpu) {
    let handles: Vec<_> = gpu.instance_handles().collect();
    for handle in handles.into_iter().step_by(5) {
        gpu.set_instance_visible(handle, false).unwrap();
    }
}

fn read(context: &GpuContext, buffer: &wgpu::Buffer, bytes: u64) -> Vec<u8> {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut readback = BufferReadback::new(context.device(), bytes, 1 << 20).unwrap();
    readback.encode(&mut encoder, buffer).unwrap();
    let submission = encoder.finish().submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}

/// Rebuilds the camera view's committed gather with one buffer copy per selected range and
/// compares the bytes, current records always and previous transforms when `previous`.
pub(super) fn assert_gather_matches_copies(context: &GpuContext, gpu: &SceneGpu, previous: bool) {
    let gather = gpu.committed_gather(0);
    let batches: BTreeSet<_> = gather.keys.iter().map(|(key, _)| *key).collect();
    assert!(
        batches.len() >= 3,
        "only {} batches selected",
        batches.len()
    );
    assert!(
        gather.keys.len() > batches.len(),
        "no batch selected two levels"
    );
    assert!(
        gather.copies.len() > gather.keys.len(),
        "no group gathered several ranges"
    );
    assert_gather_equals_plain_copies(context, &gather, previous);
}

/// Compares the committed view buffers of `gather` with one buffer copy per selected range.
pub(super) fn assert_gather_equals_plain_copies(
    context: &GpuContext,
    gather: &crate::draw::compaction::CommittedGather,
    previous: bool,
) {
    let device = context.device();
    let create = |label| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: gather.bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    let (current, previous_reference) = (create("copied records"), create("copied previous"));
    let mut encoder = context.submission().begin(device).unwrap();
    for copy in &gather.copies {
        let ranges = (copy.source, copy.destination, copy.bytes);
        encoder.copy_buffer_to_buffer(&copy.current, ranges.0, &current, ranges.1, ranges.2);
        if previous {
            let source = copy.previous.as_ref().expect("previous batch buffer");
            encoder.copy_buffer_to_buffer(
                source,
                ranges.0,
                &previous_reference,
                ranges.1,
                ranges.2,
            );
        }
    }
    encoder.finish().submit().unwrap();
    assert_eq!(
        read(context, &gather.current, gather.bytes),
        read(context, &current, gather.bytes)
    );
    if previous {
        let gathered = gather.previous.as_ref().expect("previous view buffer");
        assert_eq!(
            read(context, gathered, gather.bytes),
            read(context, &previous_reference, gather.bytes)
        );
    }
}

#[test]
fn gpu_gather_matches_plain_copies_of_a_fragmented_selection() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let pipelines = ScenePipelines::new(device, COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(device, context.submission(), &fragmented(), &pipelines).unwrap();
    hide_every_fifth(&mut gpu);
    let target = OffscreenTarget::new(
        device,
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let mut renderer = FrameRenderer::new(device);
    // A first gather, an edit with a moved view, then growth that relays out the mirror.
    for step in 0..3 {
        match step {
            1 => {
                let handle = gpu.instance_handles().nth(7).unwrap();
                let mut transform = *object(7).transform();
                transform.position_millimeters[0] += 600;
                gpu.set_instance_transform(handle, transform).unwrap();
            }
            2 => {
                for index in 0..40 {
                    gpu.add_instance(object(index), true).unwrap();
                }
            }
            _ => {}
        }
        context
            .submission()
            .prepare(device, |encoder, _| {
                renderer.encoding().encode_scene(
                    device,
                    encoder,
                    (&mut gpu, &pipelines),
                    (&target.color_attachment(), &target.depth_attachment()),
                    unlit(pose_at(1_500 * step)),
                )?;
                Ok::<_, Box<dyn std::error::Error>>(())
            })
            .unwrap()
            .submit()
            .unwrap();
        assert!(gpu.views[0].compaction.observed.compacted_draws.get() > 0);
        assert_gather_matches_copies(&context, &gpu, false);
    }
}
