//! Mixed rigid and skinned publication after a physically submitted prefix.

use super::publication::completed_color;
use super::retained_retry::{
    CallerPreparation, CallerRegistration, assert_preparations, refuse_caller_suffix,
};
use super::*;
use crate::draw::tests::skinning::{ribbon, shifted, skeleton};
use worldrunner_render_plan::{SceneAffineMatrix, SceneGeometry, SceneSkinPose, SkinPoseHandle};

#[derive(Debug, PartialEq, Eq)]
struct Products {
    color: Vec<u8>,
    g3: Vec<u8>,
}

fn ribbon_object(pose: u32) -> SceneObject {
    fixture::object([1_000, 0, 0], 1, 0).with_skin_pose(SkinPoseHandle::from_raw(pose))
}

fn mixed_source() -> RenderScene {
    let base = render_source();
    let poses = [0, 500]
        .into_iter()
        .map(|shift| {
            let palette = vec![SceneAffineMatrix::IDENTITY, shifted(shift)];
            SceneSkinPose::new(palette.clone(), palette).unwrap()
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![fixture::quad(400), ribbon()],
            vec![fixture::object([-1_500, 0, 0], 0, 0), ribbon_object(0)],
            vec![skeleton(2)],
            poses,
        )
        .unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn read_g3(context: &GpuContext, frame: &FrameRenderer) -> Vec<u8> {
    read(
        context,
        frame
            .feature_target(FeatureTarget::Gbuffer(GbufferTarget::G3))
            .unwrap(),
    )
}

fn completed(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    id: u64,
) -> Products {
    let color = completed_color(context, frame, renderer, id);
    let objects = renderer.gpu.statistics().passes.prepass_objects;
    assert_eq!(objects.recording, crate::draw::ObjectRecording::Compacted);
    assert_eq!(
        objects.meshes,
        crate::draw::DrawCount {
            calls: 2,
            instances: 2,
        }
    );
    assert_eq!(renderer.view_stats().drawn_per_lod, vec![2]);
    Products {
        color,
        g3: read_g3(context, frame),
    }
}

fn move_to_b(renderer: &mut ShadowedScene) {
    let gpu = renderer.scene_mut();
    let handles: Vec<_> = gpu.instance_handles().collect();
    assert_eq!(handles.len(), 2);
    let mut moved = *gpu.instance(handles[0]).unwrap().0.transform();
    moved.position_millimeters[0] += 400;
    gpu.set_instance_transform(handles[0], moved).unwrap();
    let previous = *gpu.instance(handles[1]).unwrap().0.transform();
    let updated = ribbon_object(1);
    assert_eq!(updated.transform(), &previous);
    gpu.set_instance_object(handles[1], updated).unwrap();
}

fn refused_prefix(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    initial: &Products,
) -> Vec<u8> {
    let statistics = renderer.gpu.statistics();
    let selection = renderer.view_stats();
    refuse_caller_suffix(context, frame, renderer);
    assert_eq!(renderer.gpu.statistics(), statistics);
    assert_eq!(renderer.view_stats(), selection);
    assert_eq!(
        read(context, frame.targets().scene().texture()),
        initial.color,
        "refused suffix changed the completed A color"
    );
    read_g3(context, frame)
}

fn schedule(context: &GpuContext, source: &RenderScene, refusal: bool) -> (Products, Products) {
    let (mut frame, mut renderer) = fitted_frame(context, source);
    assert!(frame.targets().velocity_texture().is_none());
    let initial = completed(context, &mut frame, &mut renderer, 0);
    let caller = refusal.then(CallerPreparation::refusing);
    if let Some(caller) = &caller {
        frame
            .register(
                context.device(),
                Box::new(CallerRegistration(caller.clone())),
            )
            .unwrap();
    }
    assert_preparations(caller.as_ref(), 0);
    move_to_b(&mut renderer);
    let prefix = caller
        .as_ref()
        .map(|_| refused_prefix(context, &mut frame, &mut renderer, &initial));
    assert_preparations(caller.as_ref(), 1);
    let fresh = completed(context, &mut frame, &mut renderer, 1);
    assert_preparations(caller.as_ref(), 2);
    if let Some(prefix) = prefix {
        assert_eq!(prefix, fresh.g3, "retry changed the submitted B prefix");
    }
    let statistics = renderer.gpu.statistics();
    let selection = renderer.view_stats();
    assert_eq!(
        completed(context, &mut frame, &mut renderer, 2),
        fresh,
        "unchanged B completion changed its color or G3"
    );
    assert_preparations(caller.as_ref(), 3);
    assert_eq!(renderer.gpu.statistics(), statistics);
    assert_eq!(renderer.view_stats(), selection);
    (initial, fresh)
}

fn assert_regions_changed(before: &Products, after: &Products) {
    assert_eq!(before.g3.len(), after.g3.len());
    assert_eq!(before.color.len(), after.color.len());
    assert_eq!(before.color.len(), before.g3.len() * 4);
    let width = usize::try_from(view().viewport().width).unwrap();
    // Rigid geometry stays left of x = 0; only joint 1 changes the right-hand silhouettes.
    for (label, columns) in [
        ("rigid", 0..width / 2),
        ("weighted ribbon", width / 2 + 1..width),
    ] {
        let mut coverage = [0_usize; 2];
        let mut changed = [0_usize; 2];
        for (index, (old, new)) in before
            .g3
            .chunks_exact(2)
            .zip(after.g3.chunks_exact(2))
            .enumerate()
        {
            if !columns.contains(&(index % width)) {
                continue;
            }
            coverage[0] += usize::from(u16::from_le_bytes([old[0], old[1]]) != 0);
            coverage[1] += usize::from(u16::from_le_bytes([new[0], new[1]]) != 0);
            changed[0] += usize::from(old != new);
            let bytes = index * 8..(index + 1) * 8;
            changed[1] += usize::from(before.color[bytes.clone()] != after.color[bytes]);
        }
        assert!(
            coverage.iter().all(|count| *count > 0),
            "{label} is invisible"
        );
        assert!(changed[0] > 0, "{label} G3 did not change");
        assert!(changed[1] > 0, "{label} color did not change");
    }
}

#[test]
fn gpu_mixed_rigid_skinned_pose_switch_survives_caller_prefix_refusal() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let source = mixed_source();
    let control = schedule(&context, &source, false);
    let retry = schedule(&context, &source, true);
    assert_regions_changed(&control.0, &control.1);
    assert_eq!(retry.0, control.0, "completed A products");
    assert_eq!(retry.1, control.1, "completed B products");
    assert!(pollster::block_on(scope.pop()).is_none());
}
