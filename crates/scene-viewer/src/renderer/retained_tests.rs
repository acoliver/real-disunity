use super::*;
use worldrunner_render_plan::*;
use worldrunner_render_scene::offscreen::{OffscreenTarget, RgbaImage};

fn source() -> RenderScene {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into_boxed_slice(),
    };
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let vertices = [
        [-1000, -1000, 0],
        [1000, -1000, 0],
        [1000, 1000, 0],
        [-1000, 1000, 0],
    ]
    .into_iter()
    .map(|position| SceneVertex {
        position_millimeters: position,
        normal_micro: [0, 0, 1_000_000],
        texcoord_micro: [0, 0],
    })
    .collect();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let transform = SceneTransform {
        position_millimeters: [0; 3],
        rotation: SceneRotation::IDENTITY,
        scale_millionths: 1_000_000,
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material]).unwrap(),
        geometry: SceneGeometry::new(
            vec![mesh],
            vec![SceneObject::new(
                transform,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: crate::tests::scene(false).effects().cloned(),
    })
    .unwrap()
}

fn frame(resident: &mut Resident, context: &GpuContext, target: &mut OffscreenTarget) -> RgbaImage {
    let options = crate::tests::options();
    let view = crate::OrbitCamera::new(options.pose, options.orbit)
        .unwrap()
        .view(options.camera, target.viewport())
        .unwrap();
    let targets = crate::presentation::Targets {
        color: target.color_view().clone(),
        depth: target.depth_view().clone(),
    };
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    resident
        .render(context, &view, &targets, &mut encoder)
        .unwrap();
    context.queue().submit([encoder.finish()]);
    target.read_rgba8(context).unwrap()
}

fn shadow_settings() -> DirectionalShadowSettings {
    DirectionalShadowSettings {
        focus_meters: [0.0, 0.0, 0.0].into(),
        half_extent_meters: 4.0,
        depth_half_extent_meters: 4.0,
        resolution: 128,
        depth_bias: 0.001,
        normal_bias_meters: 0.01,
    }
}

#[test]
fn viewer_callback_edits_flush_in_plain_and_shadowed_residents() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for shadows in [None, Some(shadow_settings())] {
        let mut resident = Resident::new(&context, &source(), shadows).unwrap();
        let mut target = OffscreenTarget::new(
            context.device(),
            Viewport {
                width: 64,
                height: 64,
            },
        )
        .unwrap();
        let initial = frame(&mut resident, &context, &mut target);
        assert!(
            initial
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel[..3] != [0, 0, 0])
        );
        let handle = resident.scene_mut().instance_handles().next().unwrap();
        let mut update: crate::SceneUpdate =
            Box::new(move |gpu| gpu.set_instance_visible(handle, false));
        update(resident.scene_mut()).unwrap();
        let hidden = frame(&mut resident, &context, &mut target);
        assert!(
            hidden
                .pixels
                .chunks_exact(4)
                .all(|pixel| pixel[..3] == [0, 0, 0])
        );
        let (object, _) = resident.scene_mut().instance(handle).unwrap();
        resident
            .scene_mut()
            .set_instance_transform(
                handle,
                SceneTransform {
                    position_millimeters: [100_000, 0, 0],
                    ..*object.transform()
                },
            )
            .unwrap();
        resident
            .scene_mut()
            .set_instance_visible(handle, true)
            .unwrap();
        assert_eq!(frame(&mut resident, &context, &mut target), hidden);
        assert!(
            resident
                .scene_mut()
                .flush_instances(context.device(), context.queue())
                .is_empty()
        );
    }
}
