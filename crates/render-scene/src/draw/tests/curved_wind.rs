fn disabled_wind(mode: u32) -> Option<SceneWind> {
    if mode == 0 {
        return None;
    }
    let band = SceneWindBand::new(1_000_000, 1_000_000, 0).unwrap();
    let shape = SceneFoliageWind::new(
        SceneDirection::new([0, 0, 1_000_000]).unwrap(),
        [2000, 1],
        1_000_000,
        [band; 2],
        SceneWindPhase::InstancePosition,
    )
    .unwrap()
    .with_strength(if mode == 2 { 0 } else { 1_000_000 })
    .unwrap();
    Some(
        SceneWind::new(
            SceneDirection::new([1_000_000, 0, 0]).unwrap(),
            if mode == 1 { 0 } else { 700 },
            1000,
            if mode == 3 { 750_000 } else { 0 },
        )
        .unwrap()
        .with_foliage(shape),
    )
}

#[test]
fn disabled_curvature_preserves_material_entry_depth_bytes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let base = scene_with_wind();
    let material = base.resources().materials()[0];
    let features = *material.features().unwrap();
    let mut baseline = None;
    for mode in 0..4 {
        let mut changed = features;
        changed.wind = disabled_wind(mode);
        let mut materials = base.resources().materials().to_vec();
        materials[0] = material.with_features(changed).unwrap();
        let scene = RenderScene::new(SceneParts {
            resources: SceneResources::new(base.resources().textures().to_vec(), materials)
                .unwrap(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: base.effects().cloned(),
        })
        .unwrap();
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        let mut frame = frame_from(pose());
        frame.animation_time_milliseconds = 0;
        gpu.set_frame(context.queue(), frame);
        let depth = read_depth(
            &context,
            &gpu,
            layouts.object_pipelines(),
            (Some(COLOR_FORMAT), None),
        );
        assert!(
            depth
                .chunks_exact(4)
                .any(|value| value != 1.0_f32.to_le_bytes())
        );
        if let Some(expected) = &baseline {
            assert_eq!(&depth, expected, "disabled mode {mode}");
        } else {
            baseline = Some(depth);
        }
    }
}

#[test]
fn phase_sources_and_local_hinge_translation_reach_production_geometry() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let base = scene_with_wind();
    let mut images = Vec::new();
    for (phase, origin) in [
        (SceneWindPhase::WorldPosition, [0; 3]),
        (SceneWindPhase::InstancePosition, [0; 3]),
        (SceneWindPhase::VertexOcclusion, [0; 3]),
        (SceneWindPhase::WorldPosition, [0, 0, 1000]),
    ] {
        let mut materials = base.resources().materials().to_vec();
        let mut features = *materials[0].features().unwrap();
        let wind = features.wind.unwrap();
        let shape = wind.foliage().unwrap();
        let replacement = SceneFoliageWind::new(
            shape.axis(),
            shape.extents_millimeters(),
            shape.curve_micro(),
            shape.bands(),
            phase,
        )
        .unwrap()
        .with_origin(origin);
        features.wind = Some(wind.with_foliage(replacement));
        materials[0] = materials[0].with_features(features).unwrap();
        let scene = RenderScene::new(SceneParts {
            resources: SceneResources::new(base.resources().textures().to_vec(), materials)
                .unwrap(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: base.effects().cloned(),
        })
        .unwrap();
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        let mut frame = frame_from(CameraPose {
            eye_millimeters: [0, -4000, 2500],
            target_millimeters: [0, 0, 1000],
        });
        frame.animation_time_milliseconds = 100;
        gpu.set_frame(context.queue(), frame);
        let depth = read_depth(
            &context,
            &gpu,
            layouts.object_pipelines(),
            (Some(COLOR_FORMAT), None),
        );
        assert!(depth.chunks_exact(4).any(|v| v != 1.0_f32.to_le_bytes()));
        for previous in &images {
            assert_ne!(&depth, previous);
        }
        images.push(depth);
    }
}

#[test]
fn curved_bounds_remain_finite_and_cull_distant_objects() {
    let scene = scene_with_wind();
    let instances = crate::instances::Instances::new(&scene);
    let meshes: Vec<_> = scene
        .geometry()
        .meshes()
        .iter()
        .map(crate::draw::visibility::MeshSelection::new)
        .collect();
    let mut selection = crate::draw::visibility::Selection::default();
    selection.rebuild(
        &instances,
        &meshes,
        frame_from(pose()).view_projection,
        [0; 3],
    );
    assert_eq!(selection.stats.culled, 0);
    let mut transform = identity();
    transform.position_millimeters = [100_000_000; 3];
    let object = SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let far = RenderScene::new(SceneParts {
        resources: scene.resources().clone(),
        geometry: SceneGeometry::new(scene.geometry().meshes().to_vec(), vec![object]).unwrap(),
        terrain: scene.terrain().cloned(),
        effects: scene.effects().cloned(),
    })
    .unwrap();
    selection.rebuild(
        &crate::instances::Instances::new(&far),
        &meshes,
        frame_from(pose()).view_projection,
        [0; 3],
    );
    assert_eq!(selection.stats.culled, 1);
    assert!(selection.ranges.is_empty());
}

#[test]
fn default_wind_preserves_legacy_vertex_arithmetic_and_material_prefix() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let base = scene_with_wind();
    let mut materials = base.resources().materials().to_vec();
    let mut features = *materials[0].features().unwrap();
    let wind = features.wind.unwrap();
    features.wind = Some(
        SceneWind::new(
            wind.direction(),
            wind.amplitude_millimeters(),
            wind.period_milliseconds(),
            wind.phase_micro(),
        )
        .unwrap(),
    );
    materials[0] = materials[0].with_features(features).unwrap();
    let bytes = crate::pipeline::MaterialUniform::from_material(&materials[0]).to_bytes();
    assert!(bytes[160..].iter().all(|value| *value == 0));
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: base.terrain().cloned(),
        effects: base.effects().cloned(),
    })
    .unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
    let current = crate::shader::ENVIRONMENT_WGSL;
    let call =
        "scene_geometry_displacement(out.world_position, vertex.ambient_occlusion, instance)";
    assert_eq!(current.matches(call).count(), 1);
    let legacy = current.replace(call, "scene_wind_offset(material.wind_direction_amplitude, material.wind_clock.xy, bitcast<u32>(frame.grading.z))");
    for time in [0, 250, 1000, u32::MAX] {
        let mut frame = frame_from(pose());
        frame.animation_time_milliseconds = time;
        frame.unlit = true;
        frame.exposure = 1.0;
        gpu.set_frame(context.queue(), frame);
        assert_eq!(
            super::foliage::pass_color(&context, &gpu, &layouts, current, "fs_masked"),
            super::foliage::pass_color(&context, &gpu, &layouts, &legacy, "fs_masked"),
        );
    }
}

use super::*;
use worldrunner_render_plan::{
    SceneAmbientOcclusion, SceneFoliageWind, SceneMaterialFeatures, SceneTerrainContact, SceneTint,
    SceneWind, SceneWindBand, SceneWindPhase,
};

fn curved_material() -> SceneMaterial {
    let band = SceneWindBand::new(1_000_000, 300_000, 150_000).unwrap();
    let detail = SceneWindBand::new(3_000_000, 200_000, 1_000_000).unwrap();
    let shape = SceneFoliageWind::new(
        SceneDirection::new([0, 0, 1_000_000]).unwrap(),
        [2000, 1000],
        1_000_000,
        [band, detail],
        SceneWindPhase::WorldPosition,
    )
    .unwrap();
    let wind = SceneWind::new(
        SceneDirection::new([1_000_000, 0, 0]).unwrap(),
        700,
        1000,
        0,
    )
    .unwrap()
    .with_foliage(shape);
    material(
        SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        true,
    )
    .with_features(SceneMaterialFeatures {
        tint: SceneTint::WHITE,
        ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
        wind: Some(wind),
        layer: None,
        translucency: None,
    })
    .unwrap()
    .with_terrain_contact(
        SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 500)
            .unwrap()
            .with_normal_fold(2000, 0)
            .unwrap(),
    )
}

fn scene_with_wind() -> RenderScene {
    let material = curved_material();
    let mut vertices = card(None).lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[0] /= 3;
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let base = super::fold_translucency::folded_scene(0);
    let mut textures = base.resources().textures().to_vec();
    textures[0] = super::foliage::cutout_texture();
    let mut materials = base.resources().materials().to_vec();
    materials[0] = material;
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(vec![mesh], base.geometry().objects().to_vec()).unwrap(),
        terrain: base.terrain().cloned(),
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn curved_motion_has_identical_production_pass_depth_and_fixed_clock_bytes() {
    let context = context();
    let device = context.device();
    let forward = ScenePipelines::new(device, COLOR_FORMAT);
    let scene = scene_with_wind();
    let gpu = SceneGpu::upload(device, context.submission(), &scene, &forward).unwrap();
    let screen = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 224,
        usage: BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let (metadata, group) = crate::scene_metadata::pipelines(device, &screen, false);
    let (reflection, reflection_group) = crate::scene_metadata::pipelines(device, &screen, true);
    let shadow = crate::shadows::wind_test_pipelines(device, &forward);
    let mut previous = None;
    for time in [250, 1750, 250] {
        let mut frame = frame_from(CameraPose {
            eye_millimeters: [0, -4000, 2500],
            target_millimeters: [0, 0, 1000],
        });
        frame.animation_time_milliseconds = time;
        gpu.set_frame(context.queue(), frame);
        let main = read_depth(
            &context,
            &gpu,
            forward.object_pipelines(),
            (Some(COLOR_FORMAT), None),
        );
        assert!(main.chunks_exact(4).any(|v| v != 1.0_f32.to_le_bytes()));
        for (pipelines, state) in [
            (
                metadata.object_pipelines(),
                (Some(TextureFormat::Rgba16Float), Some(&group)),
            ),
            (
                reflection.object_pipelines(),
                (Some(TextureFormat::Rgba16Float), Some(&reflection_group)),
            ),
            (&shadow, (None, None)),
        ] {
            assert_eq!(main, read_depth(&context, &gpu, pipelines, state));
        }
        if time == 250 {
            if let Some(previous) = &previous {
                assert_eq!(&main, previous);
            }
            previous = Some(main);
        } else {
            assert_ne!(Some(&main), previous.as_ref());
        }
    }
}

pub(super) fn read_depth(
    context: &GpuContext,
    gpu: &SceneGpu,
    pipelines: &[wgpu::RenderPipeline; 8],
    state: (Option<TextureFormat>, Option<&wgpu::BindGroup>),
) -> Vec<u8> {
    let device = context.device();
    let depth = super::foliage::coverage_depth(device);
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let view = state.0.map(|format| test_color_view(device, format));
    let colors = color_attachments(view.as_ref());
    let buffer = device.create_buffer(&BufferDescriptor {
        label: None,
        size: u64::from(TARGET * TARGET * 4),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
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
        if let Some(group) = state.1 {
            pass.set_bind_group(3, group, &[]);
        }
        gpu.draw_objects(&mut pass, &gpu.frame_group, pipelines, None);
    }
    super::foliage::copy_depth(&mut encoder, &depth, &buffer);
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &buffer)
}

fn test_color_view(device: &Device, format: TextureFormat) -> TextureView {
    let texture = device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&TextureViewDescriptor::default())
}

fn color_attachments(view: Option<&TextureView>) -> Vec<Option<RenderPassColorAttachment<'_>>> {
    view.map(|view| {
        Some(RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        })
    })
    .into_iter()
    .collect()
}
