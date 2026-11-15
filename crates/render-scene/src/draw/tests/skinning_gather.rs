//! The compute gather carries skin descriptors with the instance records of skinned batches.
//!
//! The scene has a rigid ribbon batch and a skinned ribbon batch on one material. Skinned
//! placements take one of three poses, every fifth placement is hidden and every seventh stands
//! far outside the view, so the skinned batch selects several short ranges with holes between
//! them, and the rigid batch's records sit between skinned groups in the view buffer. A skinned
//! vertex stage reads the descriptor at its record's index, so the gathered descriptors must
//! equal plain copies of the same slot ranges from the batch's skin stream, at the same
//! compacted positions as the records.
//!
//! Previous transforms are not gathered here: a pass that writes object motion refuses skinned
//! geometry until skinned objects have a previous pose, so no frame tracks previous transforms
//! beside a skinned batch yet.
use super::compaction::Rig;
use super::skinning::{ribbon, shifted, skeleton};
use super::*;
use crate::draw::compaction::CompactionBytes;
use worldrunner_render_gpu::census::ResourceClass;
use worldrunner_render_plan::{SceneAffineMatrix, SceneSkinPose, SkinPoseHandle};
use worldrunner_render_post::BufferReadback;

/// Skinned placements in the scene; the test adds as many again to grow the batch.
const SKINNED: i64 = 30;
const POSES: u32 = 3;

/// The ribbon of [`ribbon`] without influences, as a rigid mesh.
fn rigid_ribbon() -> SceneMesh {
    let source = ribbon();
    let lod = &source.lods()[0];
    SceneMesh::new(vec![
        SceneMeshLod::new(0, lod.vertices().to_vec(), lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap()
}

/// Skinned placement `index`: six columns of rows further back, every seventh far to the side.
fn skinned(index: i64, pose: u32) -> SceneObject {
    let column = index % 6;
    let row = (index / 6) % 5;
    let x = if index % 7 == 3 {
        60_000
    } else {
        -10_000 + 4_000 * column
    };
    SceneObject::new(
        SceneTransform {
            position_millimeters: [x, -6_000 + 1_500 * row + 10 * index, 0],
            ..identity()
        },
        MeshHandle::from_raw(1),
        MaterialHandle::from_raw(0),
    )
    .with_skin_pose(SkinPoseHandle::from_raw(pose))
}

fn rigid(index: i64) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [-9_000 + 3_500 * index, 3_000, 0],
            ..identity()
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}

fn scene() -> RenderScene {
    let poses = (0..POSES)
        .map(|pose| {
            let palette = vec![
                SceneAffineMatrix::IDENTITY,
                shifted(250 * i64::from(pose) - 200),
            ];
            SceneSkinPose::new(palette.clone(), palette).expect("two joints")
        })
        .collect();
    let objects = (0..6)
        .map(rigid)
        .chain((0..SKINNED).map(|index| skinned(index, pose_of(index))))
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![rigid_ribbon(), ribbon()],
            objects,
            vec![skeleton(2)],
            poses,
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

fn pose_of(index: i64) -> u32 {
    u32::try_from(index).unwrap() % POSES
}

/// Hides every fifth skinned placement.
fn hide_every_fifth_skinned(gpu: &mut SceneGpu) {
    let handles: Vec<_> = gpu.instance_handles().skip(6).collect();
    for handle in handles.into_iter().step_by(5) {
        gpu.set_instance_visible(handle, false).unwrap();
    }
}

/// Gives skinned placement `index` (counted after the six rigid ones) the pose `pose`.
fn repose(gpu: &mut SceneGpu, index: i64, pose: u32) {
    let handle = gpu
        .instance_handles()
        .nth(6 + usize::try_from(index).unwrap())
        .unwrap();
    gpu.set_instance_object(handle, skinned(index, pose))
        .expect("the pose belongs to the scene");
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

/// Rebuilds the camera view's committed gather with one buffer copy per selected range, from
/// the batch buffers and the skinned batches' skin streams, and compares the bytes: every record,
/// and the skin descriptor at every skinned record's position. Returns the number of skinned
/// ranges compared.
fn assert_skinned_gather_matches_copies(context: &GpuContext, gpu: &SceneGpu) -> usize {
    let gather = gpu.committed_gather(0);
    let device = context.device();
    let create = |label, size: u64| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size.max(wgpu::COPY_BUFFER_ALIGNMENT),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    let current = create("copied records", gather.bytes);
    let skin = create("copied skin", gather.skin_bytes);
    let mut encoder = context.submission().begin(device).unwrap();
    let mut skinned = Vec::new();
    for copy in &gather.copies {
        let ranges = (copy.source, copy.destination, copy.bytes);
        encoder.copy_buffer_to_buffer(&copy.current, ranges.0, &current, ranges.1, ranges.2);
        if let Some(stream) = &copy.skin {
            encoder.copy_buffer_to_buffer(
                &stream.buffer,
                stream.source,
                &skin,
                stream.destination,
                stream.bytes,
            );
            let start = usize::try_from(stream.destination).unwrap();
            skinned.push(start..start + usize::try_from(stream.bytes).unwrap());
        }
    }
    encoder.finish().submit().unwrap();
    assert_eq!(
        read(context, &gather.current, gather.bytes),
        read(context, &current, gather.bytes)
    );
    let gathered = gather.skin.as_ref().expect("a skinned scene gathers skin");
    let (gathered, copied) = (
        read(context, gathered, gather.skin_bytes),
        read(context, &skin, gather.skin_bytes),
    );
    // Records of the rigid batch leave their descriptors unwritten, so only skinned ranges count.
    for range in &skinned {
        assert_eq!(gathered[range.clone()], copied[range.clone()], "{range:?}");
    }
    let poses: std::collections::BTreeSet<_> = skinned
        .iter()
        .flat_map(|range| copied[range.clone()].chunks(16).map(<[u8]>::to_vec))
        .collect();
    assert!(poses.len() > 1, "every gathered descriptor is the same");
    skinned.len()
}

fn frame(step: i64) -> FrameUniform {
    super::compaction::unlit(super::compaction::pose_at(1_500 * step))
}

/// The Instance tally of the rig's census: every batch buffer and skin stream plus everything
/// compaction holds. Returns what compaction holds.
fn counted(rig: &Rig) -> CompactionBytes {
    let batches: u64 = rig
        .gpu
        .instances
        .batches
        .values()
        .flat_map(|batch| {
            [
                batch.buffer.as_deref(),
                batch.previous.as_deref(),
                batch
                    .skin
                    .as_ref()
                    .and_then(|stream| stream.buffer.as_deref()),
            ]
        })
        .map(|buffer| buffer.map_or(0, wgpu::Buffer::size))
        .sum();
    let compaction = rig.gpu.compaction_bytes();
    assert_eq!(
        rig.context
            .submission()
            .census()
            .snapshot()
            .get(ResourceClass::Instance),
        batches + compaction.total(),
        "{compaction:?}"
    );
    compaction
}

/// Compacted and ranged drawing agree, the gathered records and descriptors equal plain copies,
/// and the skin halves of the mirror and the view buffers count in the scene's census until the
/// scene drops, through a pose switch that changes only descriptors and growth that grows the
/// skin stream and relays out the mirror.
#[test]
fn gpu_gather_matches_plain_copies_of_skinned_records_and_their_skin_descriptors() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut compacted = Rig::over(&scene(), false);
    let mut ranged = Rig::over(&scene(), true);
    for rig in [&mut compacted, &mut ranged] {
        hide_every_fifth_skinned(&mut rig.gpu);
    }
    let mut held = Vec::new();
    for step in 0..3 {
        for rig in [&mut compacted, &mut ranged] {
            match step {
                1 => {
                    repose(&mut rig.gpu, 1, 2);
                    repose(&mut rig.gpu, 8, 0);
                }
                2 => {
                    for index in 0..SKINNED {
                        rig.gpu
                            .add_instance(skinned(index, pose_of(index + 1)), true)
                            .unwrap();
                    }
                }
                _ => {}
            }
        }
        let (a, b) = (compacted.render(&frame(step)), ranged.render(&frame(step)));
        assert!(lit_pixels(&a.pixels) > 0, "step {step}");
        assert_eq!(a.pixels, b.pixels, "step {step}");
        assert!(compacted.compacted_draws() > 0);
        let skinned_ranges =
            assert_skinned_gather_matches_copies(&compacted.context, &compacted.gpu);
        assert!(
            skinned_ranges > 2,
            "step {step}: only {skinned_ranges} skinned ranges"
        );
        let bytes = counted(&compacted);
        assert!(
            bytes.skin > 0 && bytes.mirror > 0 && bytes.views > 0,
            "{bytes:?}"
        );
        held.push(bytes);
    }
    assert!(held[2].skin > held[0].skin, "{held:?}");
    let census = compacted.context.submission().census().clone();
    drop(compacted);
    assert_eq!(census.snapshot().get(ResourceClass::Instance), 0);
}
