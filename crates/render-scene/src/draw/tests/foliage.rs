use super::*;
use worldrunner_render_plan::{SceneAmbientOcclusion, SceneMaterialFeatures, SceneTint, SceneWind};

pub(super) fn cutout_texture() -> SceneTexture {
    let mut bytes = bc1(WHITE_565);
    let offset = bytes.len() - 8;
    bytes[offset..].copy_from_slice(&[0, 0, 255, 255, 0xdd, 0xdd, 0xdd, 0xdd]);
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Srgb,
        bytes: bytes.into(),
    }
}

fn features() -> SceneMaterialFeatures {
    SceneMaterialFeatures {
        tint: SceneTint::WHITE,
        ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
        wind: None,
        layer: None,
        translucency: None,
    }
}

pub(super) fn foliage_scene(
    features: Option<SceneMaterialFeatures>,
    tint: SceneTint,
    ao: u32,
) -> RenderScene {
    let base = quad();
    let lod = base.lods()[0]
        .clone()
        .with_vertex_occlusion(vec![ao; 4])
        .unwrap();
    let mut material = material(
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        true,
    );
    if let Some(features) = features {
        material = material.with_features(features).unwrap();
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![cutout_texture()], vec![material]).unwrap(),
        geometry: SceneGeometry::new(
            vec![SceneMesh::new(vec![lod]).unwrap()],
            vec![
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                )
                .with_tint(tint),
            ],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

#[test]
fn tint_and_vertex_ao_endpoints_and_neutral_output_are_exact() {
    let context = context();
    let stock = foliage_scene(None, SceneTint::WHITE, 1_000_000);
    let neutral = foliage_scene(Some(features()), SceneTint::WHITE, 1_000_000);
    assert_eq!(render(&context, &stock), render(&context, &neutral));
    for tint in [
        SceneTint::new([0; 3]).unwrap(),
        SceneTint::new([0, 1_000_000, 0]).unwrap(),
    ] {
        let instance = foliage_scene(None, tint, 1_000_000);
        let material = foliage_scene(
            Some(SceneMaterialFeatures { tint, ..features() }),
            SceneTint::WHITE,
            1_000_000,
        );
        assert_eq!(render(&context, &instance), render(&context, &material));
    }
    let vertex = foliage_scene(
        Some(SceneMaterialFeatures {
            ambient_occlusion: SceneAmbientOcclusion::new(1_000_000, true).unwrap(),
            ..features()
        }),
        SceneTint::WHITE,
        0,
    );
    let factor = foliage_scene(
        Some(SceneMaterialFeatures {
            ambient_occlusion: SceneAmbientOcclusion::new(0, false).unwrap(),
            ..features()
        }),
        SceneTint::WHITE,
        1_000_000,
    );
    assert_eq!(render(&context, &vertex), render(&context, &factor));
    assert_ne!(render(&context, &vertex), render(&context, &stock));
}

#[test]
fn wind_and_alpha_have_identical_depth_bytes_in_every_geometry_pass() {
    let context = context();
    let wind = SceneWind::new(
        SceneDirection::new([1_000_000, 0, 0]).unwrap(),
        500,
        1000,
        0,
    )
    .unwrap();
    let scene = foliage_scene(
        Some(SceneMaterialFeatures {
            wind: Some(wind),
            ..features()
        }),
        SceneTint::WHITE,
        1_000_000,
    );
    let device = context.device();
    let pipelines = ScenePipelines::new(device, TextureFormat::Rgba8Unorm);
    let gpu = SceneGpu::upload(
        device,
        &worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone()),
        &scene,
        &pipelines,
    )
    .unwrap();
    let mut frame = frame_from(pose());
    frame.animation_time_milliseconds = 250;
    gpu.set_frame(context.queue(), frame);
    let [forward, prepass, metadata, shadow] = geometry_sources();
    let mut reference = None;
    for (source, entry, color) in [
        (forward.as_str(), "fs_masked", true),
        (prepass.as_str(), "fs_masked", true),
        (metadata.as_str(), "fs_masked", true),
        (shadow.as_str(), "fs_depth_masked", false),
    ] {
        let bytes = pass_depth(&context, &gpu, &pipelines, source, entry, color);
        assert!(bytes.chunks_exact(4).any(|v| v != 1.0_f32.to_le_bytes()));
        if let Some(reference) = &reference {
            assert_eq!(&bytes, reference, "{entry}");
        } else {
            reference = Some(bytes);
        }
    }
    frame.animation_time_milliseconds = 0;
    gpu.set_frame(context.queue(), frame);
    let still = pass_depth(&context, &gpu, &pipelines, &forward, "fs_masked", true);
    assert_ne!(
        Some(still),
        reference,
        "fixed-time wind must change the silhouette"
    );
}

pub(super) fn pass_depth(
    context: &GpuContext,
    gpu: &SceneGpu,
    layouts: &ScenePipelines,
    source: &str,
    entry: &str,
    color: bool,
) -> Vec<u8> {
    pass_readback(context, gpu, layouts, source, entry, (color, false))
}

pub(super) fn pass_color(
    context: &GpuContext,
    gpu: &SceneGpu,
    layouts: &ScenePipelines,
    source: &str,
    entry: &str,
) -> Vec<u8> {
    pass_readback(context, gpu, layouts, source, entry, (true, true))
}

fn pass_readback(
    context: &GpuContext,
    gpu: &SceneGpu,
    layouts: &ScenePipelines,
    source: &str,
    entry: &str,
    attachments: (bool, bool),
) -> Vec<u8> {
    let (color, read_color) = attachments;
    let device = context.device();
    let pipelines = pass_pipelines(device, layouts, source, entry, color);
    let depth = coverage_depth(device);
    let color_target = color_target(device);
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let color_view = color_target.create_view(&TextureViewDescriptor::default());
    let colors = if color {
        vec![Some(RenderPassColorAttachment {
            view: &color_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        })]
    } else {
        vec![]
    };
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: u64::from(TARGET * TARGET * 4),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &colors,
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        gpu.draw_objects(&mut pass, &gpu.frame_group, &pipelines, None);
    }
    copy_attachment(
        &mut encoder,
        if read_color { &color_target } else { &depth },
        &readback,
        read_color,
    );
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &readback)
}

fn pass_pipelines(
    device: &Device,
    layouts: &ScenePipelines,
    source: &str,
    entry: &str,
    color: bool,
) -> [wgpu::RenderPipeline; 8] {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[
            Some(layouts.frame_layout()),
            Some(layouts.material_layout()),
        ],
        immediate_size: 0,
    });
    let vertices = crate::geometry::vertex_attributes();
    let instances = crate::geometry::instance_attributes();
    let buffers = crate::renderer::vertex_buffers(&vertices, &instances);
    let targets = if color {
        vec![Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })]
    } else {
        vec![]
    };
    crate::pipeline::PipelineKey::ALL.map(|key| {
        crate::renderer::build(
            device,
            key,
            &layout,
            &buffers,
            1,
            wgpu::FragmentState {
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            },
        )
    })
}

pub(super) fn geometry_sources() -> [String; 4] {
    let forward = crate::shader::ENVIRONMENT_WGSL.to_owned();
    let base = forward
        .replace("fn fs_relief_opaque(", "fn unused_relief_opaque(")
        .replace("fn fs_relief_masked(", "fn unused_relief_masked(")
        .replace("fn fs_opaque(", "fn unused_opaque(")
        .replace("fn fs_masked(", "fn unused_masked(");
    let prepass = format!(
        "{base}\n{}",
        include_str!("../../scene_metadata/geometry.wgsl")
            .replace("METADATA_OUTPUT", "vec4<f32>(normal, 1.0)")
    );
    let metadata = format!(
        "{base}\n{}",
        include_str!("../../scene_metadata/geometry.wgsl")
            .replace("METADATA_OUTPUT", "vec4<f32>(normal, roughness)")
    );
    let shadow = format!(
        "{forward}\n{}",
        include_str!("../../shader/shadow_depth.wgsl")
    );
    [forward, prepass, metadata, shadow]
}

pub(super) fn coverage_depth(device: &Device) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn copy_depth(encoder: &mut CommandEncoder, depth: &Texture, readback: &Buffer) {
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: depth,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        TexelCopyBufferInfo {
            buffer: readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TARGET * 4),
                rows_per_image: None,
            },
        },
        Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
    );
}

fn card_transition_scene(coverage: u32) -> RenderScene {
    use worldrunner_render_plan::SceneCard;
    let near = card(None).lods()[0].clone();
    let far = SceneMeshLod::new(
        1,
        near.vertices().to_vec(),
        near.indices().to_vec(),
        Some(BillboardPolicy::CrossQuads),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::CrossQuads, [2000, 2000], coverage).unwrap())
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![cutout_texture()],
            vec![material(
                SceneAlphaMode::Mask {
                    cutoff_micro: 500_000,
                },
                true,
            )],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![SceneMesh::with_distances(vec![near, far], vec![100, 20_000]).unwrap()],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

#[test]
fn mesh_card_transition_has_exact_complementary_coverage_in_all_passes() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let authority =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone());
    for masked in [false, true] {
        let mut reference = None;
        for coverage in [0, 123_457, 500_000, 999_999, 1_000_000] {
            let scene = card_transition_scene(coverage);
            let scene = if masked {
                scene
            } else {
                with_material(&scene, &material(SceneAlphaMode::Opaque, true))
            };
            let gpu = SceneGpu::upload(context.device(), &authority, &scene, &pipelines).unwrap();
            gpu.set_frame(context.queue(), frame_from(pose()));
            for (index, source) in geometry_sources().iter().enumerate() {
                let entry = match (index == 3, masked) {
                    (false, false) => "fs_opaque",
                    (false, true) => "fs_masked",
                    (true, false) => "fs_depth_opaque",
                    (true, true) => "fs_depth_masked",
                };
                let bytes = pass_depth(&context, &gpu, &pipelines, source, entry, index != 3);
                assert!(
                    bytes.chunks_exact(4).any(|v| v != 1.0_f32.to_le_bytes()),
                    "coverage={coverage}"
                );
                if let Some(reference) = &reference {
                    assert_eq!(
                        &bytes, reference,
                        "coverage={coverage}, pass={index}, masked={masked}"
                    );
                } else {
                    reference = Some(bytes);
                }
            }
        }
    }
}

fn with_material(scene: &RenderScene, material: &SceneMaterial) -> RenderScene {
    let mut materials = scene.resources().materials().to_vec();
    materials[0] = *material;
    RenderScene::new(SceneParts {
        resources: SceneResources::new(scene.resources().textures().to_vec(), materials).unwrap(),
        geometry: scene.geometry().clone(),
        terrain: scene.terrain().cloned(),
        effects: scene.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn foliage_translucency_zero_is_stock_and_one_transmits_backlighting() {
    let context = context();
    let stock = scene(SceneAlphaMode::Opaque, true, 1);
    let base = stock.resources().materials()[0];
    let neutral = with_material(&stock, &base.with_translucency(0).unwrap());
    let transmitted = with_material(&stock, &base.with_translucency(1_000_000).unwrap());
    let mut frame = frame_from(pose());
    frame.sun_direction = glam::Vec3::NEG_Z;
    frame.sun_irradiance = glam::Vec3::splat(0.5);
    frame.ambient_sky = glam::Vec3::ZERO;
    frame.ambient_ground = glam::Vec3::ZERO;
    frame.exposure = 1.0;
    assert_eq!(
        render_frame(&context, &stock, &frame),
        render_frame(&context, &neutral, &frame)
    );
    assert_ne!(
        render_frame(&context, &stock, &frame),
        render_frame(&context, &transmitted, &frame)
    );
}

pub(super) fn contact_scene(height: i64, contact: bool, black: bool) -> RenderScene {
    use worldrunner_render_plan::SceneTerrainContact;
    let mut material = material(SceneAlphaMode::Opaque, false);
    if contact {
        material = material.with_terrain_contact(
            SceneTerrainContact::new(SceneTint::new([0; 3]).unwrap(), 1_000_000, 0, 500).unwrap(),
        );
    }
    if black {
        material = material
            .with_features(SceneMaterialFeatures {
                tint: SceneTint::new([0; 3]).unwrap(),
                ..features()
            })
            .unwrap();
    }
    let mut placement = identity();
    placement.position_millimeters[2] = height;
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -4000,
        origin_north_millimeters: -4000,
        sample_spacing_millimeters: 8000,
        heights_millimeters: vec![-500; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(1),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture(), flat_normal_texture(), white_texture()],
            vec![material, terrain_material()],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad()],
            vec![SceneObject::new(
                placement,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: Some(terrain),
        effects: Some(effects()),
    })
    .unwrap()
}

#[test]
fn terrain_contact_full_and_zero_endpoints_match_exact_material_bytes() {
    let context = context();
    let mut frame = frame_from(pose());
    frame.unlit = true;
    frame.exposure = 1.0;
    for (height, black) in [(-500, true), (0, false)] {
        assert_eq!(
            render_frame(&context, &contact_scene(height, true, false), &frame),
            render_frame(&context, &contact_scene(height, false, black), &frame)
        );
    }
}

#[test]
fn foliage_resources_retire_after_scene_drop() {
    let instance = worldrunner_render_gpu::device::headless_instance();
    let context = pollster::block_on(GpuContext::from_instance(&instance)).unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let before = instance.generate_report().unwrap();
    let authority =
        worldrunner_render_gpu::submission::SubmissionAuthority::new(context.queue().clone());
    for scene in [
        card_transition_scene(500_000),
        contact_scene(-500, true, false),
    ] {
        let gpu = SceneGpu::upload(context.device(), &authority, &scene, &pipelines).unwrap();
        gpu.set_frame(context.queue(), frame_from(pose()));
        let [source, ..] = geometry_sources();
        let _ = pass_depth(&context, &gpu, &pipelines, &source, "fs_masked", true);
        drop(gpu);
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let after = instance.generate_report().unwrap();
        assert_eq!(
            after.hub.buffers.num_kept_from_user,
            before.hub.buffers.num_kept_from_user
        );
        assert_eq!(
            after.hub.textures.num_kept_from_user,
            before.hub.textures.num_kept_from_user
        );
        assert_eq!(
            after.hub.bind_groups.num_kept_from_user,
            before.hub.bind_groups.num_kept_from_user
        );
    }
}

#[test]
fn camera_motion_changes_lod_without_repacking_or_reordering_instances() {
    use crate::draw::visibility::{MeshSelection, Selection};
    let scene = card_transition_scene(500_000);
    let mut instances = crate::instances::Instances::new(&scene);
    let object = scene.geometry().objects()[0];
    let added = instances.add(object, true).unwrap();
    let handles: Vec<_> = instances.handles().collect();
    let bytes: Vec<_> = instances
        .batches
        .values()
        .map(|batch| batch.bytes.clone())
        .collect();
    let meshes: Vec<_> = scene
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut selection = Selection::default();
    for (eye, expected) in [([0; 3], 0), ([0, -2000, 2000], 1), ([0; 3], 0)] {
        selection.rebuild(
            &instances,
            &meshes,
            glam::Mat4::from_scale(glam::Vec3::splat(0.1)),
            eye,
        );
        assert_eq!(selection.ranges.len(), 1);
        assert_eq!(selection.ranges[0].slots, 0..2);
        assert_eq!(selection.ranges[0].lod, expected);
        assert_eq!(instances.handles().collect::<Vec<_>>(), handles);
        assert_eq!(
            instances
                .batches
                .values()
                .map(|batch| batch.bytes.clone())
                .collect::<Vec<_>>(),
            bytes
        );
    }
    instances.remove(added).unwrap();
    let replacement = instances.add(object, true).unwrap();
    assert_ne!(added, replacement);
    assert!(instances.get(added).is_err());
}

#[test]
fn terrain_contact_uses_rendered_lod_triangles_not_discarded_height_samples() {
    let context = context();
    let mut frame = frame_from(pose());
    frame.unlit = true;
    frame.exposure = 1.0;
    let base = contact_scene(-500, true, false);
    let mut heights = vec![-500; 9];
    heights[4] = 10_000;
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 3,
        rows: 3,
        origin_east_millimeters: -4000,
        origin_north_millimeters: -4000,
        sample_spacing_millimeters: 4000,
        heights_millimeters: heights,
        surface_indices: vec![0; 9],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(1),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::UniformSteps { steps: 1 },
    })
    .unwrap();
    let scene = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: Some(terrain),
        effects: base.effects().cloned(),
    })
    .unwrap();
    let black = material(SceneAlphaMode::Opaque, false)
        .with_features(SceneMaterialFeatures {
            tint: SceneTint::new([0; 3]).unwrap(),
            ..features()
        })
        .unwrap();
    assert_eq!(
        render_frame(&context, &scene, &frame),
        render_frame(&context, &with_material(&scene, &black), &frame)
    );
}

fn copy_attachment(
    encoder: &mut CommandEncoder,
    texture: &Texture,
    readback: &Buffer,
    color: bool,
) {
    if color {
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TARGET * 4),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width: TARGET,
                height: TARGET,
                depth_or_array_layers: 1,
            },
        );
    } else {
        copy_depth(encoder, texture, readback);
    }
}
