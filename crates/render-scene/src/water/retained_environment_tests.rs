use super::*;
use worldrunner_render_plan::{SceneSpecularCube, SceneTexture, TexelFormat, TextureColorSpace};

fn uploaded_maps(
    context: &GpuContext,
    intensity: u32,
) -> crate::sky::preparation::PreparedEnvironmentMaps {
    let cube = SceneTexture::SpecularCube(
        SceneSpecularCube::rgba16_float(
            1,
            1,
            [0x3800u16, 0x3400, 0x3000, 0x3c00]
                .repeat(6)
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
                .into(),
        )
        .unwrap(),
    );
    let lut = SceneTexture::Texels {
        format: TexelFormat::Rg16Float,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: [0x3800u16, 0x3400]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
            .into(),
    };
    let upload = |texture: &SceneTexture| {
        crate::upload::upload_texture(
            context.device(),
            context.queue(),
            &crate::texture::prepare_texture(texture).unwrap(),
        )
        .unwrap()
    };
    crate::sky::preparation::PreparedEnvironmentMaps::from_uploaded(
        &upload(&cube),
        &upload(&lut),
        worldrunner_render_plan::SceneSpecularConvention::default(),
        intensity,
    )
    .unwrap()
}
#[test]
fn uploaded_constant_environment_matches_independent_water_split_sum() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let maps = uploaded_maps(&context, 1_000_000);
    let mut renderer = regressions::renderer(&context, 1);
    renderer
        .install_water(context.device(), settings())
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
    let render = |renderer: &mut crate::compositor::FrameRenderer| {
        encode_fixture(&context, renderer, (&color, &depth), &frame, 0.5)
            .finish()
            .submit()
            .unwrap();
        read(&context, renderer.targets().texture())
    };
    let baseline = render(&mut renderer);
    assert_eq!(baseline, render(&mut renderer));
    renderer.set_water_environment(&maps).unwrap();
    let reflected = render(&mut renderer);
    let offset = (16 * 32 + 16) * 8;
    for (channel, radiance) in [0.5f64, 0.25, 0.125].into_iter().enumerate() {
        let range = offset + channel * 2..offset + channel * 2 + 2;
        let delta = f64::from(numerical::positive_half(&reflected[range.clone()]))
            - f64::from(numerical::positive_half(&baseline[range]));
        let expected = radiance * (0.02037 * 0.5 + 0.25);
        assert!((delta - expected).abs() < 0.001, "{delta} != {expected}");
    }
    renderer.clear_environment(context.device());
    assert_eq!(baseline, render(&mut renderer));
}
