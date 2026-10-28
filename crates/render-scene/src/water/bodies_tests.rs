use super::*;
use worldrunner_render_plan::SceneWaterSurface;

#[test]
fn basin_clips_dry_shores_and_interpolates_crossings() {
    let mut body = settings();
    body.surface = SceneWaterSurface::Basin {
        columns: 2,
        rows: 2,
        bed_millimeters: vec![-1000, 1000, -1000, 1000].into_boxed_slice(),
    };
    let vertices = super::super::geometry::vertices(&body).unwrap();
    assert!(!vertices.is_empty());
    assert!(vertices.iter().all(|p| p[0] <= 0.001 && p[2] == 0.0));
    assert!(vertices.iter().any(|p| p[0].abs() < 0.001));
    body.surface = SceneWaterSurface::Basin {
        columns: 2,
        rows: 2,
        bed_millimeters: vec![1000; 4].into_boxed_slice(),
    };
    assert!(super::super::geometry::vertices(&body).unwrap().is_empty());
}

#[test]
fn authored_mesh_uses_body_elevation_and_rejects_invalid_indices() {
    let mut body = settings();
    body.height_millimeters = 2500;
    body.surface = SceneWaterSurface::Mesh {
        positions_millimeters: vec![[0, 0], [1000, 0], [0, 1000]].into_boxed_slice(),
        indices: vec![0, 1, 2].into_boxed_slice(),
    };
    assert!(
        super::super::geometry::vertices(&body)
            .unwrap()
            .iter()
            .all(|p| p[2].to_bits() == 2.5f32.to_bits())
    );
    body.surface = SceneWaterSurface::Mesh {
        positions_millimeters: vec![[0, 0]].into_boxed_slice(),
        indices: vec![0, 1, 2].into_boxed_slice(),
    };
    assert!(validate_settings(&body).is_err());
}

#[test]
fn multiple_heights_order_independently_of_body_order_at_one_and_four_samples() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut low = settings();
        low.height_millimeters = -1000;
        low.color_micro = [800_000, 20_000, 40_000];
        low.absorption_micro = [1_000_000; 3];
        low.flow_millimeters_per_second = [1000, 500];
        let high = settings();
        let mut outputs = Vec::new();
        for bodies in [
            vec![low.clone(), high.clone()],
            vec![high.clone(), low.clone()],
        ] {
            let mut renderer = regressions::renderer(&context, samples);
            renderer
                .install_water_bodies(context.device(), bodies)
                .unwrap();
            let color = AttachmentView::new(
                ResourceRef::new("scene color"),
                renderer.targets().scene().texture(),
            )
            .unwrap();
            let depth = AttachmentView::new(
                ResourceRef::new("scene depth"),
                renderer.targets().depth().texture(),
            )
            .unwrap();
            let frame = FrameUniform::new(&fixture_view(), None).unwrap();
            encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5)
                .finish()
                .submit()
                .unwrap();
            outputs.push(read(&context, renderer.targets().texture()));
            let bytes = read(&context, renderer.post_water_depth().unwrap().texture());
            let offset = (16 * 32 + 16) * 4;
            assert!((4.0..6.0).contains(&f32::from_le_bytes(
                bytes[offset..offset + 4].try_into().unwrap()
            )));
            let cancelled = encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.25);
            drop(cancelled);
            assert_eq!(
                read(&context, renderer.post_water_depth().unwrap().texture()),
                bytes,
                "cancelled frame must not alter submitted surface depth"
            );
            assert!(
                renderer.post_water_depth().is_some(),
                "cancelled encoding preserves previously submitted readiness"
            );
            renderer.resize(context.device(), [16, 16]).unwrap();
            assert_eq!(renderer.water.as_ref().unwrap().settings.len(), 2);
            context
                .device()
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}

#[test]
fn empty_bodies_do_not_enable_water_or_snapshots() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut renderer = regressions::renderer(&context, 1);
    renderer
        .install_water_bodies(context.device(), Vec::new())
        .unwrap();
    assert!(renderer.water.is_none());
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5)
        .finish()
        .submit()
        .unwrap();
    assert!(renderer.opaque_snapshot().is_none());
    assert_eq!(
        read(&context, renderer.targets().texture()),
        contracts::bytes_for_color(0x3800)
    );
}
