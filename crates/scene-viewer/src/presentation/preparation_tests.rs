#[test]
fn source_preparation_cancellation_releases_only_pending_storage() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let mut presentation = fixed(&context, &source(128));
    let limits = context.device().limits();
    let baseline = presentation.scheduler.resident_bytes(&limits).unwrap();
    let reservation = presentation.prepare_scene(&context, &source(128)).unwrap();
    let output =
        worldrunner_render_scene::draw::source_environment::resident_allocations(&source(128))
            .validate(&limits)
            .unwrap();
    assert!(presentation.scheduler.resident_bytes(&limits).unwrap() > baseline + output);
    drop(reservation.transient);
    assert_eq!(
        presentation.scheduler.resident_bytes(&limits).unwrap(),
        baseline + output
    );
    drop(reservation.resident);
    assert_eq!(
        presentation.scheduler.resident_bytes(&limits).unwrap(),
        baseline
    );
    assert_eq!(presentation.scheduler.generation(), 0);
}

use super::*;
use worldrunner_render_plan::{
    RenderScene, SceneResources, SceneSpecularCube, SceneSpecularPreparation, SceneTexture,
};

pub(crate) fn source(size: u32) -> RenderScene {
    let scene = crate::renderer::retained_tests::source();
    let mut textures = scene.resources().textures().to_vec();
    textures.push(SceneTexture::SpecularCube(
        SceneSpecularCube::rgba16_float(1, 1, vec![0; 48].into_boxed_slice())
            .unwrap()
            .with_preparation(SceneSpecularPreparation {
                convention: worldrunner_render_plan::SceneSpecularConvention::default(),
                cube_size: size,
                mip_count: size.ilog2() + 1,
                samples: 1,
                source_lod_tail: 1,
            })
            .unwrap(),
    ));
    let resources = SceneResources::new(textures, scene.resources().materials().to_vec()).unwrap();
    scene
        .with_resources(
            resources,
            scene.geometry().clone(),
            scene.effects().cloned(),
        )
        .unwrap()
}

#[test]
fn source_preparation_admission_preserves_presentation() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let mut presentation = Presentation::new(
        &context,
        COLOR,
        Viewport {
            width: 8,
            height: 8,
        },
    );
    let generation = presentation.scheduler.generation();
    let color = presentation.targets.color.texture().clone();
    let _reservation = presentation.prepare_scene(&context, &source(4)).unwrap();
    assert_eq!(presentation.scheduler.generation(), generation);
    assert_eq!(presentation.targets.color.texture(), &color);
}

pub(crate) fn fixed(
    context: &worldrunner_render_scene::device::GpuContext,
    scene: &RenderScene,
) -> Presentation {
    let size = Viewport {
        width: 8,
        height: 8,
    };
    let settings = RendererConfiguration::legacy([8, 8], COLOR);
    let mut peak = settings.allocations();
    peak.extend(settings.allocations());
    peak.extend(planning::additional(size, None));
    peak.extend(planning::additional(size, None));
    peak.extend(worldrunner_render_scene::draw::source_environment::allocations(scene).unwrap());
    let bytes = peak.validate(&context.device().limits()).unwrap();
    Presentation::with_settings(
        context,
        COLOR,
        [size, size],
        crate::FrameProfile::Legacy,
        None,
        &PresentationPolicy {
            budget: crate::PresentationBudget(bytes).into(),
            recipes: false,
            capture_pool: crate::CapturePoolPolicy::default(),
        },
    )
    .unwrap()
}

#[test]
fn source_resident_resize_counts_cube_once() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let scene = source(128);
    let mut presentation = fixed(&context, &scene);
    let _resident = resident(&context, &scene, &mut presentation);
    presentation
        .resize(
            &context,
            Viewport {
                width: 8,
                height: 8,
            },
            Viewport {
                width: 9,
                height: 8,
            },
        )
        .unwrap();
    assert_eq!(presentation.scheduler.generation(), 1);
}

#[test]
fn source_resident_settings_replacement_counts_cube_once() {
    let context =
        pollster::block_on(worldrunner_render_scene::device::GpuContext::request()).unwrap();
    let scene = source(128);
    let mut presentation = fixed(&context, &scene);
    let _resident = resident(&context, &scene, &mut presentation);
    let mut settings = presentation.scheduler.configuration().clone();
    settings.clear_color = wgpu::Color::WHITE;
    presentation
        .scheduler
        .reconfigure(&context, settings)
        .unwrap();
    assert_eq!(presentation.scheduler.generation(), 1);
}

fn resident(
    context: &worldrunner_render_scene::device::GpuContext,
    scene: &RenderScene,
    presentation: &mut Presentation,
) -> worldrunner_render_scene::draw::SceneGpu {
    let reservation = presentation.prepare_scene(context, scene).unwrap();
    let pipelines =
        worldrunner_render_scene::renderer::ScenePipelines::new(context.device(), COLOR);
    let mut gpu = worldrunner_render_scene::draw::SceneGpu::upload(
        context.device(),
        context.submission(),
        scene,
        &pipelines,
    )
    .unwrap();
    gpu.retain_resource_reservation(reservation.resident);
    drop(reservation.transient);
    gpu
}
