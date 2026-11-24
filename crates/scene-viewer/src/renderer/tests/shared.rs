//! A window drawn on a device made before it, which another user (a player's offscreen work)
//! shares: the viewer presents through that device's submission authority, and refuses by name a
//! scene the device cannot draw.
use super::*;
use worldrunner_render_gpu::device::DeviceRequirements;
use worldrunner_render_plan::{
    SceneGeometry, SceneParts, SceneResources, TextureColorSpace, TextureEncoding,
};
use worldrunner_render_post::TimingMode;
use worldrunner_render_scene::device::DeviceError;
use worldrunner_render_scene::device::DeviceRequest;

/// The instance and a device without block compression, made before any surface exists.
fn shared(event_loop: &EventLoop<()>) -> super::super::Shared {
    let descriptor = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
        event_loop.owned_display_handle(),
    ));
    let backends = descriptor.backends;
    let instance = wgpu::Instance::new(descriptor);
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let context = pollster::block_on(GpuContext::from_adapter(
        adapter,
        &DeviceRequest {
            features: wgpu::Features::empty(),
            timing: TimingMode::Disabled,
            shared: DeviceRequirements::default(),
        },
    ))
    .unwrap();
    super::super::Shared {
        instance,
        backends,
        context,
    }
}

fn encoded_scene() -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![SceneTexture::Encoded {
                encoding: TextureEncoding::DdsContainer,
                color_space: TextureColorSpace::Srgb,
                bytes: vec![1, 2, 3].into_boxed_slice(),
            }],
            vec![],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

pub(super) fn presents_through_a_shared_device(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    let shared = shared(event_loop);
    let device = shared.context.device().clone();
    let authority = shared.context.submission().clone();
    let mut renderer = pollster::block_on(Renderer::new_shared(
        event_loop.owned_display_handle(),
        window.clone(),
        &crate::tests::scene(false),
        shared,
    ))
    .unwrap();
    assert!(
        renderer.context.device() == &device,
        "the viewer draws on the shared device"
    );
    let before = authority.submitted_serial();
    present(&mut renderer);
    assert!(
        authority.submitted_serial() > before,
        "the frame went through the shared submission authority"
    );
    renderer.drain().unwrap();
}

pub(super) fn refuses_a_scene_the_shared_device_cannot_draw(
    event_loop: &EventLoop<()>,
    window: &Arc<Window>,
) {
    let shared = shared(event_loop);
    let adapter = shared.context.adapter().get_info().name;
    let refused = pollster::block_on(Renderer::new_shared(
        event_loop.owned_display_handle(),
        window.clone(),
        &encoded_scene(),
        shared,
    ));
    match refused {
        Err(ViewerError::Device(DeviceError::MissingFeatures {
            adapter: named,
            missing,
        })) => {
            assert_eq!(named, adapter);
            assert_eq!(missing, wgpu::Features::TEXTURE_COMPRESSION_BC);
        }
        Err(other) => panic!("expected the missing block compression by name, got {other}"),
        Ok(_) => panic!("a device without block compression drew an encoded scene"),
    }
}
