//! Values the tests build the way Unity's serialized shader describes them.

use worldrunner_unity_serialized::shader_state::{
    Depth, Fog, ShaderState, StateValue, Stencil, StencilOps, TargetBlend,
};

pub fn fixed(value: f32) -> StateValue {
    StateValue {
        value,
        property: None,
    }
}

pub fn named(value: f32, property: &str) -> StateValue {
    StateValue {
        value,
        property: Some(property.to_owned()),
    }
}

fn stencil_ops() -> StencilOps {
    StencilOps {
        pass: fixed(0.0),
        fail: fixed(0.0),
        z_fail: fixed(0.0),
        compare: fixed(8.0),
    }
}

fn target_blend() -> TargetBlend {
    TargetBlend {
        src: fixed(1.0),
        dst: fixed(0.0),
        src_alpha: fixed(1.0),
        dst_alpha: fixed(0.0),
        op: fixed(0.0),
        op_alpha: fixed(0.0),
        color_mask: fixed(15.0),
    }
}

/// A pass with no blending, back-face culling, `ZTest LEqual` and `ZWrite On`.
pub fn opaque_state() -> ShaderState {
    ShaderState {
        name: String::new(),
        blend: (0..8).map(|_| target_blend()).collect(),
        separate_blend: false,
        depth: Depth {
            clip: fixed(1.0),
            test: fixed(4.0),
            write: fixed(1.0),
            cull: fixed(2.0),
            conservative: fixed(0.0),
            offset_factor: fixed(0.0),
            offset_units: fixed(0.0),
            alpha_to_mask: fixed(0.0),
        },
        stencil: Stencil {
            both: stencil_ops(),
            front: stencil_ops(),
            back: stencil_ops(),
            read_mask: fixed(255.0),
            write_mask: fixed(255.0),
            reference: fixed(0.0),
        },
        fog: Fog {
            start: fixed(0.0),
            end: fixed(0.0),
            density: fixed(0.0),
            color: [fixed(0.0), fixed(0.0), fixed(0.0), fixed(0.0)],
            color_property: None,
            mode: 0,
        },
        gpu_program_id: 0,
        tags: Vec::new(),
        lod: 0,
        lighting: false,
    }
}

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use worldrunner_render_program::module::{ModuleBinding, StageKind, StageModule, StageProgram};
use worldrunner_unity_serialized::shader_program::{
    Binding, ProgramParameters, SamplerParam, TextureParam,
};

/// SPIR-V for a WGSL module, written as a Vulkan shader: the position output is not flipped,
/// as in the modules Unity ships.
pub fn spirv(wgsl: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(wgsl).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    let options = spv::Options {
        flags: spv::WriterFlags::empty(),
        ..spv::Options::default()
    };
    spv::write_vec(&module, &info, &options, None).unwrap()
}

/// A binding table entry whose prepared binding is Unity's.
pub fn same(set: u32, binding: u32) -> ModuleBinding {
    ModuleBinding {
        set,
        unity_binding: binding,
        binding,
        sampler_binding: None,
    }
}

pub fn binding(index: i32) -> Binding {
    Binding {
        name_index: index,
        index,
        array_size: 1,
    }
}

pub fn texture(index: i32, dimension: i8) -> TextureParam {
    TextureParam {
        name_index: index,
        index,
        sampler_index: -1,
        multisampled: false,
        dimension,
    }
}

pub fn sampler(bind_point: i32) -> SamplerParam {
    SamplerParam {
        sampler: 0,
        bind_point,
    }
}

pub fn parameters(
    constant_buffers: &[i32],
    textures: Vec<TextureParam>,
    samplers: Vec<SamplerParam>,
) -> ProgramParameters {
    ProgramParameters {
        constant_buffer_bindings: constant_buffers.iter().map(|i| binding(*i)).collect(),
        textures,
        samplers,
        ..ProgramParameters::default()
    }
}

pub fn stage(
    wgsl: &str,
    (entry_point, kind): (&str, StageKind),
    bindings: Vec<ModuleBinding>,
    parameters: ProgramParameters,
) -> StageProgram {
    let module = StageModule {
        words: spirv(wgsl),
        entry_point: entry_point.to_owned(),
        kind,
        bindings,
    };
    StageProgram::new(module, parameters).unwrap()
}

use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{ProgramRuntime, TargetHandle, TargetImage};

/// The device and submission authority every runtime in a test shares, obtained the way the
/// renderer's own crates obtain them.
pub struct Gpu {
    pub device: wgpu::Device,
    pub authority: SubmissionAuthority,
}

impl Gpu {
    /// A device on the adapter the environment selects: lavapipe in CI, or a hardware GPU.
    pub fn new() -> Self {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("the program tests need a GPU adapter");
        announce_adapter(&adapter);
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
                .expect("the adapter gives a device");
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
        }
    }

    pub fn runtime(&self) -> ProgramRuntime {
        ProgramRuntime::new(self.device.clone(), self.authority.clone())
    }
}

/// A runtime on its own device and authority.
pub fn runtime() -> ProgramRuntime {
    Gpu::new().runtime()
}

/// Records `record` into one authority frame and submits it, as a caller composing a frame does.
pub fn submit<T>(
    runtime: &mut ProgramRuntime,
    record: impl FnOnce(&mut ProgramRuntime, &mut wgpu::CommandEncoder) -> Result<T, ProgramError>,
) -> Result<(T, wgpu::SubmissionIndex), ProgramError> {
    let authority = runtime.authority().clone();
    let device = runtime.device().clone();
    let mut frame = authority.begin(&device)?;
    let value = record(runtime, &mut frame)?;
    let index = frame.finish().submit()?;
    Ok((value, index))
}

/// Submits `record` as a frame and discards the submission.
pub fn run(
    runtime: &mut ProgramRuntime,
    record: impl FnOnce(&mut ProgramRuntime, &mut wgpu::CommandEncoder) -> Result<(), ProgramError>,
) -> Result<(), ProgramError> {
    submit(runtime, record).map(|_| ())
}

/// The color texture of `target` after the work submitted so far.
pub fn read_target(runtime: &mut ProgramRuntime, target: TargetHandle) -> TargetImage {
    let (pending, index) = submit(runtime, |runtime, encoder| {
        runtime.read_target(encoder, target)
    })
    .unwrap();
    pending.resolve(index).unwrap()
}

/// Names the adapter once per test binary so a log shows which GPU produced the results.
fn announce_adapter(adapter: &wgpu::Adapter) {
    static ANNOUNCED: std::sync::Once = std::sync::Once::new();
    ANNOUNCED.call_once(|| {
        let info = adapter.get_info();
        eprintln!(
            "render-program tests adapter: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
    });
}

pub fn words_to_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
