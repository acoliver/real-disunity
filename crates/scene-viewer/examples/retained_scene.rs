//! Run with `cargo run -p worldrunner-scene-viewer --example retained_scene`.
//! A native application edits typed scene data without GPU scheduling calls.
#[test]
fn consumer_updates_3d_scene_using_only_cpu_types() {
    let mut host = RetainedHost::new(scene().unwrap(), options()).unwrap();
    let before = host.object(ObjectId(0)).unwrap();
    MovingObject
        .update(
            &mut host,
            &RetainedTick {
                frame: 1,
                elapsed: std::time::Duration::ZERO,
                dt: std::time::Duration::ZERO,
            },
        )
        .unwrap();
    let after = host.object(ObjectId(0)).unwrap();
    assert_ne!(before.transform(), after.transform());
    assert_eq!(before.mesh(), after.mesh());
    assert_eq!(host.objects().count(), 1);
}

use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneAlphaMode, SceneCamera, SceneGeometry,
    SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject, SceneParts,
    SceneResources, SceneRotation, SceneTexture, SceneTransform, SceneValidationError, SceneVertex,
    TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{
    FrameProfile, ObjectId, OrbitLimits, RetainedDriver, RetainedHost, RetainedTick,
    RetainedUpdate, ViewerOptions,
};

struct MovingObject;
impl RetainedDriver for MovingObject {
    fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
        let object = host.object(ObjectId(0)).expect("example object");
        let transform = SceneTransform {
            position_millimeters: [
                i64::try_from(tick.frame % 240).expect("bounded phase") * 10 - 1200,
                0,
                0,
            ],
            ..*object.transform()
        };
        host.apply(RetainedUpdate::Replace {
            id: ObjectId(0),
            object: SceneObject::new(transform, object.mesh(), object.material()),
        })
        .map_err(|error| error.to_string())?;
        if host.key_held("escape") {
            host.request_exit();
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    RetainedHost::new(scene()?, options())?.run(&mut MovingObject)?;
    Ok(())
}

fn scene() -> Result<RenderScene, SceneValidationError> {
    let vertices = [[-1000, 0, 0], [1000, 0, 0], [0, 0, 2000]]
        .into_iter()
        .map(|position| SceneVertex {
            tangent_micro: None,
            position_millimeters: position,
            normal_micro: [0, -1_000_000, 0],
            texcoord_micro: [0, 0],
        })
        .collect();
    let mesh = SceneMesh::new(vec![SceneMeshLod::new(0, vertices, vec![0, 1, 2], None)?])?;
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })?;
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![80, 160, 240, 255].into(),
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material])?,
        geometry: SceneGeometry::new(
            vec![mesh],
            vec![SceneObject::new(
                SceneTransform {
                    position_millimeters: [0; 3],
                    rotation: SceneRotation::IDENTITY,
                    scale_millionths: 1_000_000,
                },
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )?,
        terrain: None,
        effects: None,
    })
}

fn options() -> ViewerOptions {
    ViewerOptions {
        presentation_budget: worldrunner_scene_viewer::PresentationBudget::default(),
        camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        title: "Retained native scene".into(),
        size: [800, 600],
        pose: CameraPose {
            eye_millimeters: [0, -5000, 2500],
            target_millimeters: [0, 0, 800],
        },
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        orbit: OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 100_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
        shadows: None,
        frame_profile: FrameProfile::Legacy,
        frames_in_flight: 2,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        gpu_timing: false,
        adapter_callback: None,
        cadence: worldrunner_scene_viewer::PresentationCadence::default(),
        ssr: None,
        render_clock: worldrunner_scene_viewer::RenderClock::default(),
    }
}
