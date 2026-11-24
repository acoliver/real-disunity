//! The overlay draws render textures the host owns. Each test makes a small texture on the shared
//! device, offers it as a `UiTarget` and reads the pixels the overlay pass writes.
use super::FrameRenderer;
use crate::{
    Viewport,
    device::GpuContext,
    draw::SceneGpu,
    offscreen::OffscreenTarget,
    renderer::ScenePipelines,
    simulation::{UiOverlay, UiTarget},
};
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace, TextureFilter, TextureSampler, TextureWrap,
};
use worldrunner_world_simulation::graphics::RenderTextureId;
use worldrunner_world_simulation::ui::UiTexture;
use worldrunner_world_simulation::ui::presentation::{UiDraw, UiDrawKind, UiPresentation};

const SIDE: u32 = 4;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0; 4];

fn id() -> RenderTextureId {
    RenderTextureId::from_bits(5)
}

fn sampler() -> TextureSampler {
    TextureSampler {
        filter: TextureFilter::Point,
        wrap_u: TextureWrap::Clamp,
        wrap_v: TextureWrap::Clamp,
    }
}

/// A one texel wide texture of `texels`, row 0 first in memory, as a camera's target is made.
fn host_texture(
    context: &GpuContext,
    format: wgpu::TextureFormat,
    texels: &[[u8; 4]],
) -> wgpu::Texture {
    context.device().create_texture_with_data(
        context.submission().queue(),
        &wgpu::TextureDescriptor {
            label: Some("host render texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: u32::try_from(texels.len()).unwrap(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &texels.concat(),
    )
}

fn draw(texture: UiTexture) -> UiDraw {
    let side = f32::from(u16::try_from(SIDE).unwrap());
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [
            [0., 0., 0.],
            [side, 0., 0.],
            [side, side, 0.],
            [0., side, 0.],
        ],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        corner_colors: None,
        distance_field: None,
        texture: Some(texture),
        material: None,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, SIDE, SIDE],
    }
}

fn overlay(
    texture: UiTexture,
    textures: &BTreeMap<u64, SceneTexture>,
) -> std::sync::Arc<UiOverlay> {
    let presentation = UiPresentation {
        draws: vec![draw(texture)],
        ..UiPresentation::default()
    };
    let layers = worldrunner_world_simulation::ui::presentation::UiLayers::default();
    std::sync::Arc::new(UiOverlay::from_presentation(&presentation, textures, &layers).unwrap())
}

/// The pixels, top row first, that the overlay pass writes over a cleared target when the overlay
/// is staged once for each offer of `offers`; the last staging is the one drawn.
fn shown(
    context: &GpuContext,
    overlay: &std::sync::Arc<UiOverlay>,
    offers: &[&[UiTarget]],
) -> Vec<[u8; 4]> {
    let device = context.device();
    let pipelines = ScenePipelines::new(device, OffscreenTarget::COLOR_FORMAT, 1).unwrap();
    let mut scene = SceneGpu::empty(device, context.submission(), &pipelines).unwrap();
    for targets in offers {
        scene
            .set_ui_overlay(device, context.submission(), overlay, targets)
            .unwrap();
    }
    let config =
        worldrunner_render_post::FrameConfig::new([SIDE, SIDE], OffscreenTarget::COLOR_FORMAT, 1);
    let mut renderer =
        FrameRenderer::with_output(device, config, OffscreenTarget::COLOR_FORMAT).unwrap();
    renderer.prepare_scene_inputs(device, &scene);
    renderer.overlay.prepare_depth(device, None, &frame());
    let mut target = OffscreenTarget::new(
        device,
        Viewport {
            width: SIDE,
            height: SIDE,
        },
    )
    .unwrap();
    let encode = |encoder: &mut wgpu::CommandEncoder,
                  _: &mut worldrunner_render_gpu::submission::Publications| {
        renderer.encode_output(encoder, target.color_view())
    };
    context
        .submission()
        .prepare(device, encode)
        .unwrap()
        .submit()
        .unwrap();
    target
        .read_rgba8(context)
        .unwrap()
        .pixels
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect()
}

fn frame() -> crate::frame::FrameUniform {
    let view = crate::SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -5_000, 0],
            target_millimeters: [0; 3],
        },
        worldrunner_render_plan::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: SIDE,
            height: SIDE,
        },
    )
    .unwrap();
    crate::frame::FrameUniform::unlit(&view)
}

fn at(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
    pixels[(y * SIDE + x) as usize]
}

#[test]
fn an_overlay_quad_shows_a_bound_render_texture_with_its_bottom_row_at_the_bottom() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    // Memory row 0 is the bottom row of a render texture: red below, blue above.
    let texture = host_texture(&context, wgpu::TextureFormat::Rgba8Unorm, &[RED, BLUE]);
    let target = UiTarget::new(id(), Some(texture), sampler()).unwrap();
    let pixels = shown(
        &context,
        &overlay(UiTexture::Target(id()), &BTreeMap::new()),
        &[&[target]],
    );
    assert_eq!(
        at(&pixels, 1, 0),
        BLUE,
        "the top of the screen shows the top row"
    );
    assert_eq!(
        at(&pixels, 1, SIDE - 1),
        RED,
        "the bottom shows the bottom row"
    );
}

#[test]
fn an_unbound_slot_draws_the_transparent_texel() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let unwritten = UiTarget::new(id(), None, sampler()).unwrap();
    for offered in [vec![], vec![unwritten]] {
        let pixels = shown(
            &context,
            &overlay(UiTexture::Target(id()), &BTreeMap::new()),
            &[&offered],
        );
        assert!(pixels.iter().all(|p| *p == CLEAR), "{pixels:?}");
    }
}

#[test]
fn a_rebound_texture_replaces_the_old_one() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let red = host_texture(&context, wgpu::TextureFormat::Rgba8Unorm, &[RED]);
    let blue = host_texture(&context, wgpu::TextureFormat::Rgba8Unorm, &[BLUE]);
    let first = UiTarget::new(id(), Some(red), sampler()).unwrap();
    let second = UiTarget::new(id(), Some(blue), sampler()).unwrap();
    let pixels = shown(
        &context,
        &overlay(UiTexture::Target(id()), &BTreeMap::new()),
        &[&[first], &[second]],
    );
    assert_eq!(at(&pixels, 2, 2), BLUE);
}

fn stored(color_space: TextureColorSpace, texel: [u8; 4]) -> BTreeMap<u64, SceneTexture> {
    BTreeMap::from([(
        1 << 40,
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space,
            width: 1,
            height: 1,
            mips: 1,
            bytes: texel.to_vec().into(),
        }
        .with_sampler(sampler()),
    )])
}

fn target_matches_stored(format: wgpu::TextureFormat, color_space: TextureColorSpace) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let texel = [128, 64, 200, 255];
    let texture = host_texture(&context, format, &[texel]);
    let target = UiTarget::new(id(), Some(texture), sampler()).unwrap();
    let from_target = shown(
        &context,
        &overlay(UiTexture::Target(id()), &BTreeMap::new()),
        &[&[target]],
    );
    let from_stored = shown(
        &context,
        &overlay(UiTexture::Image(1 << 40), &stored(color_space, texel)),
        &[&[]],
    );
    assert_eq!(at(&from_target, 1, 1), at(&from_stored, 1, 1), "{format:?}");
}

#[test]
fn an_srgb_target_matches_the_same_texel_stored_as_srgb() {
    target_matches_stored(wgpu::TextureFormat::Rgba8UnormSrgb, TextureColorSpace::Srgb);
}

#[test]
fn a_linear_target_matches_the_same_texel_stored_as_linear() {
    target_matches_stored(wgpu::TextureFormat::Rgba8Unorm, TextureColorSpace::Linear);
}

#[test]
fn a_stored_image_shows_its_first_row_at_the_top() {
    // The control for the orientation test above: the readback is top row first, and a stored
    // texture, whose first row is its top, is not turned over.
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let image = BTreeMap::from([(
        1 << 40,
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 2,
            mips: 1,
            bytes: [RED, BLUE].concat().into(),
        }
        .with_sampler(sampler()),
    )]);
    let pixels = shown(
        &context,
        &overlay(UiTexture::Image(1 << 40), &image),
        &[&[]],
    );
    assert_eq!(at(&pixels, 1, 0), RED);
    assert_eq!(at(&pixels, 1, SIDE - 1), BLUE);
}
