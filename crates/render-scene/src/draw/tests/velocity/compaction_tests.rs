//! Compacted drawing gathers previous-frame transforms like ranged drawing reads them.
//!
//! The previous bytes of an edited slot are rewritten one frame after the edit without any new
//! edit, so the frame after an edit must gather them again for the residual to settle to zero.
use super::super::compaction::{cards, pose_at, unlit};
use super::*;

struct Rig {
    context: GpuContext,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    renderer: FrameRenderer,
    color: AttachmentView,
    depth: AttachmentView,
}

impl Rig {
    fn new(ranged_only: bool) -> Self {
        let context = context();
        let (scene, effects) = with_taa(&cards());
        let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
        let mut gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
        let handles: Vec<_> = gpu.instance_handles().collect();
        for hidden in [4, 3] {
            gpu.set_instance_visible(handles[hidden], false).unwrap();
        }
        gpu.views[0]
            .compaction
            .observed
            .ranged_only
            .set(ranged_only);
        let (renderer, color, depth) = taa_target(&context, &effects, MsaaSamples::X1);
        Self {
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
        let mut encoder = self
            .context
            .submission()
            .begin(self.context.device())
            .unwrap();
        crate::compositor::encode_scene(
            &mut self.renderer,
            self.context.device(),
            &mut encoder,
            (&mut self.gpu, &self.pipelines),
            (&self.color, &self.depth),
            unlit(pose_at(x)),
        )
        .unwrap();
        encoder.finish().submit().unwrap();
        let texture = self.renderer.targets().velocity_texture().unwrap().clone();
        read_velocity(&self.context, &texture)
    }

    fn staged(&self) -> usize {
        self.gpu.views[0].compaction.observed.staged.get()
    }
}

#[test]
fn compacted_and_ranged_velocity_agree_while_an_edit_settles() {
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
