mod bias;
mod cascades;
mod culling;
mod fixture;
mod geometry;
mod hdr;
mod receiver;
mod retained;
mod scheduler;
mod selection;
mod ssao;
mod subresources;
mod terrain;
mod validation;

use glam::{Vec3, Vec4};
use worldrunner_render_plan::*;
use worldrunner_render_post::{PassSlot, ResourceRef};

use super::*;
use crate::offscreen::capture_scene;
use fixture::{scene, settings, view};

fn context() -> GpuContext {
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("shadow GPU acceptance requires a real adapter/device; never skip");
    println!("shadow GPU: {:?}", context.adapter().get_info());
    context
}

#[test]
fn settings_reject_invalid_fits_and_biases() {
    let limits = wgpu::Limits::default();
    settings().validate(&limits).unwrap();
    for invalid in [
        DirectionalShadowSettings {
            focus_meters: Vec3::NAN,
            ..settings()
        },
        DirectionalShadowSettings {
            half_extent_meters: 0.0,
            ..settings()
        },
        DirectionalShadowSettings {
            half_extent_meters: f32::INFINITY,
            ..settings()
        },
        DirectionalShadowSettings {
            depth_half_extent_meters: -1.0,
            ..settings()
        },
        DirectionalShadowSettings {
            depth_half_extent_meters: f32::MAX,
            ..settings()
        },
        DirectionalShadowSettings {
            depth_bias: -0.001,
            ..settings()
        },
        DirectionalShadowSettings {
            depth_bias: 1.0,
            ..settings()
        },
        DirectionalShadowSettings {
            normal_bias_meters: f32::NAN,
            ..settings()
        },
        DirectionalShadowSettings {
            normal_bias_meters: -1.0,
            ..settings()
        },
    ] {
        assert!(
            matches!(invalid.validate(&limits), Err(ShadowError::InvalidSettings)),
            "{invalid:?}"
        );
    }
}

#[test]
fn settings_reject_unbounded_allocations_and_honor_device_limits() {
    let limits = wgpu::Limits::default();
    for resolution in [0, MAX_SHADOW_RESOLUTION + 1, u32::MAX] {
        assert!(matches!(
            DirectionalShadowSettings {
                resolution,
                ..settings()
            }
            .validate(&limits),
            Err(ShadowError::Resolution { .. })
        ));
    }
    let small = wgpu::Limits {
        max_texture_dimension_2d: 128,
        ..limits
    };
    assert!(matches!(
        settings().validate(&small),
        Err(ShadowError::Resolution { limit: 128, .. })
    ));
}

#[test]
fn light_projection_maps_focus_and_handles_vertical_sun() {
    for sun in [Vec3::Z, -Vec3::Z, Vec3::new(0.6, 0.0, 0.8)] {
        let matrix = settings().matrix(sun).unwrap();
        let center = matrix.project_point3(settings().focus_meters);
        assert!(center.abs_diff_eq(Vec3::new(0.0, 0.0, 0.5), 1e-5));
        assert!(matrix.project_point3(settings().focus_meters + sun).z < center.z);
    }
}

fn with_cascades(effects: &SceneEffects, extents: Vec<u64>, blend: u64) -> SceneEffects {
    let mut parts =
        fixture::effects_parts(i64::from(effects.sun().direction.components_micro()[0]));
    parts.shadows = SceneShadowCascades::new(extents, blend).unwrap();
    SceneEffects::new(parts).unwrap()
}

#[test]
fn cascades_and_blending_are_supported_with_a_bounded_count() {
    let effects = fixture::effects(600_000);
    for supported in [
        with_cascades(&effects, vec![1_000, 4_000], 0),
        with_cascades(&effects, vec![4_000], 1),
    ] {
        settings::validate_effects(&supported).unwrap();
    }
}

#[test]
fn shadowed_shader_is_valid_wgsl() {
    for source in [resources::SHADOWED_WGSL, resources::SHADOWED_TERRAIN_WGSL] {
        let module = naga::front::wgsl::parse_str(source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test projects known visible points into a small bounded viewport"
)]
fn at_view(image: &RgbaImage, position: Vec3, view: &SceneView) -> u8 {
    let ndc = view.view_projection_matrix().project_point3(position);
    let x = ((ndc.x * 0.5 + 0.5) * 257.0) as usize;
    let y = ((-ndc.y * 0.5 + 0.5) * 241.0) as usize;
    image.pixels[(y * 257 + x) * 4]
}

fn at(image: &RgbaImage, position: Vec3) -> u8 {
    at_view(image, position, &view())
}

fn assert_shadow(image: &RgbaImage, baseline: &RgbaImage, point: Vec3) {
    let lit = at(baseline, point);
    let dark = at(image, point);
    assert!(
        i16::from(lit) - i16::from(dark) > 40,
        "receiver {point:?}: baseline {lit}, shadow {dark}"
    );
}

fn assert_lit(image: &RgbaImage, baseline: &RgbaImage, point: Vec3) {
    let lit = at(baseline, point);
    let actual = at(image, point);
    assert!(
        lit.abs_diff(actual) <= 2,
        "receiver {point:?}: baseline {lit}, actual {actual}"
    );
}

#[test]
fn gpu_receiver_darkens_only_in_geometric_shadow_and_has_pcf_edges() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    assert_shadow(&image, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    assert_lit(&image, &baseline, Vec3::new(1.0, 0.0, 0.0));

    let inverse = view().view_projection_matrix().inverse();
    let eye = view().eye_meters();
    let mut dark_inside = 0;
    let mut lit_outside = 0;
    let mut soft_edge = 0;
    for y in 0_u16..241 {
        for x in 0_u16..257 {
            let clip = Vec4::new(
                (f32::from(x) + 0.5) / 257.0 * 2.0 - 1.0,
                1.0 - (f32::from(y) + 0.5) / 241.0 * 2.0,
                0.5,
                1.0,
            );
            let world = inverse * clip;
            let ray = world.truncate() / world.w - eye;
            let ground = eye + ray * (-eye.z / ray.z);
            if ground.x.abs() > 2.8 || ground.y.abs() > 2.8 {
                continue;
            }
            let on_caster = eye + ray * ((1.0 - eye.z) / ray.z);
            if on_caster.x.abs() < 0.32 && on_caster.y.abs() < 0.32 {
                continue;
            }
            let offset = (usize::from(y) * 257 + usize::from(x)) * 4;
            let diff = i16::from(baseline.pixels[offset]) - i16::from(image.pixels[offset]);
            let distance = (ground.x + 0.75).abs().max(ground.y.abs());
            if distance < 0.23 {
                assert!(
                    diff > 40,
                    "inside geometric shadow at {ground:?}, delta {diff}"
                );
                dark_inside += 1;
            } else if distance > 0.38 {
                assert!(
                    diff.abs() <= 2,
                    "outside geometric shadow at {ground:?}, delta {diff}"
                );
                lit_outside += 1;
            } else if (5..40).contains(&diff) {
                soft_edge += 1;
            }
        }
    }
    println!("geometric receiver pixels: dark={dark_inside}, lit={lit_outside}, PCF={soft_edge}");
    assert!(dark_inside > 50 && lit_outside > 5_000 && soft_edge > 10);
    assert!(
        image == capture_scene_shadowed(&context, &source, &view(), settings()).unwrap(),
        "repeat captures deterministic"
    );
}

#[test]
fn gpu_no_occluder_and_receiver_bias_do_not_create_self_shadow_acne() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    for bias in [0.001, 0.003] {
        let image = capture_scene_shadowed(
            &context,
            &source,
            &view(),
            DirectionalShadowSettings {
                depth_bias: bias,
                ..settings()
            },
        )
        .unwrap();
        let max_delta = baseline
            .pixels
            .iter()
            .zip(&image.pixels)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            max_delta <= 1,
            "unoccluded sloped-light receiver self acne: delta={max_delta}"
        );
    }
    // A positive bias must not erase contact-area occlusion.
    let caster = scene(&[[0, 0, 100]], 600_000, None);
    let baseline = capture_scene(&context, &caster, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &caster, &view(), settings()).unwrap();
    assert_shadow(&image, &baseline, Vec3::new(-0.34, 0.0, 0.0));
}

#[test]
fn gpu_retained_frame_updates_light_fit_resolution_and_moves_object_shadow() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let first = target.read_rgba8(&context).unwrap();
    renderer.set_effects(&fixture::effects(-600_000)).unwrap();
    renderer
        .set_settings(
            &context,
            DirectionalShadowSettings {
                resolution: 1024,
                ..settings()
            },
        )
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let second = target.read_rgba8(&context).unwrap();
    let reversed = scene(&[[0, 0, 1_000]], -600_000, None);
    let baseline = capture_scene(&context, &reversed, &view()).unwrap();
    assert_shadow(&second, &baseline, Vec3::new(0.75, 0.0, 0.0));
    assert_lit(&second, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    assert_ne!(first, second);
    let moved = scene(&[[1_500, 0, 1_000]], 600_000, None);
    let baseline = capture_scene(&context, &moved, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &moved, &view(), settings()).unwrap();
    assert_shadow(&image, &baseline, Vec3::new(0.75, 0.0, 0.0));
    assert_lit(&image, &baseline, Vec3::new(-0.75, 0.0, 0.0));
    renderer
        .set_settings(
            &context,
            DirectionalShadowSettings {
                focus_meters: Vec3::new(20.0, 0.0, 0.0),
                ..settings()
            },
        )
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let outside = target.read_rgba8(&context).unwrap();
    assert_lit(
        &outside,
        &capture_scene(&context, &reversed, &view()).unwrap(),
        Vec3::new(0.75, 0.0, 0.0),
    );
}

#[test]
fn gpu_alpha_cutouts_and_multiple_instances_cast_their_actual_coverage() {
    let context = context();
    let source = scene(&[[0, 0, 1_000], [1_500, 0, 1_000]], 600_000, None);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    for x in [-0.75, 0.75] {
        assert_shadow(&image, &baseline, Vec3::new(x, 0.0, 0.0));
    }
    let transparent = scene(&[[0, 0, 1_000]], 600_000, Some([0; 4]));
    let image = capture_scene_shadowed(&context, &transparent, &view(), settings()).unwrap();
    let empty = capture_scene(&context, &scene(&[], 600_000, None), &view()).unwrap();
    assert!(
        image == empty,
        "discarded caster must neither draw nor cast a rectangular shadow"
    );
    let half = scene(&[[0, 0, 1_000]], 600_000, Some([255, 255, 0, 0]));
    let baseline = capture_scene(&context, &half, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &half, &view(), settings()).unwrap();
    assert_shadow(&image, &baseline, Vec3::new(-0.93, 0.0, 0.0));
    assert_lit(&image, &baseline, Vec3::new(-0.57, 0.0, 0.0));
}

#[test]
fn gpu_opt_in_rejects_unsupported_features_and_updates_atomically() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let mut parts = fixture::effects_parts(600_000);
    parts.water = Some(SceneWaterBody {
        height_millimeters: 0,
        color_micro: [50_000, 200_000, 250_000],
        absorption_micro: [300_000, 100_000, 50_000],
        roughness_micro: 200_000,
        east_min_millimeters: 0,
        north_min_millimeters: 0,
        east_extent_millimeters: 1_000,
        north_extent_millimeters: 1_000,
    });
    let water = RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(SceneEffects::new(parts).unwrap()),
    })
    .unwrap();
    assert!(matches!(
        ShadowedScene::new(&context, &water, settings()),
        Err(ShadowError::Draw(DrawError::Frame(
            FrameError::UnsupportedEffect("water requires HDR")
        )))
    ));
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    assert!(
        renderer
            .set_settings(
                &context,
                DirectionalShadowSettings {
                    resolution: 0,
                    ..settings()
                }
            )
            .is_err()
    );
    assert!(matches!(
        renderer.set_effects(&with_cascades(
            source.effects().unwrap(),
            vec![100, 200, 300, 400, 500],
            0
        )),
        Err(ShadowError::UnsupportedCascades)
    ));
    assert!(matches!(
        renderer.set_effects(water.effects().unwrap()),
        Err(ShadowError::Schedule(
            worldrunner_render_post::FrameError::WaterSettings(_)
        ))
    ));
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert!(
        target.read_rgba8(&context).unwrap() == capture_scene(&context, &source, &view()).unwrap(),
        "invalid updates must leave usable old resources and effects"
    );
}

#[test]
fn gpu_shadows_leave_ambient_and_emission_intact_and_bias_is_effective() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let no_shadow = capture_scene_shadowed(
        &context,
        &source,
        &view(),
        DirectionalShadowSettings {
            depth_bias: 0.9,
            ..settings()
        },
    )
    .unwrap();
    assert!(
        no_shadow == baseline,
        "explicit large bias must suppress depth occlusion"
    );
    let mut effects = fixture::effects_parts(600_000);
    effects.sun.irradiance_micro = 0;
    let emissive = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: Some(TextureHandle::from_raw(0)),
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(
            source.resources().textures().to_vec(),
            vec![emissive, source.resources().materials()[1]],
        )
        .unwrap(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(SceneEffects::new(effects).unwrap()),
    })
    .unwrap();
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    assert!(
        image == baseline,
        "shadow visibility must not multiply hemisphere ambient or emission"
    );
    assert!(at(&image, Vec3::new(-0.75, 0.0, 0.0)) > 150);
}

fn render_target(
    renderer: &mut ShadowedScene,
    context: &GpuContext,
    view: &SceneView,
    target: &OffscreenTarget,
) -> Result<(), ShadowError> {
    renderer.render(
        context,
        view,
        &target.color_attachment(),
        &target.depth_attachment(),
    )
}
