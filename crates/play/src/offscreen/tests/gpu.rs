//! A synthetic world with a camera render, a blit and a copy, through the adapter and
//! render-program's offscreen state on a real adapter (lavapipe in CI), with predicted pixels.
//!
//! The shaders are the synthetic player shaders of render-program (a camera shader tagged
//! `Alpha` that fills with the material's `_Tint` where `_Place` puts the quad, and three blit
//! passes: tint, texel size, inversion). Every expected pixel comes from the placement, the
//! colors and the inversion, not from the output.

use glam::{Quat, Vec3};
use worldrunner_disunity_runtime::{
    MeshChannel, MeshChannels, MeshRange, PlayerTexture, TextureSampler,
};
use worldrunner_render_gpu::configuration::{AllocationBudget, AllocationSet, Configuration};
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_render_program::unity::offscreen::{
    OffscreenOperations, OffscreenState, PendingImage,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    BLIT, blit_parameters, blit_passes, camera_passes, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, SyntheticShader, shader as synthetic_shader,
};
use worldrunner_unity_serialized::shader::ShaderProperty;
use worldrunner_world_simulation::graphics::{
    MaterialEdit, MaterialId, OffscreenWork, PropertyValue, RenderTextureId, ShaderProperties,
    ShaderSource, TextureRef, WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, World};

use super::super::adapter::{OffscreenAdapter, build};
use super::super::assets::{AssetError, Assets};
use super::super::defaults::{ShaderFill, declared};
use super::super::frame::{Adapted, Settings};
use super::super::shaders::{LoadedShader, ShaderLoadError, ShaderSupply};
use super::support::{camera, render_texture, renderer_with, world};
use worldrunner_world_simulation::graphics::GraphicsSnapshot;

pub(super) const MESH: u64 = (1 << 41) + 1;
pub(super) const SIDE: u32 = 4;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CYAN: [u8; 4] = [0, 255, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const WHITE: [u8; 4] = [255; 4];

/// Shaders by the file their source names: `camera` and `blit`.
pub(super) struct Synthetic;

impl ShaderSupply for Synthetic {
    fn load(&mut self, source: &ShaderSource) -> Result<LoadedShader, ShaderLoadError> {
        let synthetic = match source.file.as_str() {
            "camera" => camera_passes(),
            "untagged" => {
                let mut synthetic = camera_passes();
                for pass in &mut synthetic.shader.parsed.sub_shaders[0].passes {
                    pass.tags.clear();
                    pass.state.tags.clear();
                }
                synthetic
            }
            "blit" => blit_passes("_MainTex"),
            "gradient" => gradient_passes(),
            // The camera shader with `_Albedo` declared a 2D texture property of the default
            // named after the file.
            "default_white" | "default_black" | "default_bump" => {
                let mut synthetic = camera_passes();
                let name = source.file.trim_start_matches("default_");
                synthetic.shader.parsed.properties = vec![ShaderProperty {
                    name: "_Albedo".to_owned(),
                    description: String::new(),
                    attributes: Vec::new(),
                    kind: 4,
                    flags: 0,
                    default_value: [0.0; 4],
                    default_texture: name.to_owned(),
                    texture_dimension: 2,
                }];
                synthetic
            }
            _ => return Err(ShaderLoadError::NoVulkanPlatform),
        };
        let texture_defaults = declared(&synthetic.shader);
        let program = ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap();
        Ok(LoadedShader {
            passes: program.pass_count(),
            program,
            queue: 2000,
            texture_defaults,
            declared_defaults: Vec::new(),
            fill: ShaderFill::default(),
        })
    }
}

/// A fragment stage that writes the texture coordinates as red and green over blue and alpha of
/// one, and one that writes the tint. The blit's texture and tint are read with a weight of zero
/// so that both passes bind what the blit's other passes do.
const GRADIENT: &str = "
@fragment fn fs_gradient(in: VOut) -> @location(0) vec4<f32> {
    let unused = textureSample(main_tex, main_sampler, in.uv) * material.tint * 0.0;
    return unused + vec4<f32>(in.uv.x, in.uv.y, 1.0, 1.0);
}
@fragment fn fs_flat(in: VOut) -> @location(0) vec4<f32> {
    let unused = textureSample(main_tex, main_sampler, in.uv) * 0.0;
    return unused + material.tint;
}
";

/// A blit shader of two passes: 0 writes the texture coordinates as red and green, 1 the tint.
fn gradient_passes() -> SyntheticShader {
    let wgsl = format!("{BLIT}{GRADIENT}");
    no_cull(synthetic_shader(
        &[],
        &["fs_gradient", "fs_flat"].map(|fragment| PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], &wgsl, fragment, blit_parameters("_MainTex"))],
        }),
    ))
}

/// One quad of two triangles over clip space at depth 0.5, as the managed engine would store it.
pub(super) struct Quad;

impl Assets for Quad {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        if identity != MESH || known != 0 {
            return Ok(None);
        }
        Ok(Some(MeshChannels {
            generation: 1,
            vertex_count: 4,
            channels: vec![
                MeshChannel {
                    number: 0,
                    dimension: 3,
                    data: vec![
                        -1.0, -1.0, 0.5, 1.0, -1.0, 0.5, -1.0, 1.0, 0.5, 1.0, 1.0, 0.5,
                    ],
                },
                MeshChannel {
                    number: 4,
                    dimension: 2,
                    data: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                },
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            ranges: vec![MeshRange {
                start: 0,
                count: 6,
                base_vertex: 0,
                topology: 0,
            }],
        }))
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// The quad without its texture coordinates.
pub(super) struct BareQuad;

impl Assets for BareQuad {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        let mut mesh = Quad.mesh(identity, known)?;
        if let Some(mesh) = &mut mesh {
            mesh.channels.retain(|channel| channel.number != 4);
        }
        Ok(mesh)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Quad.texture(identity, known)
    }

    fn sampler(&self, identity: u64) -> Result<Option<TextureSampler>, AssetError> {
        Quad.sampler(identity)
    }
}

/// A device with its submission authority, a program runtime and the arena the renderer
/// reserved for it.
pub(super) struct Gpu {
    pub(super) device: wgpu::Device,
    pub(super) authority: SubmissionAuthority,
    pub(super) runtime: ProgramRuntime,
    pub(super) arena: ProgramArena,
}

pub(super) fn gpu() -> Gpu {
    gpu_with_budget(64 << 20)
}

/// A device whose runtime may hold `budget` bytes of storage.
pub(super) fn gpu_with_budget(budget: u64) -> Gpu {
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("the offscreen tests need a GPU adapter");
    // Block compression where offered, as the player asks for it.
    let requirements = DeviceRequirements {
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..DeviceRequirements::default()
    };
    let (device, queue, _) = pollster::block_on(request_device(&adapter, &requirements)).unwrap();
    let authority = SubmissionAuthority::new(queue);
    let budget = AllocationBudget(budget);
    let runtime = ProgramRuntime::new(device.clone(), authority.clone(), budget);
    let limits = device.limits();
    let mut renderer = Configuration::new(
        &authority,
        (),
        AllocationSet::default(),
        &limits,
        AllocationBudget(1 << 30),
    )
    .unwrap();
    let reservation = renderer
        .reserve::<()>(&limits, ProgramRuntime::arena_allocations(budget.0))
        .unwrap();
    let arena = runtime.open_arena(reservation).unwrap();
    Gpu {
        device,
        authority,
        runtime,
        arena,
    }
}

pub(super) struct Rig {
    pub(super) device: wgpu::Device,
    authority: SubmissionAuthority,
    pub(super) runtime: ProgramRuntime,
    pub(super) arena: ProgramArena,
    pub(super) state: OffscreenState,
    pub(super) adapter: OffscreenAdapter,
}

impl Rig {
    pub(super) fn new() -> Self {
        let Gpu {
            device,
            authority,
            runtime,
            arena,
        } = gpu();
        let state = OffscreenState::new(&runtime);
        let settings = Settings {
            light_modes: vec!["Alpha".to_owned()],
            ..Settings::default()
        };
        Self {
            device,
            authority,
            runtime,
            arena,
            state,
            adapter: OffscreenAdapter::with_supply(Synthetic, settings),
        }
    }

    /// Takes the world's frame, adapts it and builds it, with a readback of `reads` after the
    /// last operation.
    fn frame(
        &mut self,
        world: &mut World,
        reads: &[RenderTextureId],
    ) -> (Adapted, OffscreenOperations, Vec<PendingImage>) {
        let snapshot = GraphicsSnapshot::take(world).unwrap();
        let mut adapted = self
            .adapter
            .adapt(&snapshot, &Quad, &mut self.state, &mut self.runtime);
        let mut operations = build(
            &mut self.state,
            &mut self.runtime,
            &self.arena,
            &mut adapted,
        )
        .unwrap();
        let pending = reads
            .iter()
            .map(|id| {
                let name = super::super::ids::render_texture(*id);
                operations.read_back(name, 0, 0).unwrap()
            })
            .collect();
        (adapted, operations, pending)
    }

    /// Runs the operations as the one submission of a frame.
    pub(super) fn run(&self, operations: OffscreenOperations) {
        let mut encoder = self.authority.begin(&self.device).unwrap();
        let admitted = operations.admit(&encoder).unwrap();
        admitted.encode(None, &mut encoder).unwrap();
        encoder.finish().submit().unwrap();
    }
}

pub(super) fn shader(
    world: &mut World,
    file: &str,
) -> worldrunner_world_simulation::graphics::ShaderId {
    world
        .create_shader(WorldShader {
            name: file.into(),
            source: Some(ShaderSource {
                file: file.into(),
                path_id: 1,
            }),
        })
        .unwrap()
}

pub(super) fn vector(world: &mut World, material: MaterialId, name: &str, value: [f32; 4]) {
    world
        .edit_material(
            material,
            MaterialEdit::Property(PropertyValue::Vector(name.into(), value)),
        )
        .unwrap();
}

pub(super) fn target(world: &mut World, format: u32) -> RenderTextureId {
    target_with_mips(world, format, 1)
}

fn target_with_mips(world: &mut World, format: u32, mip_count: u32) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format,
            depth_bits: 0,
            mip_count,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn pixel(bytes: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
    bytes[(y * SIDE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

/// The blit and the copy of the scene: A to B through every pass of the blit shader, B to C.
fn blit_and_copy(
    world: &mut World,
    (a, b, c): (RenderTextureId, RenderTextureId, RenderTextureId),
) {
    let blit = shader(world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(world, material, "_Tint", [1.0; 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(a),
            target: b,
            material: Some(material),
            pass: -1,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::CopyTexture {
            source: TextureRef::Render(b),
            source_element: 0,
            source_mip: 0,
            target: c,
            target_element: 0,
            target_mip: 0,
        })
        .unwrap();
}

/// A camera that clears to blue and draws a red quad over the left half of its target, through
/// a renderer whose property block places the quad.
pub(super) fn camera_render(world: &mut World, into: RenderTextureId) {
    let camera = camera(
        world,
        Camera {
            background: [0.0, 0.0, 1.0, 1.0],
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let shader = shader(world, "camera");
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", [1.0, 0.0, 0.0, 1.0]);
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector(
            "_Place".into(),
            [0.5, 1.0, -0.5, 0.0],
        ))
        .unwrap();
    let placed = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, placed);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: into,
        })
        .unwrap();
}

#[test]
fn a_camera_a_blit_of_every_pass_and_a_copy_run_with_the_players_shaders() {
    let mut rig = Rig::new();
    let mut world = world();
    let (a, b, c) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    camera_render(&mut world, a);
    blit_and_copy(&mut world, (a, b, c));
    let (adapted, operations, reads) = rig.frame(&mut world, &[a, b, c]);
    // The camera, one blit record for each of the three passes, and the copy.
    assert_eq!((adapted.report.records, adapted.report.built), (3, 5));
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    rig.run(operations);
    let [a, b, c]: [Vec<u8>; 3] = reads
        .into_iter()
        .map(|image| image.bytes(&rig.device))
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    // The camera: the quad covers clip x from -1 to 0, columns 0 and 1; the rest is the clear color.
    for y in 0..SIDE as usize {
        assert_eq!(pixel(&a, (0, y)), RED);
        assert_eq!(pixel(&a, (1, y)), RED);
        assert_eq!(pixel(&a, (2, y)), BLUE);
        assert_eq!(pixel(&a, (3, y)), BLUE);
    }
    // The blit ran its passes in order: the last, the inversion, is what is left. Run the other
    // way round, the tint pass would leave the source unchanged.
    for y in 0..SIDE as usize {
        assert_eq!(pixel(&b, (1, y)), CYAN);
        assert_eq!(pixel(&b, (2, y)), YELLOW);
    }
    // The copy.
    assert_eq!(c, b);
}

#[test]
fn a_render_texture_with_a_mip_chain_is_sampled_at_the_level_the_camera_wrote() {
    let mut rig = Rig::new();
    let mut world = world();
    let a = target_with_mips(&mut world, 8, 3);
    let (b, c) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    camera_render(&mut world, a);
    blit_and_copy(&mut world, (a, b, c));
    let (adapted, operations, reads) = rig.frame(&mut world, &[b, c]);
    assert_eq!(adapted.report.failures["draw:SampledWrittenLevelsOnly"], 3);
    assert!(
        !adapted
            .report
            .failures
            .keys()
            .any(|name| name.starts_with("record:")),
        "{:?}",
        adapted.report.failures
    );
    rig.run(operations);
    let mut images = reads.into_iter().map(|image| image.bytes(&rig.device));
    let (b, c) = (images.next().unwrap(), images.next().unwrap());
    // As with a single level: the last blit pass inverts what the camera drew.
    for y in 0..SIDE as usize {
        assert_eq!(pixel(&b, (1, y)), CYAN);
        assert_eq!(pixel(&b, (2, y)), YELLOW);
    }
    assert_eq!(c, b);
}

#[test]
fn a_renderer_whose_shader_has_no_pass_for_the_camera_is_not_drawn_and_is_counted() {
    let mut rig = Rig::new();
    let mut world = world();
    let target = render_texture(&mut world, [SIDE, SIDE]);
    camera_render(&mut world, target);
    // The blit shader has no `Alpha` pass, so the camera has nothing to draw of it.
    let blit = shader(&mut world, "blit");
    let material = world.create_material(blit).unwrap();
    let ignored = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        ..MeshRenderer::default()
    };
    renderer_with(&mut world, (0, true), MESH, ignored);
    let (adapted, operations, reads) = rig.frame(&mut world, &[target]);
    assert_eq!(adapted.report.failures["draw:NoPassForCamera"], 1);
    assert_eq!(adapted.report.built, 1, "the camera render itself");
    rig.run(operations);
    let image = reads.into_iter().next().unwrap().bytes(&rig.device);
    for y in 0..SIDE as usize {
        assert_eq!(pixel(&image, (0, y)), RED);
        assert_eq!(pixel(&image, (1, y)), RED);
        assert_eq!(pixel(&image, (2, y)), BLUE);
    }
}

#[test]
fn a_render_texture_the_renderer_cannot_draw_into_costs_its_records_and_nothing_else() {
    let mut rig = Rig::new();
    let mut world = world();
    let (a, b, c) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
    );
    // Format 7 (RGB8) is not one render-program draws into.
    let odd = target(&mut world, 7);
    camera_render(&mut world, odd);
    blit_and_copy(&mut world, (a, b, c));
    let (adapted, operations, reads) = rig.frame(&mut world, &[b, c]);
    assert_eq!(
        adapted.report.failures["rendertexture:UnsupportedFormat"],
        1
    );
    assert_eq!(adapted.report.failures["record:UnusableRenderTexture"], 1);
    assert_eq!(adapted.report.causes["rendertexture:UnsupportedFormat"], 1);
    assert_eq!(
        adapted.report.examples["rendertexture:UnsupportedFormat"],
        "GraphicsFormat 7 is not one this renderer draws into or samples"
    );
    assert_eq!(
        adapted.report.built, 4,
        "the three blit passes and the copy"
    );
    rig.run(operations);
    let mut images = reads.into_iter().map(|image| image.bytes(&rig.device));
    let (b, c) = (images.next().unwrap(), images.next().unwrap());
    // A was never drawn into, so it is zero, and the inversion of zero rgb with alpha 1 is white.
    for y in 0..SIDE as usize {
        for x in 0..SIDE as usize {
            assert_eq!(pixel(&b, (x, y)), WHITE);
        }
    }
    assert_eq!(c, b);
}

#[test]
fn a_record_that_does_not_build_on_its_own_is_dropped_and_counted_by_the_error() {
    let mut rig = Rig::new();
    let mut world = world();
    let (a, b, c) = (
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [SIDE, SIDE]),
        render_texture(&mut world, [8, 8]),
    );
    blit_and_copy(&mut world, (a, b, c));
    // The copy of a 4 by 4 texture into an 8 by 8 one cannot be done.
    let (adapted, operations, reads) = rig.frame(&mut world, &[b]);
    assert_eq!(adapted.report.failures["record:CopyMismatch"], 1);
    assert_eq!(adapted.report.built, 3);
    rig.run(operations);
    let b = reads.into_iter().next().unwrap().bytes(&rig.device);
    assert_eq!(pixel(&b, (0, 0)), WHITE);
}

/// The quad's assets, and one 1x1 texture under `HELD` that reads as unchanged once its
/// generation is known, as the engine's store does.
struct HeldTexture;

const HELD: u64 = 1 << 40;

impl Assets for HeldTexture {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        if identity != HELD || known == 1 {
            return Ok(None);
        }
        Ok(Some(PlayerTexture {
            format: 4,
            width: 1,
            height: 1,
            mips: 1,
            linear: false,
            generation: 1,
            distance_spread: 0,
            bytes: vec![255; 4],
        }))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

#[test]
fn a_stored_texture_the_engine_does_not_hold_is_counted_each_frame_and_one_it_does_is_not() {
    use super::support::{material, set, snapshot, texture_slot};
    use worldrunner_world_simulation::graphics::PropertyValue as WorldValue;
    let mut rig = Rig::new();
    let mut world = world();
    let blit = shader(&mut world, "blit");
    let material = material(&mut world, blit);
    for (name, identity) in [("_Held", HELD), ("_Other", 1 << 41)] {
        set(
            &mut world,
            material,
            WorldValue::Texture(name.into(), texture_slot(TextureRef::Asset(identity))),
        );
    }
    let target = render_texture(&mut world, [SIDE, SIDE]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(target),
            target,
            material: Some(material),
            pass: 0,
        })
        .unwrap();
    let snapshot = snapshot(&mut world);
    // The second frame finds the held texture unchanged, which is not a texture missing.
    for _ in 0..2 {
        let adapted = rig
            .adapter
            .adapt(&snapshot, &HeldTexture, &mut rig.state, &mut rig.runtime);
        assert_eq!(adapted.report.failures.get("texture:NotStored"), Some(&1));
    }
}
