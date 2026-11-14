//! The deferred G-buffer observation on a small terrain scene: the visible frame stays the
//! forward frame, the four targets are cleared every frame, and a producer drawn in the
//! `OpaqueGbuffer` slot through `scene_gbuffer_encode` stores what the CPU reference decodes.

use glam::{Mat3, Vec3};
use wgpu::util::DeviceExt;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{RenderScene, SceneOpaquePath, SceneSsr};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameConfig, FrameError,
    FrameFeature, PassSlot, PrepareContext, PreparedFeature, Readback, ResourceRef, ToneMap,
};

use super::capture_tests::scenes;
use crate::compositor::{ConfiguredFrameRenderer, FeatureTarget, RendererConfiguration};
use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::dynamic::capture_tests::{SIZE, context};
use crate::frame::FrameUniform;
use crate::gbuffer::reference::{self, Codes, Surface, angle_degrees, sphere};
// The GPU must meet the bounds the CPU tests establish for the reference.
use crate::gbuffer::tests::{GEOMETRIC_DEGREES, shading_bound};
use crate::gbuffer::{GBUFFER_FORMATS, GBUFFER_WGSL, GbufferTarget};
use crate::renderer::ScenePipelines;

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(super) fn reflections() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 1_000_000,
        ..SceneSsr::default()
    }
}

/// A plain HDR frame, and one with reflections, whose depth prepass completes scene depth
/// before Opaque.
#[derive(Clone, Copy, Debug)]
pub(super) enum Frame {
    Plain,
    Reflecting,
    Flat,
}

impl Frame {
    pub(super) fn scene(self) -> RenderScene {
        match self {
            Self::Plain => scenes::plain(),
            Self::Reflecting => scenes::reflecting(reflections()),
            Self::Flat => scenes::flat_reflecting(reflections()),
        }
    }

    pub(super) fn uniform(self, scene: &RenderScene) -> FrameUniform {
        match self {
            Self::Plain => scenes::frame(scenes::pose()),
            Self::Reflecting | Self::Flat => {
                scenes::frame_with(scenes::pose(), scene.effects().expect("scene effects"))
            }
        }
    }

    fn configuration(self, opaque_path: SceneOpaquePath) -> RendererConfiguration {
        let mut settings =
            RendererConfiguration::legacy([SIZE; 2], wgpu::TextureFormat::Rgba8UnormSrgb);
        settings.frame = FrameConfig::new([SIZE; 2], HDR, 1);
        settings.hdr = Some((1.0, ToneMap::Reinhard));
        if matches!(self, Self::Reflecting | Self::Flat) {
            settings.ssr = Some(reflections());
        }
        settings.opaque_path = opaque_path;
        settings
    }
}

pub(super) struct Drawn {
    pub(super) renderer: ConfiguredFrameRenderer,
    pub(super) gpu: SceneGpu,
    pub(super) pipelines: ScenePipelines,
    pub(super) frame: FrameUniform,
}

fn prepare(context: &GpuContext, frame: Frame, opaque_path: SceneOpaquePath) -> Drawn {
    prepare_scene(context, frame, opaque_path, &frame.scene())
}

/// [`prepare`] for `scene`, which has `frame`'s effects.
pub(super) fn prepare_scene(
    context: &GpuContext,
    frame: Frame,
    opaque_path: SceneOpaquePath,
    scene: &RenderScene,
) -> Drawn {
    let scene = scene.clone();
    let uniform = frame.uniform(&scene);
    let pipelines = ScenePipelines::new(context.device(), HDR);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    gpu.set_frame(context.queue(), uniform);
    let renderer = ConfiguredFrameRenderer::new(
        context,
        frame.configuration(opaque_path),
        AllocationBudget(1 << 28),
    )
    .unwrap();
    Drawn {
        renderer,
        gpu,
        pipelines,
        frame: uniform,
    }
}

/// Encodes `frames` frames and returns the resolved scene colour of the last.
pub(super) fn render(
    context: &GpuContext,
    drawn: &mut Drawn,
    frames: usize,
) -> Result<Vec<u8>, FrameError> {
    for _ in 0..frames {
        let targets = drawn.renderer.targets();
        let color =
            AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture())?;
        let depth =
            AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture())?;
        let mut encoder = context.submission().begin(context.device()).unwrap();
        drawn.renderer.encoding().encode_scene(
            context.device(),
            &mut encoder,
            (&mut drawn.gpu, &drawn.pipelines),
            (&color, &depth),
            drawn.frame,
        )?;
        drawn.renderer.targets().encode_resolve(&mut encoder);
        encoder.finish().submit().unwrap();
    }
    Ok(read(context, drawn.renderer.targets().texture(), HDR))
}

pub(super) fn read(
    context: &GpuContext,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], format, 1),
        1 << 24,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

fn gbuffer(context: &GpuContext, renderer: &ConfiguredFrameRenderer) -> [Vec<u8>; 4] {
    GbufferTarget::ALL.map(|target| {
        let texture = renderer
            .feature_target(FeatureTarget::Gbuffer(target))
            .unwrap_or_else(|| panic!("{target:?} is not allocated"));
        assert_eq!(texture.format(), target.format());
        assert_eq!([texture.width(), texture.height()], [SIZE; 2]);
        assert_eq!(texture.sample_count(), 1);
        read(context, texture, target.format())
    })
}

/// The texels of the target bytes `g3` (`R16Uint`) that a producer covered.
fn covered(g3: &[u8]) -> impl Iterator<Item = (usize, u16)> + '_ {
    g3.chunks(2)
        .map(|code| u16::from_le_bytes([code[0], code[1]]))
        .enumerate()
        .filter(|&(_, code)| code != 0)
}

/// Without the depth prepass the scene stores nothing and the four targets stay clear; with it
/// the scene's terrain is stored. The visible frame is the forward frame either way.
#[test]
fn gbuffer_observation_keeps_the_forward_frame_and_stores_the_prepassed_scene() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for frame in [Frame::Plain, Frame::Reflecting] {
        let mut forward = prepare(&context, frame, SceneOpaquePath::Forward);
        let expected = render(&context, &mut forward, 3).unwrap();
        assert!(
            expected.chunks(8).any(|texel| texel != &expected[..8]),
            "{frame:?}: the terrain drew nothing"
        );
        for target in GbufferTarget::ALL {
            assert!(
                forward
                    .renderer
                    .feature_target(FeatureTarget::Gbuffer(target))
                    .is_none(),
                "{frame:?}: Forward allocated {target:?}"
            );
        }
        let mut observed = prepare(&context, frame, SceneOpaquePath::GBUFFER_OBSERVATION);
        let colour = render(&context, &mut observed, 3).unwrap();
        assert!(colour == expected, "{frame:?}: the observed frame differs");
        let targets = gbuffer(&context, &observed.renderer);
        if matches!(frame, Frame::Plain) {
            for (target, bytes) in GbufferTarget::ALL.iter().zip(&targets) {
                assert!(
                    bytes.iter().all(|&byte| byte == 0),
                    "{frame:?}: {target:?} is not clear"
                );
            }
        } else {
            assert!(
                covered(&targets[3]).next().is_some(),
                "{frame:?}: the scene stored no terrain"
            );
        }
    }
}

/// Level terrain stores its up normal (+Z, the heightfield axis) in G3 at every covered texel, with terrain's AO of 1, no
/// emission, no flags and the reserved channels zero.
#[test]
fn gbuffer_observation_stores_level_terrain_facing_up() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut forward = prepare(&context, Frame::Flat, SceneOpaquePath::Forward);
    let expected = render(&context, &mut forward, 1).unwrap();
    let mut observed = prepare(&context, Frame::Flat, SceneOpaquePath::GBUFFER_OBSERVATION);
    let colour = render(&context, &mut observed, 1).unwrap();
    assert!(colour == expected, "the producer changed the visible frame");
    let targets = gbuffer(&context, &observed.renderer);
    let texels = covered(&targets[3]).collect::<Vec<_>>();
    assert!(
        texels.len() * 4 >= (SIZE * SIZE) as usize,
        "the level terrain covers {} of {} texels",
        texels.len(),
        SIZE * SIZE
    );
    for (pixel, code) in texels {
        let normal = reference::g3_normal(code);
        let off = angle_degrees(normal, Vec3::Z);
        assert!(
            off <= GEOMETRIC_DEGREES,
            "{pixel}: geometric normal off by {off} degrees"
        );
        assert_eq!(code & 3, 0, "{pixel}: terrain sets no flags");
        let g1 = &targets[1][pixel * 4..pixel * 4 + 4];
        let g2 = &targets[2][pixel * 4..pixel * 4 + 4];
        assert_eq!(
            g1[3], 0,
            "{pixel}: G1 alpha is the lighting record, 0 for terrain"
        );
        assert_eq!([g2[2], g2[3]], [0, 255], "{pixel}: terrain emission and AO");
    }
}

#[test]
fn gbuffer_observation_refuses_a_multisampled_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut settings = Frame::Plain.configuration(SceneOpaquePath::GBUFFER_OBSERVATION);
    settings.frame = FrameConfig::new([SIZE; 2], HDR, 4);
    let error = ConfiguredFrameRenderer::new(&context, settings, AllocationBudget(1 << 28))
        .err()
        .expect("a multisampled observation is refused");
    assert!(
        error
            .to_string()
            .contains("opaque admission supports one sample, received 4"),
        "{error}"
    );
}

/// Known surfaces, one per pixel. Pixel zero faces straight away from the camera, the
/// spheremap's singular point.
fn surfaces(rotation: Mat3) -> Vec<Surface> {
    let count = u16::try_from(SIZE * SIZE).expect("a small probe");
    let shading = sphere(count);
    let geometric: Vec<_> = sphere(count).into_iter().rev().collect();
    (0..usize::from(count))
        .map(|index| {
            let unit = |modulus: u16| {
                let step = u16::try_from(index % usize::from(modulus)).expect("below modulus");
                f32::from(step) / f32::from(modulus - 1)
            };
            Surface {
                shading_normal: if index == 0 {
                    rotation.transpose() * Vec3::NEG_Z
                } else {
                    shading[index]
                },
                // Pixel four (no flags) stores the octahedral corner code (0, 0) remapped.
                geometric_normal: if index == 4 {
                    Vec3::new(-1e-3, -1e-3, -1.0).normalize()
                } else {
                    geometric[index]
                },
                base_colour: Vec3::new(unit(7), unit(11), unit(13)),
                roughness: unit(17),
                metal: unit(19),
                emission: unit(23),
                ao: unit(29),
                flags: u32::try_from(index % 4).unwrap(),
                lighting_record: u8::try_from(index % 256).unwrap(),
            }
        })
        .collect()
}

fn rotation() -> Mat3 {
    Mat3::from_euler(glam::EulerRot::ZXY, 0.7, -1.1, 0.3)
}

/// A producer that writes [`surfaces`] through `scene_gbuffer_encode`, optionally with the
/// slot's scene depth attached (tested `Always`, never written).
struct Probe {
    depth: Option<wgpu::TextureFormat>,
}

struct ProbeDraw {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

impl FrameFeature for Probe {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::OpaqueGbuffer);
        declaration.passes[0].color_format = Some(GBUFFER_FORMATS[0]);
        declaration.passes[0].additional_color_formats = GBUFFER_FORMATS[1..].to_vec();
        declaration.passes[0].depth_format = self.depth;
        declaration
    }

    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        let device = context.device;
        let rotation = rotation();
        let mut values: Vec<f32> = [rotation.x_axis, rotation.y_axis, rotation.z_axis]
            .into_iter()
            .flat_map(|column| column.extend(0.0).to_array())
            .collect();
        for surface in surfaces(rotation) {
            values.extend(surface.shading_normal.extend(surface.roughness).to_array());
            values.extend(surface.geometric_normal.extend(surface.metal).to_array());
            values.extend(surface.base_colour.extend(surface.emission).to_array());
            let flags = f32::from(u8::try_from(surface.flags).expect("two bits"));
            values.extend([surface.ao, flags, f32::from(surface.lighting_record), 0.0]);
        }
        let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_le_bytes).collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gbuffer probe surfaces"),
            contents: &bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
        let source = format!(
            "{GBUFFER_WGSL}
@group(0) @binding(0) var<storage, read> probe: array<vec4<f32>>;
@vertex
fn vs_probe(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {{
    let uv = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.5, 1.0);
}}
@fragment
fn fs_probe(@builtin(position) position: vec4<f32>) -> SceneGbufferTargets {{
    let base = 3u + 4u * (u32(position.y) * {SIZE}u + u32(position.x));
    let view_from_scene = mat3x3<f32>(probe[0].xyz, probe[1].xyz, probe[2].xyz);
    let a = probe[base];
    let b = probe[base + 1u];
    let c = probe[base + 2u];
    let d = probe[base + 3u];
    return scene_gbuffer_encode(
        SceneGbufferSurface(a.xyz, b.xyz, c.xyz, a.w, b.w, c.w, d.x, u32(d.y), u32(d.z)),
        view_from_scene,
    );
}}
"
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gbuffer probe"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let targets = GBUFFER_FORMATS.map(|format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuffer probe"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_probe"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: self.depth.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_probe"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gbuffer probe surfaces"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Ok(Box::new(ProbeDraw { pipeline, group }))
    }
}

impl PreparedFeature for ProbeDraw {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.slot, PassSlot::OpaqueGbuffer);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn check_decoded(targets: &[Vec<u8>; 4]) {
    let rotation = rotation();
    let pixels = (SIZE * SIZE) as usize;
    for (pixel, expected) in surfaces(rotation).iter().enumerate().take(pixels) {
        let codes = Codes::from_targets(&targets[0], &targets[1], &targets[2], &targets[3], pixel);
        let mirrored = reference::encode(expected, rotation);
        let near = |a: u16, b: u16| a.abs_diff(b) <= 1;
        // At the singular point every code on the boundary circle decodes to (0, 0, -1) and
        // the rounding of the rotated normal picks one, so only the decoded normal is compared.
        if (rotation * expected.shading_normal).z > -0.9999 {
            assert!(
                near(codes.g0[0], mirrored.g0[0]) && near(codes.g0[1], mirrored.g0[1]),
                "{pixel}: {codes:?} {mirrored:?}"
            );
        }
        assert_eq!(codes.g0[2..], [0, 0], "{pixel}: G0 reserved channels");
        assert_eq!(
            codes.g1[3], expected.lighting_record,
            "{pixel}: G1 lighting record"
        );
        for channel in 0..3 {
            assert!(
                near(codes.g1[channel].into(), mirrored.g1[channel].into()),
                "{pixel}: {codes:?} {mirrored:?}"
            );
        }
        for channel in 0..4 {
            assert!(
                near(codes.g2[channel].into(), mirrored.g2[channel].into()),
                "{pixel}: {codes:?} {mirrored:?}"
            );
        }
        assert_ne!(codes.g3, 0, "{pixel}: a covered pixel stored zero");
        if pixel == 4 {
            assert_eq!(
                codes.g3,
                (127 << 9) | (127 << 2),
                "the corner code is remapped"
            );
        }
        let decoded = reference::decode(&codes, rotation.transpose());
        let view_z = (rotation * expected.shading_normal).z;
        let bound = shading_bound(view_z);
        let shading = angle_degrees(decoded.shading_normal, expected.shading_normal);
        assert!(
            shading <= bound,
            "{pixel}: shading normal off by {shading} degrees"
        );
        let geometric = angle_degrees(decoded.geometric_normal, expected.geometric_normal);
        assert!(
            geometric <= GEOMETRIC_DEGREES,
            "{pixel}: geometric normal off by {geometric} degrees"
        );
        assert_eq!(decoded.flags, expected.flags, "{pixel}: flags");
        assert_eq!(
            decoded.lighting_record, expected.lighting_record,
            "{pixel}: lighting record"
        );
        assert!(
            (decoded.ao - expected.ao).abs() <= 1.0 / 255.0,
            "{pixel}: ao"
        );
        assert!(
            (decoded.emission - expected.emission).abs() <= 2.0 / 255.0,
            "{pixel}: emission"
        );
    }
}

#[test]
fn gbuffer_slot_producer_writes_surfaces_the_reference_decodes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // In a prepass frame the scene's own producer tests the finished depth in this slot, so a
    // producer drawn with it must declare that depth too.
    for (frame, with_depth) in [(Frame::Plain, false), (Frame::Reflecting, true)] {
        let mut drawn = prepare(&context, frame, SceneOpaquePath::GBUFFER_OBSERVATION);
        let mut forward = prepare(&context, frame, SceneOpaquePath::Forward);
        let expected = render(&context, &mut forward, 1).unwrap();
        let depth = drawn.renderer.targets().depth().texture().format();
        let probe = Probe {
            depth: with_depth.then_some(depth),
        };
        drawn
            .renderer
            .renderer_mut()
            .register(context.device(), Box::new(probe))
            .unwrap();
        let rendered = render(&context, &mut drawn, 1);
        if with_depth && !depth.has_stencil_aspect() {
            // Without the stencil depth the prepass does not complete scene depth before
            // Opaque, so the slot has no depth to lend and says so.
            assert_eq!(
                rendered.err(),
                Some(FrameError::AttachmentContract(PassSlot::OpaqueGbuffer))
            );
            continue;
        }
        let colour = rendered.unwrap();
        assert!(
            colour == expected,
            "{frame:?}: the producer changed the visible frame"
        );
        // Printed so that a failing decode names its frame.
        println!("GBUFFER-PROBE frame={frame:?} depth={with_depth}");
        check_decoded(&gbuffer(&context, &drawn.renderer));
    }
}
