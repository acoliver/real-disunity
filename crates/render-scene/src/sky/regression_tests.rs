use super::scene_tests::*;
use super::*;
use crate::view::{CameraPose, OrthographicLens, SceneView, Viewport};
use crate::{device::GpuContext, draw::SceneGpu, frame::FrameUniform, renderer::ScenePipelines};
use worldrunner_render_plan::*;

#[test]
fn physical_mesh_composition_preserves_depth_exposure_and_fog() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut sky = atmosphere(0);
    sky.scattering = Some(super::scattering::tests::settings());
    let objects = scene(0, true, false).geometry().objects().to_vec();
    let occluded = render(&context, &custom(sky.clone(), quad(), objects), 1.0);
    for pixel in [3, 5] {
        assert!(occluded.depth(pixel) < 1.0);
        assert!(occluded.rgb(pixel)[0] > 0.9);
        assert!(occluded.rgb(pixel)[1] < 0.001);
    }
    let source = custom(sky.clone(), quad(), vec![]);
    let unit = render(&context, &source, 1.0);
    let twice = render(&context, &source, 2.0);
    assert!(unit.rgb(4).into_iter().all(f32::is_finite));
    for (actual, expected) in twice.rgb(4).into_iter().zip(unit.rgb(4)) {
        assert!((actual - expected * 2.0).abs() < 0.005);
    }
    assert_eq!(unit.depth(4).to_bits(), 1.0_f32.to_bits());
    sky.meshes.clear();
    let background = render(&context, &custom(sky.clone(), quad(), vec![]), 1.0);
    assert!(unit.rgb(4)[1] > background.rgb(4)[1]);
    sky.meshes = atmosphere(0).meshes;
    sky.fog = atmosphere(100_000).fog;
    let fogged = render(&context, &custom(sky, quad(), vec![]), 1.0);
    assert!(fogged.rgb(4)[2] > unit.rgb(4)[2]);
}

fn custom(sky: SceneAtmosphere, mesh: SceneMesh, objects: Vec<SceneObject>) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures().to_vec(), vec![material(1), material(2)])
            .unwrap(),
        geometry: SceneGeometry::new(vec![mesh], objects).unwrap(),
        terrain: None,
        effects: Some(base_effects(false).with_atmosphere(sky).unwrap()),
    })
    .unwrap()
}

#[test]
fn regression_b1_nearest_celestial_surface_is_index_order_independent() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut captures = Vec::new();
    for reverse in [false, true] {
        let mut vertices = Vec::new();
        for (y, u) in [(-100, 250_000), (100, 750_000)] {
            for [x, z] in [[-707, -707], [707, -707], [707, 707], [-707, 707]] {
                vertices.push(SceneVertex {
                    tangent_micro: None,
                    position_millimeters: [x, y, z],
                    normal_micro: [0, -1_000_000, 0],
                    texcoord_micro: [u, 0],
                });
            }
        }
        let mut indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
        if reverse {
            indices.rotate_left(6);
        }
        let mesh =
            SceneMesh::new(vec![SceneMeshLod::new(0, vertices, indices, None).unwrap()]).unwrap();
        let mut tex = textures().to_vec();
        tex[2] = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 2,
            height: 1,
            mips: 1,
            bytes: vec![255, 0, 0, 255, 0, 255, 0, 255].into(),
        };
        let source = RenderScene::new(SceneParts {
            resources: SceneResources::new(tex, vec![material(1), material(2)]).unwrap(),
            geometry: SceneGeometry::new(vec![mesh], vec![]).unwrap(),
            terrain: None,
            effects: Some(effects(0, false)),
        })
        .unwrap();
        let image = render(&context, &source, 1.0);
        assert_eq!(
            image.rgb(4).map(f32::to_bits),
            [1.0, 0.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(image.depth(4).to_bits(), 1.0f32.to_bits());
        captures.push(image.color);
    }
    assert_eq!(captures[0], captures[1]);
}

#[test]
fn regression_b2_orthographic_body_behind_eye_is_clipped() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let front = render(&context, &scene(0, false, false), 1.0);
    assert_eq!(
        front.rgb(4).map(f32::to_bits),
        [0.0, 1.0, 0.0].map(f32::to_bits)
    );
    let mut sky = atmosphere(0);
    sky.meshes[0].direction = SceneDirection::new([0, -1_000_000, 0]).unwrap();
    let behind = render(&context, &custom(sky, quad(), vec![]), 1.0);
    assert_eq!(
        behind.rgb(4).map(f32::to_bits),
        [0.0, 0.0, 2.0].map(f32::to_bits)
    );
    assert_eq!(behind.depth, front.depth);
    let mut distant = atmosphere(0);
    distant.meshes[0].distance_millimeters = 200_000;
    let image = render(&context, &custom(distant, quad(), vec![]), 1.0);
    assert_eq!(
        image.rgb(4).map(f32::to_bits),
        [0.0, 1.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(image.depth, front.depth);
}

#[test]
fn regression_b3_billboard_is_translation_invariant() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for mode in [BillboardPolicy::YawLocked, BillboardPolicy::Spherical] {
        let mesh = SceneMesh::new(vec![
            SceneMeshLod::new(
                0,
                quad().lods()[0].vertices().to_vec(),
                vec![0, 1, 2, 0, 2, 3],
                Some(mode),
            )
            .unwrap(),
        ])
        .unwrap();
        let source = custom(atmosphere(0), mesh, vec![]);
        let baseline = render(&context, &source, 1.0);
        let moved = SceneView::orthographic_with_up(
            CameraPose {
                eye_millimeters: [10_000, 10_000, 0],
                target_millimeters: [10_000, 11_000, 0],
            },
            OrthographicLens {
                vertical_half_extent_meters: 2.0,
                near_meters: 0.1,
                far_meters: 100.0,
            },
            [0.0, 0.0, 1.0],
            Viewport {
                width: 9,
                height: 3,
            },
        )
        .unwrap();
        assert_eq!(
            baseline.color,
            render_at(&context, &source, 1.0, &moved).color
        );
    }
}

#[test]
fn regression_b4_resident_frame_fog_controls_objects_sky_and_discs() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut sky = atmosphere(100_000);
    sky.meshes.clear();
    sky.discs.push(SceneCelestialDisc {
        direction: SceneDirection::new([0, 1_000_000, 0]).unwrap(),
        angular_radius_millidegrees: 30_000,
        rotation_millidegrees: 0,
        texture: None,
        radiance_micro: [0, 1_000_000, 0],
    });
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [-1500, 2000, 0],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let source = custom(sky, quad(), vec![object]);
    let pipelines = ScenePipelines::new(context.device(), wgpu::TextureFormat::Rgba16Float);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut frame =
        FrameUniform::new(&perspective_view(60_000, [0, 1000, 0]), source.effects()).unwrap();
    frame.height_fog = None;
    let clear = render_resident(&context, &gpu, &pipelines, &frame);
    assert_eq!(
        clear.rgb(0).map(f32::to_bits),
        [0.0, 0.0, 2.0].map(f32::to_bits)
    );
    assert_eq!(
        clear.rgb(2).map(f32::to_bits),
        [1.0, 0.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        clear.rgb(4).map(f32::to_bits),
        [0.0, 1.0, 0.0].map(f32::to_bits)
    );
    frame.height_fog = Some(
        worldrunner_render_lighting::atmosphere::HeightFog::new(
            100.0,
            0.0,
            0.0,
            0.0,
            Vec3::splat(4.0),
        )
        .unwrap(),
    );
    let fogged = render_resident(&context, &gpu, &pipelines, &frame);
    for x in [0, 2, 4, 5, 8] {
        assert_eq!(fogged.rgb(x).map(f32::to_bits), [4.0; 3].map(f32::to_bits));
    }
}

#[test]
fn regression_b5_orthographic_wall_uses_parallel_rays_and_pixel_height() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut sky = atmosphere(100_000);
    sky.meshes.clear();
    sky.fog.as_mut().unwrap().falloff_micro_per_meter = 500_000;
    let wall = SceneObject::new(
        SceneTransform {
            position_millimeters: [0, 2000, 0],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 20_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let source = custom(sky, quad(), vec![wall]);
    let image = render(&context, &source, 1.0);
    let t = (-0.2f32).exp();
    for x in 0..9 {
        assert!((image.rgb(x)[0] - t).abs() < 0.001);
    }
    for (y, z) in [(0, 4.0f32 / 3.0), (2, -4.0 / 3.0)] {
        let expected = (-0.2 * (-0.5 * z).exp()).exp();
        assert!((image.rgb_at(4, y)[0] - expected).abs() < 0.001);
    }
    let empty = custom(
        {
            let mut s = atmosphere(100_000);
            s.meshes.clear();
            s.fog.as_mut().unwrap().falloff_micro_per_meter = 500_000;
            s
        },
        quad(),
        vec![],
    );
    let background = render(&context, &empty, 1.0);
    for (y, z) in [(0, 4.0f32 / 3.0), (2, -4.0 / 3.0)] {
        let expected = 1.0 + (-2.0 * (-0.5 * z).exp()).exp();
        assert!((background.rgb_at(4, y)[2] - expected).abs() < 0.002);
    }
}

#[test]
fn regression_b6_perspective_dome_mips_are_periodic_at_longitude_seam() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut tex = textures().to_vec();
    let mut bytes = Vec::new();
    for mip in 0..7 {
        let color = if mip == 0 {
            [255, 0, 0, 255]
        } else if mip == 1 {
            [0, 255, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        let width = (64 >> mip).max(1);
        let period = (width / 2).max(1);
        for _y in 0..(32 >> mip).max(1) {
            for x in 0..width {
                let mut pixel = color;
                if x % period >= period / 4 && x % period < 3 * period / 4 {
                    pixel[0] /= 2;
                    pixel[1] /= 2;
                    pixel[2] /= 2;
                }
                bytes.extend(pixel);
            }
        }
    }
    tex[3] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 64,
        height: 32,
        mips: 7,
        bytes: bytes.into(),
    };
    let mut sky = atmosphere(0);
    sky.meshes.clear();
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(tex, vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: Some(base_effects(true).with_atmosphere(sky).unwrap()),
    })
    .unwrap();
    let capture =
        |direction| render_at(&context, &source, 1.0, &perspective_view(10_000, direction));
    let ordinary = capture([1000, 0, 0]);
    let seam = capture([-1000, 0, 0]);
    assert_eq!(seam.color, ordinary.color);
    assert!(seam.rgb(4)[2] < 0.01);
    let minified = render_at(
        &context,
        &source,
        1.0,
        &perspective_view(100_000, [1000, 0, 0]),
    );
    assert!(
        minified.rgb(4)[2] > 0.4,
        "ordinary minification must still select populated coarse mips"
    );
}

fn perspective_view(fov: u32, target: [i64; 3]) -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0; 3],
            target_millimeters: target,
        },
        SceneCamera {
            vertical_fov_millidegrees: fov,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: 9,
            height: 3,
        },
    )
    .unwrap()
}
