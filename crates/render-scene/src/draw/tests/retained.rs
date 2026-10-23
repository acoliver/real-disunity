use super::*;
use crate::draw::InstanceError;
use crate::instances::Instances;
use crate::offscreen::OffscreenTarget;

fn moved(x: i64) -> SceneTransform {
    SceneTransform {
        position_millimeters: [x, 0, 0],
        ..identity()
    }
}

fn commit(instances: &mut Instances) {
    for batch in instances.batches.values_mut() {
        let ranges = batch.pending_ranges();
        batch.commit(&ranges);
    }
}

#[test]
fn handles_reject_foreign_removed_and_reused_slots() {
    let source = scene(SceneAlphaMode::Opaque, false, 1);
    let mut instances = Instances::new(&source);
    let mut foreign = Instances::new(&source);
    let handle = instances.handles().next().unwrap();
    assert!(matches!(
        foreign.remove(handle),
        Err(InstanceError::ForeignHandle)
    ));
    instances.set_transform(handle, moved(123)).unwrap();
    instances.set_visible(handle, false).unwrap();
    assert_eq!(
        instances.get(handle).unwrap(),
        (
            SceneObject::new(
                moved(123),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0)
            ),
            false
        )
    );
    instances.remove(handle).unwrap();
    assert!(matches!(
        instances.get(handle),
        Err(InstanceError::StaleHandle)
    ));
    assert!(matches!(
        instances.remove(handle),
        Err(InstanceError::StaleHandle)
    ));
    let replacement = instances.add(source.geometry().objects()[0], true).unwrap();
    assert_ne!(replacement, handle);
    assert_eq!(instances.handles().collect::<Vec<_>>(), vec![replacement]);
    assert!(matches!(
        instances.set_transform(handle, identity()),
        Err(InstanceError::StaleHandle)
    ));
    assert!(matches!(
        instances.set_visible(handle, true),
        Err(InstanceError::StaleHandle)
    ));
}

#[test]
fn invalid_edits_preserve_state_and_resource_tables() {
    let source = scene(SceneAlphaMode::Opaque, false, 1);
    let mut instances = Instances::new(&source);
    let handle = instances.handles().next().unwrap();
    let invalid = SceneTransform {
        scale_millionths: 0,
        ..identity()
    };
    assert!(matches!(
        instances.set_transform(handle, invalid),
        Err(InstanceError::Transform(_))
    ));
    for object in [
        SceneObject::new(
            invalid,
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        ),
        SceneObject::new(
            identity(),
            MeshHandle::from_raw(1),
            MaterialHandle::from_raw(0),
        ),
        SceneObject::new(
            identity(),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(1),
        ),
    ] {
        assert!(instances.add(object, true).is_err());
    }
    assert_eq!(
        instances.get(handle).unwrap(),
        (source.geometry().objects()[0], true)
    );
    assert_eq!(instances.handles().count(), 1);
}

#[test]
fn repeated_edits_merge_only_adjacent_changed_slots() {
    let source = scene(SceneAlphaMode::Opaque, false, 5);
    let mut instances = Instances::new(&source);
    let handles: Vec<_> = instances.handles().collect();
    commit(&mut instances);
    for index in [0, 1, 3] {
        instances.set_transform(handles[index], moved(100)).unwrap();
        instances.set_transform(handles[index], moved(200)).unwrap();
    }
    instances.set_transform(handles[4], moved(100)).unwrap();
    instances.set_transform(handles[4], identity()).unwrap();
    let batch = instances.batches.values().next().unwrap();
    assert_eq!(batch.pending_ranges(), vec![0..2, 3..4]);
    commit(&mut instances);
    assert!(
        instances
            .batches
            .values()
            .all(|batch| batch.pending_ranges().is_empty())
    );
}

#[test]
fn removal_visibility_and_lowest_free_slot_preserve_survivor_order() {
    let source = scene(SceneAlphaMode::Opaque, false, 5);
    let mut instances = Instances::new(&source);
    let handles: Vec<_> = instances.handles().collect();
    commit(&mut instances);
    instances.remove(handles[3]).unwrap();
    instances.remove(handles[1]).unwrap();
    instances.set_visible(handles[2], false).unwrap();
    let batch = instances.batches.values().next().unwrap();
    assert_eq!(batch.visible_ranges(), vec![0..1, 4..5]);
    assert!(batch.pending_ranges().is_empty());
    let replacement = instances.add(source.geometry().objects()[0], true).unwrap();
    instances.set_transform(replacement, moved(200)).unwrap();
    let batch = instances.batches.values().next().unwrap();
    assert_eq!(batch.visible_ranges(), vec![0..2, 4..5]);
    assert_eq!(batch.pending_ranges(), vec![1..2]);
}

#[test]
fn batches_sort_by_pipeline_mesh_and_material_even_for_late_additions() {
    let resources = SceneResources::new(
        vec![white_texture()],
        vec![
            material(SceneAlphaMode::Opaque, false),
            material(SceneAlphaMode::Opaque, true),
        ],
    )
    .unwrap();
    let source = RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![quad(), quad()], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut instances = Instances::new(&source);
    for (mesh, material) in [(1, 1), (1, 0), (0, 0), (0, 1)] {
        instances
            .add(
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(mesh),
                    MaterialHandle::from_raw(material),
                ),
                true,
            )
            .unwrap();
    }
    assert_eq!(
        instances.batches.keys().copied().collect::<Vec<_>>(),
        vec![(0, 0, 0), (0, 1, 0), (1, 0, 1), (1, 1, 1)]
    );
}

#[test]
fn gpu_transform_visibility_growth_and_removal_use_sparse_writes() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(context.device(), context.queue(), &source, &pipelines).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let handles: Vec<_> = gpu.instance_handles().collect();
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let initial = target.render(&context, &gpu, &pipelines).unwrap();
    assert!(lit_pixels(&initial.pixels) > 0);
    gpu.set_instance_transform(handles[0], moved(100_000))
        .unwrap();
    let writes = gpu.flush_instances(context.device(), context.queue());
    assert_eq!(
        writes
            .iter()
            .map(|write| write.instances.clone())
            .collect::<Vec<_>>(),
        vec![0..1]
    );
    assert_eq!(target.render(&context, &gpu, &pipelines).unwrap(), initial);
    gpu.set_instance_visible(handles[1], false).unwrap();
    assert!(
        gpu.flush_instances(context.device(), context.queue())
            .is_empty()
    );
    assert_eq!(
        lit_pixels(&target.render(&context, &gpu, &pipelines).unwrap().pixels),
        0
    );
    gpu.set_instance_transform(handles[0], identity()).unwrap();
    gpu.flush_instances(context.device(), context.queue());
    assert_eq!(target.render(&context, &gpu, &pipelines).unwrap(), initial);
    assert_gpu_growth(
        &context,
        &mut gpu,
        &source,
        &mut target,
        &pipelines,
        &initial,
    );
    gpu.remove_instance(handles[0]).unwrap();
    assert!(
        gpu.flush_instances(context.device(), context.queue())
            .is_empty()
    );
    assert_eq!(
        lit_pixels(&target.render(&context, &gpu, &pipelines).unwrap().pixels),
        0
    );
}

fn assert_gpu_growth(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    source: &RenderScene,
    target: &mut OffscreenTarget,
    pipelines: &ScenePipelines,
    initial: &crate::offscreen::RgbaImage,
) {
    let mut additions = Vec::new();
    for _ in 0..8 {
        additions.push(
            gpu.add_instance(
                SceneObject::new(
                    moved(100_000),
                    source.geometry().objects()[0].mesh(),
                    MaterialHandle::from_raw(0),
                ),
                true,
            )
            .unwrap(),
        );
    }
    let writes = gpu.flush_instances(context.device(), context.queue());
    assert_eq!(
        writes
            .iter()
            .map(|write| write.instances.clone())
            .collect::<Vec<_>>(),
        vec![2..10]
    );
    assert_eq!(&target.render(context, gpu, pipelines).unwrap(), initial);
    assert!(
        gpu.flush_instances(context.device(), context.queue())
            .is_empty()
    );
    for handle in additions {
        gpu.remove_instance(handle).unwrap();
    }
}
