#[test]
fn ssr_misses_use_uploaded_environment_radiance() {
    let (scene, mut environment) = source(false);
    environment.consumers = SceneEnvironmentConsumers::Reflections;
    let effects = scene
        .effects()
        .unwrap()
        .clone()
        .with_ssr(worldrunner_render_plan::SceneSsr {
            max_steps: 1,
            origin_skip_pixels: 4096,
            ..worldrunner_render_plan::SceneSsr::default()
        })
        .unwrap();
    let scene = scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects.clone()))
        .unwrap();
    let host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [64, 64],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host.windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events);
    run.host_mut()
        .apply(RetainedUpdate::Settings(Box::new(
            effects.with_environment(Some(environment)),
        )))
        .unwrap();
    let reflected = capture(&mut run, &events);
    assert_ne!(baseline, reflected);
    assert_missed(&mut run, &events);
    run.shutdown().unwrap();
}
fn assert_missed(run: &mut WindowlessRetained<'_>, events: &RefCell<Vec<RetainedEvent>>) {
    let recipe = crate::RetainedCaptureRecipe::new(
        1,
        1,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::SsrConfidence],
        1024 * 1024,
    )
    .unwrap();
    let generation = run.host().accepted_generation();
    let id = run
        .host_mut()
        .request_capture_recipe(generation, recipe)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        run.frame().unwrap();
        run.poll().unwrap();
        if let Some(products) = events.borrow().iter().find_map(|event| match event {
            RetainedEvent::Capture {
                id: actual,
                outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
                ..
            } if *actual == id => Some(products.clone()),
            _ => None,
        }) {
            assert!(
                products
                    .ssr_confidence
                    .unwrap()
                    .pixels
                    .iter()
                    .all(|value| *value == 0.0)
            );
            return;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
use super::*;
use worldrunner_render_plan::{
    SceneEnvironment, SceneEnvironmentConsumers, SceneSpecularCube, SceneTexture, TexelFormat,
    TextureColorSpace, TextureHandle,
};

fn source(water: bool) -> (RenderScene, SceneEnvironment) {
    let base = if water {
        crate::tests::scene(true)
    } else {
        crate::renderer::retained_tests::source()
    };
    let mut textures = base.resources().textures().to_vec();
    let cube = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::SpecularCube(
        SceneSpecularCube::rgba16_float(
            1,
            1,
            [0x4400u16, 0x3c00, 0x3800, 0x3c00]
                .repeat(6)
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
                .into(),
        )
        .unwrap(),
    ));
    let brdf_lut = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg16Float,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: [0x3800u16, 0x3400]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
            .into(),
    });
    let resources = SceneResources::new(textures, base.resources().materials().to_vec()).unwrap();
    let scene = base
        .with_resources(resources, base.geometry().clone(), base.effects().cloned())
        .unwrap();
    (
        scene,
        SceneEnvironment {
            cube,
            brdf_lut,
            convention: worldrunner_render_plan::SceneSpecularConvention::default(),
            consumers: SceneEnvironmentConsumers::Water,
        },
    )
}

#[test]
fn retained_environment_updates_and_removal_preserve_resident_scene() {
    let (scene, environment) = source(true);
    let initial = scene.effects().unwrap().clone();
    let host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [64, 64],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host.windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events);
    assert_eq!(baseline, capture(&mut run, &events));
    let installed = initial.clone().with_environment(Some(environment));
    run.host_mut()
        .apply(RetainedUpdate::Settings(Box::new(installed)))
        .unwrap();
    let reflected = capture(&mut run, &events);
    assert_ne!(baseline, reflected);
    assert_eq!(
        run.host()
            .statistics()
            .unwrap()
            .scene
            .textures
            .uploaded_bytes,
        0
    );
    assert_eq!(
        run.host()
            .statistics()
            .unwrap()
            .scene
            .mesh_buffers
            .uploaded_bytes,
        0
    );
    run.host_mut()
        .apply(RetainedUpdate::Settings(Box::new(initial)))
        .unwrap();
    assert_eq!(baseline, capture(&mut run, &events));
    run.shutdown().unwrap();
}
