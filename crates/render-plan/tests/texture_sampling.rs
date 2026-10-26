use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace, TextureFilter, TextureSampler, TextureWrap,
};
#[test]
fn unspecified_sampling_and_explicit_sampling_keep_payload() {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into_boxed_slice(),
    };
    assert_eq!(texture.sampler().filter, TextureFilter::Trilinear);
    let sampler = TextureSampler {
        filter: TextureFilter::Point,
        wrap_u: TextureWrap::Clamp,
        wrap_v: TextureWrap::Mirror,
    };
    let configured = texture.clone().with_sampler(sampler);
    assert_eq!(configured.sampler(), sampler);
    assert_eq!(configured.payload(), &texture);
    configured.validate().unwrap();
    let bilinear = configured.with_sampler(TextureSampler::default());
    assert_eq!(bilinear.payload(), &texture);
    assert_ne!(bilinear, texture);
    assert_eq!(bilinear.sampler().filter, TextureFilter::Bilinear);
}
