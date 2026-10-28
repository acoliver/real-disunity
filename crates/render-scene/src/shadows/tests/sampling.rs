use super::*;

fn with_objects(source: &RenderScene, objects: Vec<SceneObject>) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: SceneGeometry::new(source.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: None,
        effects: source.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn gpu_retained_cast_toggle_removes_shadow_and_non_caster_receives() {
    let context = context();
    let base = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut objects = base.geometry().objects().to_vec();
    objects[0] = objects[0].with_casts_shadow(false);
    let source = with_objects(&base, objects.clone());
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let shadowed = render(&context, &mut renderer, &mut target);
    let caster = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let generation = renderer.scene_mut().caster_generation();
    objects[1] = objects[1].with_casts_shadow(false);
    renderer
        .scene_mut()
        .set_instance_object(caster, objects[1])
        .unwrap();
    assert_ne!(renderer.scene_mut().caster_generation(), generation);
    let no_cast = render(&context, &mut renderer, &mut target);
    assert_ne!(shadowed, no_cast);
    assert_eq!(
        renderer
            .shadow_view_stats()
            .drawn_per_lod
            .iter()
            .sum::<usize>(),
        0
    );
    assert_eq!(renderer.view_stats().drawn_per_lod.iter().sum::<usize>(), 2);
    let unshadowed = capture_scene(&context, &source, &view()).unwrap();
    assert_eq!(no_cast, unshadowed);
    objects[1] = objects[1].with_casts_shadow(true);
    renderer
        .scene_mut()
        .set_instance_object(caster, objects[1])
        .unwrap();
    assert_eq!(render(&context, &mut renderer, &mut target), shadowed);
}

fn render(
    context: &GpuContext,
    renderer: &mut ShadowedScene,
    target: &mut OffscreenTarget,
) -> RgbaImage {
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode(
                context,
                &view(),
                &target.color_attachment(),
                &target.depth_attachment(),
                encoder,
            )?;
            target.encode_readback(encoder)?;
            Ok::<_, ShadowError>(())
        })
        .unwrap();
    target
        .read_encoded(context, prepared.submit().unwrap())
        .unwrap()
}

#[test]
fn gpu_strength_and_receiver_bias_have_independent_visibility_oracles() {
    let context = context();
    let source = super::cascades::source_with(
        &scene(&[], 600_000, None),
        SceneShadowCascades::new(vec![2_000, 4_000, 6_000, 8_000], 0).unwrap(),
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let probe = format!("{}{}", resources::SHADOWED_WGSL, super::selection::PROBE);
    for (reduction, expected) in [(0.0_f32, 0_u8), (0.2, 124), (0.5, 188), (1.0, 255)] {
        super::selection::upload_probe(&context, &renderer, false, 4, 0.0);
        context
            .queue()
            .write_buffer(&renderer.map.uniform, 268, &reduction.to_le_bytes());
        let image = super::selection::render_probe(&context, &renderer, &probe);
        assert!(
            image.pixels[0].abs_diff(expected) <= 1,
            "{reduction}: {:?}",
            &image.pixels[..4]
        );
        assert_eq!(image.pixels[6 * 4], 255, "lit receivers stay lit");
    }
    super::selection::upload_probe(&context, &renderer, false, 4, 0.0);
    context
        .queue()
        .write_buffer(&renderer.map.uniform, 320, &0.51_f32.to_le_bytes());
    let biased = super::selection::render_probe(&context, &renderer, &probe);
    assert_eq!(
        biased.pixels[0], 255,
        "receiver depth 0.5 minus bias 0.51 passes depth zero"
    );
}

#[test]
fn gpu_explicit_defaults_preserve_image_and_uniform_bytes() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let declared = source.effects().unwrap().shadows();
    let explicit = super::cascades::source_with(
        &source,
        declared
            .clone()
            .with_sampling(SceneShadowSampling::default())
            .unwrap(),
    );
    let baseline = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    assert_eq!(
        baseline,
        capture_scene_shadowed(&context, &explicit, &view(), settings()).unwrap()
    );
    let renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let cascades = super::super::cascades::CascadeViews::new(
        &renderer.gpu,
        &view(),
        source.effects().unwrap(),
        settings(),
        crate::frame::sun_direction(source.effects().unwrap().sun()),
    )
    .unwrap();
    let encoded = resources::uniform_bytes_with_sampling(
        &cascades,
        settings(),
        SceneShadowSampling::default(),
    );
    let mut expected = Vec::new();
    for index in 0..4 {
        expected.extend(
            cascades
                .matrices
                .get(index)
                .copied()
                .unwrap_or(glam::Mat4::IDENTITY)
                .to_cols_array(),
        );
    }
    expected.extend([
        0.0,
        settings().normal_bias_meters,
        1.0,
        0.0,
        1.0,
        cascades.blend,
        0.0,
        cascades.near,
    ]);
    expected.extend(cascades.splits);
    expected.extend(cascades.view_depth.to_array());
    expected.extend(cascades.depth_bias);
    assert_eq!(
        encoded,
        expected
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>()
    );
}

#[test]
fn gpu_retained_sampling_update_rekeys_caster_slope() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let original = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    let mut parts = fixture::effects_parts(600_000);
    parts.shadows = parts
        .shadows
        .with_sampling(SceneShadowSampling {
            caster_slope_bias_micro: 3_000,
            filter: SceneShadowFilter::Bilinear2,
            strength_micro: 800_000,
        })
        .unwrap();
    renderer
        .set_effects(&SceneEffects::new(parts).unwrap())
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let updated = render(&context, &mut renderer, &mut target);
    assert_ne!(updated, original);
    assert_eq!(renderer.caster_slope_bits, 0.003_f32.to_bits());
    renderer.set_effects(source.effects().unwrap()).unwrap();
    assert_eq!(render(&context, &mut renderer, &mut target), original);
    assert_eq!(renderer.caster_slope_bits, 0.0_f32.to_bits());
}
