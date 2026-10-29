//! The TAA stage reads the frame depth target, so a scene drawn into another depth is refused.
use super::decals::source;
use super::*;
use crate::compositor::FrameRenderer;
use worldrunner_render_post::{
    AntiAliasing, AttachmentView, FrameError, MsaaSamples, PostResolve, ResourceRef, ToneMap,
};

const HDR: TextureFormat = TextureFormat::Rgba16Float;

fn renderer(context: &GpuContext, resolve: PostResolve) -> FrameRenderer {
    FrameRenderer::hdr_antialiased(
        context,
        [TARGET, TARGET],
        COLOR_FORMAT,
        AntiAliasing::new(MsaaSamples::X1, resolve),
        ToneMap::Reinhard,
        None,
    )
    .unwrap()
}

fn attachment(name: &'static str, texture: &wgpu::Texture) -> AttachmentView {
    AttachmentView::new(ResourceRef::new(name), texture).unwrap()
}

fn foreign_depth(context: &GpuContext) -> wgpu::Texture {
    context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("host depth"),
        size: wgpu::Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

const REFUSED: FrameError = FrameError::UnsupportedEffect("TAA requires the frame depth target");

#[test]
fn a_taa_renderer_accepts_only_its_own_frame_depth() {
    let context = context();
    let taa = renderer(&context, PostResolve::taa(0.9));
    let own = attachment("scene depth", taa.targets().depth().texture());
    assert_eq!(taa.validate_taa_depth(&own), Ok(()));
    let host = foreign_depth(&context);
    assert_eq!(
        taa.validate_taa_depth(&attachment("scene depth", &host)),
        Err(REFUSED)
    );
}

#[test]
fn renderers_without_taa_accept_any_depth() {
    let context = context();
    let host = foreign_depth(&context);
    for resolve in [PostResolve::None, PostResolve::fxaa(0.5)] {
        let plain = renderer(&context, resolve);
        assert_eq!(
            plain.validate_taa_depth(&attachment("scene depth", &host)),
            Ok(())
        );
    }
}

#[test]
fn encode_scene_refuses_a_host_depth_that_is_not_the_frame_depth_before_changing_state() {
    let context = context();
    let scene = source(false, false);
    let pipelines = ScenePipelines::with_samples(context.device(), HDR, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut taa = renderer(&context, PostResolve::taa(0.9));
    let color = attachment("scene color", taa.targets().scene().texture());
    let host = foreign_depth(&context);
    let depth = attachment("scene depth", &host);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let result = crate::compositor::encode_scene(
        &mut taa,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&color, &depth),
        frame_from(pose()),
    );
    assert_eq!(result, Err(REFUSED));
    assert!(!taa.has_temporal_history());
}
