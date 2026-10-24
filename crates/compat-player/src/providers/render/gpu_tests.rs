use super::super::super::RenderControl;
use super::{AssetTable, Contribution, Renderer};
use serde_json::json;
use std::path::Path;
use worldrunner::scene::gpu::{
    CameraPose, OrthographicLens, SceneView, Viewport, device::GpuContext, dynamic::DynamicQuad,
    wgpu,
};

#[test]
fn actual_gpu_controls_distinguish_disconnected_geometry_and_invalid_output_stage() {
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("negative controls require an actual GPU");
    let graph = serde_json::from_value(json!({
        "scene":"gpu-control", "scene_guid":"independently-authored", "nodes":[],
        "components":[], "assets":[], "documents":[], "aliases":{},
        "diagnostics":[], "source_hashes":{}
    }))
    .unwrap();
    let table = AssetTable::load(Path::new("."), &graph).unwrap();
    let contribution = Contribution {
        view: SceneView::orthographic_with_up(
            CameraPose {
                eye_millimeters: [0, -10_000, 0],
                target_millimeters: [0, 0, 0],
            },
            OrthographicLens {
                vertical_half_extent_meters: 2.,
                near_meters: 0.1,
                far_meters: 100.,
            },
            [0., 0., 1.],
            Viewport {
                width: 64,
                height: 64,
            },
        )
        .unwrap(),
        quads: vec![DynamicQuad {
            tint: [0., 1., 0., 1.],
            ..DynamicQuad::default()
        }],
        clear: [0., 0., 0., 1.],
    };
    let mut connected = Renderer::new(&context, &table, RenderControl::Connected).unwrap();
    let pixels = connected.capture(&contribution).unwrap();
    let center = (32 * 64 + 32) * 4;
    assert_eq!(&pixels[center..center + 4], &[0, 255, 0, 255]);
    let mut disconnected = Renderer::new(&context, &table, RenderControl::Disconnected).unwrap();
    let pixels = disconnected.capture(&contribution).unwrap();
    assert_eq!(&pixels[center..center + 4], &[0, 0, 0, 255]);
    let Err(error) = Renderer::new(&context, &table, RenderControl::WrongStage) else {
        panic!("invalid output stage was accepted");
    };
    assert!(
        error.contains("exactly one enabled final output stage"),
        "{error}"
    );
}
