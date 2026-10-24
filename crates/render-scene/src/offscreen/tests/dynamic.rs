use super::*;
use crate::dynamic::DynamicQuad;

#[test]
fn gpu_dynamic_geometry_shares_perspective_scene_depth_and_lighting_frame() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let scene = fixture::scene(false);
    let view = fixture::view(&scene, viewport(64, 64));
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(context.device(), context.queue(), &scene, &pipelines).unwrap();
    gpu.set_frame(
        context.queue(),
        FrameUniform::new(&view, scene.effects()).unwrap(),
    );
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let center = |image: RgbaImage| -> [u8; 4] {
        image.pixels[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4]
            .try_into()
            .unwrap()
    };
    let quad = DynamicQuad {
        position: [0.0, 0.5, -0.5],
        rotation: glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2).to_array(),
        tint: [0.0, 1.0, 0.0, 1.0],
        ..DynamicQuad::default()
    };
    gpu.set_dynamic_quads(context.device(), context.queue(), &pipelines, &[quad])
        .unwrap();
    assert_eq!(
        center(target.render(&context, &gpu, &pipelines).unwrap()),
        [255, 0, 0, 255],
        "opaque scene occludes deeper dynamic geometry"
    );
    gpu.set_dynamic_quads(
        context.device(),
        context.queue(),
        &pipelines,
        &[DynamicQuad {
            position: [0.0, -0.5, 0.5],
            ..quad
        }],
    )
    .unwrap();
    assert_eq!(
        center(target.render(&context, &gpu, &pipelines).unwrap()),
        [0, 255, 0, 255],
        "nearer dynamic geometry uses the same depth and perspective view"
    );
}
