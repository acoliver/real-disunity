use super::*;

fn dark_material(masked: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(u32::from(masked)),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: if masked {
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            }
        } else {
            SceneAlphaMode::Opaque
        },
        double_sided: true,
    })
    .unwrap()
}

fn bank() -> SceneMesh {
    let vertices = [
        [-20000, 1000, -1000],
        [20000, 1000, -1000],
        [20000, 8000, 2000],
        [-20000, 8000, 2000],
    ]
    .map(|position_millimeters| SceneVertex {
        position_millimeters,
        normal_micro: [0, -393_919, 919_145],
        texcoord_micro: [0; 2],
        tangent_micro: None,
    });
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices.to_vec(), vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}

fn resources() -> SceneResources {
    let black = SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0, 0, 0, 255].into(),
    };
    let mask = SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 8,
        height: 1,
        mips: 1,
        bytes: [0, 0, 255, 0, 0, 255, 0, 0]
            .into_iter()
            .flat_map(|a| [0, 0, 0, a])
            .collect::<Vec<_>>()
            .into(),
    };
    SceneResources::new(
        vec![black, mask],
        vec![dark_material(false), dark_material(true)],
    )
    .unwrap()
}

fn sparse_bank(scale: u32, enabled: bool) -> RenderScene {
    let mut parts = effect_parts();
    parts.sun.irradiance_micro = 0;
    parts.ambient.sky_irradiance_micro = 0;
    parts.ambient.ground_irradiance_micro = 0;
    parts.water = Some(SceneWaterBody {
        roughness_micro: 100_000,
        ..body()
    });
    let mut objects = vec![SceneObject::new(
        identity(),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )];
    for x in [-1800, 0, 1800] {
        let mut placement = identity();
        placement.position_millimeters = [x, 2500, 100];
        objects.push(SceneObject::new(
            placement,
            MeshHandle::from_raw(1),
            MaterialHandle::from_raw(1),
        ));
    }
    RenderScene::new(SceneParts {
        resources: resources(),
        geometry: SceneGeometry::new(vec![bank(), card(None)], objects).unwrap(),
        terrain: None,
        effects: Some(
            SceneEffects::new(parts)
                .unwrap()
                .with_ssr(SceneSsr {
                    enabled,
                    resolution_scale_micro: scale,
                    traversal: worldrunner_render_plan::SceneSsrTraversal::Grid,
                    application_lod:
                        worldrunner_render_plan::SceneSsrLodMapping::RoughnessDownsample,
                    composition: worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment,
                    ..request()
                })
                .unwrap(),
        ),
    })
    .unwrap()
}

fn raw_pair(context: &GpuContext, renderer: &FrameRenderer) -> (Vec<[f32; 4]>, [u32; 2]) {
    let target = renderer.ssr.as_ref().unwrap().confidence_target();
    let size = [target.width(), target.height()];
    let bytes = Readback::new(
        context.device(),
        FrameConfig::new(size, TextureFormat::Rgba16Float, 1),
        100_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), target)
    .unwrap();
    let pixels = bytes
        .chunks_exact(8)
        .map(|p| {
            std::array::from_fn(|c| {
                half::f16::from_le_bytes(p[c * 2..c * 2 + 2].try_into().unwrap()).to_f32()
            })
        })
        .collect();
    (pixels, size)
}

fn finite_pair(
    raw: &[[f32; 4]],
    size: [u32; 2],
    water: &[f32],
    pixel: [u32; 2],
    radius: f32,
) -> Option<[f32; 4]> {
    let position = pixel.map(|v| gpu_scalar(usize::try_from(v).unwrap()) + 0.5);
    let center = [
        position[0] / 32.0 * gpu_scalar(usize::try_from(size[0]).unwrap()),
        position[1] / 32.0 * gpu_scalar(usize::try_from(size[1]).unwrap()),
    ];
    let mut total = 0.0;
    let mut sum = [0.0; 4];
    for y in 0..size[1] {
        for x in 0..size[0] {
            let weight = (radius
                - (gpu_scalar(usize::try_from(x).unwrap()) + 0.5 - center[0]).abs())
            .max(0.0)
                * (radius - (gpu_scalar(usize::try_from(y).unwrap()) + 0.5 - center[1]).abs())
                    .max(0.0);
            if weight == 0.0 {
                continue;
            }
            let receiver = [
                (2 * x + 1) * 32 / (2 * size[0]),
                (2 * y + 1) * 32 / (2 * size[1]),
            ];
            if water[usize::try_from(receiver[1] * 32 + receiver[0]).unwrap()] == 0.0 {
                return None;
            }
            let pair = raw[usize::try_from(y * size[0] + x).unwrap()];
            for channel in 0..4 {
                sum[channel] += weight * pair[channel];
            }
            total += weight;
        }
    }
    Some(sum.map(|v| v / total))
}

#[test]
fn sparse_masked_bank_water_matches_finite_coverage_environment_subtraction() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut failures = Vec::new();
    for scale in [1_000_000, 500_000] {
        check_draw(&context, scale, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "finite footprint public draw:\n{}",
        failures.join("\n")
    );
}

fn render_fixture(context: &GpuContext, scene: &RenderScene) -> FrameRenderer {
    render_selected_environment(
        context,
        scene,
        1,
        &environment(context),
        &frame(scene),
        SceneEnvironmentConsumers::Water,
    )
}

fn check_draw(context: &GpuContext, scale: u32, failures: &mut Vec<String>) {
    let scene = sparse_bank(scale, true);
    let actual = render_fixture(context, &scene);
    let baseline = render_fixture(context, &sparse_bank(scale, false));
    let (raw, size) = raw_pair(context, &actual);
    let (normal, material, published) = actual.water_receivers().unwrap();
    let water = channel(context, normal.texture(), 0, 3);
    let roughness = channel(context, material.texture(), 0, 3);
    let environment = channel(context, published.texture(), 0, 0);
    let after = channel(context, actual.targets().scene().texture(), 0, 0);
    let before = channel(context, baseline.targets().scene().texture(), 0, 0);
    let mut checked = 0;
    for y in 16..30 {
        for x in 4..28 {
            let i = usize::try_from(y * 32 + x).unwrap();
            let levels = request().radiance_extents([32, 32]).len() - 1;
            let lod = (roughness[i] * gpu_scalar(levels)
                - if scale == 500_000 { 1.0 } else { 0.0 })
            .max(0.0);
            let Some(pair) = finite_pair(
                &raw,
                size,
                &water,
                [x, y],
                (2.0 * lod.exp2()).clamp(2.0, 4.0),
            ) else {
                continue;
            };
            let alpha = pair[3];
            if water[i] == 0.0 || alpha <= 0.01 || alpha >= 0.99 {
                continue;
            }
            assert!(environment[i] > 0.001, "bright environment control");
            // The prepared cube is constant red .5, so its published lobe
            // measures the same forward BRDF/transmittance factor used by SSR.
            let expected = environment[i] * (2.0 * pair[0] - alpha);
            if (after[i] - before[i] - expected).abs() > 0.0002 {
                failures.push(format!(
                    "scale {scale}, ({x},{y}): delta {} != {expected}, finite coverage {alpha}",
                    after[i] - before[i]
                ));
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 4,
        "sparse geometric hits and holes must reach water: scale {scale}, checked {checked}"
    );
}
