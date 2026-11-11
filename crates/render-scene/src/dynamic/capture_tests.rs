//! Byte-level captures of the dynamic unlit-alpha material.
//!
//! Each configuration draws the same quads twice on one device: once through the scene's own
//! dynamic path and once through [`Legacy`], a frozen copy of the fixed pipeline, vertex
//! packing and material bind group that path used before it drew through the program runtime.
//! Every colour attachment is read back and compared byte for byte. The attachments the
//! material does not write start with known values, so a stray write shows as a difference.
//!
//! Each run prints one `UNLIT-CAPTURE` line per configuration and attachment with a hash of the
//! bytes, so captures taken before and after a change can be compared as text. Hashes differ
//! between adapters and are only comparable on one.

use glam::{Mat4, Quat, Vec3};
use wgpu::{
    BindGroup, Buffer, BufferUsages, Device, RenderPipeline, Texture, TextureFormat, TextureView,
};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};

use super::DynamicQuad;
use crate::device::GpuContext;
use crate::draw::{DrawCount, SceneGpu};
use crate::frame::FrameUniform;
use crate::geometry::{BILLBOARD_FIXED, pack_model_instance, push_f32, push_vec3};
use crate::pipeline::MATERIAL_UNIFORM_BYTES;
use crate::renderer::ScenePipelines;
use crate::texture::prepare_texture;
use crate::upload::{
    material_bind_group_layout, material_group, material_sampler, placeholder_with_view,
    upload_buffer, upload_texture,
};
use crate::{CameraPose, OrthographicLens, SceneView, Viewport};

pub(crate) const SIZE: u32 = 32;
pub(crate) const SRGB: TextureFormat = TextureFormat::Rgba8UnormSrgb;
pub(crate) const HDR: TextureFormat = TextureFormat::Rgba16Float;
pub(crate) const VELOCITY: TextureFormat = worldrunner_render_post::VELOCITY_FORMAT;

/// Which screen-space bind group the pipelines expect at group three.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    None,
    Ambient,
    Reflection,
}

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Plain(TextureFormat),
    Velocity,
    Ssao,
    SsaoVelocity,
    Reflection { velocity: bool },
}

pub(crate) struct Config {
    pub(crate) name: &'static str,
    pub(crate) kind: Kind,
    pub(crate) samples: u32,
}

pub(crate) const CONFIGS: [Config; 12] = [
    Config {
        name: "srgb-1x",
        kind: Kind::Plain(SRGB),
        samples: 1,
    },
    Config {
        name: "srgb-4x",
        kind: Kind::Plain(SRGB),
        samples: 4,
    },
    Config {
        name: "hdr-1x",
        kind: Kind::Plain(HDR),
        samples: 1,
    },
    Config {
        name: "hdr-4x",
        kind: Kind::Plain(HDR),
        samples: 4,
    },
    Config {
        name: "velocity-1x",
        kind: Kind::Velocity,
        samples: 1,
    },
    Config {
        name: "velocity-4x",
        kind: Kind::Velocity,
        samples: 4,
    },
    Config {
        name: "ssao-1x",
        kind: Kind::Ssao,
        samples: 1,
    },
    Config {
        name: "ssao-velocity-4x",
        kind: Kind::SsaoVelocity,
        samples: 4,
    },
    Config {
        name: "reflection-1x",
        kind: Kind::Reflection { velocity: false },
        samples: 1,
    },
    Config {
        name: "reflection-4x",
        kind: Kind::Reflection { velocity: false },
        samples: 4,
    },
    Config {
        name: "reflection-velocity-1x",
        kind: Kind::Reflection { velocity: true },
        samples: 1,
    },
    Config {
        name: "reflection-velocity-4x",
        kind: Kind::Reflection { velocity: true },
        samples: 4,
    },
];

/// Pipelines for a configuration, the colour attachments of its pass and the group-three
/// resources the pipelines' layout requires.
pub(crate) struct Built {
    pub(crate) pipelines: ScenePipelines,
    pub(crate) formats: Vec<TextureFormat>,
    pub(crate) screen: Option<BindGroup>,
}

pub(crate) fn build(device: &Device, config: &Config) -> Built {
    let samples = config.samples;
    let (pipelines, formats, screen, layout) = match config.kind {
        Kind::Plain(format) => (
            ScenePipelines::with_samples(device, format, samples).unwrap(),
            vec![format],
            Screen::None,
            None,
        ),
        Kind::Velocity => (
            ScenePipelines::with_taa_velocity(device, HDR, samples).unwrap(),
            vec![HDR, VELOCITY],
            Screen::None,
            None,
        ),
        Kind::Ssao => (
            ScenePipelines::with_ssao(device, HDR, samples).unwrap(),
            vec![HDR],
            Screen::Ambient,
            None,
        ),
        Kind::SsaoVelocity => (
            ScenePipelines::with_ssao_taa_velocity(device, HDR, samples).unwrap(),
            vec![HDR, VELOCITY],
            Screen::Ambient,
            None,
        ),
        Kind::Reflection { velocity } => {
            let layout = crate::ssr::forward::layout(device);
            let (object, terrain) = crate::ssr::forward::velocity_sources(
                (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL),
                velocity,
                false,
            );
            let pipelines = ScenePipelines::with_screen_space(
                device,
                (HDR, samples),
                &object,
                &terrain,
                None,
                Some(&layout),
            );
            let mut formats = vec![HDR; 3];
            if velocity {
                formats.push(VELOCITY);
            }
            (pipelines, formats, Screen::Reflection, Some(layout))
        }
    };
    let screen = match screen {
        Screen::None => None,
        Screen::Ambient => Some(ambient_group(device)),
        Screen::Reflection => Some(reflection_group(device, &layout.unwrap())),
    };
    Built {
        pipelines,
        formats,
        screen,
    }
}

pub(crate) fn one_texel(device: &Device) -> TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("capture screen texel"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

pub(crate) fn ambient_group(device: &Device) -> BindGroup {
    let layout = crate::ssao::ambient_layout(device);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("capture ambient"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&one_texel(device)),
        }],
    })
}

pub(crate) fn reflection_group(device: &Device, layout: &wgpu::BindGroupLayout) -> BindGroup {
    let [cube, plane] = crate::water::receivers::empty_environment(device);
    let ambient = one_texel(device);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let intensity = upload_zero(device, 16);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("capture reflection"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&ambient),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&cube),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&plane),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: intensity.as_entire_binding(),
            },
        ],
    })
}

fn upload_zero(device: &Device, bytes: u64) -> Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture zero"),
        size: bytes,
        usage: BufferUsages::UNIFORM,
        mapped_at_creation: false,
    })
}

fn view() -> SceneView {
    SceneView::orthographic(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 2.0,
            near_meters: 0.1,
            far_meters: 100.0,
        },
        Viewport {
            width: SIZE,
            height: SIZE,
        },
    )
    .unwrap()
}

fn texture(format: TexelFormat, space: TextureColorSpace, size: u32, bytes: &[u8]) -> SceneTexture {
    SceneTexture::Texels {
        format,
        color_space: space,
        width: size,
        height: size,
        mips: 1,
        bytes: bytes.into(),
    }
}

/// An atlas of four distinct texels and a one texel coverage mask.
fn textures() -> Vec<SceneTexture> {
    vec![
        texture(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Srgb,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
            ],
        ),
        texture(TexelFormat::R8Unorm, TextureColorSpace::Linear, 1, &[128]),
    ]
}

/// Quads that cover the cases the material has: a whole atlas, a sub-rectangle of it with tint
/// and alpha over it, an untextured sheared quad with an HDR tint, an overlay with a flipped
/// coverage mask, two adjacent quads that share one instanced draw, and a textured triangle
/// between them and the overlay that draws three indices.
fn quads() -> Vec<DynamicQuad> {
    vec![
        DynamicQuad {
            position: [-0.9, 0.0, 0.9],
            scale: [1.6, 1.0, 1.6],
            texture: Some(0),
            ..DynamicQuad::default()
        },
        DynamicQuad {
            position: [-0.4, 0.2, 0.5],
            scale: [1.2, 1.0, 1.2],
            texture: Some(0),
            uv_rect: [0.5, 0.0, 1.0, 0.5],
            tint: [1.0, 0.5, 0.25, 0.5],
            sorting_order: 1,
            ..DynamicQuad::default()
        },
        DynamicQuad {
            position: [0.8, 0.0, -0.6],
            scale: [1.4, 1.0, 1.4],
            corners: Some([
                [-0.5, 0.0, -0.5],
                [0.5, 0.0, -0.5],
                [0.7, 0.0, 0.5],
                [-0.3, 0.0, 0.5],
            ]),
            tint: [0.2, 0.8, 1.5, 0.5],
            ..DynamicQuad::default()
        },
        DynamicQuad {
            position: [0.9, 0.0, 1.0],
            texture: Some(1),
            uv_rect: [0.0, 1.0, 1.0, 0.0],
            tint: [1.0, 1.0, 1.0, 0.5],
            overlay: true,
            ..DynamicQuad::default()
        },
        DynamicQuad {
            position: [-1.0, 0.0, -1.2],
            tint: [1.0, 0.0, 0.0, 0.5],
            ..DynamicQuad::default()
        },
        DynamicQuad {
            position: [-0.8, 0.0, -1.0],
            tint: [0.0, 0.0, 1.0, 0.5],
            ..DynamicQuad::default()
        },
        DynamicQuad {
            position: [0.3, 0.0, 0.1],
            scale: [1.5, 1.0, 1.5],
            corners: Some([
                [-0.5, 0.0, -0.5],
                [0.6, 0.0, -0.4],
                [0.0, 0.0, 0.6],
                [0.0, 0.0, 0.0],
            ]),
            triangle_uv: Some([[0.0, 1.0], [1.0, 0.75], [0.5, 0.0]]),
            texture: Some(0),
            tint: [0.9, 1.0, 0.6, 0.75],
            sorting_order: 2,
            ..DynamicQuad::default()
        },
    ]
}

/// The values an attachment holds before anything is drawn: opaque for the colour the material
/// writes, and distinct nonzero values for the targets it must leave alone.
pub(crate) fn fill(index: usize, format: TextureFormat) -> wgpu::Color {
    match (index, format) {
        (0, _) => wgpu::Color {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            a: 1.0,
        },
        (_, VELOCITY) => wgpu::Color {
            r: 0.25,
            g: -0.5,
            b: 0.0,
            a: 0.0,
        },
        _ => wgpu::Color {
            r: 0.125,
            g: 0.25,
            b: 0.5,
            a: 0.75,
        },
    }
}

pub(crate) struct Attachments {
    pub(crate) drawn: Vec<Texture>,
    pub(crate) views: Vec<TextureView>,
    pub(crate) resolved: Vec<Texture>,
    pub(crate) depth: TextureView,
}

impl Attachments {
    pub(crate) fn new(device: &Device, formats: &[TextureFormat], samples: u32) -> Self {
        let make = |format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("capture attachment"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let copyable = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        let drawn: Vec<_> = formats
            .iter()
            .map(|&format| {
                make(
                    format,
                    samples,
                    if samples == 1 {
                        copyable
                    } else {
                        wgpu::TextureUsages::RENDER_ATTACHMENT
                    },
                )
            })
            .collect();
        let resolved = if samples == 1 {
            Vec::new()
        } else {
            formats
                .iter()
                .map(|&format| make(format, 1, copyable))
                .collect()
        };
        let depth = make(
            TextureFormat::Depth32Float,
            samples,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&wgpu::TextureViewDescriptor::default());
        let views = drawn
            .iter()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();
        Self {
            drawn,
            views,
            resolved,
            depth,
        }
    }

    /// The textures holding the single-sample result of each attachment.
    pub(crate) fn results(&self) -> &[Texture] {
        if self.resolved.is_empty() {
            &self.drawn
        } else {
            &self.resolved
        }
    }
}

/// Renders one pass over fresh attachments filled with [`fill`], lets `draw` record into it,
/// and returns the bytes of every colour attachment.
fn render(
    context: &GpuContext,
    (formats, samples): (&[TextureFormat], u32),
    draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> Vec<Vec<u8>> {
    render_cleared(context, (formats, samples), None, draw)
}

/// [`render`] with the first attachment cleared to `clear` instead of the usual fill.
fn render_cleared(
    context: &GpuContext,
    (formats, samples): (&[TextureFormat], u32),
    clear: Option<wgpu::Color>,
    draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> Vec<Vec<u8>> {
    let device = context.device();
    let attachments = Attachments::new(device, formats, samples);
    let resolve_views: Vec<_> = attachments
        .resolved
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    let colors: Vec<_> = attachments
        .views
        .iter()
        .zip(formats)
        .enumerate()
        .map(|(index, (view, &format))| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: resolve_views.get(index),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(match (index, clear) {
                        (0, Some(clear)) => clear,
                        _ => fill(index, format),
                    }),
                    store: if samples == 1 {
                        wgpu::StoreOp::Store
                    } else {
                        wgpu::StoreOp::Discard
                    },
                },
            })
        })
        .collect();
    let mut encoder = context.submission().begin(device).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("unlit capture"),
            color_attachments: &colors,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &attachments.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        draw(&mut pass);
    }
    let staging = copy_results(device, &mut encoder, attachments.results());
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, staging)
}

/// Copies each single-sample attachment into a mappable buffer with its rows padded.
pub(crate) fn copy_results(
    device: &Device,
    encoder: &mut wgpu::CommandEncoder,
    results: &[Texture],
) -> Vec<(Buffer, usize, usize)> {
    results
        .iter()
        .map(|texture| {
            let row = SIZE * texture.format().block_copy_size(None).unwrap();
            let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("capture readback"),
                size: u64::from(padded * SIZE),
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(SIZE),
                    },
                },
                texture.size(),
            );
            (buffer, row as usize, padded as usize)
        })
        .collect()
}

/// Waits for the submission and returns each attachment's bytes without the row padding.
pub(crate) fn read_results(
    device: &Device,
    index: wgpu::SubmissionIndex,
    staging: Vec<(Buffer, usize, usize)>,
) -> Vec<Vec<u8>> {
    for (buffer, ..) in &staging {
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| result.unwrap());
    }
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: None,
        })
        .unwrap();
    staging
        .into_iter()
        .map(|(buffer, row, padded)| {
            let mapped = buffer.slice(..).get_mapped_range().unwrap().to_vec();
            mapped
                .chunks_exact(padded)
                .flat_map(|line| &line[..row])
                .copied()
                .collect()
        })
        .collect()
}

/// FNV-1a over the bytes: stable across runs, toolchains and hosts, unlike the std hasher.
pub(crate) fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn report(context: &GpuContext, name: &str, images: &[Vec<u8>]) {
    let adapter = context.adapter().get_info().name;
    for (index, image) in images.iter().enumerate() {
        println!(
            "UNLIT-CAPTURE adapter={adapter:?} config={name} attachment={index} bytes={} hash={:016x}",
            image.len(),
            digest(image)
        );
    }
}

/// Draws `quads` over `textures` with the scene's dynamic path and with [`Legacy`] in the pass
/// of `config`, checks the two agree on every attachment, that quads drew something and that
/// the material left the attachments it does not write as they were, and returns the scene's
/// attachments.
fn capture(
    context: &GpuContext,
    config: &Config,
    textures: &[SceneTexture],
    quads: &[DynamicQuad],
) -> Vec<Vec<u8>> {
    capture_expecting(context, config, textures, quads, !quads.is_empty())
}

fn capture_expecting(
    context: &GpuContext,
    config: &Config,
    textures: &[SceneTexture],
    quads: &[DynamicQuad],
    drawn: bool,
) -> Vec<Vec<u8>> {
    let built = build(context.device(), config);
    let view = view();
    let mut gpu =
        SceneGpu::empty(context.device(), context.submission(), &built.pipelines).unwrap();
    gpu.set_dynamic_contribution(
        context.device(),
        context.submission(),
        &built.pipelines,
        textures,
        quads,
    )
    .unwrap();
    gpu.set_frame(context.queue(), FrameUniform::unlit(&view));
    let legacy = Legacy::new(context, &built.pipelines, textures, quads, &view);
    let bind_screen = |pass: &mut wgpu::RenderPass<'_>| {
        if let Some(screen) = &built.screen {
            pass.set_bind_group(3, screen, &[]);
        }
    };
    let scene = render(context, (&built.formats, config.samples), |pass| {
        bind_screen(pass);
        gpu.draw(pass, &built.pipelines);
    });
    let reference = render(context, (&built.formats, config.samples), |pass| {
        bind_screen(pass);
        legacy.draw(pass);
    });
    let untouched = render(context, (&built.formats, config.samples), |_| {});
    assert_eq!(scene.len(), built.formats.len());
    for (index, ((scene, reference), untouched)) in
        scene.iter().zip(&reference).zip(&untouched).enumerate()
    {
        assert!(
            scene == reference,
            "{}: attachment {index} differs from the fixed pipeline's output",
            config.name
        );
        if index == 0 && drawn {
            assert!(
                scene != untouched,
                "{}: the quads drew nothing into the colour attachment",
                config.name
            );
        } else {
            assert!(
                scene == untouched,
                "{}: attachment {index} was written though the material leaves it alone",
                config.name
            );
        }
    }
    report(context, config.name, &scene);
    scene
}

pub(crate) fn context() -> GpuContext {
    GpuContext::shared_for_test_with_features(wgpu::Features::empty()).expect("actual GPU required")
}

pub(crate) fn supported(context: &GpuContext, config: &Config) -> bool {
    let formats: &[TextureFormat] = match config.kind {
        Kind::Plain(format) => &[format],
        _ => &[HDR, VELOCITY],
    };
    formats
        .iter()
        .chain(&[TextureFormat::Depth32Float])
        .all(|&format| {
            context
                .adapter()
                .get_texture_format_features(format)
                .flags
                .sample_count_supported(config.samples)
        })
}

#[test]
fn dynamic_quads_match_the_fixed_pipeline_byte_for_byte_in_every_target_set() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (textures, quads) = (textures(), quads());
    for config in &CONFIGS {
        if !supported(&context, config) {
            println!(
                "UNLIT-CAPTURE config={} skipped: format or samples unsupported",
                config.name
            );
            continue;
        }
        capture(&context, config, &textures, &quads);
    }
}

#[test]
fn an_untextured_batch_and_an_empty_contribution_match_the_fixed_pipeline() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let config = &CONFIGS[2];
    let batch: Vec<_> = (0..5u8)
        .map(|index| DynamicQuad {
            position: [f32::from(index) * 0.3 - 0.6, f32::from(index) * 0.1, 0.0],
            tint: [0.2 * f32::from(index), 0.5, 1.0, 0.4],
            scale: [0.5; 3],
            ..DynamicQuad::default()
        })
        .collect();
    capture(&context, config, &[], &batch);
    let nothing = capture(&context, config, &[], &[]);
    let untouched = render(
        &context,
        (&build(context.device(), config).formats, 1),
        |_| {},
    );
    assert_eq!(nothing, untouched, "no quads draw nothing");
}

mod clipping;
mod legacy;
mod world;

use legacy::Legacy;

#[test]
fn native_sprites_match_the_fixed_pipeline_through_the_frame_renderer() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    world::captures(&context, "sprite", &world::sprite_frame());
}

#[test]
fn world_glyphs_match_the_fixed_pipeline_through_the_frame_renderer() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    world::captures(&context, "glyph", &world::glyph_frame());
}
