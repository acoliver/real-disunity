//! Compacted view drawing matches ranged drawing and restages its gather only when it must.
//!
//! The scene is eight cards in two batches (materials 0 and 2). Cards alternate between a near
//! level (one material slot) and a far level (two material slots and an LOD transition), and two
//! of them are hidden, so each batch selects several short ranges with holes between them.
use super::*;
use crate::compositor::FrameRenderer;
use crate::draw::InstanceHandle;
use crate::offscreen::{OffscreenTarget, RgbaImage};
use worldrunner_render_plan::{SceneCard, SceneLodMaterialSlot};

pub(super) fn slot(material: u32, indices: std::ops::Range<u32>) -> SceneLodMaterialSlot {
    SceneLodMaterialSlot::new(MaterialHandle::from_raw(material), indices).unwrap()
}

fn mesh() -> SceneMesh {
    let near = card(None).lods()[0]
        .clone()
        .with_material_slots(vec![slot(0, 0..6)])
        .unwrap();
    let far = SceneMeshLod::new(
        1,
        near.vertices().to_vec(),
        near.indices().to_vec(),
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_material_slots(vec![slot(2, 0..3), slot(0, 3..6)])
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [2000, 2000], 500_000).unwrap())
    .unwrap();
    SceneMesh::with_distances(vec![near, far], vec![20_000, 200_000]).unwrap()
}

pub(super) fn placement(index: i64) -> SceneObject {
    let far = (index / 2) % 2 == 1;
    // Neighbouring cards overlap on screen, so their depths differ and no draw order can tie.
    let depth = 100 * index + if far { 0 } else { -12_000 };
    let transform = SceneTransform {
        position_millimeters: [-5_950 + 1_700 * index, depth, 0],
        scale_millionths: if far { 3_000_000 } else { 1_000_000 },
        ..identity()
    };
    let material = if index % 2 == 0 { 0 } else { 2 };
    SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(material),
    )
}

pub(super) fn cards() -> RenderScene {
    let source = super::lod_material::scene(true);
    source
        .with_resources(
            source.resources().clone(),
            SceneGeometry::new(vec![mesh()], (0..8).map(placement).collect()).unwrap(),
            None,
        )
        .unwrap()
}

pub(super) fn pose_at(x: i64) -> CameraPose {
    CameraPose {
        eye_millimeters: [x, -24_000, 3_000],
        target_millimeters: [x, 0, 1_000],
    }
}

pub(super) fn unlit(pose: CameraPose) -> FrameUniform {
    let mut frame = frame_from(pose);
    frame.unlit = true;
    frame
}

struct Rig {
    context: GpuContext,
    pipelines: ScenePipelines,
    gpu: SceneGpu,
    target: OffscreenTarget,
    renderer: FrameRenderer,
}

impl Rig {
    fn new(ranged_only: bool) -> Self {
        let context = context();
        let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
        let mut gpu =
            SceneGpu::upload(context.device(), context.submission(), &cards(), &pipelines).unwrap();
        let handles: Vec<_> = gpu.instance_handles().collect();
        // Slot 2 of material 0 and slot 1 of material 2.
        for hidden in [4, 3] {
            gpu.set_instance_visible(handles[hidden], false).unwrap();
        }
        gpu.views[0]
            .compaction
            .observed
            .ranged_only
            .set(ranged_only);
        Self {
            target: OffscreenTarget::new(
                context.device(),
                Viewport {
                    width: TARGET,
                    height: TARGET,
                },
            )
            .unwrap(),
            renderer: FrameRenderer::new(context.device()),
            context,
            pipelines,
            gpu,
        }
    }

    fn prepare(
        &mut self,
        frame: &FrameUniform,
    ) -> worldrunner_render_gpu::submission::PreparedFrame {
        let Self {
            context,
            pipelines,
            gpu,
            target,
            renderer,
        } = self;
        context
            .submission()
            .prepare(context.device(), |encoder, _| {
                crate::compositor::encode_scene(
                    renderer,
                    context.device(),
                    encoder,
                    (gpu, pipelines),
                    (&target.color_attachment(), &target.depth_attachment()),
                    *frame,
                )?;
                target.encode_readback(encoder)?;
                Ok::<_, Box<dyn std::error::Error>>(())
            })
            .unwrap()
    }

    fn render(&mut self, frame: &FrameUniform) -> RgbaImage {
        let submission = self.prepare(frame).submit().unwrap();
        self.target.read_encoded(&self.context, submission).unwrap()
    }

    fn handles(&self) -> Vec<InstanceHandle> {
        self.gpu.instance_handles().collect()
    }

    fn staged(&self) -> usize {
        self.gpu.views[0].compaction.observed.staged.get()
    }

    fn compacted_draws(&self) -> usize {
        self.gpu.views[0].compaction.observed.compacted_draws.get()
    }
}

fn moved(handle_x: i64) -> SceneTransform {
    SceneTransform {
        position_millimeters: [handle_x, -12_000, 0],
        ..identity()
    }
}

fn count(image: &RgbaImage, color: [u8; 3]) -> usize {
    image
        .pixels
        .chunks_exact(4)
        .filter(|p| p[..3] == color)
        .count()
}

#[test]
fn compacted_and_ranged_drawing_produce_identical_pixels() {
    let mut compacted = Rig::new(false);
    let mut ranged = Rig::new(true);
    let edits = [None, Some((0, moved(-4_500))), Some((5, moved(3_500)))];
    let mut previous = None;
    for edit in edits {
        for rig in [&mut compacted, &mut ranged] {
            if let Some((index, transform)) = edit {
                let handle = rig.handles()[index];
                rig.gpu.set_instance_transform(handle, transform).unwrap();
            }
        }
        let frame = unlit(pose_at(0));
        let (a, b) = (compacted.render(&frame), ranged.render(&frame));
        assert_eq!(a.pixels, b.pixels);
        assert!(count(&a, [255, 0, 0]) > 20 && count(&a, [0, 255, 0]) > 15);
        assert_ne!(previous.replace(a.pixels.clone()), Some(a.pixels));
    }
    assert!(compacted.compacted_draws() >= 3);
    assert_eq!(ranged.compacted_draws(), 0);
}

#[test]
fn a_still_frame_stages_no_gather_and_each_input_change_stages_one() {
    let mut rig = Rig::new(false);
    rig.render(&unlit(pose_at(0)));
    let first = rig.staged();
    // Batch 0 selects slots 0, 1 and 3 in three ranges; batch 2 selects 0, 2 and 3 in three.
    assert_eq!(first, 6);
    rig.render(&unlit(pose_at(0)));
    assert_eq!(rig.staged(), first);
    let handle = rig.handles()[0];
    rig.gpu
        .set_instance_transform(handle, moved(-7_000))
        .unwrap();
    rig.render(&unlit(pose_at(0)));
    let edited = rig.staged();
    assert!(edited > first);
    rig.render(&unlit(pose_at(1_000)));
    let viewed = rig.staged();
    assert!(viewed > edited);
    rig.render(&unlit(pose_at(1_000)));
    assert_eq!(rig.staged(), viewed);
}

#[test]
fn growing_a_batch_buffer_and_the_view_buffer_restages_the_gather() {
    let mut rig = Rig::new(false);
    let reference = rig.render(&unlit(pose_at(0)));
    let small = rig.gpu.views[0].compaction.committed_capacity();
    let staged = rig.staged();
    let template = placement(0);
    for _ in 0..40 {
        rig.gpu.add_instance(template, true).unwrap();
    }
    let grown = rig.render(&unlit(pose_at(0)));
    assert!(rig.staged() > staged);
    assert!(rig.gpu.views[0].compaction.committed_capacity() > small);
    // The added cards sit on the first card, so only the pixels of a deeper stack could differ.
    assert_eq!(count(&grown, [255, 0, 0]), count(&reference, [255, 0, 0]));
    let settled = rig.staged();
    rig.render(&unlit(pose_at(0)));
    assert_eq!(rig.staged(), settled);
}

#[test]
fn a_cancelled_frame_leaves_the_committed_compaction_and_the_next_frame_correct() {
    let mut rig = Rig::new(false);
    let reference = rig.render(&unlit(pose_at(0)));
    let committed = rig.gpu.views[0]
        .compaction
        .committed
        .borrow()
        .groups
        .clone();
    let staged = rig.staged();
    // A different view gathers into the committed buffer inside the cancelled transaction.
    rig.prepare(&unlit(pose_at(2_000))).cancel();
    assert!(rig.staged() > staged);
    let after = rig.gpu.views[0]
        .compaction
        .committed
        .borrow()
        .groups
        .clone();
    assert!(std::rc::Rc::ptr_eq(&committed, &after));
    let staged = rig.staged();
    let again = rig.render(&unlit(pose_at(0)));
    assert_eq!(again.pixels, reference.pixels);
    assert_eq!(rig.staged(), staged);
    let moved_view = rig.render(&unlit(pose_at(2_000)));
    assert!(rig.staged() > staged);
    let mut ranged = Rig::new(true);
    assert_eq!(
        moved_view.pixels,
        ranged.render(&unlit(pose_at(2_000))).pixels
    );
}

#[test]
fn a_cancelled_edit_is_gathered_again_by_the_frame_that_publishes_it() {
    let mut rig = Rig::new(false);
    rig.render(&unlit(pose_at(0)));
    let handle = rig.handles()[1];
    rig.gpu
        .set_instance_transform(handle, moved(-1_000))
        .unwrap();
    rig.prepare(&unlit(pose_at(0))).cancel();
    let staged = rig.staged();
    let published = rig.render(&unlit(pose_at(0)));
    assert!(rig.staged() > staged);
    let mut ranged = Rig::new(true);
    let handle = ranged.handles()[1];
    ranged
        .gpu
        .set_instance_transform(handle, moved(-1_000))
        .unwrap();
    assert_eq!(published.pixels, ranged.render(&unlit(pose_at(0))).pixels);
}
