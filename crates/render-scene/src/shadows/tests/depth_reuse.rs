//! Shadow receivers must agree across independent and prepass-owned depth.
use super::*;
use crate::compositor::FrameRenderer;
use crate::scene_metadata::receiver::ReceiverReference;
use worldrunner_render_post::{Readback, ToneMap};

fn source(terrain: bool, offset: i64) -> RenderScene {
    let base = scene(&[[0, 0, 1_000], [400, 500, 800]], 600_000, None);
    let mut meshes = base.geometry().meshes().to_vec();
    let lod = &meshes[0].lods()[0];
    let vertices = lod
        .vertices()
        .iter()
        .map(|v| SceneVertex {
            position_millimeters: [
                v.position_millimeters[0],
                v.position_millimeters[1],
                v.position_millimeters[0] / 5 + v.position_millimeters[1] / 7,
            ],
            normal_micro: [-196_000, -140_000, 970_000],
            ..*v
        })
        .collect();
    meshes[0] = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap();
    let base = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(meshes, base.geometry().objects().to_vec()).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    let base = if terrain {
        super::terrain::terrain_scene(&base, false, false)
    } else {
        base
    };
    let ground = base.terrain().map(|t| {
        SceneTerrain::new(SceneTerrainParts {
            columns: t.columns(),
            rows: t.rows(),
            origin_east_millimeters: t.origin_east_millimeters() + offset,
            origin_north_millimeters: t.origin_north_millimeters() + offset,
            sample_spacing_millimeters: t.sample_spacing_millimeters(),
            heights_millimeters: (0_i32..49)
                .map(|i| (i % 7 - 3) * 200 + (i / 7 - 3) * 143)
                .collect(),
            surface_indices: t.surface_indices().to_vec(),
            surfaces: t.surfaces().to_vec(),
            lod_policy: t.lod_policy(),
        })
        .unwrap()
    });
    let objects = base
        .geometry()
        .objects()
        .iter()
        .map(|object| {
            let mut transform = *object.transform();
            transform.position_millimeters[0] += offset;
            transform.position_millimeters[1] += offset;
            SceneObject::new(transform, object.mesh(), object.material())
        })
        .collect();
    let mut effects = fixture::effects_parts(600_000);
    effects.ssao = Some(crate::ssr::neutral_ao());
    effects.shadows =
        SceneShadowCascades::camera_splits(vec![7_000, 12_000, 20_000], 200_000).unwrap();
    RenderScene::new(SceneParts {
        resources: textured_resources(&base, terrain),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: ground,
        effects: Some(SceneEffects::new(effects).unwrap()),
    })
    .unwrap()
}

fn textured_resources(base: &RenderScene, terrain: bool) -> SceneResources {
    let mut textures = base.resources().textures().to_vec();
    let normal = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 32,
        height: 32,
        mips: 1,
        bytes: (0_u32..1024)
            .flat_map(|i| {
                [
                    u8::try_from(40 + i * 37 % 176).unwrap(),
                    u8::try_from(40 + i * 61 % 176).unwrap(),
                    255,
                    255,
                ]
            })
            .collect::<Vec<_>>()
            .into(),
    };
    let mut materials = base.resources().materials().to_vec();
    if terrain {
        textures[2] = normal;
    } else {
        textures.push(normal);
        materials[0] = SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: Some(TextureHandle::from_raw(2)),
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 1_000_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: false,
        })
        .unwrap();
    }
    SceneResources::new(textures, materials).unwrap()
}

pub(super) fn reference_receiver() -> String {
    let current = include_str!("../../shader/shadows.wgsl");
    let original = include_str!("../../draw/tests/forward_reference/shadows.wgsl");
    let function = crate::scene_metadata::receiver::function_source;
    let mut reference = current.to_owned();
    for (name, frozen) in [
        ("single_shadow_visibility", "single_shadow_visibility"),
        ("cascaded_sunlight_visibility", "raw_sunlight_visibility"),
    ] {
        let replacement = function(original, frozen).replacen(
            &format!("fn {frozen}("),
            &format!("fn {name}("),
            1,
        );
        reference = reference.replace(function(current, name), &replacement);
    }
    reference
}

fn reference_pipelines(context: &GpuContext) -> ScenePipelines {
    let (object, terrain) = resources::receiver_sources(true, false, false);
    let current = include_str!("../../shader/shadows.wgsl");
    let receiver = reference_receiver();
    let terrain = crate::scene_metadata::receiver::hardware_receiver_reference(&terrain);
    assert_eq!(terrain.matches(current).count(), 1);
    let layout = crate::scene_metadata::receiver::shadow_layout(context.device());
    let mut pipelines = ScenePipelines::with_screen_space(
        context.device(),
        (wgpu::TextureFormat::Rgba16Float, 1),
        &object.replace(current, &receiver),
        &terrain.replace(current, &receiver),
        Some(&layout),
        Some(&crate::ssao::ambient_layout(context.device())),
    );
    pipelines.ssao = true;
    pipelines
}

#[test]
fn frozen_receiver_keeps_its_solver_inside_the_current_visibility_fade() {
    let current = include_str!("../../shader/shadows.wgsl");
    let frozen = include_str!("../../draw/tests/forward_reference/shadows.wgsl");
    let reference = reference_receiver();
    let function = crate::scene_metadata::receiver::function_source;
    assert_eq!(
        function(&reference, "cascaded_sunlight_visibility"),
        function(frozen, "raw_sunlight_visibility").replacen(
            "fn raw_sunlight_visibility(",
            "fn cascaded_sunlight_visibility(",
            1,
        ),
    );
    assert_eq!(
        function(&reference, "single_shadow_visibility"),
        function(frozen, "single_shadow_visibility"),
    );
    assert_eq!(
        function(&reference, "raw_sunlight_visibility"),
        function(current, "raw_sunlight_visibility"),
    );
}

fn capture(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    options: (bool, bool),
) -> Vec<u8> {
    let (independent, reference) = options;

    let size = [view.viewport().width, view.viewport().height];
    let mut frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    if independent {
        frame.independent_depth_for_test(context.device());
    }
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        DirectionalShadowSettings {
            focus_meters: view.eye_meters()
                + view
                    .resolved_view()
                    .view_matrix()
                    .inverse()
                    .z_axis
                    .truncate()
                    * -7.0,
            depth_bias: 0.00001,
            normal_bias_meters: 0.012,
            ..settings()
        },
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    if reference {
        renderer.pipelines = reference_pipelines(context);
        frame.receiver_reference = Some(if independent {
            ReceiverReference::FrozenIndependent
        } else {
            ReceiverReference::FrozenReuse
        });
    }
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let mut readback = Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        16 * 1024 * 1024,
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encode_with(
            context,
            view,
            (&color, &depth),
            &mut encoder,
            Some(&mut frame),
        )
        .unwrap();
    assert_eq!(
        frame.forward_depth(&renderer.gpu, &depth),
        if independent {
            crate::renderer::depth::DepthMode::Independent
        } else {
            crate::renderer::depth::DepthMode::Reuse
        },
        "both depth-ownership policies must execute"
    );
    readback.encode(&mut encoder, color.texture()).unwrap();
    let submission = encoder.finish().submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}

#[test]
fn gpu_shadow_sloped_receivers_depth_reuse_is_byte_exact_at_cascade_boundaries() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut differences = Vec::new();
    for terrain in [false, true] {
        for offset in [0, 8_192_000] {
            let source = source(terrain, offset);
            for shift in [0, 113, 791] {
                let view = SceneView::new(
                    crate::CameraPose {
                        eye_millimeters: [offset + shift, offset - 4_000, 6_500],
                        target_millimeters: [offset, offset, 0],
                    },
                    *source.effects().unwrap().camera(),
                    view().viewport(),
                )
                .unwrap();
                let baseline = capture(&context, &source, &view, (true, false));
                let reuse = capture(&context, &source, &view, (false, false));
                let main = capture(&context, &source, &view, (true, true));
                let main_reuse = capture(&context, &source, &view, (false, true));
                let main_diff = main
                    .chunks_exact(8)
                    .zip(main_reuse.chunks_exact(8))
                    .filter(|(a, b)| a != b)
                    .count();
                let image_delta = main
                    .chunks_exact(8)
                    .zip(baseline.chunks_exact(8))
                    .filter(|(a, b)| a != b)
                    .count();
                println!(
                    "SHADOW_MAIN terrain={terrain} offset={offset} shift={shift} reuse_pixels={main_diff} image_delta={image_delta}"
                );
                let pixels = baseline
                    .chunks_exact(8)
                    .zip(reuse.chunks_exact(8))
                    .filter(|(a, b)| a != b)
                    .count();
                println!(
                    "SHADOW_REUSE terrain={terrain} offset={offset} shift={shift} pixels={pixels}"
                );
                if pixels != 0 {
                    differences.push((terrain, offset, shift, pixels));
                }
                assert!(baseline.chunks_exact(8).any(|p| p[..6] != [0; 6]));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "shadowed reuse differs: {differences:?}"
    );
}
