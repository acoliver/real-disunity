//! Compacted drawing gathers previous-frame transforms like ranged drawing reads them.
//!
//! The previous bytes of an edited slot are rewritten one frame after the edit without any new
//! edit, so the frame after an edit must gather them again for the residual to settle to zero.
use super::super::compaction::{cards, pose_at, unlit};
use super::*;
use crate::test_support::upload_scene;

struct Rig {
    context: GpuContext,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    /// Draws the same frames without writing velocity.
    colour_only: ScenePipelines,
    renderer: FrameRenderer,
    color: AttachmentView,
    depth: AttachmentView,
}

impl Rig {
    fn new(ranged_only: bool) -> Self {
        let mut rig = Self::of(&cards());
        let handles: Vec<_> = rig.gpu.instance_handles().collect();
        for hidden in [4, 3] {
            rig.gpu
                .set_instance_visible(handles[hidden], false)
                .unwrap();
        }
        rig.gpu.views[0]
            .compaction
            .observed
            .ranged_only
            .set(ranged_only);
        rig
    }

    fn of(source: &RenderScene) -> Self {
        let context = context();
        let (scene, effects) = with_taa(source);
        let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
        let gpu = upload_scene(context.device(), context.submission(), &scene, &pipelines).unwrap();
        let (renderer, color, depth) = taa_target(&context, &effects, MsaaSamples::X1);
        Self {
            colour_only: ScenePipelines::new(context.device(), HDR, 1).unwrap(),
            context,
            gpu,
            pipelines,
            renderer,
            color,
            depth,
        }
    }

    fn edit(&mut self, index: usize, x: i64) {
        let handle = self.gpu.instance_handles().nth(index).unwrap();
        let transform = SceneTransform {
            position_millimeters: [x, -12_000, 0],
            ..identity()
        };
        self.gpu.set_instance_transform(handle, transform).unwrap();
    }

    /// Draws one frame and returns its velocity target.
    fn velocity(&mut self, x: i64) -> Vec<[f32; 2]> {
        self.draw(x, true);
        let texture = self.renderer.targets().velocity_texture().unwrap().clone();
        read_velocity(&self.context, &texture)
    }

    /// Draws one frame, with or without writing velocity.
    fn draw(&mut self, x: i64, velocity: bool) {
        let pipelines = if velocity {
            &self.pipelines
        } else {
            &self.colour_only
        };
        let mut encoder = self
            .context
            .submission()
            .begin(self.context.device())
            .unwrap();
        self.renderer
            .encoding()
            .encode_scene(
                self.context.device(),
                &mut encoder,
                (&mut self.gpu, pipelines),
                (&self.color, &self.depth),
                unlit(pose_at(x)),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
    }

    fn staged(&self) -> usize {
        self.gpu.views[0].compaction.observed.staged.get()
    }
}

#[test]
fn compacted_and_ranged_velocity_agree_while_an_edit_settles() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut compacted = Rig::new(false);
    let mut ranged = Rig::new(true);
    let mut moving = 0;
    for frame in 0..4 {
        for rig in [&mut compacted, &mut ranged] {
            if frame == 1 {
                rig.edit(0, -4_500);
                rig.edit(5, 3_500);
            }
        }
        let (a, b) = (compacted.velocity(0), ranged.velocity(0));
        assert_eq!(a, b, "frame {frame}");
        let nonzero = a.iter().filter(|texel| **texel != [0.0, 0.0]).count();
        match frame {
            1 => moving = nonzero,
            2..=3 => assert_eq!(nonzero, 0, "frame {frame}"),
            _ => {}
        }
    }
    assert!(moving > 10, "only {moving} moving texels");
}

#[test]
fn the_frame_after_an_edit_gathers_the_previous_transforms_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new(false);
    rig.velocity(0);
    let first = rig.staged();
    rig.velocity(0);
    assert_eq!(rig.staged(), first, "a still frame stages nothing");
    rig.edit(0, -4_500);
    rig.velocity(0);
    let edited = rig.staged();
    assert!(edited > first);
    rig.velocity(0);
    let settled = rig.staged();
    assert!(
        settled > edited,
        "previous transforms converge a frame later"
    );
    rig.velocity(0);
    assert_eq!(rig.staged(), settled);
}

#[test]
fn gpu_gather_with_previous_transforms_matches_plain_copies() {
    use super::super::compaction_gather::{
        assert_gather_matches_copies, fragmented, hide_every_fifth,
    };
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::of(&fragmented());
    hide_every_fifth(&mut rig.gpu);
    // The edited slot's previous transform differs for one frame and converges the next.
    for frame in 0..3 {
        if frame == 1 {
            rig.edit(6, -2_500);
        }
        rig.velocity(0);
        assert_gather_matches_copies(&rig.context, &rig.gpu, true);
    }
}

/// The batch buffers and every buffer the gather adds (the mirror with its previous-transform
/// half, the view buffers and the run lists) count as instance bytes in the scene's census,
/// replaced buffers stop counting when the mirror and the batches grow, and nothing is left
/// once the scene drops.
#[test]
fn gpu_gather_buffers_count_in_the_census_until_the_scene_drops() {
    use super::super::compaction_gather::{fragmented, hide_every_fifth, object};
    use worldrunner_render_gpu::census::ResourceClass;
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::of(&fragmented());
    hide_every_fifth(&mut rig.gpu);
    // Each test context has its own authority, so only this scene counts in its census.
    let census = rig.context.submission().census().clone();
    let counted = |rig: &Rig| {
        let batches: u64 = rig
            .gpu
            .instances
            .batches
            .values()
            .flat_map(|batch| [batch.buffer.as_deref(), batch.previous.as_deref()])
            .map(|buffer| buffer.map_or(0, wgpu::Buffer::size))
            .sum();
        let compaction = rig.gpu.compaction_bytes();
        assert_eq!(
            census.snapshot().get(ResourceClass::Instance),
            batches + compaction.total(),
            "{compaction:?}"
        );
        compaction
    };
    rig.velocity(0);
    let first = counted(&rig);
    assert!(
        first.mirror > 0 && first.views > 0 && first.runs > 0,
        "{first:?}"
    );
    for index in 0..40 {
        rig.gpu.add_instance(object(index), true).unwrap();
    }
    rig.velocity(0);
    let grown = counted(&rig);
    assert!(grown.mirror > first.mirror, "{first:?} {grown:?}");
    drop(rig);
    assert_eq!(census.snapshot().get(ResourceClass::Instance), 0);
}

/// A still scene under a moving camera has nothing to copy into the mirror and nothing to
/// recheck once its batches have been gathered with their previous transforms.
#[test]
fn a_still_scene_under_a_moving_camera_leaves_the_mirror_alone_with_velocity_on() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new(false);
    for _ in 0..3 {
        rig.velocity(0);
    }
    let settled = (
        rig.gpu.instances.content_generation(),
        rig.gpu.mirror_scans(),
        rig.gpu.committed_mirror(),
    );
    for x in [900, 1_800, -1_200, 0, 2_500, 600] {
        rig.velocity(x);
        assert_eq!(rig.gpu.instances.content_generation(), settled.0, "x {x}");
        assert_eq!(rig.gpu.mirror_scans(), settled.1, "x {x}");
        assert_eq!(rig.gpu.committed_mirror(), settled.2, "x {x}");
    }
}

/// Drawing a frame without velocity and then with it again leaves the gather of the previous
/// transforms as ranged drawing reads them.
#[test]
fn toggling_velocity_keeps_compacted_and_ranged_velocity_in_agreement() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut compacted = Rig::new(false);
    let mut ranged = Rig::new(true);
    for (frame, (x, velocity)) in [(0, true), (800, true), (1_600, false), (2_400, true)]
        .into_iter()
        .enumerate()
    {
        if frame == 3 {
            compacted.edit(1, 3_500);
            ranged.edit(1, 3_500);
        }
        let scans = compacted.gpu.mirror_scans();
        for rig in [&mut compacted, &mut ranged] {
            rig.draw(x, velocity);
        }
        if frame == 2 {
            assert!(
                compacted.gpu.mirror_scans() > scans,
                "drawing without velocity went unnoticed"
            );
        }
        if velocity {
            let texture = |rig: &Rig| rig.renderer.targets().velocity_texture().unwrap().clone();
            let (a, b) = (
                read_velocity(&compacted.context, &texture(&compacted)),
                read_velocity(&ranged.context, &texture(&ranged)),
            );
            assert_eq!(a, b, "frame {frame}");
        }
    }
}
