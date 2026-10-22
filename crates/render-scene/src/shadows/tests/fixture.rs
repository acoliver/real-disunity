use glam::Vec3;
use worldrunner_render_plan::SceneDirection;
use worldrunner_render_plan::*;

use super::super::DirectionalShadowSettings;
use crate::{CameraPose, SceneView, Viewport};

pub(super) fn settings() -> DirectionalShadowSettings {
    DirectionalShadowSettings {
        focus_meters: Vec3::new(0.0, 0.0, 0.5),
        half_extent_meters: 4.0,
        depth_half_extent_meters: 4.0,
        resolution: 512,
        depth_bias: 0.001,
        normal_bias_meters: 0.01,
    }
}

pub(super) fn effects(light_x: i64) -> SceneEffects {
    SceneEffects::new(effects_parts(light_x)).unwrap()
}

pub(super) fn effects_parts(light_x: i64) -> SceneEffectsParts {
    SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: SceneSun {
            direction: SceneDirection::new([light_x.try_into().unwrap(), 0, 800_000]).unwrap(),
            irradiance_micro: 1_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 100_000,
            ground_irradiance_micro: 100_000,
        },
        shadows: SceneShadowCascades::new(vec![4_000], 0).unwrap(),
        ssao: None,
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 20_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: SceneAntiAlias::Disabled,
    }
}

pub(super) fn quad(half: i64) -> SceneMesh {
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            [
                ([-half, -half, 0], [0, 0]),
                ([half, -half, 0], [1_000_000, 0]),
                ([half, half, 0], [1_000_000, 1_000_000]),
                ([-half, half, 0], [0, 1_000_000]),
            ]
            .into_iter()
            .map(|(position, uv)| SceneVertex {
                position_millimeters: position,
                normal_micro: [0, 0, 1_000_000],
                texcoord_micro: uv,
            })
            .collect(),
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn material(texture: u32, alpha_mode: SceneAlphaMode, double_sided: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided,
    })
    .unwrap()
}

pub(super) fn scene(casters: &[[i64; 3]], light_x: i64, alpha: Option<[u8; 4]>) -> RenderScene {
    let texture = |alphas: [u8; 4]| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 4,
        height: 1,
        mips: 1,
        bytes: alphas
            .into_iter()
            .flat_map(|a| [220, 220, 220, a])
            .collect::<Vec<_>>()
            .into(),
    };
    let mut objects = vec![object([0, 0, 0], 0, 0)];
    objects.extend(casters.iter().map(|&p| object(p, 1, 1)));
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture([255; 4]), texture(alpha.unwrap_or([255; 4]))],
            vec![
                material(0, SceneAlphaMode::Opaque, false),
                material(
                    1,
                    alpha.map_or(SceneAlphaMode::Opaque, |_| SceneAlphaMode::Mask {
                        cutoff_micro: 500_000,
                    }),
                    false,
                ),
            ],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![quad(3_000), quad(300)], objects).unwrap(),
        terrain: None,
        effects: Some(effects(light_x)),
    })
    .unwrap()
}

pub(super) fn object(position: [i64; 3], mesh: u32, material: u32) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: position,
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 0,
                w: ROTATION_MICRO_SCALE,
            },
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(material),
    )
}

pub(super) fn view() -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -4_000, 6_500],
            target_millimeters: [0, 0, 0],
        },
        *effects(600_000).camera(),
        Viewport {
            width: 257,
            height: 241,
        },
    )
    .unwrap()
}

pub(super) fn caster_scene(
    casters: Vec<SceneObject>,
    mesh: SceneMesh,
    alpha: Option<[u8; 4]>,
    double_sided: bool,
) -> RenderScene {
    let source = scene(&[], 600_000, alpha);
    let caster = material(
        1,
        source.resources().materials()[1].alpha_mode(),
        double_sided,
    );
    let mut objects = source.geometry().objects().to_vec();
    objects.extend(casters);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            source.resources().textures().to_vec(),
            vec![source.resources().materials()[0], caster],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![quad(3_000), mesh], objects).unwrap(),
        terrain: None,
        effects: source.effects().cloned(),
    })
    .unwrap()
}

pub(super) fn terrain_scene(source: &RenderScene) -> RenderScene {
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: Some(terrain),
        effects: source.effects().cloned(),
    })
    .unwrap()
}
