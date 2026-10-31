use super::*;

fn default_output(context: &GpuContext, legacy: bool) -> Vec<Vec<u8>> {
    let mut renderer = regressions::renderer(context, 1);
    renderer
        .install_water(context.device(), settings())
        .unwrap();
    if legacy {
        let source = water_source(false).replace(" || SCENE_DIRECT_DIFFUSE != 0u)", ")");
        let water = renderer.water.as_mut().unwrap();
        water.pipeline =
            pipeline_with_source(context.device(), 1, &water.layout, false, [0; 5], &source);
    }
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
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    frame.unlit = false;
    frame.sun_direction = glam::Vec3::Z;
    frame.sun_irradiance = glam::Vec3::ONE;
    frame.ambient_sky = glam::Vec3::splat(0.25);
    encode_fixture(context, &mut renderer, (&color, &depth), &frame, 0.5)
        .finish()
        .submit()
        .unwrap();
    let mut outputs = vec![read(context, renderer.targets().texture())];
    outputs.extend(
        renderer.water.as_ref().unwrap().receivers[0]
            .iter()
            .map(|texture| read(context, texture)),
    );
    outputs
}

#[test]
fn default_water_convention_preserves_legacy_color_and_receiver_bytes() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let expected = default_output(&context, true);
    let actual = default_output(&context, false);
    assert_eq!(actual, expected);
    assert!(
        actual[1]
            .chunks_exact(8)
            .any(|pixel| pixel[6..8] == [0, 0x3c])
    );
    assert_eq!(
        terms::constants([0; 5]),
        crate::renderer::specular_constants([0; 7])
    );
}
