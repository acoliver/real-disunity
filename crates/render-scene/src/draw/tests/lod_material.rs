use super::foliage::{geometry_sources, pass_depth};
use super::*;
use crate::offscreen::OffscreenTarget;
use worldrunner_render_plan::{SceneLodMaterialSlot, TexelFormat};

fn slot(material: u32, indices: std::ops::Range<u32>) -> SceneLodMaterialSlot {
    SceneLodMaterialSlot::new(MaterialHandle::from_raw(material), indices).unwrap()
}

fn slot_material(texture: u32, alpha_mode: SceneAlphaMode) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided: true,
    })
    .unwrap()
}

fn scene(explicit: bool) -> RenderScene {
    let near = quad().lods()[0].clone();
    let far = SceneMeshLod::new(1, near.vertices().to_vec(), near.indices().to_vec(), None)
        .unwrap()
        .with_material_slots(vec![slot(1, 0..3), slot(2, 3..6)])
        .unwrap();
    let near = if explicit {
        near.with_material_slots(vec![slot(0, 0..6)]).unwrap()
    } else {
        near
    };
    let transparent = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0; 4].into(),
    };
    let clear = slot_material(
        1,
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
    );
    let green = slot_material(2, SceneAlphaMode::Opaque);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture(RED_565), transparent, texture(0x07e0)],
            vec![material(SceneAlphaMode::Opaque, true), clear, green],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![SceneMesh::with_distances(vec![near, far], vec![5_000, 20_000]).unwrap()],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn selection_frame(distance: i64) -> FrameUniform {
    let mut frame = frame_from(pose());
    frame.unlit = true;
    frame.camera_position =
        glam::Vec3::new(0.0, 0.0, crate::geometry::millimeter_to_meter(distance));
    frame.camera_position_millimeters = [0, 0, distance];
    frame
}

#[test]
fn material_slots_preserve_packed_geometry_and_default_pixels() {
    let default = scene(false);
    let explicit = scene(true);
    let plain = super::super::PreparedScene::new(&default).unwrap();
    let bound = super::super::PreparedScene::new(&explicit).unwrap();
    for (a, b) in plain.meshes[0].lods.iter().zip(&bound.meshes[0].lods) {
        assert_eq!(a.vertices, b.vertices);
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.transition, b.transition);
    }
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let mut reference = None;
    for source in [&default, &explicit] {
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), source, &pipelines).unwrap();
        gpu.set_frame(context.queue(), selection_frame(5_000));
        let image = target.render(&context, &gpu, &pipelines).unwrap();
        assert!(image.pixels.chunks_exact(4).any(|p| p == [255, 0, 0, 255]));
        if let Some(reference) = &reference {
            assert_eq!(&image, reference);
        }
        reference = Some(image);
    }
}

#[test]
fn selected_level_changes_material_topology_and_pipeline() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let near = target.render(&context, &gpu, &pipelines).unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_001));
    let far = target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
    assert!(far.pixels.chunks_exact(4).any(|p| p == [0, 255, 0, 255]));
    assert!(!far.pixels.chunks_exact(4).any(|p| p[0] != 0));
    let near_count = near.pixels.chunks_exact(4).filter(|p| p[0] != 0).count();
    let far_count = far.pixels.chunks_exact(4).filter(|p| p[1] != 0).count();
    assert!(far_count > 20 && far_count < near_count * 3 / 5);
}

#[test]
fn slot_cutouts_have_the_same_silhouette_in_every_geometry_pass() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_001));
    let mut reference = None;
    for (index, source) in geometry_sources().iter().enumerate() {
        let entry = if index == 3 {
            "fs_depth_masked"
        } else {
            "fs_masked"
        };
        let depth = pass_depth(&context, &gpu, &pipelines, source, entry, index != 3);
        assert!(depth.chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()));
        if let Some(reference) = &reference {
            assert_eq!(&depth, reference, "geometry pass {index}");
        }
        reference = Some(depth);
    }
}

#[test]
fn scene_rejects_a_level_material_outside_the_resource_table() {
    let source = scene(true);
    let lod = quad().lods()[0]
        .clone()
        .with_material_slots(vec![slot(3, 0..6)])
        .unwrap();
    let geometry = SceneGeometry::new(vec![SceneMesh::new(vec![lod]).unwrap()], vec![]).unwrap();
    assert!(matches!(
        source.with_resources(source.resources().clone(), geometry, None),
        Err(
            worldrunner_render_plan::SceneValidationError::HandleOutOfRange {
                resource: "material",
                handle: 3,
                ..
            }
        )
    ));
}

fn transition_scene(coverage: u32) -> RenderScene {
    use worldrunner_render_plan::SceneCard;
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
    .with_material_slots(vec![slot(2, 0..6)])
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [2000, 2000], coverage).unwrap())
    .unwrap();
    let source = scene(true);
    source
        .with_resources(
            source.resources().clone(),
            SceneGeometry::new(
                vec![SceneMesh::with_distances(vec![near, far], vec![100, 20_000]).unwrap()],
                vec![SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(1),
                )],
            )
            .unwrap(),
            None,
        )
        .unwrap()
}

#[test]
fn transition_complement_keeps_the_preceding_levels_material() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    for (coverage, expected) in [(0, [255, 0, 0, 255]), (1_000_000, [0, 255, 0, 255])] {
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            &transition_scene(coverage),
            &pipelines,
        )
        .unwrap();
        let mut frame = frame_from(pose());
        frame.unlit = true;
        gpu.set_frame(context.queue(), frame);
        let image = target.render(&context, &gpu, &pipelines).unwrap();
        let coloured: Vec<_> = image
            .pixels
            .chunks_exact(4)
            .filter(|p| p[0] != 0 || p[1] != 0)
            .collect();
        assert!(coloured.len() > 20);
        assert!(coloured.iter().all(|p| **p == expected));
    }
}
