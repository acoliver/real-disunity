use worldrunner_render_plan::*;

pub(super) fn scene(rectangular: bool) -> RenderScene {
    let (columns, rows, spacing, origin) = if rectangular {
        (3, 5, 2000, [-2000, -4000])
    } else {
        (2, 2, 4000, [-2000, -2000])
    };
    let textures =
        [[255; 4], [128, 128, 255, 255], [0, 180, 0, 255]].map(|color| SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: color.to_vec().into(),
        });
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
        columns,
        rows,
        origin_east_millimeters: origin[0],
        origin_north_millimeters: origin[1],
        sample_spacing_millimeters: spacing,
        heights_millimeters: vec![0; (columns * rows) as usize],
        surface_indices: vec![0; (columns * rows) as usize],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures.to_vec(), vec![material]).unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}
