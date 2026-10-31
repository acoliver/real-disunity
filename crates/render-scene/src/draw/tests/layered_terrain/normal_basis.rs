fn with_strength(scene: &RenderScene, strength: f32) -> RenderScene {
    let terrain = scene
        .terrain()
        .unwrap()
        .clone()
        .with_material_normal_strength(Some(strength))
        .unwrap();
    scene
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}

fn reference_source(normal: &str) -> String {
    let source = crate::shader::TERRAIN_WGSL;
    let start = source
        .find("    let strength = bitcast<f32>(terrain.surfaces[0].normal_strength_bits")
        .unwrap();
    let end = start + source[start..].find("    // Terrain has neither").unwrap();
    format!("{}{normal}\n{}", &source[..start], &source[end..])
}

#[test]
fn terrain_normal_strength_zero_is_geometric_one_preserves_bytes_two_increases_contrast() {
    let context = context();
    let scene = fixture([191, 128, 238, 255], 0);
    let geometric_source = reference_source("    let normal = geometric;");
    let original_source = reference_source(
        "    let normal = normalize(tangent * blended.normal.x + bitangent * blended.normal.y + geometric * blended.normal.z);",
    );
    for cached in [false, true] {
        let frame = sunlight(0, 1.0);
        let baseline = render_with_source(&context, &scene, cached, &frame, &original_source);
        let default = render_with_frame(&context, &scene, cached, &frame);
        assert_eq!(
            default, baseline,
            "default strength changed pre-strength pixels"
        );
        assert_eq!(
            render_with_frame(&context, &with_strength(&scene, 1.0), cached, &frame),
            baseline
        );
        let geometric = render_with_source(&context, &scene, cached, &frame, &geometric_source);
        let flat = with_strength(&scene, 0.0);
        assert_eq!(
            render_with_frame(&context, &flat, cached, &frame),
            geometric
        );
        assert_eq!(responses(&context, &flat, cached), [0, 0]);
        let double = with_strength(&scene, 2.0);
        let contrast = responses(&context, &scene, cached)[0];
        let increased = responses(&context, &double, cached)[0];
        println!("cached={cached} strength=1 contrast={contrast}, strength=2 contrast={increased}");
        assert!(increased > contrast + 30);
        let horizontal = with_strength(&fixture([255, 128, 128, 255], 0), 0.0);
        assert_eq!(
            render_with_frame(&context, &horizontal, cached, &frame),
            geometric
        );
    }
}

use super::*;

fn fixture(normal: [u8; 4], rotation: u32) -> RenderScene {
    configured_fixture(normal, &[rotation], true, vec![0; 4])
}

fn configured_fixture(
    normal: [u8; 4],
    rotations: &[u32],
    layered: bool,
    heights: Vec<i32>,
) -> RenderScene {
    let resources = SceneResources::new(
        vec![
            texels([1; 2], &[100, 100, 100, 0]),
            texels([1; 2], &normal),
            texels([1; 2], &[128; 4]),
        ],
        vec![terrain_material()],
    )
    .unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 2000,
        heights_millimeters: heights,
        surface_indices: vec![0; 4],
        surfaces: rotations
            .iter()
            .map(|rotation| {
                let mut appearance = layered_surface(0).appearance();
                appearance.rotation_arcminutes = *rotation;
                if !layered {
                    appearance.layered = None;
                }
                SceneTerrainSurface::new(MaterialHandle::from_raw(0), appearance)
            })
            .collect(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    let terrain = attach_layered_source(terrain, layered, rotations.len());
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}

fn attach_layered_source(terrain: SceneTerrain, layered: bool, layer_count: usize) -> SceneTerrain {
    if layered {
        terrain
            .with_layered_source(TerrainLayeredSource {
                channel_map: None,
                type_grid: TerrainTypeGrid::new(
                    [2; 2],
                    [0; 2],
                    1000,
                    if layer_count == 2 {
                        vec![0, 1, 0, 1]
                    } else {
                        vec![0; 4]
                    },
                )
                .unwrap(),
                filter: TerrainSplatFilter::Bilinear,
                colorization: None,
                anisotropy: worldrunner_render_plan::TextureAnisotropy::default(),
            })
            .unwrap()
    } else {
        terrain
    }
}

fn sunlight(axis: usize, sign: f32) -> FrameUniform {
    let mut frame = matching_frame();
    let mut direction = glam::Vec3::Z;
    direction[axis] = sign;
    frame.sun_direction = direction.normalize();
    frame.sun_irradiance = glam::Vec3::ONE;
    frame.ambient_sky = glam::Vec3::ZERO;
    frame.ambient_ground = glam::Vec3::ZERO;
    frame
}

fn brightness(pixels: &[u8]) -> i32 {
    pixels[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3]
        .iter()
        .map(|value| i32::from(*value))
        .sum()
}

fn responses(context: &GpuContext, scene: &RenderScene, cached: bool) -> [i32; 2] {
    std::array::from_fn(|axis| {
        let positive = brightness(&render_with_frame(
            context,
            scene,
            cached,
            &sunlight(axis, 1.0),
        ));
        let negative = brightness(&render_with_frame(
            context,
            scene,
            cached,
            &sunlight(axis, -1.0),
        ));
        positive - negative
    })
}

fn assert_directional(context: &GpuContext, layered: bool, cached: bool) {
    for rotation in [0, 5400] {
        for (axis, normal, polarity) in [
            (0, [191, 128, 238, 255], 1),
            (1, [128, 191, 238, 255], 1),
            (0, [64, 128, 238, 255], -1),
            (1, [128, 64, 238, 255], -1),
        ] {
            let scene = configured_fixture(normal, &[rotation], layered, vec![0; 4]);
            let actual = responses(context, &scene, cached);
            let target = if rotation == 0 { axis } else { 1 - axis };
            let sign = if rotation != 0 && axis == 0 { -1 } else { 1 };
            assert!(
                actual[target] * sign * polarity > 30,
                "layered={layered} cached={cached} rotation={rotation} axis={axis}: {actual:?}"
            );
            assert!(
                actual[1 - target].abs() < 10,
                "cross-axis response: {actual:?}"
            );
        }
    }
}

#[test]
fn terrain_normal_basis_tracks_authored_u_and_v_in_live_and_cached_draws() {
    let context = context();
    for cached in [false, true] {
        assert_directional(&context, true, cached);
    }
}

#[test]
fn terrain_conventional_palette_normal_basis_tracks_authored_u_and_v() {
    assert_directional(&context(), false, false);
}

#[test]
fn terrain_two_layers_unrotate_each_normal_before_blending() {
    let context = context();
    let scene = configured_fixture([191, 128, 238, 255], &[0, 5400], true, vec![0; 4]);
    for cached in [false, true] {
        let actual = responses(&context, &scene, cached);
        assert!(actual[0] > 30, "east component missing: {actual:?}");
        assert!(actual[1] < -30, "south component missing: {actual:?}");
    }
}

#[test]
fn terrain_sloped_heightfield_normal_basis_matches_world_frame() {
    let context = context();
    let scene = configured_fixture([191, 128, 238, 255], &[0], true, vec![0, 400, 600, 1000]);
    let n = glam::DVec3::new(-0.2, -0.3, 1.0).normalize();
    let t = glam::DVec3::new(1.0, 0.0, 0.2).normalize();
    let b = n.cross(t);
    let local = glam::DVec3::new(127.0 / 255.0, 1.0 / 255.0, 221.0 / 255.0).normalize();
    let expected = t * local.x + b * local.y + n * local.z;
    let oracle = reference_source(&format!(
        "    let normal = vec3<f32>({}, {}, {});",
        expected.x, expected.y, expected.z
    ));
    let geometric = reference_source("    let normal = geometric;");
    for cached in [false, true] {
        for axis in 0..2 {
            for sign in [-1.0, 1.0] {
                let frame = sunlight(axis, sign);
                let actual = render_with_frame(&context, &scene, cached, &frame);
                let expected = render_with_source(&context, &scene, cached, &frame, &oracle);
                assert!(actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 1));
                assert_eq!(
                    render_with_frame(&context, &with_strength(&scene, 0.0), cached, &frame),
                    render_with_source(&context, &scene, cached, &frame, &geometric)
                );
            }
        }
    }
}
