//! Values the tests build the way Unity's serialized shader describes them.

pub use worldrunner_render_program::unity::synthetic::{fixed, named, opaque_state};

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use worldrunner_render_program::unity::module::{
    ResourceBinding, StageKind, StageModule, StageParameters, StageProgram,
};
use worldrunner_unity_serialized::shader_parameters::{
    DescriptorSlot, ResourceBinding as PlayerBinding, ResourceKind, VariantParameters,
};
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
pub fn same(set: u32, binding: u32) -> ResourceBinding {
    ResourceBinding {
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
    bindings: Vec<ResourceBinding>,
    parameters: impl Into<StageParameters>,
) -> StageProgram {
    let module = StageModule {
        words: spirv(wgsl),
        entry_point: entry_point.to_owned(),
        kind,
        bindings,
    };
    StageProgram::new(module, parameters).unwrap()
}

/// Unity bound texture and sampler as one combined image sampler at 1; the split gave the
/// sampler binding 2 and moved the draw constants from Unity's 2 to 3.
pub fn split_table() -> Vec<ResourceBinding> {
    vec![
        same(0, 0),
        ResourceBinding {
            set: 0,
            unity_binding: 1,
            binding: 1,
            sampler_binding: Some(2),
        },
        ResourceBinding {
            set: 0,
            unity_binding: 2,
            binding: 3,
            sampler_binding: None,
        },
    ]
}

/// The two ways a stage's parameters are described: an editor-style sub program's, or a
/// player variant's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Program,
    Variant,
}

pub const FORMS: [Form; 2] = [Form::Program, Form::Variant];

/// Runs `body` once for each form.
pub fn each_form(body: fn(Form)) {
    for form in FORMS {
        body(form);
    }
}

/// A texture's dimension, which the two forms code differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dim {
    D2,
    Cube,
}

impl Dim {
    fn program(self) -> i8 {
        match self {
            Self::D2 => 2,
            Self::Cube => 4,
        }
    }

    fn variant(self) -> i32 {
        match self {
            Self::D2 => 4,
            Self::Cube => 8,
        }
    }
}

/// What one stage binds, by Unity binding, before it is described in either form.
#[derive(Clone, Debug, Default)]
pub struct Resources {
    pub buffers: Vec<i32>,
    pub textures: Vec<(i32, Dim)>,
    pub samplers: Vec<i32>,
}

impl Resources {
    pub fn new(buffers: &[i32], textures: &[(i32, Dim)], samplers: &[i32]) -> Self {
        Self {
            buffers: buffers.to_vec(),
            textures: textures.to_vec(),
            samplers: samplers.to_vec(),
        }
    }
}

/// The stage bit a variant's binding names for a stage. A compute stage has none, so its
/// bindings name no stage.
pub fn stage_bit(kind: StageKind) -> u8 {
    match kind {
        StageKind::Vertex => DescriptorSlot::VERTEX,
        StageKind::Fragment => DescriptorSlot::FRAGMENT,
        StageKind::Compute => 0,
    }
}

pub fn slot(set: u16, binding: u16, stages: u8) -> DescriptorSlot {
    DescriptorSlot {
        binding,
        set,
        stages,
    }
}

pub fn player_binding(name: &str, slot: DescriptorSlot, kind: ResourceKind) -> PlayerBinding {
    PlayerBinding {
        name: name.to_owned(),
        slot,
        kind,
    }
}

fn unity_binding(index: i32) -> u16 {
    u16::try_from(index).unwrap()
}

/// A pass's stages described as one player variant: each resource names every stage that
/// binds it.
fn variant_of(pass: &[(StageKind, &Resources)]) -> VariantParameters {
    let mut bindings: Vec<PlayerBinding> = Vec::new();
    let mut add = |name: String, index: i32, kind: ResourceKind, stage: StageKind| {
        let bit = stage_bit(stage);
        if let Some(found) = bindings
            .iter_mut()
            .find(|b| b.name == name && b.kind == kind)
        {
            found.slot.stages |= bit;
        } else {
            bindings.push(player_binding(
                &name,
                slot(0, unity_binding(index), bit),
                kind,
            ));
        }
    };
    for (stage, resources) in pass {
        for buffer in &resources.buffers {
            add(
                format!("B{buffer}"),
                *buffer,
                ResourceKind::ConstantBuffer,
                *stage,
            );
        }
        for (index, dim) in &resources.textures {
            let kind = ResourceKind::Texture {
                sampler: None,
                dimension: dim.variant(),
            };
            add(format!("T{index}"), *index, kind, *stage);
        }
        for index in &resources.samplers {
            let kind = ResourceKind::Sampler { state: 0 };
            add(format!("S{index}"), *index, kind, *stage);
        }
    }
    VariantParameters {
        constant_buffers: Vec::new(),
        bindings,
    }
}

/// The parameters of `kind`'s stage in `pass`, in `form`. An editor-style sub program
/// describes one stage; a player variant describes the whole pass.
pub fn describe(form: Form, kind: StageKind, pass: &[(StageKind, &Resources)]) -> StageParameters {
    match form {
        Form::Variant => variant_of(pass).into(),
        Form::Program => {
            let (_, resources) = pass.iter().find(|(k, _)| *k == kind).unwrap();
            let textures = resources
                .textures
                .iter()
                .map(|(index, dim)| texture(*index, dim.program()))
                .collect();
            let samplers = resources.samplers.iter().map(|b| sampler(*b)).collect();
            parameters(&resources.buffers, textures, samplers).into()
        }
    }
}

/// A stage of `pass`, described in `form`.
pub fn stage_in(
    form: Form,
    wgsl: &str,
    entry: (&str, StageKind),
    bindings: Vec<ResourceBinding>,
    pass: &[(StageKind, &Resources)],
) -> StageProgram {
    stage(wgsl, entry, bindings, describe(form, entry.1, pass))
}

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::unity::runtime::{ProgramRuntime, TargetHandle, TargetImage};

/// A budget the tests that are not about budgets never reach.
pub const AMPLE: AllocationBudget = AllocationBudget(1 << 30);

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
        self.runtime_with(AMPLE)
    }

    pub fn runtime_with(&self, budget: AllocationBudget) -> ProgramRuntime {
        ProgramRuntime::new(self.device.clone(), self.authority.clone(), budget)
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
