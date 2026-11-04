//! The fixture behind the player program frame tests: a configured compositor, a resident
//! unlit scene, a player program runtime sharing the compositor's submission authority, and
//! two patches of that program to draw.

use super::super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{AttachmentView, FrameError, ResourceRef};
use worldrunner_render_program::module::{ResourceBinding, StageKind, StageProgram};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{
    AdmittedPrograms, BoundResource, DrawPacket, Geometry, GraphicsProgram, ProgramArena,
    ProgramRuntime, TargetSignature, VertexInput,
};
use worldrunner_render_program::state::RenderState;
use worldrunner_unity_serialized::shader_program::{
    Binding, ConstantBuffer, ProgramParameters, ValueParam,
};

/// Bytes the program arena reserves, and the cap of the runtime that uses it.
pub(super) const ARENA: u64 = 4096;

pub(super) const RED: [u8; 4] = [255, 0, 0, 255];
pub(super) const GREEN: [u8; 4] = [0, 255, 0, 255];
pub(super) const BLUE: [u8; 4] = [0, 0, 255, 255];
pub(super) const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Clip depth of the patch in front of the engine's card, and of the patch behind it.
pub(super) const NEAR_DEPTH: f32 = 0.5;
pub(super) const FAR_DEPTH: f32 = 0.99;

/// The card stands 3 m in front of the eye. With the 0.1 m near plane and 1000 m far plane the
/// standard projection maps a view distance `d` to `f * (d - n) / (d * (f - n))`.
pub(super) fn engine_depth() -> f32 {
    let (near, far, distance) = (0.1_f32, 1000.0_f32, 3.0_f32);
    far * (distance - near) / (distance * (far - near))
}

/// Where the card lands: it spans 2 m by 2 m in a 60 degree view from 3 m, centered on the
/// eye, so it covers `1 / (3 tan 30 degrees)` = 0.577 of the half extent on every side.
pub(super) const CARD_EDGE: f64 = 0.577_35;

const WGSL: &str = "
struct Patch { color: vec4<f32> }
@group(0) @binding(0) var<uniform> fill: Patch;
@vertex fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return fill.color; }";

const COLOR_NAME: i32 = 0;
const BUFFER_NAME: i32 = 1;

fn spirv() -> Vec<u32> {
    use naga::back::spv;
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    let module = naga::front::wgsl::parse_str(WGSL).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    // A player's module is written for Vulkan, so its position output is not flipped.
    let options = spv::Options {
        flags: spv::WriterFlags::empty(),
        ..spv::Options::default()
    };
    spv::write_vec(&module, &info, &options, None).unwrap()
}

/// The fragment stage's parameters: one constant buffer at binding 0 holding `_PatchColor`.
fn parameters() -> ProgramParameters {
    ProgramParameters {
        constant_buffers: vec![ConstantBuffer {
            name_index: BUFFER_NAME,
            matrices: vec![],
            vectors: vec![ValueParam {
                name_index: COLOR_NAME,
                index: 0,
                array_size: 1,
                kind: 0,
                dimension: 4,
            }],
            structs: vec![],
            size: 16,
            partial: false,
        }],
        constant_buffer_bindings: vec![Binding {
            name_index: BUFFER_NAME,
            index: 0,
            array_size: 1,
        }],
        ..ProgramParameters::default()
    }
}

fn stage(entry: &str, kind: StageKind, parameters: ProgramParameters) -> StageProgram {
    let module = worldrunner_render_program::module::StageModule {
        words: spirv(),
        entry_point: entry.to_owned(),
        kind,
        bindings: vec![ResourceBinding {
            set: 0,
            unity_binding: 0,
            binding: 0,
            sampler_binding: None,
        }],
    };
    StageProgram::new(module, parameters).unwrap()
}

/// A rectangle of the program: its vertices, which carry the depth, and its color constants.
pub(super) struct Patch {
    input: VertexInput,
    bindings: Vec<BoundResource>,
}

pub(super) struct Fixture {
    pub context: GpuContext,
    pub renderer: ConfiguredFrameRenderer,
    pub gpu: SceneGpu,
    pub pipelines: ScenePipelines,
    pub runtime: ProgramRuntime,
    pub arena: ProgramArena,
    program: GraphicsProgram,
    patches: Vec<Patch>,
    output: Texture,
}

/// The red engine card: a 2 m card on the ground plane standing at the origin, facing the eye.
pub(super) fn engine_scene() -> RenderScene {
    build(
        texture(RED_565),
        &material(SceneAlphaMode::Opaque, false),
        card(None),
        1,
    )
}

impl Fixture {
    /// The engine's red card under `settings` and the pipelines `pipelines` builds.
    pub fn new(
        settings: RendererConfiguration,
        pipelines: impl FnOnce(&Device) -> ScenePipelines,
    ) -> Self {
        Self::with_scene(&engine_scene(), settings, pipelines)
    }

    pub fn with_scene(
        scene: &RenderScene,
        settings: RendererConfiguration,
        pipelines: impl FnOnce(&Device) -> ScenePipelines,
    ) -> Self {
        Self::with_context(context(), scene, settings, pipelines)
    }

    /// The same fixture on a device the caller opened, for tests that need a feature the shared
    /// helper does not request.
    pub fn with_context(
        context: GpuContext,
        scene: &RenderScene,
        settings: RendererConfiguration,
        pipelines: impl FnOnce(&Device) -> ScenePipelines,
    ) -> Self {
        let device = context.device();
        let mut renderer =
            ConfiguredFrameRenderer::new(&context, settings, AllocationBudget(1 << 26)).unwrap();
        let pipelines = pipelines(device);
        let mut gpu = SceneGpu::upload(device, context.submission(), scene, &pipelines).unwrap();
        gpu.post = renderer.configuration().post;
        let reservation = renderer
            .reserve(&context, ProgramRuntime::arena_allocations(ARENA))
            .unwrap();
        let mut runtime = ProgramRuntime::new(
            device.clone(),
            context.submission().clone(),
            AllocationBudget(ARENA),
        );
        let arena = runtime.open_arena(reservation).unwrap();
        let vertex = stage("vs_main", StageKind::Vertex, ProgramParameters::default());
        let fragment = stage("fs_main", StageKind::Fragment, parameters());
        let program = runtime.graphics_program(&vertex, &fragment).unwrap();
        // Clip Y grows toward the bottom of the image. The near patch is right of center and
        // overlaps the card's right edge; the far patch is lower left and overlaps its corner.
        let patches = vec![
            patch(
                &mut runtime,
                &fragment,
                [0.2, -0.2, 0.8, 0.2],
                NEAR_DEPTH,
                [0.0, 1.0, 0.0, 1.0],
            ),
            patch(
                &mut runtime,
                &fragment,
                [-0.8, 0.4, -0.2, 0.8],
                FAR_DEPTH,
                [0.0, 0.0, 1.0, 1.0],
            ),
        ];
        let output = device.create_texture(&TextureDescriptor {
            label: Some("program frame output"),
            size: Extent3d {
                width: TARGET,
                height: TARGET,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Self {
            context,
            renderer,
            gpu,
            pipelines,
            runtime,
            arena,
            program,
            patches,
            output,
        }
    }

    /// The signature of a single-sample opaque pass in the scene's color format.
    pub fn signature(&self) -> TargetSignature {
        TargetSignature::new(&[self.pipelines.color_format], Some(DEPTH_FORMAT), 1).unwrap()
    }

    /// Replaces the arena with a fresh one over a new reservation and drops the old arena, as a
    /// host does when it lets go of the arena the admitted draws came from.
    pub fn drop_arena(&mut self) {
        let reservation = self
            .renderer
            .reserve(&self.context, ProgramRuntime::arena_allocations(ARENA))
            .unwrap();
        let fresh = self.runtime.open_arena(reservation).unwrap();
        drop(std::mem::replace(&mut self.arena, fresh));
    }

    /// Admits both patches, in the order near then far, into the frame being recorded.
    pub fn admit(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        signature: TargetSignature,
    ) -> Result<AdmittedPrograms, worldrunner_render_program::error::ProgramError> {
        self.admit_geometry(
            encoder,
            signature,
            Geometry::Vertices { first: 0, count: 6 },
        )
    }

    /// Admits both patches drawn with `geometry`.
    pub fn admit_geometry(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        signature: TargetSignature,
        geometry: Geometry,
    ) -> Result<AdmittedPrograms, worldrunner_render_program::error::ProgramError> {
        let state = RenderState {
            cull: None,
            depth_compare: wgpu::CompareFunction::LessEqual,
            depth_write: true,
            color_write: wgpu::ColorWrites::ALL,
            blend: None,
        };
        let packets: Vec<_> = self
            .patches
            .iter()
            .map(|patch| DrawPacket {
                program: &self.program,
                state: &state,
                front_face: wgpu::FrontFace::Cw,
                vertex_inputs: std::slice::from_ref(&patch.input),
                bindings: &patch.bindings,
                geometry,
            })
            .collect();
        self.runtime
            .admit_draws(encoder, &self.arena, signature, &packets)
    }

    /// Encodes the resident scene through the configured compositor.
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) -> Result<(), FrameError> {
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            self.renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            self.renderer.targets().depth().texture(),
        )
        .unwrap();
        self.renderer.encoding().encode_scene(
            self.context.device(),
            encoder,
            (&mut self.gpu, &self.pipelines),
            (&color, &depth),
            frame(),
        )
    }

    /// Resolves the scene, runs the stock output stage and records a copy of the output into a
    /// mappable buffer, all in the frame being recorded.
    pub fn output_copy(&self, encoder: &mut wgpu::CommandEncoder) -> Buffer {
        self.renderer.targets().encode_resolve(encoder);
        let view = self.output.create_view(&TextureViewDescriptor::default());
        self.renderer.encode_output(encoder, &view).unwrap();
        self.copy_output(encoder)
    }

    /// Records a copy of the output texture as it is, without drawing anything.
    pub fn copy_output(&self, encoder: &mut wgpu::CommandEncoder) -> Buffer {
        self.copy(&self.output, encoder)
    }

    /// Records a copy of the scene's color target as it is, without drawing anything.
    pub fn copy_scene(&self, encoder: &mut wgpu::CommandEncoder) -> Buffer {
        self.copy(self.renderer.targets().texture(), encoder)
    }

    fn copy(&self, texture: &Texture, encoder: &mut wgpu::CommandEncoder) -> Buffer {
        // The rows are 256 byte aligned at 64 pixels of any format the fixtures use.
        let row = TARGET * texture.format().block_copy_size(None).unwrap();
        let readback = self.context.device().create_buffer(&BufferDescriptor {
            label: Some("program frame readback"),
            size: u64::from(row * TARGET),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(TARGET),
                },
            },
            Extent3d {
                width: TARGET,
                height: TARGET,
                depth_or_array_layers: 1,
            },
        );
        readback
    }

    pub fn read(&self, readback: &Buffer) -> Image {
        Image(map_pixels(self.context.device(), readback))
    }
}

/// The RGBA bytes of an output readback.
pub(super) struct Image(Vec<u8>);

impl Image {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = (y * TARGET + x) as usize * 4;
        [self.0[at], self.0[at + 1], self.0[at + 2], self.0[at + 3]]
    }
}

/// The camera: 3 m south of the card, level with its middle, looking north.
pub(super) fn view() -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -3_000, 1_000],
            target_millimeters: [0, 0, 1_000],
        },
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap()
}

pub(super) fn frame() -> FrameUniform {
    FrameUniform::unlit(&view())
}

fn patch(
    runtime: &mut ProgramRuntime,
    fragment: &StageProgram,
    [x0, y0, x1, y1]: [f32; 4],
    depth: f32,
    color: [f32; 4],
) -> Patch {
    let corners = [
        [x0, y0, depth],
        [x1, y0, depth],
        [x1, y1, depth],
        [x0, y0, depth],
        [x1, y1, depth],
        [x0, y1, depth],
    ];
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let input = VertexInput {
        buffer: runtime.create_buffer("patch", &bytes).unwrap(),
        stride: 12,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    };
    let mut properties = PropertySet::new();
    properties.set("_PatchColor", PropertyValue::Vector(color));
    let names = [("_PatchColor".to_owned(), COLOR_NAME)];
    let target = fragment.constant_buffer_targets().unwrap().remove(0);
    let packed = worldrunner_render_program::constants::pack_layout(
        &target.buffer,
        &names,
        &[&properties],
        16,
    )
    .unwrap();
    let buffer = runtime.create_buffer("patch color", &packed.bytes).unwrap();
    Patch {
        input,
        bindings: vec![BoundResource::buffer(target.set, target.binding, buffer)],
    }
}
