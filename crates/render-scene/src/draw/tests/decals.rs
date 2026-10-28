use super::*;
use glam::Vec3;
use worldrunner_render_plan::{SceneDecal, SceneDecalAxis, SceneDecalReceivers, TexelFormat};
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef};

fn texel(rgba: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: rgba.to_vec().into(),
    }
}
pub(crate) fn decal() -> SceneDecal {
    SceneDecal {
        priority: 0,
        transform: identity(),
        dimensions_millimeters: [1000, 1200, 1000],
        material: MaterialHandle::from_raw(1),
        axis: SceneDecalAxis::Z,
        edge_fade_micro: 200_000,
        angle_cutoff_micro: 600_000,
        angle_fade_micro: 400_000,
        receivers: SceneDecalReceivers::ALL,
    }
}
pub(crate) fn source(slope: bool, terrain_receiver: bool) -> RenderScene {
    let mut vertices = quad().lods()[0].vertices().to_vec();
    if slope {
        for vertex in &mut vertices {
            vertex.position_millimeters[2] = vertex.position_millimeters[0] / 2;
            vertex.normal_micro = [-447_214, 0, 894_427];
        }
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let mut base = material(SceneAlphaMode::Opaque, false);
    if terrain_receiver {
        base = SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: Some(TextureHandle::from_raw(2)),
            roughness_metallic: Some(TextureHandle::from_raw(3)),
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 1_000_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: false,
        })
        .unwrap();
    }
    let decal_material = projection_material();
    let terrain = terrain_receiver.then(|| receiver_terrain(slope));
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                texel([51, 102, 153, 255]),
                texel([204, 51, 26, 128]),
                texel([128, 128, 255, 255]),
                texel([0, 255, 0, 255]),
            ],
            vec![base, decal_material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![mesh],
            if terrain_receiver {
                vec![]
            } else {
                vec![SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                )]
            },
        )
        .unwrap(),
        terrain,
        effects: None,
    })
    .unwrap()
}
fn capture(context: &GpuContext, scene: &RenderScene, samples: u32) -> Vec<u8> {
    let pipelines = ScenePipelines::with_samples(context.device(), COLOR_FORMAT, samples).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), scene, &pipelines).unwrap();
    capture_gpu(
        context,
        &mut gpu,
        &pipelines,
        samples,
        scene.effects().is_none(),
    )
}
fn capture_gpu(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    samples: u32,
    unlit: bool,
) -> Vec<u8> {
    let config = FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, samples);
    let mut renderer =
        crate::compositor::FrameRenderer::with_output(context.device(), config, COLOR_FORMAT)
            .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut frame = frame_from(pose());
    frame.unlit = unlit;
    crate::compositor::encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (gpu, pipelines),
        (&color, &depth),
        frame,
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    Readback::new(
        context.device(),
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        renderer.targets().texture(),
    )
    .unwrap()
}

// Ray/plane intersection and scalar box distances are independent of the GPU encoder.
fn oracle(x: u32, y: u32, slope: bool) -> (Vec3, f32) {
    let inverse = frame_from(pose()).view_projection.inverse();
    let ndc = Vec3::new(
        (f32::from(u16::try_from(x).unwrap()) + 0.5) / 32.0 - 1.0,
        1.0 - (f32::from(u16::try_from(y).unwrap()) + 0.5) / 32.0,
        0.0,
    );
    let near = inverse.project_point3(ndc);
    let far = inverse.project_point3(ndc + Vec3::Z);
    let normal = if slope {
        Vec3::new(-0.5, 0.0, 1.0).normalize()
    } else {
        Vec3::Z
    };
    let ray = far - near;
    let point = near + ray * (-normal.dot(near) / normal.dot(ray));
    let edge = (0.5 - point.x.abs())
        .min(0.5 - point.y.abs() / 1.2)
        .min(0.5 - point.z.abs());
    let weight =
        (edge / 0.2).clamp(0.0, 1.0) * ((normal.z - 0.6) / 0.4).clamp(0.0, 1.0) * 128.0 / 255.0;
    (point, weight)
}
fn srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}
#[test]
fn volume_coverage_matches_plane_and_slope_oracle_for_mesh_and_terrain_at_1x_and_4x() {
    let context = context();
    for terrain in [false, true] {
        for slope in [false, true] {
            let base = source(slope, terrain);
            let projected = base.with_decals(vec![decal()]).unwrap();
            for samples in [1, 4] {
                let pixels = capture(&context, &projected, samples);
                for (x, y) in [(32, 32), (37, 32), (43, 32), (23, 32), (32, 39)] {
                    let (point, weight) = oracle(x, y, slope);
                    assert!(point.x.abs() < 0.95 && point.y.abs() < 0.95);
                    let expected = Vec3::new(51.0, 102.0, 153.0)
                        .lerp(Vec3::new(204.0, 51.0, 26.0), weight)
                        / 255.0;
                    let offset = usize::try_from((y * TARGET + x) * 4).unwrap();
                    for (actual, linear) in
                        pixels[offset..offset + 3].iter().zip(expected.to_array())
                    {
                        assert!(
                            (f32::from(*actual) - srgb(linear) * 255.0).abs() < 3.5,
                            "terrain={terrain} slope={slope} samples={samples} pixel={x},{y} weight={weight} actual={actual} linear={linear}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn masked_outside_culled_and_removed_decals_preserve_default_bytes() {
    let context = context();
    let scene = source(false, false);
    for samples in [1, 4] {
        let original = capture(&context, &scene, samples);
        let mut masked = decal();
        masked.receivers = SceneDecalReceivers::TERRAIN;
        let mut outside = decal();
        outside.transform.position_millimeters[2] = 2000;
        let mut culled = decal();
        culled.transform.position_millimeters[0] = 1_000_000;
        for decals in [vec![], vec![masked], vec![outside], vec![culled]] {
            assert_eq!(
                original,
                capture(&context, &scene.with_decals(decals).unwrap(), samples)
            );
        }
    }
}
#[test]
fn albedo_and_scalar_material_channels_are_blended_before_lighting() {
    let context = context();
    let base = source(false, false);
    let mut d = decal();
    d.dimensions_millimeters = [4000; 3];
    d.edge_fade_micro = 0;
    let mut textures = base.resources().textures().to_vec();
    textures[1] = texel([204, 51, 26, 255]);
    let base = base
        .with_resources(
            SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
            base.geometry().clone(),
            Some(effects()),
        )
        .unwrap();
    let expected = base
        .with_objects_and_effects(
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(1),
            )],
            Some(effects()),
        )
        .unwrap();
    for samples in [1, 4] {
        assert_eq!(
            capture(&context, &expected, samples),
            capture(&context, &base.with_decals(vec![d]).unwrap(), samples)
        );
    }
}

#[test]
fn unsupported_decal_maps_fail_during_cpu_preparation() {
    let scene = terrain_scene(1);
    assert!(
        super::super::PreparedScene::new(
            &scene
                .with_decals(vec![SceneDecal {
                    material: MaterialHandle::from_raw(0),
                    ..decal()
                }])
                .unwrap()
        )
        .is_err()
    );
}

fn receiver_terrain(slope: bool) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -1000,
        origin_north_millimeters: -1000,
        sample_spacing_millimeters: 2000,
        heights_millimeters: if slope {
            vec![-500, 500, -500, 500]
        } else {
            vec![0; 4]
        },
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn projection_material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 200_000,
        metalness_micro: 800_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

#[test]
fn angle_cutoff_and_both_receiver_masks_reject_without_changing_pixels() {
    let context = context();
    for terrain in [false, true] {
        let scene = source(true, terrain);
        let mut masked = decal();
        masked.receivers = if terrain {
            SceneDecalReceivers::MESH
        } else {
            SceneDecalReceivers::TERRAIN
        };
        let mut grazing = decal();
        grazing.angle_cutoff_micro = 900_000;
        grazing.angle_fade_micro = 100_000;
        let original = capture(&context, &scene, 1);
        for d in [masked, grazing] {
            assert_eq!(
                original,
                capture(&context, &scene.with_decals(vec![d]).unwrap(), 1)
            );
        }
    }
}

#[test]
fn forward_layered_and_cached_pipelines_fit_sixteen_sampled_textures() {
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, _) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    assert_eq!(device.limits().max_sampled_textures_per_shader_stage, 16);
    let shadow = worldrunner_render_shadow::receiver_layout(
        &device,
        worldrunner_render_shadow::ReceiverDescriptor {
            label: None,
            dimension: worldrunner_render_shadow::ReceiverDimension::D2Array,
            texture_binding: 1,
            sampler_binding: 2,
            uniform_binding: 0,
            min_uniform_binding_size: None,
        },
    );
    let screen = crate::ssr::forward::layout(&device);
    let object = crate::ssr::forward::source(
        &crate::ssao::ambient_source(crate::shader::ENVIRONMENT_WGSL),
        false,
    );
    let terrain = crate::ssr::forward::source(
        &crate::ssao::ambient_source(crate::shader::TERRAIN_WGSL),
        true,
    );
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_screen_space(
            &device,
            (TextureFormat::Rgba16Float, samples),
            &object,
            &terrain,
            Some(&shadow),
            Some(&screen),
        );
        assert!(pipelines.layered.is_some());
    }
}

#[test]
fn retained_transform_and_removal_change_pixels_without_unrelated_allocations() {
    let context = context();
    let scene = source(false, true).with_decals(vec![decal()]).unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mesh = gpu.meshes[0].lods[0].vertices.clone();
    let material = gpu.materials[0].clone();
    let texture = gpu.retained.textures[0].clone();
    let terrain = gpu.terrain.as_ref().unwrap().uniform.clone();
    let original = capture_gpu(&context, &mut gpu, &pipelines, 1, true);
    let group = gpu.decals.group();
    let mut moved = decal();
    moved.transform.position_millimeters[0] = 300;
    gpu.set_decal_placements(vec![(0, moved)]).unwrap();
    let edited = capture_gpu(&context, &mut gpu, &pipelines, 1, true);
    assert_ne!(original, edited);
    assert_eq!(group, gpu.decals.group());
    gpu.set_decal_placements(vec![]).unwrap();
    let removed = capture_gpu(&context, &mut gpu, &pipelines, 1, true);
    assert_ne!(removed, edited);
    assert_eq!(removed, capture(&context, &source(false, true), 1));
    assert_eq!(mesh, gpu.meshes[0].lods[0].vertices);
    assert_eq!(material, gpu.materials[0]);
    assert_eq!(texture, gpu.retained.textures[0]);
    assert_eq!(terrain, gpu.terrain.as_ref().unwrap().uniform);
}

#[test]
fn decal_edits_preserve_resident_terrain_pages() {
    let context = context();
    let scene = super::layered_terrain::layered_scene(
        worldrunner_render_plan::TerrainSplatFilter::Bilinear,
    );
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    gpu.enable_terrain_pages(
        context.device(),
        context.submission(),
        &pipelines,
        super::layered_terrain::page_source(),
    )
    .unwrap();
    let frame = frame_from(pose());
    prepare_root_page(&context, &gpu, &frame);
    let initial = capture_gpu(&context, &mut gpu, &pipelines, 1, true);
    let request = gpu
        .terrain_residency_request(&frame, [TARGET; 2])
        .unwrap()
        .unwrap();
    assert!(gpu.terrain_residency_ready(&request).unwrap());
    let table = gpu
        .terrain
        .as_ref()
        .unwrap()
        .pages
        .as_ref()
        .unwrap()
        .borrow()
        .tables[0]
        .clone();
    let d = SceneDecal {
        material: MaterialHandle::from_raw(0),
        ..decal()
    };
    gpu.set_decal_placements(vec![(8, d)]).unwrap();
    capture_gpu(&context, &mut gpu, &pipelines, 1, true);
    let mut moved = d;
    moved.transform.position_millimeters[0] = 300;
    gpu.set_decal_placements(vec![(8, moved)]).unwrap();
    capture_gpu(&context, &mut gpu, &pipelines, 1, true);
    gpu.set_decal_placements(vec![]).unwrap();
    assert_eq!(
        initial,
        capture_gpu(&context, &mut gpu, &pipelines, 1, true)
    );
    assert!(gpu.terrain_residency_ready(&request).unwrap());
    assert_eq!(
        table,
        gpu.terrain
            .as_ref()
            .unwrap()
            .pages
            .as_ref()
            .unwrap()
            .borrow()
            .tables[0]
    );
}

#[test]
fn bounded_uploads_and_shared_palettes_survive_camera_edits_and_cancelled_frames() {
    let context = context();
    let placements: Vec<_> = (0..2000)
        .map(|i| {
            let mut d = decal();
            d.transform.position_millimeters[0] = if i < 40 { 0 } else { 1_000_000 };
            d
        })
        .collect();
    let scene = source(false, false).with_decals(placements).unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let frame = frame_from(pose());
    let bytes = gpu.decals.prepare(&frame, &gpu.frame_buffer);
    assert_eq!(bytes.len(), super::super::decals::BYTES);
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 32);
    let slot0 = gpu.decals.group();
    gpu.select_frame_slot(1);
    let bytes = gpu.decals.prepare(&frame, &gpu.frame_buffer);
    assert_eq!(bytes.len(), super::super::decals::BYTES);
    assert_ne!(slot0, gpu.decals.group());
    let mut transaction = context.submission().begin(context.device()).unwrap();
    let mut upload =
        worldrunner_render_post::PrepareContext::new(context.device(), &mut transaction).unwrap();
    gpu.commit_frame(&mut upload, frame);
    upload.finish();
    drop(transaction);
    let moved = frame_from(CameraPose {
        eye_millimeters: [1_000_000, -2000, 2000],
        target_millimeters: [1_000_000, 0, 0],
    });
    let bytes = gpu.decals.prepare(&moved, &gpu.frame_buffer);
    assert_eq!(bytes.len(), super::super::decals::BYTES);
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 32);
    gpu.select_frame_slot(0);
    assert_eq!(slot0, gpu.decals.group());
    gpu.set_decal_placements(vec![(9000, decal())]).unwrap();
    assert_ne!(
        capture_gpu(&context, &mut gpu, &pipelines, 1, true),
        capture(&context, &source(false, false), 1)
    );
}

fn prepare_root_page(context: &GpuContext, gpu: &SceneGpu, frame: &FrameUniform) {
    let mut transaction = context.submission().begin(context.device()).unwrap();
    gpu.prepare_terrain_pages(context.device(), &mut transaction, frame, [TARGET; 2])
        .unwrap();
    transaction.finish().submit().unwrap();
}
