use super::lod_material::scene;
use super::*;
use crate::draw::{PreparedMeshReplacement, ResourceBase};
use crate::offscreen::OffscreenTarget;
use worldrunner_render_plan::MeshHandle;

#[test]
fn replacing_a_mesh_changes_the_casters_and_keeps_the_placements() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = scene(true);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let handles: Vec<_> = gpu.instance_handles().collect();
    let generation = gpu.caster_generation();
    let handle = MeshHandle::from_raw(0);
    let replacement =
        PreparedMeshReplacement::new(&source, handle, &source.geometry().meshes()[0]).unwrap();
    gpu.replace_mesh_prepared(context.device(), context.queue(), replacement)
        .unwrap();
    assert_ne!(gpu.caster_generation(), generation);
    assert_eq!(gpu.instance_handles().collect::<Vec<_>>(), handles);
}

#[test]
fn a_replacement_for_a_missing_mesh_is_refused_and_changes_nothing() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = scene(true);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let base: ResourceBase = gpu.resource_base();
    let generation = gpu.caster_generation();
    let stale = {
        let mut grown = source.clone();
        grown
            .append_resources(worldrunner_render_plan::SceneAppend {
                meshes: vec![source.geometry().meshes()[0].clone()],
                ..Default::default()
            })
            .unwrap();
        PreparedMeshReplacement::new(
            &grown,
            MeshHandle::from_raw(u32::try_from(base.meshes).unwrap()),
            &grown.geometry().meshes()[0],
        )
        .unwrap()
    };
    assert!(
        gpu.replace_mesh_prepared(context.device(), context.queue(), stale)
            .is_err()
    );
    assert_eq!(gpu.resource_base(), base);
    assert_eq!(gpu.caster_generation(), generation);
}

/// The quad and a quarter-size copy, switching at `near_until` millimetres.
fn two_lod_quad(near_until: u64) -> SceneMesh {
    let near = quad().lods()[0].clone();
    let far = SceneMeshLod::new(
        1,
        near.vertices()
            .iter()
            .map(|vertex| SceneVertex {
                tangent_micro: None,
                position_millimeters: vertex.position_millimeters.map(|v| v / 4),
                ..*vertex
            })
            .collect(),
        near.indices().to_vec(),
        None,
    )
    .unwrap();
    SceneMesh::with_distances(vec![near, far], vec![near_until, 20_000]).unwrap()
}

#[test]
fn a_still_view_selects_again_after_its_mesh_is_replaced() {
    let context = context();
    let source = build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, false),
        two_lod_quad(5_000),
        1,
    );
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let mut frame = frame_from(pose());
    frame.unlit = true;
    frame.camera_position = glam::Vec3::new(0.0, 0.0, 5.001);
    frame.camera_position_millimeters = [0, 0, 5_001];
    gpu.set_frame(context.queue(), frame);
    target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
    let replacement =
        PreparedMeshReplacement::new(&source, MeshHandle::from_raw(0), &two_lod_quad(10_000))
            .unwrap();
    gpu.replace_mesh_prepared(context.device(), context.queue(), replacement)
        .unwrap();
    target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![1, 0]);
}
