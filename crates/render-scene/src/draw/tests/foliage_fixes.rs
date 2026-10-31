use super::foliage::{contact_scene, geometry_sources, pass_color, pass_depth};
use super::*;

fn transition_scene(coverage: u32) -> RenderScene {
    use worldrunner_render_plan::SceneCard;
    let near = card(None).lods()[0].clone();
    let mut vertices = near.vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[1] = vertex.position_millimeters[0] / 3;
        vertex.texcoord_micro = [
            vertex.texcoord_micro[1] * 3 + 123_456,
            vertex.texcoord_micro[0] * 2,
        ];
    }
    let far = SceneMeshLod::new(
        1,
        vertices,
        vec![0, 1, 3, 1, 2, 3],
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [2400, 2400], coverage).unwrap())
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, true)],
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

fn accepted(
    context: &GpuContext,
    gpu: &SceneGpu,
    layouts: &ScenePipelines,
    source: &str,
    index: usize,
    selection: (bool, bool),
) -> Vec<bool> {
    let (mesh, full) = selection;
    let condition = if mesh { "==" } else { "!=" };
    let source = source.replace(
        "fn scene_geometry_covered(input: VertexOutput) -> bool {",
        &format!("fn scene_geometry_covered(input: VertexOutput) -> bool {{\nif (input.coverage & 0x80000000u) {condition} 0u {{ return false; }}\n{}", if full { "return true;" } else { "" }),
    );
    let entry = if index == 3 {
        "fs_depth_opaque"
    } else {
        "fs_opaque"
    };
    pass_depth(context, gpu, layouts, &source, entry, index != 3)
        .chunks_exact(4)
        .map(|pixel| pixel != 1.0_f32.to_le_bytes())
        .collect()
}

#[test]
fn screen_pixel_transition_is_exclusive_and_complete_on_overlapping_geometry() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for coverage in [0, 123_457, 500_000, 1_000_000] {
        let gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            &transition_scene(coverage),
            &layouts,
        )
        .unwrap();
        for eye in [[0, -4000, 2000], [2000, -4000, 2000]] {
            let frame = frame_from(CameraPose {
                eye_millimeters: eye,
                target_millimeters: [0, 0, 1000],
            });
            gpu.set_frame(context.queue(), frame);
            let mut reference = None;
            for (index, source) in geometry_sources().iter().enumerate() {
                let mesh = accepted(&context, &gpu, &layouts, source, index, (true, false));
                let card = accepted(&context, &gpu, &layouts, source, index, (false, false));
                let mesh_full = accepted(&context, &gpu, &layouts, source, index, (true, true));
                let card_full = accepted(&context, &gpu, &layouts, source, index, (false, true));
                let mut overlap = 0;
                for pixel in 0..mesh.len() {
                    assert!(
                        !(mesh[pixel] && card[pixel]),
                        "double acceptance: coverage={coverage}, pass={index}, pixel={pixel}"
                    );
                    if mesh_full[pixel] && card_full[pixel] {
                        overlap += 1;
                        assert!(
                            mesh[pixel] || card[pixel],
                            "hole: coverage={coverage}, pass={index}, pixel={pixel}"
                        );
                    }
                }
                assert!(overlap > 20);
                if let Some(reference) = &reference {
                    assert_eq!(&(mesh, card), reference, "pass={index}");
                } else {
                    reference = Some((mesh, card));
                }
            }
        }
    }
}

fn reduced_contact_scene(enabled: bool, steps: u8) -> RenderScene {
    let base = contact_scene(-500, enabled, false);
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 5,
        rows: 5,
        origin_east_millimeters: -4000,
        origin_north_millimeters: -4000,
        sample_spacing_millimeters: 2000,
        heights_millimeters: vec![-500; 25],
        surface_indices: vec![0; 25],
        surfaces: base.terrain().unwrap().surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::UniformSteps { steps },
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: Some(terrain),
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn unused_contact_keeps_only_inactive_binding_with_reduced_terrain() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &reduced_contact_scene(false, 1),
        &layouts,
    )
    .unwrap();
    assert_eq!(gpu.retained.contact_heights.size(), 48);
}

#[test]
fn enabled_contact_admits_reduced_samples_and_rejects_device_limit_before_binding() {
    let context = context();
    let (device, queue) =
        pollster::block_on(context.adapter().request_device(&wgpu::DeviceDescriptor {
            required_features: crate::device::REQUIRED_FEATURES,
            required_limits: wgpu::Limits {
                max_storage_buffer_binding_size: 64,
                ..Default::default()
            },
            ..Default::default()
        }))
        .unwrap();
    let authority = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue);
    let layouts = ScenePipelines::new(&device, COLOR_FORMAT);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let reduced = SceneGpu::upload(
        &device,
        &authority,
        &reduced_contact_scene(true, 2),
        &layouts,
    )
    .unwrap();
    assert_eq!(reduced.retained.contact_heights.size(), 48);
    let rejected = SceneGpu::upload(
        &device,
        &authority,
        &reduced_contact_scene(true, 1),
        &layouts,
    );
    assert!(matches!(
        rejected,
        Err(DrawError::ContactHeightLimit {
            required: 68,
            limit: 64
        })
    ));
    let inactive = SceneGpu::upload(
        &device,
        &authority,
        &reduced_contact_scene(false, 1),
        &layouts,
    )
    .unwrap();
    assert_eq!(inactive.retained.contact_heights.size(), 48);
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn fitted_normal_scene(fit: bool) -> RenderScene {
    use worldrunner_render_plan::SceneCard;
    let mut vertices = card(None).lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.normal_micro = if fit {
            [0, -MICRO, 3 * MICRO]
        } else {
            [0, -MICRO, MICRO]
        };
        if !fit {
            vertex.position_millimeters[2] *= 3;
        }
    }
    let mut lod = SceneMeshLod::new(
        0,
        vertices,
        vec![0, 1, 2, 0, 2, 3],
        Some(BillboardPolicy::CrossQuads),
    )
    .unwrap();
    if fit {
        lod = lod
            .with_card(
                SceneCard::new(BillboardPolicy::CrossQuads, [2000, 6000], 1_000_000).unwrap(),
            )
            .unwrap();
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, true)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![SceneMesh::new(vec![lod]).unwrap()],
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
fn fitted_normals_match_baked_geometry_in_shading_and_normal_metadata() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let fitted = fitted_normal_scene(true);
    let baked = fitted_normal_scene(false);
    let frame = frame_from(CameraPose {
        eye_millimeters: [0, -10_000, 4000],
        target_millimeters: [0, 0, 3000],
    });
    // The fitted normal is computed in the shader and the baked one is stored, so the renders hold
    // the same ideal value and may round one code apart.
    worldrunner_render_gpu::tolerance_test_support::assert_paths_agree(
        &render_frame(&context, &fitted, &frame),
        &render_frame(&context, &baked, &frame),
        "fitted versus baked shading",
    );
    let sources = geometry_sources();
    for source in [&sources[1], &sources[2]] {
        let mut reference: Option<Vec<u8>> = None;
        for scene in [&fitted, &baked] {
            let gpu =
                SceneGpu::upload(context.device(), context.submission(), scene, &layouts).unwrap();
            gpu.set_frame(context.queue(), frame);
            let pixels = pass_color(&context, &gpu, &layouts, source, "fs_opaque");
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[2] != 0));
            if let Some(reference) = &reference {
                worldrunner_render_gpu::tolerance_test_support::assert_paths_agree(
                    &pixels,
                    reference,
                    "fitted versus baked normal metadata",
                );
            } else {
                reference = Some(pixels);
            }
        }
    }
}

fn assert_depth_agrees(current: &[u8], stock: &[u8], entry: &str) {
    assert_eq!(current.len(), stock.len());
    let bound = worldrunner_render_gpu::tolerance_test_support::ARITHMETIC + 2.0_f64.powi(-23);
    let flipped = current
        .chunks_exact(4)
        .zip(stock.chunks_exact(4))
        .filter(|(a, b)| {
            let a = f64::from(f32::from_le_bytes((*a).try_into().unwrap()));
            let b = f64::from(f32::from_le_bytes((*b).try_into().unwrap()));
            (a - b).abs() > bound
        })
        .count();
    assert!(
        flipped * 200 <= current.len() / 4,
        "{entry}: {flipped} of {} depth pixels differ by more than {bound}",
        current.len() / 4
    );
}

/// Lights chosen so a white up-facing quad stays below saturation: sun 1, sky 0.2, ground 0.05.
const MODEL_SUN: f32 = 1.0;
const MODEL_SKY: f32 = 0.2;

/// Checks the opaque quad against the f64 lighting model at pixels inside the flat-lit quad,
/// for the current shader and for the independent stock text. A scale error in the shared
/// lighting (sun, ambient or the specular terms) moves the current output off the model; the
/// stock text is separate source, so it stays on it.
fn assert_flat_lit_pixels_match_model(context: &GpuContext, layouts: &ScenePipelines, stock: &str) {
    use crate::lit_model_tests::{
        assert_display_matches_radiance, overhead_rough_radiance, pixel_view_towards_eye,
    };
    let scene = scene(SceneAlphaMode::Opaque, true, 1);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, layouts).unwrap();
    let mut frame = frame_from(pose());
    assert_eq!(
        frame.sun_direction,
        glam::Vec3::Z,
        "the model assumes an overhead sun"
    );
    frame.sun_irradiance = glam::Vec3::splat(MODEL_SUN);
    frame.ambient_sky = glam::Vec3::splat(MODEL_SKY);
    frame.ambient_ground = glam::Vec3::splat(0.05);
    frame.exposure = 1.0;
    gpu.set_frame(context.queue(), frame);
    for (name, source) in [
        ("current", crate::shader::ENVIRONMENT_WGSL),
        ("stock", stock),
    ] {
        let pixels = pass_color(context, &gpu, layouts, source, "fs_opaque");
        for pixel in [(32, 32), (24, 36), (40, 28), (32, 44)] {
            let view = pixel_view_towards_eye([0.0, -2.0, 2.0], [0.0; 3], 60.0, TARGET, pixel);
            let radiance =
                overhead_rough_radiance(1.0, f64::from(MODEL_SUN), f64::from(MODEL_SKY), view);
            let at = ((pixel.1 * TARGET + pixel.0) * 4) as usize;
            for channel in 0..3 {
                assert_display_matches_radiance(
                    pixels[at + channel],
                    radiance,
                    &format!("{name} shading at pixel {pixel:?} channel {channel}"),
                );
            }
        }
    }
}

#[test]
fn feature_off_output_matches_stock_shader_snapshot_within_derived_bounds() {
    // Fixtures are the MIT engine's unmodified shaders at 0d387d8718885158cdadb343196a62992420a692.
    let stock = concat!(
        worldrunner_render_lighting::lighting_wgsl!(),
        worldrunner_render_post::post_wgsl!(),
        include_str!("fixtures/stock_lighting.wgsl"),
        include_str!("../../shader/unshadowed.wgsl"),
        include_str!("fixtures/stock_environment.wgsl"),
    );
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    assert_flat_lit_pixels_match_model(&context, &layouts, stock);
    for (scene, entry) in [
        (scene(SceneAlphaMode::Opaque, true, 1), "fs_opaque"),
        (
            super::foliage::foliage_scene(
                None,
                worldrunner_render_plan::SceneTint::WHITE,
                1_000_000,
            ),
            "fs_masked",
        ),
        (
            card_scene(Some(BillboardPolicy::YawLocked), turned_away()),
            "fs_opaque",
        ),
    ] {
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        gpu.set_frame(context.queue(), frame_from(pose()));
        let expected = pass_color(&context, &gpu, &layouts, stock, entry);
        assert!(expected.chunks_exact(4).any(|pixel| pixel[..3] != [0; 3]));
        // The snapshot is older source (separate lighting and environment text, different
        // expression order), so its arithmetic is not the current text: the colour agrees
        // within the shared path bound rather than byte for byte.
        worldrunner_render_gpu::tolerance_test_support::assert_paths_agree(
            &pass_color(
                &context,
                &gpu,
                &layouts,
                crate::shader::ENVIRONMENT_WGSL,
                entry,
            ),
            &expected,
            entry,
        );
        // Depth is f32 from the unchanged vertex transform (range 1). It agrees within the
        // shared arithmetic allowance, except at pixels where a one-ulp position difference
        // moves a triangle edge across a pixel centre; those are bounded to 0.5% of pixels.
        let current = pass_depth(
            &context,
            &gpu,
            &layouts,
            crate::shader::ENVIRONMENT_WGSL,
            entry,
            true,
        );
        let stock_depth = pass_depth(&context, &gpu, &layouts, stock, entry, true);
        assert_depth_agrees(&current, &stock_depth, entry);
    }
}
