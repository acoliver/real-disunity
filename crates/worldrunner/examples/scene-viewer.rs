use worldrunner::scene::plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner::scene::{
    gpu::{
        CameraPose,
        shadows::{CascadeRefreshPolicy, DirectionalShadowSettings},
    },
    plan::{
        MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode,
        SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading, SceneEffects,
        SceneEffectsParts, SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh,
        SceneMeshLod, SceneObject, SceneParts, SceneResources, SceneRotation, SceneShadowCascades,
        SceneSky, SceneSun, SceneTexture, SceneTransform, SceneVertex, TexelFormat,
        TextureColorSpace, TextureHandle,
    },
    viewer::{
        OrbitLimits, OrbitNavigation, PresentMode, RetainedDriver, RetainedEvent,
        RetainedEventHost, RetainedHost, RetainedInput, RetainedTick, RetainedUpdate,
        ViewerOptions, ViewerStart,
    },
};
use worldrunner_render_plan::{
    SceneDirection, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy,
};
const MICRO: i64 = 1_000_000;

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn effects(terrain: bool) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: std::env::args().any(|arg| arg == "--water").then_some(
            worldrunner_render_plan::SceneWaterBody {
                material: worldrunner_render_plan::SceneWaterSettings::STOCK,
                surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
                flow_millimeters_per_second: [0; 2],
                height_millimeters: 100,
                east_min_millimeters: -900,
                north_min_millimeters: -900,
                east_extent_millimeters: 1800,
                north_extent_millimeters: 1800,
                color_micro: [50000, 200_000, 250_000],
                absorption_micro: [300_000, 100_000, 50000],
                roughness_micro: 200_000,
            },
        ),
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
            direction: SceneDirection::new(if terrain {
                [600_000, 0, 800_000]
            } else {
                [0, 0, 1_000_000]
            })
            .unwrap(),
            irradiance_micro: if terrain { 1_000_000 } else { 0 },
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: if terrain { 100_000 } else { 1_000_000 },
            ground_irradiance_micro: if terrain { 100_000 } else { 1_000_000 },
        },
        shadows: SceneShadowCascades::new(vec![2_000, 4_000, 8_000, 16_000], 100_000)
            .expect("four coverage cascades are valid"),
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
    .expect("the example effects are valid")
}

fn identity() -> SceneTransform {
    SceneTransform {
        position_millimeters: [0; 3],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 0,
            w: ROTATION_MICRO_SCALE,
        },
        scale_millionths: 1_000_000,
    }
}

/// A vertex given in Y-up (x, up, z) millimetres, placed in the scene's Z-up basis
/// (east, north, elevation) as `(x, -z, up)`, facing up.
fn vertex([x, up, z]: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex {
        position_millimeters: [x, -z, up],
        normal_micro: [0, 0, MICRO],
        tangent_micro: None,
        texcoord_micro: texcoord,
    }
}

/// A two-metre ground quad at elevation zero, facing up.
///
/// The winding is counter-clockwise seen from above, which is what the
/// pipelines declare as front-facing. A quad wound the other way would vanish
/// entirely under back-face culling.
fn quad() -> SceneMesh {
    let half = 1_000;
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-half, 0, half], [0, 0]),
                vertex([half, 0, half], [MICRO, 0]),
                vertex([half, 0, -half], [MICRO, MICRO]),
                vertex([-half, 0, -half], [0, MICRO]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .expect("the example quad is a valid level"),
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
    .expect("the example material carries in-range scalars")
}

fn terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 7,
        rows: 7,
        origin_east_millimeters: -3_000,
        origin_north_millimeters: -3_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: (0..49)
            .map(|i| if i % 7 == 4 { 2_000 } else { 0 })
            .collect(),
        surface_indices: vec![0; 49],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(1),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn scene(with_terrain: bool) -> RenderScene {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255, 0, 0, 255].into(),
    };
    let map = |bytes: [u8; 4]| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Vec::from(bytes).into(),
    };
    let terrain_material = SceneMaterial::new(SceneMaterialParts {
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
    let terrain = with_terrain.then(terrain);
    let mut placement = identity();
    if with_terrain {
        placement.position_millimeters[2] = 1_500;
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture, map([128, 128, 255, 255]), map([0, 255, 0, 255])],
            vec![material(SceneAlphaMode::Opaque, false), terrain_material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad()],
            vec![SceneObject::new(
                placement,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain,
        effects: Some(effects(with_terrain)),
    })
    .unwrap()
}

fn refresh_policy() -> CascadeRefreshPolicy {
    if std::env::args().any(|arg| arg == "--staggered-shadows") {
        CascadeRefreshPolicy {
            intervals: [1, 2, 4, 4],
            phases: [0, 0, 1, 3],
            max_non_near: 1,
            light_delta_radians: 0.01,
            projection_delta_texels: 2.0,
        }
    } else {
        CascadeRefreshPolicy::default()
    }
}

/// Orbits the camera with the window's pointer and wheel, resets it with R or Home and closes the
/// window on Escape. The retained host draws the camera it is sent and leaves moving it to the
/// application.
struct Orbit {
    navigation: OrbitNavigation,
    size: [u32; 2],
    lens: SceneCamera,
    convention: worldrunner_camera::ViewConvention,
}

impl RetainedDriver for Orbit {
    fn update(&mut self, host: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        self.size = host.size();
        if let Some(pose) = self.navigation.take_moved() {
            host.apply(RetainedUpdate::Camera {
                pose,
                lens: self.lens,
                convention: self.convention,
            })
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        if let RetainedEvent::Input(input) = event {
            self.navigation.accept(input, self.size);
            if let RetainedInput::Key {
                key, pressed: true, ..
            } = input
                && key == "escape"
            {
                host.request_exit();
            }
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let terrain = std::env::args().any(|arg| arg == "--terrain");
    let shadows =
        std::env::args()
            .any(|arg| arg == "--shadows")
            .then_some(DirectionalShadowSettings {
                focus_meters: worldrunner::glam::Vec3::ZERO,
                half_extent_meters: 4.0,
                depth_half_extent_meters: 4.0,
                resolution: 512,
                depth_bias: 0.001,
                normal_bias_meters: 0.01,
                refresh_policy: refresh_policy(),
            });
    let options = ViewerOptions {
        activity_reason: worldrunner_scene_viewer::ActivityReason::default(),
        presentation_budget: worldrunner_scene_viewer::PresentationBudget::default().into(),
        capture_pool: worldrunner_scene_viewer::CapturePoolPolicy::default(),
        window_size_unit: worldrunner_scene_viewer::WindowSizeUnit::default(),
        window_mode: worldrunner_scene_viewer::WindowMode::default(),
        camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        frames_in_flight: 2,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        texture_tiers: None,
        gpu_timing: false,
        adapter_callback: None,
        cadence: worldrunner_scene_viewer::PresentationCadence::default(),
        ssr: None,
        render_clock: worldrunner_scene_viewer::RenderClock::default(),
        game: worldrunner_scene_viewer::GameRendering::default(),
        frame_profile: frame_profile(),
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        title: "Generic scene viewer".into(),
        size: [800, 600],
        pose: CameraPose {
            eye_millimeters: if terrain {
                [0, -7_000, 6_000]
            } else {
                [0, -2000, 2000]
            },
            target_millimeters: [0, 0, 0],
        },
        orbit: OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 10_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
        shadows,
    };
    let mut driver = Orbit {
        navigation: OrbitNavigation::default(),
        size: options.size,
        lens: options.camera,
        convention: options.camera_convention,
    };
    driver.navigation.start(options.pose, options.orbit)?;
    RetainedHost::new(scene(terrain), options)?.run(ViewerStart::new()?, &mut driver)?;
    Ok(())
}

fn frame_profile() -> worldrunner::scene::viewer::FrameProfile {
    use worldrunner::scene::viewer::{FrameProfile, ToneMap};
    let samples = if std::env::args().any(|arg| arg == "--msaa") {
        4
    } else {
        1
    };
    if let Some(value) =
        std::env::args().find_map(|arg| arg.strip_prefix("--render-scale=").map(str::to_owned))
    {
        let millionths = value
            .parse()
            .expect("render scale is an integer in millionths");
        return FrameProfile::HdrUpscaled {
            samples,
            tone_map: ToneMap::Reinhard,
            scale: RenderScale::new(millionths).expect("render scale must be 500000..=1000000"),
            upscale: UpscaleSettings {
                mode: if std::env::args().any(|arg| arg == "--bilinear") {
                    UpscaleMode::Bilinear
                } else {
                    UpscaleMode::Easu
                },
                sharpness: std::env::args().any(|arg| arg == "--rcas").then_some(0.0),
            },
        };
    }
    FrameProfile::Hdr {
        samples,
        tone_map: ToneMap::Reinhard,
    }
}
