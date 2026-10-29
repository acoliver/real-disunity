use super::*;
use crate::surface::{SurfaceAction, SurfaceTransition};

fn startup() -> (wgpu::SurfaceCapabilities, wgpu::SurfaceConfiguration) {
    let capabilities = wgpu::SurfaceCapabilities {
        formats: vec![wgpu::TextureFormat::Bgra8UnormSrgb],
        present_modes: vec![wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox],
        alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
        usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
        ..Default::default()
    };
    let config = configuration(
        &capabilities,
        SurfacePolicy {
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            size: [800, 600],
            present_mode: wgpu::PresentMode::Mailbox,
            desired_maximum_frame_latency: 5,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        },
    )
    .unwrap();
    validate_reconfiguration(&capabilities, &config).unwrap();
    (capabilities, config)
}

fn assert_removed_mode(
    mut capabilities: wgpu::SurfaceCapabilities,
    config: &wgpu::SurfaceConfiguration,
    transition: &SurfaceTransition,
) {
    capabilities.present_modes = vec![wgpu::PresentMode::Fifo];
    assert_eq!(transition.action(0), SurfaceAction::Configure);
    assert!(matches!(
        validate_reconfiguration(&capabilities, config),
        Err(SurfaceError::UnsupportedPresentMode(
            wgpu::PresentMode::Mailbox
        ))
    ));
    assert_eq!(config.present_mode, wgpu::PresentMode::Mailbox);
    assert_eq!(config.desired_maximum_frame_latency, 5);
    assert_eq!(transition.action(0), SurfaceAction::Configure);
}

#[test]
fn resize_and_outdated_revalidate_a_mode_supported_at_startup() {
    for resize in [false, true] {
        let (capabilities, mut config) = startup();
        let mut transition = SurfaceTransition::new([config.width, config.height]);
        if resize {
            config.width = 1024;
            config.height = 768;
            transition.resize([1024, 768]);
        } else {
            transition.outdated();
        }
        assert_removed_mode(capabilities, &config, &transition);
    }
}

#[test]
fn resume_revalidates_a_mode_supported_before_suspension() {
    let (capabilities, mut config) = startup();
    let mut transition = SurfaceTransition::new([config.width, config.height]);
    transition.resize([0, 600]);
    assert_eq!(transition.action(0), SurfaceAction::Suspended);
    config.width = 640;
    config.height = 480;
    transition.resize([640, 480]);
    assert_removed_mode(capabilities, &config, &transition);
}
