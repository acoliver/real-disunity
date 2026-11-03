//! A Unity pass built by hand and the steps that draw it: choose variants, build the stages,
//! pack constants, bind a texture and draw a textured quad.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::buffers::ParamName;
use worldrunner_render_program::constants::pack_layout;
use worldrunner_render_program::module::{
    ResourceBinding, StageKind, StageModule, StageParameters, StageProgram,
};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{
    BoundResource, BufferHandle, DrawRequest, Geometry, GraphicsProgram, ProgramRuntime,
    TargetHandle, TargetImage, VertexInput,
};
use worldrunner_render_program::state::RenderState;
use worldrunner_render_program::variant::{select_player_sub_program, select_sub_program};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer as PlayerBuffer, DescriptorSlot, ResourceKind, VariantParameters,
};
use worldrunner_unity_serialized::shader_program::{
    BindChannels, Binding, ConstantBuffer, PlayerSubProgram, Program, ProgramParameters,
    SamplerParam, SubProgram, TextureParam, ValueParam,
};

use crate::common::{Form, player_binding, read_target, run, same, slot, spirv, split_table};
use crate::fixtures::textured;

pub const TINT: i32 = 0;
pub const OFFSET: i32 = 1;
pub const SCALE: i32 = 2;
pub const MAIN_TEX: i32 = 3;
const MATERIAL: i32 = 10;
const DRAW: i32 = 11;

/// Keyword indices of the two fragment variants.
pub const MULTIPLY: u16 = 0;
pub const ADD: u16 = 1;

const FLOAT: i8 = 0;
const TEX2D: i8 = 2;

/// How the pass binds its texture and sampler.
#[derive(Clone, Copy)]
pub enum Samplers {
    /// A separate texture and sampler, each with its own Unity binding.
    Separate,
    /// A combined image sampler that the importer split; the sampler took binding 2 and the
    /// draw constants moved from Unity's binding 2 to 3.
    Split,
}

pub struct Pass {
    pub names: Vec<(String, i32)>,
    pub vertex: Program,
    pub fragment: Program,
    pub blobs: Vec<Vec<u32>>,
    pub table: Vec<ResourceBinding>,
    /// The player variant's parameters, which describe both stages of every variant; `None`
    /// when the stages' sub programs carry editor-style parameters.
    variant: Option<VariantParameters>,
}

impl Pass {
    /// What names the main texture in a stage's texture targets.
    fn main_tex(&self) -> ParamName {
        match self.variant {
            None => ParamName::Index(MAIN_TEX),
            Some(_) => ParamName::Named("_MainTex".to_owned()),
        }
    }
}

fn vector(name_index: i32, offset: i32) -> ValueParam {
    ValueParam {
        name_index,
        index: offset,
        array_size: 1,
        kind: FLOAT,
        dimension: 4,
    }
}

fn buffer(name_index: i32, vectors: Vec<ValueParam>, size: i32) -> ConstantBuffer {
    ConstantBuffer {
        name_index,
        matrices: vec![],
        vectors,
        structs: vec![],
        size,
        partial: false,
    }
}

fn cb_binding(name_index: i32, index: i32) -> Binding {
    Binding {
        name_index,
        index,
        array_size: 1,
    }
}

fn parameters(draw_binding: Option<i32>, samplers: Samplers) -> ProgramParameters {
    let mut parameters = ProgramParameters {
        constant_buffers: vec![
            buffer(MATERIAL, vec![vector(TINT, 0), vector(OFFSET, 16)], 32),
            buffer(DRAW, vec![vector(SCALE, 0)], 16),
        ],
        constant_buffer_bindings: vec![cb_binding(MATERIAL, 0)],
        ..ProgramParameters::default()
    };
    if let Some(binding) = draw_binding {
        parameters
            .constant_buffer_bindings
            .push(cb_binding(DRAW, binding));
    } else {
        parameters.textures.push(TextureParam {
            name_index: MAIN_TEX,
            index: 1,
            sampler_index: 2,
            multisampled: false,
            dimension: TEX2D,
        });
        if matches!(samplers, Samplers::Separate) {
            parameters.samplers.push(SamplerParam {
                sampler: 0,
                bind_point: 2,
            });
        }
    }
    parameters
}

const VERTEX: u8 = DescriptorSlot::VERTEX;
const FRAGMENT: u8 = DescriptorSlot::FRAGMENT;

fn member(name: &str, offset: i32) -> BufferMember {
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

/// The pass as a player variant describes it: both stages' buffers and bindings, with the
/// stages that use each.
fn variant_parameters(draw_binding: u16, samplers: Samplers) -> VariantParameters {
    let layout = |name: &str, members, size| PlayerBuffer {
        name: name.to_owned(),
        size,
        members,
        structs: vec![],
    };
    let texture_slot = slot(0, 1, FRAGMENT);
    let (sampler_slot, sampler_binding) = match samplers {
        Samplers::Separate => (slot(0, 2, FRAGMENT), true),
        Samplers::Split => (texture_slot, false),
    };
    let texture = ResourceKind::Texture {
        sampler: Some(sampler_slot),
        dimension: 4,
    };
    let mut bindings = vec![
        player_binding(
            "Material",
            slot(0, 0, VERTEX | FRAGMENT),
            ResourceKind::ConstantBuffer,
        ),
        player_binding(
            "Draw",
            slot(0, draw_binding, VERTEX),
            ResourceKind::ConstantBuffer,
        ),
        player_binding("_MainTex", texture_slot, texture),
    ];
    if sampler_binding {
        bindings.push(player_binding(
            "",
            sampler_slot,
            ResourceKind::Sampler { state: 0 },
        ));
    }
    VariantParameters {
        constant_buffers: vec![
            layout("", vec![], 0),
            layout(
                "Material",
                vec![member("_Tint", 0), member("_Offset", 16)],
                32,
            ),
            layout("Draw", vec![member("_Scale", 0)], 16),
        ],
        bindings,
    }
}

fn sub_program(blob_index: u32, keywords: &[u16], parameters: ProgramParameters) -> SubProgram {
    SubProgram {
        blob_index,
        channels: BindChannels {
            channels: vec![],
            source_map: 0,
        },
        keyword_indices: keywords.to_vec(),
        hardware_tier: 0,
        gpu_program_type: 0,
        parameters,
        requirements: 0,
    }
}

fn player_sub_program(blob_index: u32, keywords: &[u16]) -> PlayerSubProgram {
    PlayerSubProgram {
        blob_index,
        keyword_indices: keywords.to_vec(),
        requirements: 0,
        gpu_program_type: 0,
    }
}

/// The vertex and fragment programs, and for a player variant the parameters that describe both.
fn programs(form: Form, samplers: Samplers) -> (Program, Program, Option<VariantParameters>) {
    let draw_binding = match samplers {
        Samplers::Separate => 3,
        Samplers::Split => 2,
    };
    match form {
        Form::Program => {
            let vertex_parameters = parameters(Some(draw_binding), samplers);
            let fragment_parameters = parameters(None, samplers);
            let vertex = Program {
                sub_programs: vec![sub_program(0, &[], vertex_parameters)],
                ..Program::default()
            };
            let fragment = Program {
                sub_programs: vec![
                    sub_program(1, &[MULTIPLY], fragment_parameters.clone()),
                    sub_program(2, &[ADD], fragment_parameters),
                ],
                keyword_state_mask: vec![MULTIPLY, ADD],
                ..Program::default()
            };
            (vertex, fragment, None)
        }
        Form::Variant => {
            let vertex = Program {
                player_sub_programs: vec![vec![player_sub_program(0, &[])]],
                ..Program::default()
            };
            let fragment = Program {
                player_sub_programs: vec![vec![
                    player_sub_program(1, &[MULTIPLY]),
                    player_sub_program(2, &[ADD]),
                ]],
                keyword_state_mask: vec![MULTIPLY, ADD],
                ..Program::default()
            };
            let variant = variant_parameters(u16::try_from(draw_binding).unwrap(), samplers);
            (vertex, fragment, Some(variant))
        }
    }
}

/// The pass with a vertex program, and a fragment program of a multiply and an add variant.
/// `form` says whether the variants carry editor-style parameters or are player variants whose
/// parameters describe both stages.
pub fn pass(samplers: Samplers, form: Form) -> Pass {
    let table = match samplers {
        Samplers::Separate => (0..4).map(|b| same(0, b)).collect(),
        Samplers::Split => split_table(),
    };
    let names = [
        ("_Tint", TINT),
        ("_Offset", OFFSET),
        ("_Scale", SCALE),
        ("_MainTex", MAIN_TEX),
        ("Material", MATERIAL),
        ("Draw", DRAW),
    ]
    .map(|(name, index)| (name.to_owned(), index))
    .to_vec();
    let (vertex, fragment, variant) = programs(form, samplers);
    Pass {
        names,
        vertex,
        fragment,
        variant,
        blobs: vec![
            spirv(&textured("texel * material.tint")),
            spirv(&textured("texel * material.tint")),
            spirv(&textured("texel + vec4<f32>(material.offset.xyz, 0.0)")),
        ],
        table,
    }
}

fn stage(
    pass: &Pass,
    program: &Program,
    enabled: &BTreeSet<u16>,
    entry: (&str, StageKind),
) -> StageProgram {
    let (blob_index, parameters): (u32, StageParameters) = match &pass.variant {
        None => {
            let chosen = select_sub_program(program, enabled).unwrap();
            (chosen.blob_index, chosen.parameters.clone().into())
        }
        Some(variant) => {
            let chosen = select_player_sub_program(program, enabled).unwrap();
            (chosen.blob_index, variant.clone().into())
        }
    };
    let module = StageModule {
        words: pass.blobs[blob_index as usize].clone(),
        entry_point: entry.0.to_owned(),
        kind: entry.1,
        bindings: pass.table.clone(),
    };
    StageProgram::new(module, parameters).unwrap()
}

/// What to draw: the keywords and properties, the render state and the quad's triangles.
pub struct Scene<'a> {
    pub pass: &'a Pass,
    pub enabled: &'a [u16],
    pub material: &'a PropertySet,
    pub globals: &'a PropertySet,
    pub state: &'a RenderState,
    /// What the target holds before the draw.
    pub clear: [f64; 4],
    /// Indices into the quad's four corners: top left, top right, bottom right, bottom left.
    pub indices: &'a [u16],
}

pub struct Rendered {
    pub image: TargetImage,
    pub missing: Vec<String>,
}

/// The four texels of the 2x2 texture, as RGBA bytes in row-major order.
pub const TEXELS: [[u8; 4]; 4] = [
    [200, 100, 50, 255],
    [10, 20, 30, 255],
    [255, 255, 255, 128],
    [0, 128, 64, 255],
];

/// Material properties: a tint, an offset whose w is the quad's clip depth, and the globals'
/// scale, which maps the quad onto the whole target.
pub fn properties() -> (PropertySet, PropertySet) {
    let mut material = PropertySet::new();
    material.set("_Tint", PropertyValue::Vector(TINT_VALUE));
    material.set("_Offset", PropertyValue::Vector(OFFSET_VALUE));
    let mut globals = PropertySet::new();
    globals.set("_Scale", PropertyValue::Vector([1.0, 1.0, 0.0, 0.0]));
    (material, globals)
}

pub const TINT_VALUE: [f32; 4] = [0.9, 0.5, 0.3, 0.6];
pub const OFFSET_VALUE: [f32; 4] = [0.1, 0.2, 0.3, 0.5];

fn quad(runtime: &mut ProgramRuntime) -> VertexInput {
    // Position then uv; the top left corner is clip (-1, -1) in Vulkan's Y-down clip space.
    let corners: [[f32; 4]; 4] = [
        [-1.0, -1.0, 0.0, 0.0],
        [1.0, -1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0, 1.0],
        [-1.0, 1.0, 0.0, 1.0],
    ];
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    VertexInput {
        buffer: runtime.create_buffer("quad", &bytes).unwrap(),
        stride: 16,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2].to_vec(),
    }
}

/// Packs each constant buffer the stages declare once, from the material then the globals,
/// and uploads it at its prepared binding. Also returns the properties that were missing.
fn constant_bindings(
    runtime: &mut ProgramRuntime,
    scene: &Scene<'_>,
    program: &GraphicsProgram,
    stages: [&StageProgram; 2],
) -> (Vec<BoundResource>, Vec<String>) {
    let mut packed: BTreeMap<(u32, u32), Vec<u8>> = BTreeMap::new();
    let mut missing = Vec::new();
    for stage in stages {
        for target in stage.constant_buffer_targets().unwrap() {
            let min = program
                .layout()
                .slot(target.set, target.binding)
                .unwrap()
                .min_size;
            let result = pack_layout(
                &target.buffer,
                &scene.pass.names,
                &[scene.material, scene.globals],
                usize::try_from(min).unwrap(),
            )
            .unwrap();
            missing.extend(result.missing);
            packed
                .entry((target.set, target.binding))
                .or_insert(result.bytes);
        }
    }
    let bindings = packed
        .iter()
        .map(|((set, binding), bytes)| {
            BoundResource::buffer(
                *set,
                *binding,
                runtime.create_buffer("constants", bytes).unwrap(),
            )
        })
        .collect();
    (bindings, missing)
}

/// The program, bindings and cleared 2x2 target of a textured quad, before any geometry is
/// drawn.
pub struct Setup {
    pub program: GraphicsProgram,
    pub bindings: Vec<BoundResource>,
    pub target: TargetHandle,
    /// The quad's corners: four vertices of position then uv, 16 bytes each.
    pub inputs: Vec<VertexInput>,
    pub missing: Vec<String>,
}

impl Setup {
    /// A draw of `geometry` into the target with the scene's state.
    pub fn request<'a>(
        &'a self,
        state: &'a RenderState,
        inputs: &'a [VertexInput],
        geometry: Geometry,
    ) -> DrawRequest<'a> {
        DrawRequest {
            program: &self.program,
            state,
            front_face: wgpu::FrontFace::Cw,
            target: self.target,
            vertex_inputs: inputs,
            bindings: &self.bindings,
            geometry,
        }
    }
}

/// Builds the textured pass for `scene` and clears a 2x2 target to `scene.clear`.
pub fn setup(runtime: &mut ProgramRuntime, scene: &Scene<'_>) -> Setup {
    let enabled: BTreeSet<u16> = scene.enabled.iter().copied().collect();
    let vertex = stage(
        scene.pass,
        &scene.pass.vertex,
        &enabled,
        ("vs_main", StageKind::Vertex),
    );
    let fragment = stage(
        scene.pass,
        &scene.pass.fragment,
        &enabled,
        ("fs_main", StageKind::Fragment),
    );
    let program = runtime.graphics_program(&vertex, &fragment).unwrap();

    let (mut bindings, missing) = constant_bindings(runtime, scene, &program, [&vertex, &fragment]);
    let texels: Vec<u8> = TEXELS.iter().flatten().copied().collect();
    let texture = runtime
        .create_texture_2d("main", [2, 2], wgpu::TextureFormat::Rgba8Unorm, &texels)
        .unwrap();
    for target in fragment.texture_targets().unwrap() {
        assert_eq!(target.name, scene.pass.main_tex());
        bindings.push(BoundResource::texture(target.set, target.binding, texture));
    }

    let target = runtime
        .create_render_target("quad", [2, 2], wgpu::TextureFormat::Rgba8Unorm, None)
        .unwrap();
    run(runtime, |runtime, encoder| {
        runtime.clear_target(encoder, target, scene.clear, 1.0)
    })
    .unwrap();
    let inputs = vec![quad(runtime)];
    Setup {
        program,
        bindings,
        target,
        inputs,
        missing,
    }
}

/// A buffer of 16-bit indices.
pub fn index_buffer(runtime: &mut ProgramRuntime, indices: &[u16]) -> BufferHandle {
    let bytes: Vec<u8> = indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    runtime.create_buffer("indices", &bytes).unwrap()
}

/// Draws the quad with the textured pass into a cleared 2x2 target and reads it back.
pub fn render(runtime: &mut ProgramRuntime, scene: &Scene<'_>) -> Rendered {
    let prepared = setup(runtime, scene);
    let buffer = index_buffer(runtime, scene.indices);
    run(runtime, |runtime, encoder| {
        runtime.draw(
            encoder,
            &prepared.request(
                scene.state,
                &prepared.inputs,
                Geometry::Indexed {
                    buffer,
                    format: wgpu::IndexFormat::Uint16,
                    first: 0,
                    count: u32::try_from(scene.indices.len()).unwrap(),
                    base_vertex: 0,
                },
            ),
        )
    })
    .unwrap();
    Rendered {
        image: read_target(runtime, prepared.target),
        missing: prepared.missing,
    }
}

/// Both triangles of the quad, wound clockwise as displayed.
pub const QUAD: [u16; 6] = [0, 1, 2, 0, 2, 3];
