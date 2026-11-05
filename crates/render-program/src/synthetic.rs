//! Synthetic player shaders for tests: the records, parameters and tables of a Unity player's
//! Vulkan platform, built from WGSL. The stage modules are written the way a player stores them,
//! one module per stage with one entry point.

pub mod shaders;

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use worldrunner_unity_serialized::shader::{
    CompiledBlobs, ParsedShader, Pass, Shader, StagePrograms, SubShader,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, VariantParameters,
};
use worldrunner_unity_serialized::shader_program::{PlayerSubProgram, Program};
use worldrunner_unity_serialized::shader_records::{RECORD_VERSION, SPIRV_PROGRAM};
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

/// SPIR-V of one entry point of a WGSL module, written as a Vulkan shader: the position output
/// is not flipped.
pub fn stage_spirv(wgsl: &str, entry: &str, stage: naga::ShaderStage) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(wgsl).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    let options = spv::Options {
        flags: spv::WriterFlags::empty(),
        ..spv::Options::default()
    };
    let pipeline = spv::PipelineOptions {
        shader_stage: stage,
        entry_point: entry.to_owned(),
    };
    spv::write_vec(&module, &info, &options, Some(&pipeline)).unwrap()
}

/// A float vector member of a constant buffer.
pub fn vector_member(name: &str, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind: 0,
        rows: 1,
        columns: 4,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

pub fn constant_buffer(name: &str, size: i32, members: Vec<BufferMember>) -> ConstantBuffer {
    ConstantBuffer {
        name: name.to_owned(),
        size,
        members,
        structs: Vec::new(),
    }
}

pub const VERTEX: u8 = DescriptorSlot::VERTEX;
pub const FRAGMENT: u8 = DescriptorSlot::FRAGMENT;

pub fn slot(set: u16, binding: u16, stages: u8) -> DescriptorSlot {
    DescriptorSlot {
        binding,
        set,
        stages,
    }
}

pub fn binding(name: &str, slot: DescriptorSlot, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        name: name.to_owned(),
        slot,
        kind,
    }
}

/// One variant of a pass: its keywords, WGSL with one entry point for each stage, parameters and
/// vertex input table.
pub struct VariantSpec {
    pub keywords: Vec<&'static str>,
    pub wgsl: String,
    pub entries: [&'static str; 2],
    pub parameters: VariantParameters,
    /// Source channel and shader input of each vertex input.
    pub channels: Vec<[i8; 2]>,
}

pub struct PassSpec {
    pub tags: Vec<(&'static str, &'static str)>,
    pub variants: Vec<VariantSpec>,
}

/// The shader and the chunks of its Vulkan platform.
pub struct SyntheticShader {
    pub shader: Shader,
    pub chunks: Vec<Vec<u8>>,
}

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, value: u32) {
        self.0.extend(value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.0.extend(value.to_le_bytes());
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
        self.0.resize(self.0.len().next_multiple_of(4), 0);
    }

    /// A length, the bytes and padding to four.
    fn string(&mut self, text: &str) {
        self.u32(u32::try_from(text.len()).unwrap());
        self.bytes(text.as_bytes());
    }

    fn count(&mut self, len: usize) {
        self.u32(u32::try_from(len).unwrap());
    }
}

fn packed(slot: DescriptorSlot) -> u32 {
    u32::from(slot.binding) | (u32::from(slot.set) << 16) | (u32::from(slot.stages) << 26)
}

fn members(w: &mut Writer, members: &[BufferMember]) {
    w.count(members.len());
    for member in members {
        w.string(&member.name);
        w.i32(member.kind);
        w.i32(member.rows);
        w.i32(member.columns);
        w.u32(u32::from(member.is_matrix));
        w.i32(member.array_size);
        w.i32(member.offset);
    }
}

fn resource_binding(w: &mut Writer, binding: &ResourceBinding) {
    w.string(&binding.name);
    let (tag, slot) = match binding.kind {
        ResourceKind::Texture { .. } => (0, binding.slot),
        ResourceKind::ConstantBuffer => (1, binding.slot),
        ResourceKind::Buffer => (2, binding.slot),
        ResourceKind::StorageImage => (3, binding.slot),
        ResourceKind::Sampler { .. } => (4, binding.slot),
    };
    w.u32(tag);
    w.u32(packed(slot));
    match binding.kind {
        ResourceKind::Texture { sampler, dimension } => {
            w.u32(sampler.map_or(u32::MAX, packed));
            w.i32(dimension);
        }
        ResourceKind::ConstantBuffer => w.u32(0),
        ResourceKind::Buffer | ResourceKind::StorageImage => w.u32(1),
        ResourceKind::Sampler { state } => w.u32(state),
    }
}

/// The record of a variant's parameters.
fn parameters_record(parameters: &VariantParameters) -> Vec<u8> {
    let mut w = Writer(Vec::new());
    w.u32(RECORD_VERSION);
    w.count(parameters.constant_buffers.len());
    for buffer in &parameters.constant_buffers {
        w.string(&buffer.name);
        w.i32(buffer.size);
        members(&mut w, &buffer.members);
        w.count(0);
    }
    w.count(parameters.bindings.len());
    for binding in &parameters.bindings {
        resource_binding(&mut w, binding);
    }
    w.0
}

const SLOTS: usize = 6;
const WRAPPER: usize = 4 + SLOTS * 8;

/// The code of a Vulkan program: a wrapper that names the vertex module in slot 0 and the
/// fragment module in slot 1.
fn program_code(vertex: &[u32], fragment: &[u32]) -> Vec<u8> {
    let modules = [vertex, fragment];
    let mut w = Writer(vec![0; WRAPPER]);
    let mut at = WRAPPER;
    for (slot, words) in modules.iter().enumerate() {
        let size = words.len() * 4;
        let header = 4 + slot * 8;
        w.0[header..header + 4].copy_from_slice(&u32::try_from(at).unwrap().to_le_bytes());
        w.0[header + 4..header + 8].copy_from_slice(&u32::try_from(size).unwrap().to_le_bytes());
        w.0.extend(words.iter().flat_map(|word| word.to_le_bytes()));
        at += size;
    }
    w.0
}

/// The record of a variant's program.
fn program_record(variant: &VariantSpec) -> Vec<u8> {
    let vertex = stage_spirv(&variant.wgsl, variant.entries[0], naga::ShaderStage::Vertex);
    let fragment = stage_spirv(
        &variant.wgsl,
        variant.entries[1],
        naga::ShaderStage::Fragment,
    );
    let mut w = Writer(Vec::new());
    w.u32(RECORD_VERSION);
    w.u32(u32::from(SPIRV_PROGRAM.cast_unsigned()));
    for _ in 0..4 {
        w.u32(0);
    }
    w.count(variant.keywords.len());
    for keyword in &variant.keywords {
        w.string(keyword);
    }
    let code = program_code(&vertex, &fragment);
    w.count(code.len());
    w.bytes(&code);
    // The vertex input table: a mask of the source channels, then the pairs.
    let mask = variant
        .channels
        .iter()
        .fold(0_i32, |mask, [source, _]| mask | (1 << source));
    w.i32(mask);
    w.count(variant.channels.len());
    for [source, target] in &variant.channels {
        w.i32(i32::from(*source));
        w.i32(i32::from(*target));
    }
    w.0
}

/// A chunk holding the table of entries and then the entries.
fn chunk(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut w = Writer(Vec::new());
    w.count(entries.len());
    let mut at = 4 + entries.len() * 12;
    for entry in entries {
        w.u32(u32::try_from(at).unwrap());
        w.u32(u32::try_from(entry.len()).unwrap());
        w.u32(0);
        at += entry.len();
    }
    for entry in entries {
        w.0.extend(entry);
    }
    w.0
}

/// The pass of `spec`, whose variants' records are appended to `entries` as a record and its
/// parameters.
fn pass(keyword_names: &[&str], spec: &PassSpec, entries: &mut Vec<Vec<u8>>) -> Pass {
    let mut variants = Vec::new();
    let mut parameters = Vec::new();
    for variant in &spec.variants {
        let record = u32::try_from(entries.len()).unwrap();
        entries.push(program_record(variant));
        entries.push(parameters_record(&variant.parameters));
        let keyword_indices = variant
            .keywords
            .iter()
            .map(|keyword| {
                keyword_names
                    .iter()
                    .position(|name| name == keyword)
                    .and_then(|index| u16::try_from(index).ok())
                    .unwrap()
            })
            .collect();
        variants.push(PlayerSubProgram {
            blob_index: record,
            keyword_indices,
            requirements: 0,
            gpu_program_type: SPIRV_PROGRAM,
        });
        parameters.push(record + 1);
    }
    let mut state = opaque_state();
    state.tags = spec
        .tags
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    let vertex = Program {
        player_sub_programs: vec![variants],
        parameter_blob_indices: vec![parameters],
        ..Program::default()
    };
    Pass {
        name_indices: Vec::new(),
        kind: 0,
        state,
        program_mask: 0b11,
        programs: StagePrograms {
            vertex,
            ..StagePrograms::default()
        },
        has_instancing_variant: false,
        has_procedural_instancing_variant: false,
        use_name: String::new(),
        name: String::new(),
        texture_name: String::new(),
        tags: Vec::new(),
    }
}

/// Builds a shader of one sub-shader with `passes`, whose keywords are listed in
/// `keyword_names`. Each pass keeps the variants as the vertex program's player variants, as the
/// Unity 6 player stores Vulkan programs.
pub fn shader(keyword_names: &[&str], passes: &[PassSpec]) -> SyntheticShader {
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let built = passes
        .iter()
        .map(|spec| pass(keyword_names, spec, &mut entries))
        .collect();
    let chunks = vec![chunk(&entries)];
    let length = u32::try_from(chunks[0].len()).unwrap();
    let shader = Shader {
        name: "synthetic".to_owned(),
        parsed: ParsedShader {
            properties: Vec::new(),
            sub_shaders: vec![SubShader {
                passes: built,
                tags: Vec::new(),
                lod: 0,
            }],
            keyword_names: keyword_names.iter().map(|&name| name.to_owned()).collect(),
            keyword_flags: vec![0; keyword_names.len()],
            name: "synthetic".to_owned(),
            custom_editor: String::new(),
            fallback: String::new(),
            dependencies: Vec::new(),
            pipeline_editors: Vec::new(),
            disable_no_sub_shaders_message: false,
        },
        compiled: CompiledBlobs {
            platforms: vec![18],
            offsets: vec![vec![0]],
            compressed_lengths: vec![vec![length]],
            decompressed_lengths: vec![vec![length]],
            blob: 0..0,
            stage_counts: vec![2],
        },
        dependencies: Vec::new(),
        non_modifiable_textures: Vec::new(),
        is_baked: false,
        asset_guid: [0; 16],
        asset_local_identifier: 0,
    };
    SyntheticShader { shader, chunks }
}

/// A compute stage built from WGSL, for hosts that test dispatches. `places` are the sets and
/// bindings the module declares, which `parameters` name; each is its own prepared binding.
pub fn compute_stage(
    wgsl: &str,
    entry: &str,
    places: &[(u32, u32)],
    parameters: VariantParameters,
) -> crate::module::StageProgram {
    use crate::module::{ResourceBinding, StageKind, StageModule, StageProgram};
    let bindings = places
        .iter()
        .map(|&(set, binding)| ResourceBinding {
            set,
            unity_binding: binding,
            binding,
            sampler_binding: None,
        })
        .collect();
    let module = StageModule {
        words: stage_spirv(wgsl, entry, naga::ShaderStage::Compute),
        entry_point: entry.to_owned(),
        kind: StageKind::Compute,
        bindings,
    };
    StageProgram::new(module, parameters).expect("a compute stage that parses")
}
