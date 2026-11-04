//! A uniform block built by naga from WGSL, whose layout is predicted by hand from the WGSL
//! rules, packed from a player's struct description and run on the GPU.
//!
//! The block is an array of three structs: a float4 at 0, a 4x4 matrix at 16 and an int4 at 80,
//! so a struct is 96 bytes (80 plus 16, already a multiple of 16) and the array's stride is 96.
//! The block is 288 bytes. The same shape is how a player stores its struct arrays.

use worldrunner_render_program::constants::{PackedConstants, pack_layout};
use worldrunner_render_program::module::{StageKind, StageProgram};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::runtime::{BoundResource, DispatchRequest};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, ResourceKind, StructLayout, VariantParameters,
};

use crate::common::{player_binding, run, runtime, same, slot, spirv, stage, submit};

const KERNEL: &str = "
struct Item { weight: vec4<f32>, basis: mat4x4<f32>, slot: vec4<i32> }
struct Items { items: array<Item, 3> }
@group(0) @binding(0) var<uniform> block: Items;
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(3) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let item = block.items[id.x];
    result[3u * id.x] = item.weight;
    result[3u * id.x + 1u] = item.basis[1];
    result[3u * id.x + 2u] = vec4<f32>(item.slot);
}";

/// The layout the WGSL rules give, worked out by hand: member offsets in a struct, the struct's
/// size, which is the array's stride, and the block's size.
const WEIGHT: i32 = 0;
const BASIS: i32 = 16;
const SLOT: i32 = 80;
const STRIDE: i32 = 96;
const BLOCK: i32 = 288;
const ELEMENTS: usize = 3;

const OP_DECORATE: u32 = 71;
const OP_MEMBER_DECORATE: u32 = 72;
const DECORATION_ARRAY_STRIDE: u32 = 6;
const DECORATION_OFFSET: u32 = 35;

/// The decorations naga's SPIR-V writer put on the module: member offsets by struct ID, in
/// member order, and the array strides.
struct Decorations {
    offsets: Vec<(u32, Vec<u32>)>,
    strides: Vec<u32>,
}

fn decorations(words: &[u32]) -> Decorations {
    let mut offsets: Vec<(u32, Vec<(u32, u32)>)> = Vec::new();
    let mut strides = Vec::new();
    let mut at = 5;
    while at < words.len() {
        let (opcode, count) = (
            words[at] & 0xffff,
            usize::try_from(words[at] >> 16).unwrap(),
        );
        let operands = &words[at + 1..at + count];
        match (opcode, operands) {
            (OP_MEMBER_DECORATE, &[owner, member, DECORATION_OFFSET, offset]) => {
                match offsets.iter_mut().find(|(id, _)| *id == owner) {
                    Some((_, members)) => members.push((member, offset)),
                    None => offsets.push((owner, vec![(member, offset)])),
                }
            }
            (OP_DECORATE, &[_, DECORATION_ARRAY_STRIDE, stride]) => strides.push(stride),
            _ => {}
        }
        at += count;
    }
    let ordered = |mut members: Vec<(u32, u32)>| {
        members.sort_unstable();
        members.into_iter().map(|(_, offset)| offset).collect()
    };
    Decorations {
        offsets: offsets
            .into_iter()
            .map(|(id, members)| (id, ordered(members)))
            .collect(),
        strides,
    }
}

fn member(name: &str, kind: i32, shape: (i32, i32), is_matrix: bool, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind,
        rows: shape.0,
        columns: shape.1,
        is_matrix,
        array_size: 0,
        offset,
    }
}

fn items() -> ConstantBuffer {
    ConstantBuffer {
        name: "Items".to_owned(),
        size: BLOCK,
        members: Vec::new(),
        structs: vec![StructLayout {
            name: "Item".to_owned(),
            index: 0,
            array_size: i32::try_from(ELEMENTS).unwrap(),
            size: STRIDE,
            members: vec![
                member("_Weight", 0, (1, 4), false, WEIGHT),
                member("_Basis", 0, (4, 4), true, BASIS),
                member("_Slot", 1, (1, 4), false, SLOT),
            ],
        }],
    }
}

fn variant() -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![items()],
        bindings: vec![
            player_binding("Items", slot(0, 0, 0), ResourceKind::ConstantBuffer),
            player_binding("result", slot(0, 1, 0), ResourceKind::Buffer),
        ],
    }
}

/// Values that name their member and element: the weight is `[e, 1, 2, 3] + 10 e`, the matrix's
/// float `i` is `100 e + i`, and the slot is `[e, e + 1, e + 2, e + 3]`.
fn weight(e: usize) -> [f32; 4] {
    std::array::from_fn(|i| f32::from(u8::try_from(10 * e + i).unwrap()))
}

fn basis(e: usize) -> [f32; 16] {
    std::array::from_fn(|i| f32::from(u8::try_from(100 * e + i).unwrap()))
}

fn slot_value(e: usize) -> [i32; 4] {
    std::array::from_fn(|i| i32::try_from(e + i).unwrap())
}

fn material() -> PropertySet {
    let mut material = PropertySet::new();
    material.set(
        "_Weight",
        PropertyValue::VectorArray((0..ELEMENTS).map(weight).collect()),
    );
    material.set(
        "_Basis",
        PropertyValue::MatrixArray((0..ELEMENTS).map(basis).collect()),
    );
    material.set(
        "_Slot",
        PropertyValue::IntVectorArray((0..ELEMENTS).map(slot_value).collect()),
    );
    material
}

fn pack(min_len: usize) -> PackedConstants {
    let layout = worldrunner_render_program::buffers::BufferLayout::from_variant(&items());
    pack_layout(&layout, &[], &[&material()], min_len).unwrap()
}

fn kernel() -> StageProgram {
    stage(
        KERNEL,
        ("main", StageKind::Compute),
        vec![same(0, 0), same(0, 1)],
        variant(),
    )
}

#[test]
fn the_block_naga_writes_has_the_offsets_and_stride_predicted_from_the_wgsl_rules() {
    let found = decorations(&spirv(KERNEL));
    let sizes: Vec<usize> = found.offsets.iter().map(|(_, o)| o.len()).collect();
    let item = found
        .offsets
        .iter()
        .find(|(_, offsets)| offsets.len() == 3)
        .expect("the struct of three members");
    assert_eq!(item.1, [0, 16, 80], "member counts {sizes:?}");
    // The output array of float4 has the stride of its element, 16.
    let mut strides = found.strides.clone();
    strides.sort_unstable();
    assert_eq!(
        strides,
        [16, 96],
        "the output array's stride and the struct array's"
    );
    let block = found.offsets.iter().find(|(_, o)| o.len() == 1).unwrap();
    assert_eq!(block.1, [0], "the array is the block's only member");
}

#[test]
fn every_member_of_every_element_lands_where_the_spirv_offsets_and_stride_put_it() {
    let found = decorations(&spirv(KERNEL));
    let offsets = &found.offsets.iter().find(|(_, o)| o.len() == 3).unwrap().1;
    assert!(found.strides.contains(&96), "the struct array's stride");
    let stride = usize::try_from(STRIDE).unwrap();
    let packed = pack(0);
    assert_eq!(packed.bytes.len(), usize::try_from(BLOCK).unwrap());
    let floats_at = |at: usize, count: usize| -> Vec<f32> {
        packed.bytes[at..at + 4 * count]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };
    for e in 0..ELEMENTS {
        let start = e * stride;
        let at = |k: usize| start + usize::try_from(offsets[k]).unwrap();
        assert_eq!(floats_at(at(0), 4), weight(e), "weight of element {e}");
        assert_eq!(floats_at(at(1), 16), basis(e), "basis of element {e}");
        let ints: Vec<i32> = packed.bytes[at(2)..at(2) + 16]
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(ints, slot_value(e), "slot of element {e}");
    }
}

#[test]
fn the_gpu_reads_each_element_the_packer_wrote() {
    let kernel = kernel();
    let mut runtime = runtime();
    let program = runtime.compute_program(&kernel).unwrap();
    let min_size = usize::try_from(program.layout().slot(0, 0).unwrap().min_size).unwrap();
    assert_eq!(min_size, usize::try_from(BLOCK).unwrap());
    let targets = kernel.constant_buffer_targets().unwrap();
    assert_eq!((targets[0].set, targets[0].binding), (0, 0));
    let packed = pack_layout(&targets[0].buffer, &[], &[&material()], min_size).unwrap();
    assert!(packed.missing.is_empty());

    let uniform = runtime.create_buffer("items", &packed.bytes).unwrap();
    let output = runtime
        .create_buffer("result", &[0xff; 4 * 4 * 3 * ELEMENTS])
        .unwrap();
    run(&mut runtime, |runtime, encoder| {
        runtime.dispatch(
            encoder,
            &DispatchRequest {
                program: &program,
                bindings: &[
                    BoundResource::buffer(0, 0, uniform),
                    BoundResource::buffer(0, 1, output),
                ],
                groups: [1, 1, 1],
            },
        )
    })
    .unwrap();
    let (pending, index) = submit(&mut runtime, |runtime, encoder| {
        runtime.read_buffer(encoder, output)
    })
    .unwrap();
    let got: Vec<f32> = pending
        .resolve(index)
        .unwrap()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    // The kernel writes the weight, the matrix's second column (floats 4 to 7 as packed) and the
    // slot converted to float, for each element.
    let mut want = Vec::new();
    for e in 0..ELEMENTS {
        want.extend(weight(e));
        want.extend(&basis(e)[4..8]);
        want.extend(slot_value(e).map(|v| f32::from(i16::try_from(v).unwrap())));
    }
    assert_eq!(got, want);
}
