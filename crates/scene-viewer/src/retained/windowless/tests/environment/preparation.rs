use super::*;
use worldrunner_render_plan::SceneSpecularPreparation;

fn prepared_scene(size: u32) -> RenderScene {
    let (base, environment) = source(true);
    let mut textures = base.resources().textures().to_vec();
    let SceneTexture::SpecularCube(cube) = textures[environment.cube.raw() as usize].clone() else {
        panic!("cube fixture");
    };
    textures[environment.cube.raw() as usize] = SceneTexture::SpecularCube(
        cube.with_preparation(SceneSpecularPreparation {
            convention: environment.convention,
            cube_size: size,
            mip_count: size.ilog2() + 1,
            samples: 8,
            source_lod_tail: 1,
        })
        .unwrap(),
    );
    let resources = SceneResources::new(textures, base.resources().materials().to_vec()).unwrap();
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_environment(Some(environment));
    base.with_resources(resources, base.geometry().clone(), Some(effects))
        .unwrap()
}
fn host(scene: RenderScene) -> RetainedHost {
    RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [32, 32],
            render_clock: RenderClock::Fixed(Duration::ZERO),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap()
}
#[test]
fn environment_source_preparation_rejects_budget_before_upload() {
    let mut host = host(prepared_scene(1024));
    host.options.presentation_budget = crate::PresentationBudget(300 * 1024 * 1024).into();
    let result = pollster::block_on(crate::renderer::Renderer::windowless(&host));
    assert!(matches!(result,
        Err(crate::ViewerError::Configuration(
            worldrunner_render_gpu::configuration::ConfigurationError::Budget { required, budget }
        )) if required > budget && budget == 300 * 1024 * 1024));
}

#[test]
fn environment_source_installation_publication_and_shutdown() {
    let scene = prepared_scene(4);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(scene.clone()).windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events);
    run.host_mut()
        .apply(RetainedUpdate::Resources {
            resources: scene.resources().clone(),
            meshes: scene.geometry().meshes().to_vec(),
        })
        .unwrap();
    let generation = run.host().accepted_generation();
    let deadline = Instant::now() + Duration::from_secs(10);
    while run.host().ready_generation() != generation {
        run.frame().unwrap();
        run.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    assert_eq!(capture(&mut run, &events), baseline);
    run.host_mut()
        .apply(RetainedUpdate::Resources {
            resources: scene.resources().clone(),
            meshes: scene.geometry().meshes().to_vec(),
        })
        .unwrap();
    run.shutdown().unwrap();
}
