//! The scene attachments of a set of targets are made once and then shared by every frame.
use super::Targets;
use worldrunner_render_scene::device::GpuContext;

fn targets(context: &GpuContext) -> Targets {
    let target = |format| {
        context
            .device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("scene attachment fixture"),
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    Targets::from_views(
        target(wgpu::TextureFormat::Rgba8Unorm),
        target(wgpu::TextureFormat::Depth32Float),
    )
}

#[test]
fn the_scene_attachments_name_the_scene_color_and_depth() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let targets = targets(&context);
    let (color, depth) = targets.scene_attachments().unwrap();
    assert_eq!(color.resource().name, "scene color");
    assert_eq!(color.texture(), targets.color.texture());
    assert_eq!(depth.resource().name, "scene depth");
    assert_eq!(depth.texture(), targets.depth.texture());
}

#[test]
fn every_frame_attaches_the_same_views() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let targets = targets(&context);
    let first = targets.scene_attachments().unwrap().clone();
    let (color, depth) = targets.scene_attachments().unwrap();
    assert_eq!(first.0.view(), color.view());
    assert_eq!(first.1.view(), depth.view());
}
