//! Buffers bound by sub-range: `Import::buffer_range` binds `size` bytes from `offset` of a
//! buffer, as a uniform or a storage buffer, and the draw reads that slice and no other.
//!
//! One buffer holds several slices, and the colour or marker of each differs, so a binding that
//! ignored the offset, or the size, would change a pixel.

use std::sync::Arc;

use wgpu::ShaderStages;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
    StorageAccess,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportTable, ImportedDraw, ImportedGeometry, PreparedProgramDraw,
    PreparedSet, ProgramRuntime,
};

use crate::common::{CLEAR, assert_halves, no_cull, render, signature};
use crate::support::Gpu;

const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@group(0) @binding(1) var<storage, read> table: array<vec4<f32>>;
fn rect(x0: f32, x1: f32, index: u32) -> vec4<f32> {
    var xs = array<f32, 6>(x0, x1, x1, x0, x1, x0);
    var ys = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    return vec4<f32>(xs[index], ys[index], 0.0, 1.0);
}
@vertex fn vs_left(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return rect(-1.0, 0.0, index);
}
@vertex fn vs_right(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    return rect(0.0, 1.0, index);
}
@fragment fn fs_tint() -> @location(0) vec4<f32> { return tint.color; }
@fragment fn fs_second() -> @location(0) vec4<f32> { return table[1]; }
@fragment fn fs_length() -> @location(0) vec4<f32> {
    return vec4<f32>(f32(arrayLength(&table)) * 0.125, 0.0, 0.0, 1.0);
}";

/// Four slices, told apart by colour.
const SLICES: [[f32; 4]; 4] = [
    [0.25, 0.0, 0.0, 1.0],
    [0.0, 0.5, 0.0, 1.0],
    [0.0, 0.0, 0.75, 1.0],
    [0.5, 0.5, 0.0, 1.0],
];
/// What the storage buffer holds in the second element of its marked slice.
const MARKER: [f32; 4] = [0.5, 0.25, 0.0, 1.0];

fn tint() -> EngineSemantic {
    EngineSemantic::named("range.tint")
}

fn table() -> EngineSemantic {
    EngineSemantic::named("range.table")
}

fn declare(binding: u32, source: EngineSemantic, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source: BindingSource::Engine(source),
        visibility: ShaderStages::FRAGMENT,
        kind,
    }
}

fn program(runtime: &ProgramRuntime, (vertex, fragment): (&str, &str)) -> GraphicsProgram {
    let stage = |entry: &str, kind, resources: Vec<ResourceBinding>| {
        StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(Arc::from(WGSL)),
                entry_point: entry.to_owned(),
                kind,
            },
            StageDeclarations {
                resources: ResourceLayout::new(resources),
                constants: Vec::new(),
            },
        )
        .unwrap()
    };
    let resources = match fragment {
        "fs_tint" => vec![declare(0, tint(), ResourceKind::Uniform { min_size: 16 })],
        _ => vec![declare(
            1,
            table(),
            ResourceKind::Storage {
                access: StorageAccess::ReadOnly,
                min_size: 32,
            },
        )],
    };
    runtime
        .graphics_program(
            &stage(vertex, StageKind::Vertex, Vec::new()),
            &stage(fragment, StageKind::Fragment, resources),
        )
        .unwrap()
}

fn prepare(
    runtime: &mut ProgramRuntime,
    program: &GraphicsProgram,
    (imports, sets): (&[Import], &[&PreparedSet]),
) -> Result<PreparedProgramDraw, ProgramError> {
    runtime.prepare_imported_draw(
        &ImportedDraw {
            program,
            state: &no_cull(),
            vertex_inputs: &[],
            bindings: &[],
            imports,
            sets,
            geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
            instances: 0..1,
            owners: &[],
        },
        signature(),
    )
}

/// A buffer of `slices.len()` slices `stride` bytes apart, each holding its colour first and
/// zeros after. A binding that took the whole buffer would see slice 0.
fn slices_buffer(gpu: &Gpu, stride: u64, slices: &[[f32; 4]]) -> wgpu::Buffer {
    let mut bytes = vec![0_u8; usize::try_from(stride).unwrap() * slices.len()];
    for (index, colour) in slices.iter().enumerate() {
        let at = index * usize::try_from(stride).unwrap();
        for (channel, value) in colour.iter().enumerate() {
            bytes[at + channel * 4..at + channel * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    gpu.buffer(wgpu::BufferUsages::UNIFORM, &bytes)
}

#[test]
fn a_uniform_sub_range_draw_reads_the_slice_the_offset_names() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let stride = u64::from(
        runtime
            .device()
            .limits()
            .min_uniform_buffer_offset_alignment,
    );
    let buffer = slices_buffer(&gpu, stride, &SLICES);

    let left = program(&runtime, ("vs_left", "fs_tint"));
    let right = program(&runtime, ("vs_right", "fs_tint"));
    // The left half reads slice 1 through the draw's own imports.
    let by_import = prepare(
        &mut runtime,
        &left,
        (
            &[Import::buffer_range(tint(), buffer.clone(), stride, 16)],
            &[],
        ),
    )
    .unwrap();
    // The right half reads slice 3 through a set prepared from a table.
    let table =
        ImportTable::new().with(Import::buffer_range(tint(), buffer.clone(), 3 * stride, 16));
    let set = runtime.prepare_set(&right, 0, &table, Vec::new()).unwrap();
    let by_set = prepare(&mut runtime, &right, (&[], &[&set])).unwrap();
    let image = render(&gpu, &[], &[&by_import, &by_set]);
    assert_halves(&image, SLICES[1], SLICES[3]);

    // The same buffer, bound whole, is slice 0 in both halves.
    let whole = prepare(
        &mut runtime,
        &left,
        (&[Import::buffer(tint(), buffer.clone())], &[]),
    )
    .unwrap();
    assert_halves(&render(&gpu, &[], &[&whole]), SLICES[0], CLEAR);

    // A range may be larger than the shader needs, and may end at the end of the buffer.
    let last = prepare(
        &mut runtime,
        &right,
        (
            &[Import::buffer_range(tint(), buffer, 2 * stride, 2 * stride)],
            &[],
        ),
    )
    .unwrap();
    assert_halves(&render(&gpu, &[], &[&last]), CLEAR, SLICES[2]);
}

#[test]
fn a_storage_sub_range_starts_at_its_offset_and_is_as_long_as_its_size() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let stride = u64::from(
        runtime
            .device()
            .limits()
            .min_storage_buffer_offset_alignment,
    );
    // Three strides of zeros with the marker at the second element after the first stride.
    let mut bytes = vec![0_u8; usize::try_from(3 * stride).unwrap()];
    let marked = usize::try_from(stride).unwrap() + 16;
    for (channel, value) in MARKER.iter().enumerate() {
        bytes[marked + channel * 4..marked + channel * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    let buffer = gpu.buffer(wgpu::BufferUsages::STORAGE, &bytes);

    let second = program(&runtime, ("vs_left", "fs_second"));
    let draw = prepare(
        &mut runtime,
        &second,
        (
            &[Import::buffer_range(table(), buffer.clone(), stride, 64)],
            &[],
        ),
    )
    .unwrap();
    // `table[1]` of the range is the marker; of the whole buffer it is zeros.
    assert_halves(&render(&gpu, &[], &[&draw]), MARKER, CLEAR);
    let whole = prepare(
        &mut runtime,
        &second,
        (&[Import::buffer(table(), buffer.clone())], &[]),
    )
    .unwrap();
    assert_halves(&render(&gpu, &[], &[&whole]), [0.0, 0.0, 0.0, 0.0], CLEAR);

    // The shader sees as many elements as the range holds: the size, in sixteen-byte elements.
    let length = program(&runtime, ("vs_left", "fs_length"));
    for (size, elements) in [(32_u64, 2.0_f32), (48, 3.0), (64, 4.0)] {
        let draw = prepare(
            &mut runtime,
            &length,
            (
                &[Import::buffer_range(table(), buffer.clone(), stride, size)],
                &[],
            ),
        )
        .unwrap();
        assert_halves(
            &render(&gpu, &[], &[&draw]),
            [elements * 0.125, 0.0, 0.0, 1.0],
            CLEAR,
        );
    }
}

#[test]
fn a_misaligned_short_or_overrunning_range_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let limits = runtime.device().limits();
    let (uniform, storage) = (
        u64::from(limits.min_uniform_buffer_offset_alignment),
        u64::from(limits.min_storage_buffer_offset_alignment),
    );
    eprintln!("offset alignment: uniform {uniform}, storage {storage}");
    let uniforms = slices_buffer(&gpu, uniform, &SLICES);
    let storages = gpu.buffer(
        wgpu::BufferUsages::STORAGE,
        &vec![0_u8; usize::try_from(2 * storage).unwrap()],
    );
    let len = uniforms.size();
    let tinted = program(&runtime, ("vs_left", "fs_tint"));
    let tabled = program(&runtime, ("vs_left", "fs_second"));
    let range = |semantic, buffer: &wgpu::Buffer, offset, size| {
        Import::buffer_range(semantic, buffer.clone(), offset, size)
    };
    let too_small = |binding, needed, have| ProgramError::BufferTooSmall {
        set: 0,
        binding,
        needed,
        have,
    };
    let past = |offset, size| ProgramError::BufferRangePastBuffer {
        set: 0,
        binding: 0,
        offset,
        size,
        len,
    };
    let misaligned = |binding, offset, alignment| ProgramError::BufferRangeMisaligned {
        set: 0,
        binding,
        offset,
        alignment,
    };
    let cases = [
        // An offset that is not a multiple of the limit for its kind of binding.
        (
            &tinted,
            range(tint(), &uniforms, uniform / 2, 16),
            misaligned(0, uniform / 2, uniform),
        ),
        (
            &tabled,
            range(table(), &storages, storage / 2, 32),
            misaligned(1, storage / 2, storage),
        ),
        // Fewer bytes than the declaration asks for, and none at all.
        (&tinted, range(tint(), &uniforms, 0, 8), too_small(0, 16, 8)),
        (
            &tabled,
            range(table(), &storages, 0, 16),
            too_small(1, 32, 16),
        ),
        (&tinted, range(tint(), &uniforms, 0, 0), too_small(0, 16, 0)),
        // A range that runs past the end of the buffer, or past what a u64 holds.
        (
            &tinted,
            range(tint(), &uniforms, 3 * uniform, 2 * uniform),
            past(3 * uniform, 2 * uniform),
        ),
        (
            &tinted,
            range(tint(), &uniforms, uniform, u64::MAX),
            past(uniform, u64::MAX),
        ),
    ];
    for (program, import, want) in cases {
        let refused = prepare(&mut runtime, program, (&[import], &[]));
        assert_eq!(refused.err(), Some(want));
    }
    // No refusal left a prepared draw behind.
    assert_eq!(runtime.stats().draws_prepared, 0);
}
