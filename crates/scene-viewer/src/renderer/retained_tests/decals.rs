use super::*;
use crate::{DecalUpdate, RetainedHost, RetainedUpdate};

fn source_with_normal_controls() -> RenderScene {
    use worldrunner_render_plan::{SceneNormalEncoding, SceneNormalY};
    let base = source();
    let mut textures = base.resources().textures().to_vec();
    let normal_handle = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128, 128].into(),
    });
    let normal = material(0, Some(normal_handle), None);
    let mut materials = base.resources().materials().to_vec();
    materials.extend([
        normal,
        normal.with_normal_encoding(SceneNormalEncoding::Rgb, SceneNormalY::Up),
        normal.with_normal_encoding(SceneNormalEncoding::TwoChannelXy, SceneNormalY::Down),
        normal.with_normal_scale(0).unwrap(),
    ]);
    base.with_resources(
        SceneResources::new(textures, materials).unwrap(),
        base.geometry().clone(),
        base.effects().cloned(),
    )
    .unwrap()
}

#[test]
fn unsupported_decal_normal_controls_preserve_scene_and_pending_edits() {
    let scene = source_with_normal_controls();
    let first = u32::try_from(scene.resources().materials().len()).unwrap() - 4;
    let mut host = RetainedHost::new(scene, crate::tests::options()).unwrap();
    host.apply(RetainedUpdate::Decal(DecalUpdate::Add {
        id: 7,
        decal: decal(first),
    }))
    .unwrap();
    let previous = host.scene.clone();
    let pending = format!("{:?}", host.readiness.queue.front().unwrap().update);
    for material in first + 1..first + 4 {
        for update in [
            DecalUpdate::Add {
                id: 8,
                decal: decal(material),
            },
            DecalUpdate::Replace {
                id: 7,
                decal: decal(material),
            },
        ] {
            assert!(host.apply(RetainedUpdate::Decal(update)).is_err());
            assert_eq!(host.scene, previous);
            assert_eq!(host.accepted_generation(), 1);
            assert_eq!(host.preparations_in_flight(), 1);
            assert_eq!(host.readiness.queue.len(), 1);
            let queued = host.readiness.queue.front().unwrap();
            assert_eq!(queued.generation, 1);
            assert_eq!(format!("{:?}", queued.update), pending);
            assert_eq!(queued.decal_ids, vec![7]);
            assert!(queued.scene.is_none());
        }
    }
}

fn decal(material: u32) -> SceneDecal {
    SceneDecal {
        priority: 0,
        transform: SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        dimensions_millimeters: [1000; 3],
        material: MaterialHandle::from_raw(material),
        axis: SceneDecalAxis::Z,
        edge_fade_micro: 0,
        angle_cutoff_micro: 0,
        angle_fade_micro: 0,
        receivers: SceneDecalReceivers::ALL,
        mapping: worldrunner_render_plan::SceneDecalMapping::default(),
    }
}
fn material(
    albedo: u32,
    normal: Option<TextureHandle>,
    packed: Option<TextureHandle>,
) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal,
        roughness_metallic: packed,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}
fn source_with_materials() -> RenderScene {
    let base = source();
    let mut textures = base.resources().textures().to_vec();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![255; 8].into(),
    });
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255, 0, 0, 255].into(),
    });
    let mut materials = base.resources().materials().to_vec();
    materials.extend([
        material(0, Some(TextureHandle::from_raw(0)), None),
        material(0, None, Some(TextureHandle::from_raw(0))),
        material(1, None, None),
        material(2, None, None),
    ]);
    base.with_resources(
        SceneResources::new(textures, materials).unwrap(),
        base.geometry().clone(),
        base.effects().cloned(),
    )
    .unwrap()
}

#[test]
fn unsupported_maps_and_incompatible_arrays_reject_before_acceptance_then_valid_edit_publishes() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let scene = source_with_materials().with_decals(vec![decal(0)]).unwrap();
    let mut host = RetainedHost::new(scene.clone(), crate::tests::options()).unwrap();
    for material in [1, 2, 3] {
        assert!(
            host.apply(RetainedUpdate::Decal(DecalUpdate::Add {
                id: 10,
                decal: decal(material)
            }))
            .is_err()
        );
        assert_eq!(host.scene, scene);
        assert_eq!(host.accepted_generation(), 0);
        assert_eq!(host.preparations_in_flight(), 0);
        assert!(host.readiness.queue.is_empty());
    }
    host.apply(RetainedUpdate::Decal(DecalUpdate::Replace {
        id: 0,
        decal: decal(4),
    }))
    .unwrap();
    assert!(host.readiness.queue.front().unwrap().scene.is_none());
    let mut resident = Resident::new(&context, &scene, None, crate::FrameProfile::Legacy).unwrap();
    let mut streaming = super::super::readiness::Streaming::new(None, crate::FrameProfile::Legacy);
    publish(&context, &mut streaming, &mut resident, &mut host);
    assert_eq!(host.ready_generation(), 1);
    assert_eq!(resident.scene_mut().decal_placements(), vec![(0, decal(4))]);
    assert!(host.readiness.queue.is_empty());
}

#[test]
fn retained_decal_add_transform_and_remove_publish_without_preparing_the_scene() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let scene = source_with_materials();
    let mut host = RetainedHost::new(scene.clone(), crate::tests::options()).unwrap();
    let mut resident = Resident::new(&context, &scene, None, crate::FrameProfile::Legacy).unwrap();
    let mut streaming = super::super::readiness::Streaming::new(None, crate::FrameProfile::Legacy);
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let initial = frame(&mut resident, &context, &mut target);
    host.apply(RetainedUpdate::Decal(DecalUpdate::Add {
        id: 20,
        decal: decal(4),
    }))
    .unwrap();
    publish(&context, &mut streaming, &mut resident, &mut host);
    let projected = frame(&mut resident, &context, &mut target);
    assert_ne!(projected, initial);
    let mut moved = decal(4);
    moved.transform.position_millimeters[0] = 300;
    host.apply(RetainedUpdate::Decal(DecalUpdate::Replace {
        id: 20,
        decal: moved,
    }))
    .unwrap();
    publish(&context, &mut streaming, &mut resident, &mut host);
    assert_ne!(frame(&mut resident, &context, &mut target), projected);
    host.apply(RetainedUpdate::Resources {
        resources: scene.resources().clone(),
        meshes: scene.geometry().meshes().to_vec(),
    })
    .unwrap();
    publish(&context, &mut streaming, &mut resident, &mut host);
    assert_eq!(resident.scene_mut().decal_placements(), vec![(20, moved)]);
    host.apply(RetainedUpdate::Decal(DecalUpdate::Remove(20)))
        .unwrap();
    publish(&context, &mut streaming, &mut resident, &mut host);
    assert_eq!(frame(&mut resident, &context, &mut target), initial);
    assert_eq!(host.ready_generation(), 4);
}
