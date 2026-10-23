use super::*;
use worldrunner_render_plan::*;
use worldrunner_render_scene::offscreen::{OffscreenTarget, RgbaImage};

fn terrain_source() -> RenderScene {
    let base = source();
    let mut textures = base.resources().textures().to_vec();
    for bytes in [[128, 128, 255, 255], [0, 255, 0, 255]] {
        textures.push(SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: Vec::from(bytes).into(),
        });
    }
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -1_000,
        origin_north_millimeters: -1_000,
        sample_spacing_millimeters: 2_000,
        heights_millimeters: vec![-300; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material]).unwrap(),
        geometry: base.geometry().clone(),
        terrain: Some(terrain),
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn terrain_viewer_validates_and_preserves_readbacks_and_resident_edits() {
    let source = terrain_source();
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let viewport = Viewport {
        width: 64,
        height: 64,
    };
    let options = crate::tests::options();
    let view = crate::OrbitCamera::new(options.pose, options.orbit)
        .unwrap()
        .view(options.camera, viewport)
        .unwrap();
    let expected =
        worldrunner_render_scene::offscreen::capture_scene(&context, &source, &view).unwrap();
    for shadows in [None, Some(shadow_settings())] {
        let options = crate::ViewerOptions {
            shadows,
            ..crate::tests::options()
        };
        crate::validate(&source, &options).unwrap();
        let mut resident = Resident::new(&context, &source, shadows).unwrap();
        let mut target = OffscreenTarget::new(context.device(), viewport).unwrap();
        assert_eq!(frame(&mut resident, &context, &mut target), expected);
        let handle = resident.scene_mut().instance_handles().next().unwrap();
        resident
            .scene_mut()
            .set_instance_visible(handle, false)
            .unwrap();
        let terrain_only = frame(&mut resident, &context, &mut target);
        assert_ne!(terrain_only, expected);
        assert!(
            terrain_only
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel[..3] != [0, 0, 0])
        );
        resident
            .scene_mut()
            .set_instance_visible(handle, true)
            .unwrap();
        assert_eq!(frame(&mut resident, &context, &mut target), expected);
    }
}

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
    let mut scheduler = worldrunner_render_scene::compositor::FrameRenderer::new(context.device());
    resident
        .render(context, &view, &targets, &mut encoder, &mut scheduler)
        .unwrap();
    target.encode_readback(&mut encoder).unwrap();
    let submission = context.queue().submit([encoder.finish()]);
    target.read_encoded(context, submission).unwrap()
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

#[test]
fn shadowed_viewer_encodes_geometry_output_and_capture_in_one_host_submission() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let size = Viewport {
        width: 64,
        height: 64,
    };
    let options = crate::tests::options();
    let view = crate::OrbitCamera::new(options.pose, options.orbit)
        .unwrap()
        .view(options.camera, size)
        .unwrap();
    let mut resident = Resident::new(&context, &source(), Some(shadow_settings())).unwrap();
    let mut presentation = crate::presentation::Presentation::new(context.device(), COLOR, size);
    let mut output = OffscreenTarget::new(context.device(), size).unwrap();
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    resident
        .render(
            &context,
            &view,
            &presentation.targets,
            &mut encoder,
            &mut presentation.scheduler,
        )
        .unwrap();
    if let Resident::Shadowed(renderer) = &resident {
        assert_eq!(renderer.cascade_view_stats(3).considered, 1);
    }
    presentation.present(&mut encoder, output.color_view());
    output.encode_readback(&mut encoder).unwrap();
    let submission = context.queue().submit([encoder.finish()]);
    let image = output.read_encoded(&context, submission).unwrap();
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel[..3] != [0, 0, 0])
    );
}
