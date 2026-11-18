//! Declared reducible textures: empty by default, kept across edits, and refused where a
//! consumer reads level 0 or the authored size.
use super::super::tests::{dome_sky_effects, effects, object, scene_with, terrain, texture};
use crate::scene::{
    MaterialHandle, RenderScene, SceneAlphaMode, SceneFadeCoverage, SceneGeometry, SceneMaterial,
    SceneMaterialParts, SceneParts, SceneResources, SceneTexture, SceneValidationError,
    TexelFormat, TextureColorSpace, TextureHandle,
};

fn handle(raw: u32) -> TextureHandle {
    TextureHandle::from_raw(raw)
}

fn material(albedo: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: handle(albedo),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .expect("synthetic material validates")
}

/// A square linear R8 grid with one level, the shape a fade threshold texture must have.
fn threshold() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 4,
        mips: 1,
        bytes: vec![0; 16].into(),
    }
}

fn refusal(scene: &RenderScene, raw: u32) -> &'static str {
    match scene.with_reducible_textures(vec![handle(raw)]) {
        Err(SceneValidationError::UnsupportedInput { part }) => part,
        other => panic!("expected a named refusal, got {other:?}"),
    }
}

#[test]
fn nothing_is_reducible_until_declared_and_the_declaration_survives_edits() {
    let scene = scene_with(vec![object(0, 0)], None).unwrap();
    assert!(scene.resources().reducible_textures().is_empty());
    let declared = scene.with_reducible_textures(vec![handle(0)]).unwrap();
    assert_eq!(declared.resources().reducible_textures(), [handle(0)]);
    let moved = declared
        .with_objects_and_effects(vec![object(0, 0)], Some(effects()))
        .unwrap();
    assert_eq!(moved.resources().reducible_textures(), [handle(0)]);
    let retextured = declared.resources().with_textures(vec![texture()]).unwrap();
    assert_eq!(retextured.reducible_textures(), [handle(0)]);
    let cleared = declared.with_reducible_textures(Vec::new()).unwrap();
    assert!(cleared.resources().reducible_textures().is_empty());
}

#[test]
fn a_handle_outside_the_texture_table_is_refused() {
    let scene = scene_with(vec![object(0, 0)], None).unwrap();
    assert!(matches!(
        scene.with_reducible_textures(vec![handle(1)]),
        Err(SceneValidationError::HandleOutOfRange {
            resource: "texture",
            handle: 1,
            ..
        })
    ));
}

#[test]
fn a_sky_dome_texture_is_refused_by_name() {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture()], vec![material(0)]).unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: None,
        effects: Some(dome_sky_effects()),
    })
    .unwrap();
    assert_eq!(refusal(&scene, 0), "a reducible texture is the sky dome");
}

#[test]
fn a_fade_threshold_texture_is_refused_by_name() {
    let fading = material(0).with_fade_coverage(SceneFadeCoverage::Threshold {
        texture: handle(1),
        jitter_pixels: [0, 0],
    });
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture(), threshold()], vec![fading]).unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap();
    assert_eq!(
        refusal(&scene, 1),
        "a reducible texture is a fade threshold texture"
    );
    // The material's own albedo stays declarable.
    assert!(scene.with_reducible_textures(vec![handle(0)]).is_ok());
}

#[test]
fn a_terrain_palette_map_is_refused_by_name() {
    let scene = scene_with(Vec::new(), Some(terrain(vec![MaterialHandle::from_raw(0)]))).unwrap();
    assert_eq!(
        refusal(&scene, 0),
        "a reducible texture is a terrain palette or layer source"
    );
}

#[test]
fn resource_replacement_rejects_missing_terrain_material_before_reducer_lookup() {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture(), texture()], vec![material(0), material(0)])
            .unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(terrain(vec![MaterialHandle::from_raw(1)])),
        effects: None,
    })
    .unwrap()
    .with_reducible_textures(vec![handle(1)])
    .unwrap();
    let resources = scene
        .resources()
        .clone()
        .with_materials(vec![material(0)])
        .unwrap();
    assert!(matches!(
        scene.with_resources(resources, scene.geometry().clone(), None),
        Err(SceneValidationError::HandleOutOfRange {
            part: "terrain surfaces",
            resource: "material",
            handle: 1,
            limit: 1,
        })
    ));
}

/// The test effects with one water body whose normal map is texture 0.
fn water_effects() -> crate::scene::SceneEffects {
    let base = effects();
    crate::scene::SceneEffects::new(crate::scene::effects::SceneEffectsParts {
        water: Some(crate::SceneWaterBody {
            material: crate::SceneWaterSettings {
                normal: Some(handle(0)),
                ..crate::SceneWaterSettings::STOCK
            },
            surface: crate::SceneWaterSurface::Rectangle,
            flow_millimeters_per_second: [0; 2],
            height_millimeters: 0,
            color_micro: [50_000, 200_000, 250_000],
            absorption_micro: [300_000, 100_000, 50_000],
            roughness_micro: 200_000,
            east_min_millimeters: 0,
            north_min_millimeters: 0,
            east_extent_millimeters: 2_000,
            north_extent_millimeters: 2_000,
        }),
        sky: *base.sky(),
        sun: *base.sun(),
        ambient: *base.ambient(),
        shadows: base.shadows().clone(),
        ssao: base.ssao().copied(),
        camera: *base.camera(),
        grading: *base.grading(),
        post: Vec::new(),
        anti_alias: crate::scene::effects::SceneAntiAlias::Disabled,
    })
    .expect("water effects validate")
}

#[test]
fn a_water_normal_map_is_refused_by_name() {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture(), texture()], vec![material(1)]).unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: None,
        effects: Some(water_effects()),
    })
    .unwrap();
    assert_eq!(
        refusal(&scene, 0),
        "a reducible texture is a water normal, flow or wind map"
    );
    assert!(scene.with_reducible_textures(vec![handle(1)]).is_ok());
}

#[test]
fn impostor_maps_cannot_lose_their_frame_grid() {
    use crate::{SceneImpostor, SceneImpostorLayout};
    let atlas =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [16, 16], 0, false).unwrap();
    let image = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 64,
        height: 64,
        mips: 1,
        bytes: vec![255; 64 * 64 * 4].into(),
    };
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![image], vec![material(0).with_impostor(atlas)])
            .unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    assert_eq!(
        refusal(&scene, 0),
        "a reducible texture is an impostor atlas"
    );
}
