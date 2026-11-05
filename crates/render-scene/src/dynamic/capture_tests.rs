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
use crate::draw::SceneGpu;
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

const SIZE: u32 = 32;
const SRGB: TextureFormat = TextureFormat::Rgba8UnormSrgb;
const HDR: TextureFormat = TextureFormat::Rgba16Float;
const VELOCITY: TextureFormat = worldrunner_render_post::VELOCITY_FORMAT;

/// Which screen-space bind group the pipelines expect at group three.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    None,
    Ambient,
    Reflection,
}

#[derive(Clone, Copy)]
enum Kind {
    Plain(TextureFormat),
    Velocity,
    Ssao,
    SsaoVelocity,
    Reflection { velocity: bool },
}

struct Config {
    name: &'static str,
    kind: Kind,
    samples: u32,
}

const CONFIGS: [Config; 12] = [
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
struct Built {
    pipelines: ScenePipelines,
    formats: Vec<TextureFormat>,
    screen: Option<BindGroup>,
}

fn build(device: &Device, config: &Config) -> Built {
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

fn one_texel(device: &Device) -> TextureView {
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

fn ambient_group(device: &Device) -> BindGroup {
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

fn reflection_group(device: &Device, layout: &wgpu::BindGroupLayout) -> BindGroup {
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
/// coverage mask, and two adjacent quads that share one instanced draw.
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
    ]
}

/// The values an attachment holds before anything is drawn: opaque for the colour the material
/// writes, and distinct nonzero values for the targets it must leave alone.
fn fill(index: usize, format: TextureFormat) -> wgpu::Color {
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

struct Attachments {
    drawn: Vec<Texture>,
    views: Vec<TextureView>,
    resolved: Vec<Texture>,
    depth: TextureView,
}

impl Attachments {
    fn new(device: &Device, formats: &[TextureFormat], samples: u32) -> Self {
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
    fn results(&self) -> &[Texture] {
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
    let staging: Vec<_> = attachments
        .results()
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
        .collect();
    let index = encoder.finish().submit().unwrap();
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

/// The fixed unlit-alpha path as it was before dynamic quads drew through the program runtime.
struct Legacy {
    pipeline: RenderPipeline,
    draws: Vec<LegacyDraw>,
    indices: Buffer,
    frame: BindGroup,
}

struct LegacyDraw {
    vertices: Buffer,
    instance: Buffer,
    tint: Buffer,
    count: u32,
    material: BindGroup,
}

impl Legacy {
    fn new(
        context: &GpuContext,
        pipelines: &ScenePipelines,
        textures: &[SceneTexture],
        quads: &[DynamicQuad],
        view: &SceneView,
    ) -> Self {
        let (device, queue) = (context.device(), context.queue());
        let uploaded: Vec<Texture> = textures
            .iter()
            .map(|texture| {
                upload_texture(device, queue, &prepare_texture(texture).unwrap()).unwrap()
            })
            .collect();
        let samplers: Vec<_> = textures
            .iter()
            .map(|texture| super::sampling::create_sampler(device, texture.sampler()))
            .collect();
        let views: Vec<_> = uploaded
            .iter()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();
        let (placeholder, placeholder_view) = placeholder_with_view(device, queue);
        let shared = material_sampler(device);
        let layout = material_bind_group_layout(device);
        let mut ordered: Vec<_> = quads.iter().collect();
        ordered.sort_by_key(|quad| (quad.overlay, quad.sorting_layer, quad.sorting_order));
        let draws = ordered
            .chunk_by(|a, b| {
                a.texture == b.texture
                    && a.corners == b.corners
                    && a.rect.map(f32::to_bits) == b.rect.map(f32::to_bits)
                    && a.uv_rect.map(f32::to_bits) == b.uv_rect.map(f32::to_bits)
            })
            .map(|group| {
                let quad = group[0];
                let mut instances = Vec::new();
                let mut tints = Vec::new();
                for quad in group {
                    let model = Mat4::from_scale_rotation_translation(
                        Vec3::from_array(quad.scale),
                        Quat::from_array(quad.rotation).normalize(),
                        Vec3::from_array(quad.position),
                    );
                    instances.extend_from_slice(&pack_model_instance(model));
                    for channel in quad.tint {
                        push_f32(&mut tints, channel);
                    }
                }
                let albedo = quad
                    .texture
                    .map_or(&placeholder_view, |index| &views[index]);
                let sampler = quad.texture.map_or(&shared, |index| &samplers[index]);
                let mut bound = [&placeholder_view; 5];
                bound[0] = albedo;
                let uniform = upload_buffer(
                    device,
                    queue,
                    "legacy material",
                    BufferUsages::UNIFORM,
                    &[0u8; MATERIAL_UNIFORM_BYTES],
                );
                let heights = upload_buffer(
                    device,
                    queue,
                    "legacy heights",
                    BufferUsages::STORAGE,
                    &[0u8; 48],
                );
                LegacyDraw {
                    vertices: upload_buffer(
                        device,
                        queue,
                        "legacy quad",
                        BufferUsages::VERTEX,
                        &pack_quad(quad),
                    ),
                    instance: upload_buffer(
                        device,
                        queue,
                        "legacy instances",
                        BufferUsages::VERTEX,
                        &instances,
                    ),
                    tint: upload_buffer(
                        device,
                        queue,
                        "legacy tints",
                        BufferUsages::VERTEX,
                        &tints,
                    ),
                    count: u32::try_from(group.len()).unwrap(),
                    material: material_group(device, &layout, &uniform, &bound, sampler, &heights),
                }
            })
            .collect();
        drop(placeholder);
        let indices: Vec<u8> = [0u32, 1, 2, 0, 2, 3]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let frame_bytes = upload_buffer(
            device,
            queue,
            "legacy frame",
            BufferUsages::UNIFORM,
            &FrameUniform::unlit(view).to_bytes(),
        );
        Self {
            pipeline: pipelines.legacy_unlit_alpha(),
            draws,
            indices: upload_buffer(
                device,
                queue,
                "legacy indices",
                BufferUsages::INDEX,
                &indices,
            ),
            frame: crate::draw::decals::empty_group(device, pipelines.frame_layout(), &frame_bytes),
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame, &[]);
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for draw in &self.draws {
            pass.set_bind_group(1, &draw.material, &[]);
            pass.set_vertex_buffer(0, draw.vertices.slice(..));
            pass.set_vertex_buffer(1, draw.instance.slice(..));
            pass.set_vertex_buffer(2, draw.tint.slice(..));
            pass.draw_indexed(0..6, 0, 0..draw.count);
        }
    }
}

fn pack_quad(quad: &DynamicQuad) -> Vec<u8> {
    let [left, bottom, right, top] = quad.rect;
    let [u0, v0, u1, v1] = quad.uv_rect;
    let mut bytes = Vec::new();
    for (index, (x, z, u, v)) in [
        (left, bottom, u0, v1),
        (right, bottom, u1, v1),
        (right, top, u1, v0),
        (left, top, u0, v0),
    ]
    .into_iter()
    .enumerate()
    {
        let position = quad.corners.map_or(Vec3::new(x, 0., z), |corners| {
            Vec3::from_array(corners[index])
        });
        push_vec3(&mut bytes, position);
        push_vec3(&mut bytes, Vec3::NEG_Y);
        push_f32(&mut bytes, u);
        push_f32(&mut bytes, v);
        bytes.extend_from_slice(&BILLBOARD_FIXED.to_le_bytes());
        push_f32(&mut bytes, 1.0);
        bytes.extend_from_slice(&[0; 16]);
    }
    bytes
}

/// FNV-1a over the bytes: stable across runs, toolchains and hosts, unlike the std hasher.
fn digest(bytes: &[u8]) -> u64 {
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

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("actual GPU required")
}

fn supported(context: &GpuContext, config: &Config) -> bool {
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

/// Native world contributions, which reach the dynamic path as sprites and as UI glyphs, drawn
/// through the frame renderer's scene entry point.
mod world {
    use glam::Vec3;
    use worldrunner_render_plan::simulation::{
        SpriteTextures, WorldRenderPlan, append_ui, extract_world,
    };
    use worldrunner_render_plan::ui::{UiGlyphQuad, UiRenderPlan, UiTextContribution};
    use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef};
    use worldrunner_world_simulation::presentation::{Camera, Sprite, SpriteRenderer};
    use worldrunner_world_simulation::ui::UiLayout;
    use worldrunner_world_simulation::{
        ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
    };

    use super::*;
    use crate::compositor::{FrameRenderer, encode_scene};
    use crate::simulation::WorldSceneContribution;

    const SPRITE: TextureFormat = SRGB;

    pub(super) fn sprite_frame() -> WorldSceneContribution {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let camera = world
            .create_object(ObjectDesc {
                local_transform: LocalTransform {
                    translation: Vec3::new(0., 0., -10.),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        world
            .add_component(
                camera,
                ComponentDesc::Camera(Camera {
                    half_height: 2.,
                    background: [0., 0., 0., 0.25],
                    ..Default::default()
                }),
            )
            .unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                object,
                ComponentDesc::Sprite(SpriteRenderer {
                    sprite: Some(Sprite {
                        identity: 0,
                        texture: 42,
                        rect: [0., 0., 1., 1.],
                        texture_size: [1, 1],
                        pivot: [0.5, 0.5],
                        pixels_per_unit: 0.5,
                    }),
                    color: [0.5, 1., 1., 0.5],
                    ..Default::default()
                }),
            )
            .unwrap();
        let textures = SpriteTextures {
            textures: std::collections::BTreeMap::from([(
                42,
                SceneTexture::Texels {
                    format: TexelFormat::Rgba8Unorm,
                    color_space: TextureColorSpace::Srgb,
                    width: 1,
                    height: 1,
                    mips: 1,
                    bytes: vec![128, 64, 32, 128].into_boxed_slice(),
                },
            )]),
        };
        WorldSceneContribution::new(extract_world(&world, &textures, [SIZE, SIZE]).unwrap())
            .unwrap()
    }

    pub(super) fn glyph_frame() -> WorldSceneContribution {
        let frame = sprite_frame();
        let mut plan = WorldRenderPlan {
            camera_state: None,
            camera: Some(*frame.view.resolved_view()),
            clear: frame.clear,
            quads: vec![],
            textures: std::collections::BTreeMap::new(),
        };
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let pixels = SIZE as f32;
        let ui = UiRenderPlan {
            layout: UiLayout {
                viewport: [SIZE, SIZE].into(),
                rects: vec![],
            },
            texts: vec![UiTextContribution {
                object,
                canvas: object,
                overlay: true,
                sorting_layer: 0,
                sorting_order: 0,
                color: [1., 1., 1., 0.5],
                texture: Some(SceneTexture::Texels {
                    format: TexelFormat::R8Unorm,
                    color_space: TextureColorSpace::Linear,
                    width: 1,
                    height: 1,
                    mips: 1,
                    bytes: vec![128].into_boxed_slice(),
                }),
                quads: vec![UiGlyphQuad {
                    corners: [
                        [0., 0., 0.],
                        [pixels, 0., 0.],
                        [pixels, pixels, 0.],
                        [0., pixels, 0.],
                    ],
                    uv: [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
                }],
            }],
        };
        append_ui(&mut plan, ui).unwrap();
        WorldSceneContribution::new(plan).unwrap()
    }

    /// The frame renderer's output for `contribution`, as the native sprite tests capture it.
    fn compositor(
        context: &GpuContext,
        gpu: &mut SceneGpu,
        pipelines: &ScenePipelines,
        samples: u32,
        contribution: &WorldSceneContribution,
    ) -> Vec<u8> {
        let size = [SIZE, SIZE];
        let mut renderer = FrameRenderer::with_output(
            context.device(),
            FrameConfig::new(size, SPRITE, samples),
            SPRITE,
        )
        .unwrap();
        let [r, g, b, a] = contribution.clear;
        renderer
            .set_clear_color(wgpu::Color { r, g, b, a })
            .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("capture color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("capture depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        encode_scene(
            &mut renderer,
            context.device(),
            &mut encoder,
            (gpu, pipelines),
            (&color, &depth),
            FrameUniform::unlit(&contribution.view),
        )
        .unwrap();
        renderer.targets().encode_resolve(&mut encoder);
        encoder.finish().submit().unwrap();
        Readback::new(
            context.device(),
            FrameConfig::new(size, SPRITE, 1),
            1_000_000,
        )
        .unwrap()
        .read(
            context.device(),
            context.submission(),
            renderer.targets().texture(),
        )
        .unwrap()
    }

    pub(super) fn captures(context: &GpuContext, name: &str, frame: &WorldSceneContribution) {
        for samples in [1, 4] {
            let pipelines =
                ScenePipelines::with_samples(context.device(), SPRITE, samples).unwrap();
            let mut gpu =
                SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.textures,
                &frame.quads,
            )
            .unwrap();
            let scene = compositor(context, &mut gpu, &pipelines, samples, frame);
            let legacy = Legacy::new(
                context,
                &pipelines,
                &frame.textures,
                &frame.quads,
                &frame.view,
            );
            let [r, g, b, a] = frame.clear;
            let reference = render_cleared(
                context,
                (&[SPRITE], samples),
                Some(wgpu::Color { r, g, b, a }),
                |pass| legacy.draw(pass),
            );
            assert!(
                scene == reference[0],
                "{name} at {samples} samples differs from the fixed pipeline's output"
            );
            let clear_only = render_cleared(
                context,
                (&[SPRITE], samples),
                Some(wgpu::Color { r, g, b, a }),
                |_| {},
            );
            assert!(scene != clear_only[0], "{name} drew nothing");
            report(context, &format!("{name}-{samples}x"), &[scene]);
        }
    }
}

#[test]
fn native_sprites_match_the_fixed_pipeline_through_the_frame_renderer() {
    let context = context();
    world::captures(&context, "sprite", &world::sprite_frame());
}

#[test]
fn world_glyphs_match_the_fixed_pipeline_through_the_frame_renderer() {
    let context = context();
    world::captures(&context, "glyph", &world::glyph_frame());
}
