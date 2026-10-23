use super::*;
use crate::offscreen::OffscreenTarget;

#[test]
fn gpu_switches_geometry_at_the_authored_inclusive_threshold() {
    let context = context();
    let near = quad().lods()[0].clone();
    let far = SceneMeshLod::new(
        1,
        near.vertices()
            .iter()
            .map(|vertex| SceneVertex {
                position_millimeters: vertex.position_millimeters.map(|v| v / 4),
                ..*vertex
            })
            .collect(),
        near.indices().to_vec(),
        None,
    )
    .unwrap();
    let mesh = SceneMesh::with_distances(vec![near, far], vec![5_000, 20_000]).unwrap();
    let source = build(
        white_texture(),
        material(SceneAlphaMode::Opaque, false),
        mesh,
        1,
    );
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.queue(), &source, &pipelines).unwrap();
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    // Hold projection fixed to measure the LOD geometry change independently
    // of apparent size or lighting changes caused by moving a physical camera.
    let mut frame = frame_from(pose());
    frame.unlit = true;
    frame.camera_position = glam::Vec3::new(0.0, 0.0, 5.0);
    frame.camera_position_millimeters = [0, 0, 5_000];
    gpu.set_frame(context.queue(), frame);
    let near = target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![1, 0]);
    frame.camera_position.z = 5.001;
    frame.camera_position_millimeters[2] = 5_001;
    gpu.set_frame(context.queue(), frame);
    let far = target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
    assert!(lit_pixels(&far.pixels) > 0);
    assert!(lit_pixels(&near.pixels) > lit_pixels(&far.pixels) * 4);
    frame.camera_position.z = 5.0;
    frame.camera_position_millimeters[2] = 5_000;
    gpu.set_frame(context.queue(), frame);
    assert_eq!(target.render(&context, &gpu, &pipelines).unwrap(), near);
}

#[test]
fn gpu_selected_level_owns_its_billboard_policy() {
    let context = context();
    let near = card(None).lods()[0].clone();
    let far = SceneMeshLod::new(
        1,
        near.vertices().to_vec(),
        near.indices().to_vec(),
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap();
    let mesh = SceneMesh::with_distances(vec![near, far], vec![3_000, 20_000]).unwrap();
    let source = RenderScene::new(SceneParts {
        geometry: SceneGeometry::new(
            vec![mesh],
            vec![SceneObject::new(
                SceneTransform {
                    rotation: turned_away(),
                    ..identity()
                },
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.queue(), &source, &pipelines).unwrap();
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    assert_eq!(
        lit_pixels(&target.render(&context, &gpu, &pipelines).unwrap().pixels),
        0
    );
    gpu.set_frame(
        context.queue(),
        frame_from(CameraPose {
            eye_millimeters: [0, -4_000, 2_000],
            target_millimeters: [0, 0, 1_000],
        }),
    );
    assert!(lit_pixels(&target.render(&context, &gpu, &pipelines).unwrap().pixels) > 0);
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
}
