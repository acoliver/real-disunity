use super::*;
use std::sync::Arc;
mod dependency;

pub(crate) fn layered_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::terrain_source();
    let mut textures = base.resources().textures().to_vec();
    textures[0] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![180, 90, 40, 0].into(),
    };
    textures.push(SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128].into(),
    });
    let terrain = layered_terrain().with_page_policy(Some(policy())).unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
        geometry: base.geometry().clone(),
        terrain: Some(terrain),
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn policy() -> TerrainPagePolicy {
    let settings = TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [16; 2],
        border: 1,
        filter_radius: 1,
        maximum_depth: 0,
        split_threshold_millipixels: 2000,
        merge_threshold_millipixels: 1000,
        maximum_selected_pages: 1,
        resident_bytes: 1_000_000,
        upload_bytes_per_frame: 1_000_000,
    })
    .unwrap();
    TerrainPagePolicy::new(settings, 1, 10_000_000).unwrap()
}

fn layered_terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -1000,
        origin_north_millimeters: -1000,
        sample_spacing_millimeters: 2000,
        heights_millimeters: vec![-300; 4],
        surface_indices: vec![0; 4],
        lod_policy: TerrainLodPolicy::FullResolution,
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance {
                layered: Some(TerrainLayer {
                    albedo_metallic: TextureHandle::from_raw(0),
                    normal_roughness: TextureHandle::from_raw(1),
                    height: TextureHandle::from_raw(3),
                    normal_encoding: SceneNormalEncoding::Rgb,
                    repeat_millimeters: 2000,
                    offset_millimeters: [0; 2],
                    roughness_metallic_offset_micro: [0; 2],
                }),
                ..SceneTerrainAppearance::NEUTRAL
            },
        )],
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        channel_map: None,
        type_grid: TerrainTypeGrid::new([2, 2], [-1000; 2], 2000, vec![0; 4]).unwrap(),
        filter: TerrainSplatFilter::Bilinear,
        colorization: None,
    })
    .unwrap()
}

#[test]
fn layered_source_and_page_policy_install_through_windowless_readiness() {
    let source = layered_scene();
    let terrain = Arc::new(source.terrain().unwrap().clone());
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(source.with_terrain(None).unwrap())
        .windowless(&mut driver)
        .unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Terrain(Some(terrain)))
        .unwrap();
    let _ = capture(&mut run, &events);
    assert_eq!(run.host().ready_generation(), 1);
    let warm = capture(&mut run, &events).1;
    assert_eq!(capture(&mut run, &events).1, warm);
    run.host_mut().apply(RetainedUpdate::Terrain(None)).unwrap();
    assert_ne!(capture(&mut run, &events).1, warm);
    run.shutdown().unwrap();
}

#[test]
fn page_policy_requires_layered_source_and_can_be_disabled() {
    let source = crate::renderer::retained_tests::terrain_source();
    assert!(
        source
            .terrain()
            .unwrap()
            .clone()
            .with_page_policy(Some(policy()))
            .is_err()
    );
    assert!(
        layered_terrain()
            .with_page_policy(Some(policy()))
            .unwrap()
            .with_page_policy(None)
            .unwrap()
            .page_policy()
            .is_none()
    );
}
