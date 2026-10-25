use crate::{CameraPose, SceneView, Viewport};
use worldrunner_render_plan::SceneDirection;
use worldrunner_render_plan::*;
const MICRO: i64 = 1_000_000;

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn effects() -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
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
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            // Ambient-only lighting isolates raw albedo/readback from white GGX highlights.
            irradiance_micro: 0,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
        shadows: SceneShadowCascades::new(vec![10_000, 40_000], 50_000)
            .expect("two ascending cascades are valid"),
        ssao: None,
        camera: camera(),
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    })
    .expect("the fixture effects are valid")
}

fn identity() -> SceneTransform {
    SceneTransform {
        position_millimeters: [0, 0, 0],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 0,
            w: ROTATION_MICRO_SCALE,
        },
        scale_millionths: 1_000_000,
    }
}

fn vertex(position: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex {
        position_millimeters: position,
        normal_micro: [0, 0, MICRO],
        texcoord_micro: texcoord,
    }
}

/// A two-metre ground quad at elevation zero, facing up.
///
/// The winding is counter-clockwise seen from above, which is what the
/// pipelines declare as front-facing. A quad wound the other way would vanish
/// entirely under back-face culling, which is exactly the failure the
/// single-sided test below is looking for.
fn quad() -> SceneMesh {
    let half = 1_000;
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-half, -half, 0], [0, 0]),
                vertex([half, -half, 0], [MICRO, 0]),
                vertex([half, half, 0], [MICRO, MICRO]),
                vertex([-half, half, 0], [0, MICRO]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .expect("the fixture quad is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

fn material(alpha_mode: SceneAlphaMode, double_sided: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided,
    })
    .expect("the fixture material carries in-range scalars")
}

pub(super) fn scene(malformed: bool) -> RenderScene {
    let texture = if malformed {
        SceneTexture::Encoded {
            encoding: TextureEncoding::DdsContainer,
            color_space: TextureColorSpace::Srgb,
            bytes: vec![1, 2, 3, 4].into(),
        }
    } else {
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![255, 0, 0, 255].into(),
        }
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad()],
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

pub(super) fn view(scene: &RenderScene, viewport: Viewport) -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -2_000, 2_000],
            target_millimeters: [0, 0, 0],
        },
        *scene.effects().unwrap().camera(),
        viewport,
    )
    .unwrap()
}
