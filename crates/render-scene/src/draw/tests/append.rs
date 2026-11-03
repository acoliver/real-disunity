use super::lod_material::scene;
use super::*;
use crate::draw::{PreparedMeshReplacement, ResourceBase};
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
            MeshHandle::from_raw(base.meshes as u32),
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
