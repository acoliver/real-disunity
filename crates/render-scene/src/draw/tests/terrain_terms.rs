use super::*;
use crate::compositor::terms::{CaptureTerm, TermCapture};
use crate::offscreen::LinearRgbaImage;

#[test]
fn terrain_albedo_capture_matches_two_layer_cpu_blend() {
    let context = context();
    let source = two_layer_source();
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -3_000, 5_000],
            target_millimeters: [0; 3],
        },
        camera(),
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let frame = FrameUniform::new(&view, source.effects()).unwrap();
    gpu.set_frame(context.queue(), frame);
    let capture = TermCapture::new(context.device(), [32, 32], CaptureTerm::Albedo);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            capture.encode(context.device(), encoder, &gpu);
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let mut read = worldrunner_render_post::Readback::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new([32, 32], TextureFormat::Rgba16Float, 1),
        1024 * 1024,
    )
    .unwrap();
    let bytes = read
        .read(context.device(), context.submission(), &capture.texture)
        .unwrap();
    let image =
        LinearRgbaImage::from_native(view.viewport(), TextureFormat::Rgba16Float, &bytes).unwrap();
    let inverse = frame.view_projection.inverse();
    let mut mixed = 0;
    for (i, pixel) in image.pixels.iter().enumerate().filter(|(_, p)| p[3] != 0.0) {
        let ndc = glam::Vec2::new(
            f32::from(u16::try_from(i % 32).unwrap()) + 0.5,
            f32::from(u16::try_from(i / 32).unwrap()) + 0.5,
        ) / 32.0
            * glam::Vec2::new(2.0, -2.0)
            + glam::Vec2::new(-1.0, 1.0);
        let near = inverse.project_point3(ndc.extend(0.0));
        let far = inverse.project_point3(ndc.extend(1.0));
        let world = near + (far - near) * (-near.z / (far.z - near.z));
        let green = ((world.x + 2.0) / 4.0 - 0.5).clamp(0.0, 1.0);
        assert!((pixel[0] - (1.0 - green)).abs() < 0.001);
        assert!((pixel[1] - green).abs() < 0.001);
        assert!(pixel[2].abs() < f32::EPSILON);
        mixed += usize::from(green > 0.1);
    }
    assert!(mixed > 10);
}
fn two_layer_source() -> RenderScene {
    let base = colored_terrain(false, 0);
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -2_000,
        origin_north_millimeters: -2_000,
        sample_spacing_millimeters: 4_000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0, 1, 0, 1],
        surfaces: [0, 1]
            .map(|index| {
                SceneTerrainSurface::new(
                    MaterialHandle::from_raw(index),
                    SceneTerrainAppearance::NEUTRAL,
                )
            })
            .to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: Some(terrain),
        effects: base.effects().cloned(),
    })
    .unwrap()
}
