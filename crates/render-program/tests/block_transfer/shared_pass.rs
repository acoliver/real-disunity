//! Prepared dispatches recorded one after another into one compute pass, as a frame records a run
//! of them: each reads the storage image and the storage buffer the one before it wrote. A block
//! transfer has no compute form, so it ends a frame's run.

use std::sync::Arc;

use wgpu::{ShaderStages, StorageTextureAccess, TextureFormat, TextureViewDimension};
use worldrunner_render_post::PreparedCommands;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout, StorageAccess,
};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, DispatchRequest, PreparedDispatch, ProgramRuntime, TextureHandle,
};

use crate::common::{Gpu, texture, transfer};

/// One step of the chain: reads the value and the texel the step before wrote, writes the next
/// value and its texel into the other image.
const STEP: &str = "
@group(0) @binding(0) var source: texture_storage_2d_array<r32uint, read>;
@group(0) @binding(1) var destination: texture_storage_2d_array<r32uint, write>;
@group(0) @binding(2) var<storage, read_write> values: array<u32, 2>;
@compute @workgroup_size(1) fn main() {
    let next = values[0] * 31u + textureLoad(source, vec2<i32>(0, 0), 0).x + 7u;
    values[0] = next;
    values[1] = values[1] + 1u;
    textureStore(destination, vec2<i32>(0, 0), 0, vec4<u32>(next ^ 0x5a5au, 0u, 0u, 0u));
}";

/// The value and the step count the chain starts from; both images start at zero.
const START: [u32; 2] = [1, 0];

/// The values and the two images' texels after `steps` steps, step `k` reading image `k % 2`.
fn replay(steps: usize) -> ([u32; 2], [u32; 2]) {
    let (mut values, mut images) = (START, [0_u32; 2]);
    for step in 0..steps {
        let source = step % 2;
        let next = values[0]
            .wrapping_mul(31)
            .wrapping_add(images[source])
            .wrapping_add(7);
        values = [next, values[1] + 1];
        images[1 - source] = next ^ 0x5a5a;
    }
    (values, images)
}

fn step_program(runtime: &ProgramRuntime) -> worldrunner_render_program::runtime::ComputeProgram {
    let image = |access| ResourceKind::StorageImage {
        access,
        format: TextureFormat::R32Uint,
        dimension: TextureViewDimension::D2Array,
    };
    let kinds = [
        image(StorageTextureAccess::ReadOnly),
        image(StorageTextureAccess::WriteOnly),
        ResourceKind::Storage {
            access: StorageAccess::ReadWrite,
            min_size: 0,
        },
    ];
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(
            kinds
                .into_iter()
                .zip(0..)
                .map(|(kind, binding)| ResourceBinding {
                    at: BindingAddress { set: 0, binding },
                    source: BindingSource::Unnamed,
                    visibility: ShaderStages::COMPUTE,
                    kind,
                })
                .collect(),
        ),
        constants: Vec::new(),
    };
    let module = StageModule {
        source: StageSource::Wgsl(Arc::from(STEP)),
        entry_point: "main".to_owned(),
        kind: StageKind::Compute,
    };
    runtime
        .compute_program(&StageProgram::new(module, declarations).unwrap())
        .unwrap()
}

/// Records a copy of the one texel of `image` into a new readable buffer.
fn read_texel(
    runtime: &ProgramRuntime,
    encoder: &mut wgpu::CommandEncoder,
    image: TextureHandle,
) -> wgpu::Buffer {
    let buffer = runtime.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("texel readback"),
        size: 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        runtime.gpu_texture(image).unwrap().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout::default(),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    buffer
}

fn word(device: &wgpu::Device, buffer: &wgpu::Buffer) -> u32 {
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let word = u32::from_le_bytes([mapped[0], mapped[1], mapped[2], mapped[3]]);
    drop(mapped);
    buffer.unmap();
    word
}

#[test]
fn prepared_dispatches_in_one_pass_read_what_each_wrote() {
    const STEPS: usize = 33;
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let images = [
        texture(&mut runtime, TextureFormat::R32Uint, [1, 1], 1, 1),
        texture(&mut runtime, TextureFormat::R32Uint, [1, 1], 1, 1),
    ];
    let start: Vec<u8> = START.iter().flat_map(|word| word.to_le_bytes()).collect();
    let values = runtime.create_buffer("chain values", &start).unwrap();
    let program = step_program(&runtime);
    let steps: Vec<PreparedDispatch> = (0..2)
        .map(|source| {
            let bindings = [
                BoundResource::texture(0, 0, images[source]),
                BoundResource::texture(0, 1, images[1 - source]),
                BoundResource::buffer(0, 2, values),
            ];
            runtime
                .prepare_dispatch(&DispatchRequest {
                    program: &program,
                    bindings: &bindings,
                    groups: [1, 1, 1],
                })
                .unwrap()
        })
        .collect();
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    for step in &steps {
        PreparedCommands::retain(step, &frame).unwrap();
    }
    {
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("prepared compute"),
            timestamp_writes: None,
        });
        for step in 0..STEPS {
            PreparedCommands::compute(&steps[step % 2])
                .expect("a prepared dispatch has a compute form")
                .record_in_pass(&mut pass, "step")
                .unwrap();
        }
    }
    let pending = runtime.read_buffer(&mut frame, values).unwrap();
    let texels = images.map(|image| read_texel(&runtime, &mut frame, image));
    let index = frame.finish().submit().unwrap();
    let read: Vec<u32> = pending
        .resolve(index)
        .unwrap()
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
        .collect();
    let (expected_values, expected_images) = replay(STEPS);
    assert_eq!(read, expected_values, "values after {STEPS} steps");
    assert_eq!(
        texels.map(|buffer| word(&gpu.device, &buffer)),
        expected_images,
        "texels after {STEPS} steps"
    );
}

#[test]
fn a_dispatch_has_a_compute_form_and_a_transfer_has_none() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let images = [
        texture(&mut runtime, TextureFormat::R32Uint, [1, 1], 1, 1),
        texture(&mut runtime, TextureFormat::R32Uint, [1, 1], 1, 1),
    ];
    let values = runtime.create_buffer("chain values", &[0; 8]).unwrap();
    let program = step_program(&runtime);
    let bindings = [
        BoundResource::texture(0, 0, images[0]),
        BoundResource::texture(0, 1, images[1]),
        BoundResource::buffer(0, 2, values),
    ];
    let dispatch = runtime
        .prepare_dispatch(&DispatchRequest {
            program: &program,
            bindings: &bindings,
            groups: [1, 1, 1],
        })
        .unwrap();
    assert!(PreparedCommands::compute(&dispatch).is_some());
    let source = texture(&mut runtime, TextureFormat::Rg32Uint, [2, 2], 1, 1);
    let destination = texture(&mut runtime, TextureFormat::Bc4RUnorm, [8, 8], 1, 1);
    let moved = runtime
        .prepare_block_transfer(&transfer(
            (source, 0, 0, [0, 0], [2, 2]),
            (destination, 0, 0, [8, 8]),
        ))
        .unwrap();
    assert!(PreparedCommands::compute(&moved).is_none());
}
