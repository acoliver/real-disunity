use super::{SceneGpu, ScenePipelines};
use crate::dynamic::DynamicQuad;
use worldrunner_render_gpu::submission::SubmissionAuthority;

#[derive(Clone, Copy)]
enum Setter {
    Quads,
    Contribution,
}

fn assert_corner_colors_rejected(setter: Setter, overlay: bool) {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue);
    let pipelines = ScenePipelines::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut scene = SceneGpu::empty(&device, &authority, &pipelines).unwrap();
    let ordinary = DynamicQuad::default();
    scene
        .set_dynamic_quads(&device, &authority, &pipelines, &[ordinary])
        .unwrap();
    let resident = scene.dynamic_probes();
    let invalid = DynamicQuad {
        overlay,
        corner_colors: Some([[0.; 4]; 4]),
        ..ordinary
    };
    let quads = [ordinary, invalid];
    let result = match setter {
        Setter::Quads => scene.set_dynamic_quads(&device, &authority, &pipelines, &quads),
        Setter::Contribution => {
            scene.set_dynamic_contribution(&device, &authority, &pipelines, &[], &quads)
        }
    };
    assert_eq!(
        result.map_err(|error| error.to_string()),
        Err("dynamic quad 1 corner colours require the retained UI overlay path".to_owned()),
        "scene submissions must reject corner colours with overlay={overlay}"
    );
    let after = scene.dynamic_probes();
    assert_eq!(resident.len(), 1);
    assert_eq!(after.len(), resident.len());
    assert!(resident[0].ptr_eq(&after[0]));
    assert!(resident[0].upgrade().is_some());
}

#[test]
fn scene_quads_reject_corner_colors_without_overlay_flag() {
    assert_corner_colors_rejected(Setter::Quads, false);
}

#[test]
fn scene_quads_reject_corner_colors_with_overlay_flag() {
    assert_corner_colors_rejected(Setter::Quads, true);
}

#[test]
fn scene_contribution_rejects_corner_colors_without_overlay_flag() {
    assert_corner_colors_rejected(Setter::Contribution, false);
}

#[test]
fn scene_contribution_rejects_corner_colors_with_overlay_flag() {
    assert_corner_colors_rejected(Setter::Contribution, true);
}
