//! Deferred lighting of the G-buffer observation on the terrain scenes: the visible frame stays
//! the forward frame, the lighting pass lights only pixels a producer wrote, and it lights a
//! stored surface as the forward mesh shading (`scene_finish_shading`) lights the same surface
//! with the same material. The lighting pass runs before Opaque on the depth the prepass
//! completed, so the plain frame, which has no prepass, lights nothing.

use glam::{Mat3, Vec3};
use wgpu::util::DeviceExt;
use worldrunner_render_plan::{
    RenderScene, SceneAlphaMode, SceneMaterial, SceneMaterialParts, SceneOpaquePath, SceneParts,
    SceneResources, SceneTranslucency, SceneTranslucencyParts, TextureHandle,
};
use worldrunner_render_post::{
    BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PassSlot,
    PrepareContext, PreparedFeature,
};

use super::gbuffer_tests::{Drawn, Frame, prepare_scene, read, render};
use crate::compositor::FeatureTarget;
use crate::device::GpuContext;
use crate::dynamic::capture_tests::{SIZE, context};
use crate::gbuffer::lighting::LightingTarget;
use crate::gbuffer::records::LightingRecord;
use crate::gbuffer::reference::{self, Surface};
use crate::gbuffer::{GBUFFER_FORMATS, GBUFFER_WGSL};
use crate::pipeline::MaterialUniform;

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Pixels along each edge of one surface block; the frame holds `COLUMNS` by `COLUMNS` blocks.
pub(crate) const BLOCK: u32 = 8;
pub(crate) const COLUMNS: u32 = SIZE / BLOCK;

/// Relative error allowed between the lighting pass and the forward shading of the values the
/// G-buffer stores, and the absolute floor below which values count as equal. Both evaluate
/// the same functions on the same position; what remains is half-float output rounding and
/// the CPU against GPU decode of the stored normal.
const STORED_RELATIVE: f32 = 0.005;
/// Relative error allowed against the forward shading of the exact surface before storage,
/// which adds the G-buffer quantisation (normal within 0.25 degrees facing the camera, base
/// colour within 0.0045, scalars within half a code, emission within 1/255).
const EXACT_RELATIVE: f32 = 0.05;
const ABSOLUTE: f32 = 2e-3;

fn leaf(ambient_scale_micro: u32, translucency: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 600_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
    .with_translucency_mask(
        SceneTranslucency::new(SceneTranslucencyParts {
            distortion_micro: 200_000,
            falloff_micro: 4_000_000,
            sun_scale_micro: 1_000_000,
            ambient_scale_micro,
            scale_micro: 900_000,
            base_kelvin: 6500,
            warming_kelvin: 400,
            luminance_micro: 300_000,
            saturation_micro: 0,
        })
        .unwrap(),
    )
    .with_translucency(translucency)
    .unwrap()
}

/// `frame`'s terrain scene with two leaf materials after its terrain materials. They draw
/// nothing; their lighting records are what the producer's leaf blocks refer to.
fn lit_scene(frame: Frame) -> RenderScene {
    let base = frame.scene();
    let mut materials = base.resources().materials().to_vec();
    materials.push(leaf(500_000, 400_000));
    materials.push(leaf(0, 900_000));
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: base.terrain().cloned(),
        effects: base.effects().cloned(),
    })
    .unwrap()
}

/// One block: its surface and the scene material whose record and flags it is lit with.
#[derive(Clone, Copy, Debug)]
struct Block {
    surface: Surface,
    material: usize,
    emissive: bool,
}

/// A linear base colour that sRGB storage represents exactly, from its stored byte.
fn colour(bytes: [u8; 3]) -> Vec3 {
    let decode = |byte: u8| {
        let encoded = f32::from(byte) / 255.0;
        if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    };
    Vec3::new(decode(bytes[0]), decode(bytes[1]), decode(bytes[2]))
}

fn code(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

/// Sixteen blocks: several normals, base colours, roughness and metal, two emissive blocks,
/// material AO, and leaf blocks with the scattering model under both leaf records.
fn blocks(table: &[LightingRecord], materials: &[SceneMaterial]) -> Vec<Block> {
    let index = |material: usize| {
        let record = LightingRecord::from_uniform(
            &MaterialUniform::from_material(&materials[material]).to_bytes(),
        );
        u8::try_from(table.iter().position(|stored| *stored == record).unwrap()).unwrap()
    };
    let leaves = [materials.len() - 2, materials.len() - 1];
    let normals = [
        Vec3::Z,
        Vec3::new(0.0, -1.0, 1.0).normalize(),
        Vec3::new(0.5, -0.3, 0.8).normalize(),
        Vec3::new(-0.6, -0.5, 0.6).normalize(),
    ];
    (0..COLUMNS * COLUMNS)
        .map(|block| {
            let step = u8::try_from(block).unwrap();
            let leaf = block % 5 == 3;
            let material = if leaf {
                leaves[usize::from(step / 5 % 2)]
            } else {
                0
            };
            let shading = if leaf {
                // Facing the camera and away from the overhead sun, so transmission lights it.
                Vec3::new(0.1 * f32::from(step % 3), -0.9, -0.4).normalize()
            } else {
                normals[usize::from(step % 4)]
            };
            let emissive = block == 6 || block == 13;
            Block {
                surface: Surface {
                    shading_normal: shading,
                    geometric_normal: normals[usize::from(step / 4 % 4)],
                    base_colour: colour([40 + step * 13, 200 - step * 9, 90 + step * 7]),
                    roughness: code(60 + step * 11),
                    metal: if leaf {
                        code(120 + step * 5)
                    } else {
                        code(step * 16)
                    },
                    emission: if emissive { code(150) * code(150) } else { 0.0 },
                    ao: code(255 - step * 6),
                    flags: u32::from(leaf),
                    lighting_record: index(material),
                },
                material,
                emissive,
            }
        })
        .collect()
}

/// Four vectors per block in the producer's and the reference's layout.
fn block_values(surfaces: impl Iterator<Item = Surface>) -> Vec<u8> {
    surfaces
        .flat_map(|surface| {
            [
                surface.shading_normal.extend(surface.roughness).to_array(),
                surface.geometric_normal.extend(surface.metal).to_array(),
                surface.base_colour.extend(surface.emission).to_array(),
                [
                    surface.ao,
                    f32::from(u8::try_from(surface.flags).unwrap()),
                    f32::from(surface.lighting_record),
                    0.0,
                ],
            ]
        })
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect()
}

/// A producer that writes each block's surface through `scene_gbuffer_encode` in the
/// `covered` columns of blocks and leaves the rest unwritten. In a frame with the depth
/// prepass the slot attaches the scene depth, so the producer declares its format (`depth`)
/// and draws over it without testing or writing it.
pub(crate) struct Producer {
    pub(crate) blocks: Vec<Surface>,
    pub(crate) view: Mat3,
    pub(crate) covered: u32,
    pub(crate) depth: Option<wgpu::TextureFormat>,
}

struct ProducerDraw {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

impl FrameFeature for Producer {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::OpaqueGbuffer);
        declaration.passes[0].color_format = Some(GBUFFER_FORMATS[0]);
        declaration.passes[0].additional_color_formats = GBUFFER_FORMATS[1..].to_vec();
        declaration.passes[0].depth_format = self.depth;
        declaration
    }

    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        let device = context.device;
        let mut bytes: Vec<u8> = [self.view.x_axis, self.view.y_axis, self.view.z_axis]
            .into_iter()
            .flat_map(|column| column.extend(0.0).to_array())
            .flat_map(f32::to_le_bytes)
            .collect();
        bytes.extend(block_values(self.blocks.iter().copied()));
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("deferred producer blocks"),
            contents: &bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });
        let source = format!(
            "{GBUFFER_WGSL}
@group(0) @binding(0) var<storage, read> blocks: array<vec4<f32>>;
@vertex
fn vs_blocks(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {{
    let corner = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.5, 1.0);
}}
@fragment
fn fs_blocks(@builtin(position) position: vec4<f32>) -> SceneGbufferTargets {{
    let pixel = vec2<u32>(position.xy);
    if pixel.x >= {covered}u * {BLOCK}u {{ discard; }}
    // Blocks repeat across frames larger than COLUMNS blocks.
    let block = pixel / {BLOCK}u % {COLUMNS}u;
    let base = 3u + 4u * (block.y * {COLUMNS}u + block.x);
    let a = blocks[base];
    let b = blocks[base + 1u];
    let c = blocks[base + 2u];
    let d = blocks[base + 3u];
    return scene_gbuffer_encode(
        SceneGbufferSurface(a.xyz, b.xyz, c.xyz, a.w, b.w, c.w, d.x, u32(d.y), u32(d.z)),
        mat3x3<f32>(blocks[0].xyz, blocks[1].xyz, blocks[2].xyz),
    );
}}
",
            covered = self.covered,
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("deferred producer"),
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
            label: Some("deferred producer"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_blocks"),
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
                entry_point: Some("fs_blocks"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("deferred producer blocks"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Ok(Box::new(ProducerDraw { pipeline, group }))
    }
}

impl PreparedFeature for ProducerDraw {
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

pub(crate) fn halves(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|pair| half::f16::from_le_bytes([pair[0], pair[1]]).to_f32())
        .collect()
}

fn lighting(context: &GpuContext, drawn: &Drawn, target: LightingTarget) -> Option<Vec<f32>> {
    drawn
        .renderer
        .feature_target(FeatureTarget::DeferredLighting(target))
        .map(|texture| {
            assert_eq!(texture.format(), HDR);
            assert_eq!([texture.width(), texture.height()], [SIZE; 2]);
            halves(&read(context, texture, HDR))
        })
}

/// Registers a producer of `blocks` over the first `covered` columns of blocks, declaring the
/// scene depth when `frame` has the depth prepass.
fn produce(context: &GpuContext, drawn: &mut Drawn, frame: Frame, blocks: &[Block], covered: u32) {
    let producer = Producer {
        blocks: blocks.iter().map(|block| block.surface).collect(),
        view: Mat3::from_mat4(drawn.frame.view),
        covered,
        depth: frame
            .prepassed()
            .then(|| drawn.renderer.targets().depth().texture().format()),
    };
    drawn
        .renderer
        .renderer_mut()
        .register(context.device(), Box::new(producer))
        .unwrap();
}

/// G3 of the last encoded frame, one code per pixel.
fn stored(context: &GpuContext, drawn: &Drawn) -> Vec<u16> {
    let texture = drawn
        .renderer
        .feature_target(FeatureTarget::Gbuffer(crate::gbuffer::GbufferTarget::G3))
        .expect("G3");
    read(context, texture, texture.format())
        .chunks_exact(2)
        .map(|code| u16::from_le_bytes([code[0], code[1]]))
        .collect()
}

/// Checks that `output` is lit exactly where `g3` is stored (alpha 1, some radiance) and zero
/// everywhere else, and returns the number of lit pixels.
fn lit_where_stored(label: &str, output: &[f32], g3: &[u16], colour: bool) -> usize {
    let mut lit = 0;
    for (pixel, (texel, code)) in output.chunks_exact(4).zip(g3).enumerate() {
        if *code == 0 {
            assert!(
                texel.iter().all(|value| value.to_bits() == 0),
                "{label}: pixel {pixel} is not stored but lit: {texel:?}"
            );
        } else if colour {
            assert_eq!(
                texel[3].to_bits(),
                1.0_f32.to_bits(),
                "{label}: pixel {pixel}"
            );
            assert!(
                texel[..3].iter().any(|value| *value > 0.0),
                "{label}: {pixel}"
            );
            lit += 1;
        }
    }
    lit
}

#[test]
fn deferred_lighting_keeps_the_forward_frame_and_lights_only_written_pixels() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for frame in [Frame::Plain, Frame::Reflecting] {
        let scene = lit_scene(frame);
        let mut forward = prepare_scene(&context, frame, SceneOpaquePath::Forward, &scene);
        let expected = render(&context, &mut forward, 3).unwrap();
        assert!(lighting(&context, &forward, LightingTarget::Colour).is_none());
        let reflections = matches!(frame, Frame::Reflecting);
        for probe in [false, true] {
            let label = format!("{frame:?}, probe {probe}");
            let mut drawn = prepare_scene(
                &context,
                frame,
                SceneOpaquePath::GBUFFER_OBSERVATION,
                &scene,
            );
            if probe {
                let table = drawn.gpu.lighting_record_table().records().to_vec();
                let blocks = blocks(&table, scene.resources().materials());
                produce(&context, &mut drawn, frame, &blocks, COLUMNS / 2);
            }
            let colour = render(&context, &mut drawn, 3).unwrap();
            assert!(colour == expected, "{label}: the observed frame differs");
            let g3 = stored(&context, &drawn);
            for target in [
                LightingTarget::Colour,
                LightingTarget::Environment,
                LightingTarget::Factor,
            ] {
                let output = lighting(&context, &drawn, target);
                if target != LightingTarget::Colour && !reflections {
                    assert!(output.is_none(), "{label}: {target:?} without reflections");
                    continue;
                }
                let output = output.unwrap_or_else(|| panic!("{label}: {target:?} is missing"));
                if !frame.prepassed() {
                    // The lighting pass runs before Opaque on the prepass depth, so without
                    // the prepass it lights nothing, even where the probe stored surfaces.
                    assert!(
                        output.iter().all(|value| value.to_bits() == 0),
                        "{label}: {target:?} lit without the prepass"
                    );
                    continue;
                }
                let lit = lit_where_stored(
                    &format!("{label}, {target:?}"),
                    &output,
                    &g3,
                    target == LightingTarget::Colour,
                );
                if target == LightingTarget::Colour {
                    // The scene's own producers store the terrain; the probe adds its blocks.
                    assert!(lit > g3.len() / 4, "{label}: {lit} lit pixels");
                }
            }
        }
    }
}

#[test]
fn deferred_lighting_refuses_more_than_255_lighting_records_by_name() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let base = Frame::Reflecting.scene();
    let mut materials = base.resources().materials().to_vec();
    materials.extend((0..256).map(|index| leaf(index * 1000, 100_000)));
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: base.terrain().cloned(),
        effects: base.effects().cloned(),
    })
    .unwrap();
    let mut forward = prepare_scene(
        &context,
        Frame::Reflecting,
        SceneOpaquePath::Forward,
        &scene,
    );
    render(&context, &mut forward, 1).unwrap();
    let mut observed = prepare_scene(
        &context,
        Frame::Reflecting,
        SceneOpaquePath::GBUFFER_OBSERVATION,
        &scene,
    );
    assert_eq!(observed.gpu.lighting_record_table().distinct(), 256);
    assert_eq!(
        render(&context, &mut observed, 1).err(),
        Some(FrameError::FeaturePreparation {
            feature: "opaque G-buffer",
            reason: "opaque path needs 256 material records, limit is 255".to_owned(),
        })
    );
}

#[path = "deferred_reference.rs"]
mod reference_pass;

#[test]
fn deferred_lighting_matches_the_forward_shading_of_the_same_surfaces() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // Only a frame with the depth prepass lights; the reflecting frame has one.
    {
        let frame = Frame::Reflecting;
        let scene = lit_scene(frame);
        let mut drawn = prepare_scene(
            &context,
            frame,
            SceneOpaquePath::GBUFFER_OBSERVATION,
            &scene,
        );
        let materials = scene.resources().materials();
        let table = drawn.gpu.lighting_record_table().records().to_vec();
        let blocks = blocks(&table, materials);
        produce(&context, &mut drawn, frame, &blocks, COLUMNS);
        render(&context, &mut drawn, 1).unwrap();
        let deferred = lighting(&context, &drawn, LightingTarget::Colour).unwrap();
        // The values the G-buffer stores, decoded by the CPU mirror of the encoding.
        let view = Mat3::from_mat4(drawn.frame.view);
        let stored: Vec<Block> = blocks
            .iter()
            .map(|block| Block {
                surface: reference::decode(
                    &reference::encode(&block.surface, view),
                    view.transpose(),
                ),
                ..*block
            })
            .collect();
        let pass = reference_pass::ReferencePass::new(&context, &mut drawn, frame, materials);
        let exact = pass.shade(&context, &drawn, &blocks);
        let decoded = pass.shade(&context, &drawn, &stored);
        let neutral: Vec<Block> = blocks
            .iter()
            .map(|block| Block {
                material: 0,
                ..*block
            })
            .collect();
        let unrecorded = pass.shade(&context, &drawn, &neutral);
        // The largest error relative to the reference, counting values below the absolute
        // floor as that floor: an error passes when it is within `relative` of the reference
        // or within `ABSOLUTE`.
        let worst = |reference: &[f32], relative: f32| {
            deferred
                .chunks_exact(4)
                .zip(reference.chunks_exact(4))
                .flat_map(|(got, want)| got[..3].iter().zip(&want[..3]))
                .map(|(got, want)| (got - want).abs() / want.abs().max(ABSOLUTE / relative))
                .fold(0.0_f32, f32::max)
        };
        let stored_error = worst(&decoded, STORED_RELATIVE);
        let exact_error = worst(&exact, EXACT_RELATIVE);
        // The leaf records change the forward result, so a lighting pass that ignored them
        // could not meet the stored-value bound.
        let record_effect = exact
            .chunks_exact(4)
            .zip(unrecorded.chunks_exact(4))
            .map(|(leaf, plain)| (leaf[0] - plain[0]).abs() / plain[0].abs().max(1e-3))
            .fold(0.0_f32, f32::max);
        let distinct: std::collections::BTreeSet<_> = exact
            .chunks_exact(4)
            .map(|texel| {
                texel[..3]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            })
            .collect();
        eprintln!(
            "{frame:?}: stored-value error {stored_error:.5}, exact-surface error {exact_error:.5}, \
             record effect {record_effect:.3}, {} distinct reference colours",
            distinct.len()
        );
        assert!(
            distinct.len() >= 16,
            "{frame:?}: the reference lit too few surfaces"
        );
        assert!(
            record_effect > 0.05,
            "{frame:?}: the leaf records changed nothing"
        );
        assert!(stored_error <= STORED_RELATIVE, "{frame:?}: {stored_error}");
        assert!(exact_error <= EXACT_RELATIVE, "{frame:?}: {exact_error}");
    }
}
