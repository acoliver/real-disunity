//! Dispatches and transfers prepared ahead of the frame they are recorded into: everything that
//! can fail is checked when they are prepared, and recording them writes what `dispatch` and
//! `transfer_block_image` write.

use std::sync::Arc;

use wgpu::TextureFormat;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::{BlockTransferError, ProgramError};
use worldrunner_render_program::module::{StageDeclarations, StageKind};
use worldrunner_render_program::runtime::{
    BlockSource, BoundResource, DispatchRequest, Import, ImportedDispatch, Keepalive,
    LentBlockDestination,
};

use crate::common::{
    Gpu, dispatch_writer, read_blocks, read_texture_blocks, stage, submit, texture, transfer,
    writer_dispatch, writer_program,
};
use crate::encoded::bc4_blocks;

const GRID: [u32; 2] = [2, 2];
const EXTENT: [u32; 2] = [8, 8];

fn encoded() -> Vec<Vec<u8>> {
    bc4_blocks().iter().map(|block| block.bytes()).collect()
}

/// A BC4 texture of two layers that the runtime does not hold.
fn lent(gpu: &Gpu, format: TextureFormat) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lent blocks"),
        size: wgpu::Extent3d {
            width: EXTENT[0],
            height: EXTENT[1],
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn source(texture: worldrunner_render_program::runtime::TextureHandle) -> BlockSource {
    BlockSource {
        texture,
        mip: 0,
        layer: 0,
        origin: [0, 0],
        extent: GRID,
    }
}

#[test]
fn a_prepared_dispatch_and_transfer_fill_the_blocks_their_recorded_forms_fill() {
    let Some(gpu) = Gpu::new() else { return };
    let blocks = encoded();
    let mut runtime = gpu.runtime();
    let direct_source = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let staged_source = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let direct = texture(&mut runtime, TextureFormat::Bc4RUnorm, EXTENT, 1, 1);
    let staged = texture(&mut runtime, TextureFormat::Bc4RUnorm, EXTENT, 1, 1);
    let program = writer_program(&runtime, GRID, &blocks).unwrap();
    let bindings = [BoundResource::texture(0, 0, staged_source)];
    let ((recorded, prepared), index) = submit(&mut runtime, |runtime, encoder| {
        dispatch_writer(runtime, encoder, direct_source, GRID, &blocks)?;
        runtime.transfer_block_image(
            encoder,
            &transfer((direct_source, 0, 0, [0, 0], GRID), (direct, 0, 0, EXTENT)),
        )?;
        let dispatch =
            runtime.prepare_dispatch(&writer_dispatch(&program, &bindings, GRID, &blocks))?;
        let moved = runtime.prepare_block_transfer(&transfer(
            (staged_source, 0, 0, [0, 0], GRID),
            (staged, 0, 0, EXTENT),
        ))?;
        dispatch.retain(encoder)?;
        moved.retain(encoder)?;
        dispatch.record(encoder);
        moved.record(encoder);
        Ok((
            read_blocks(runtime, encoder, direct, (0, 0), GRID),
            read_blocks(runtime, encoder, staged, (0, 0), GRID),
        ))
    })
    .unwrap();
    let expected = blocks.concat();
    assert_eq!(recorded.resolve(&gpu.device, index.clone()), expected);
    assert_eq!(prepared.resolve(&gpu.device, index), expected);
    assert_eq!(
        runtime.stats().transfer_buffers_built,
        1,
        "both transfers use the runtime's one staging buffer"
    );
}

#[test]
fn a_prepared_transfer_into_a_lent_texture_fills_its_layer() {
    let Some(gpu) = Gpu::new() else { return };
    let blocks = encoded();
    let mut runtime = gpu.runtime();
    let scratch = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let destination = lent(&gpu, TextureFormat::Bc4RUnorm);
    let owner: Keepalive = Arc::new(destination.clone());
    let before = runtime.live_bytes();
    let ((layer, other), index) = submit(&mut runtime, |runtime, encoder| {
        dispatch_writer(runtime, encoder, scratch, GRID, &blocks)?;
        let moved = runtime.prepare_lent_block_transfer(
            &source(scratch),
            &LentBlockDestination {
                texture: destination.clone(),
                mip: 0,
                layer: 1,
                extent: EXTENT,
            },
            std::slice::from_ref(&owner),
        )?;
        moved.retain(encoder)?;
        moved.record(encoder);
        Ok((
            read_texture_blocks(&gpu.device, encoder, &destination, (0, 1), GRID),
            read_texture_blocks(&gpu.device, encoder, &destination, (0, 0), GRID),
        ))
    })
    .unwrap();
    assert_eq!(layer.resolve(&gpu.device, index.clone()), blocks.concat());
    assert_eq!(
        other.resolve(&gpu.device, index),
        vec![0; 32],
        "layer 0 untouched"
    );
    assert!(
        runtime.live_bytes() > before,
        "the staging buffer is counted, the lent texture is not"
    );
}

const IMPORTED_WGSL: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var blocks: texture_storage_2d_array<rg32uint, write>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let red = u32(round(textureLoad(source, vec2<i32>(id.xy), 0).r * 255.0));
    textureStore(blocks, vec2<i32>(id.xy), 0, vec4<u32>(red, 0u, 0u, 0u));
}";

#[test]
fn an_imported_dispatch_reads_a_lent_texture() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let reds = [10_u8, 20, 30, 40];
    let page = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lent page"),
        size: wgpu::Extent3d {
            width: GRID[0],
            height: GRID[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texels: Vec<u8> = reds.iter().flat_map(|red| [*red, 0, 0, 255]).collect();
    gpu.authority.queue().write_texture(
        page.as_image_copy(),
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(GRID[0] * 4),
            rows_per_image: Some(GRID[1]),
        },
        page.size(),
    );
    let binding = |binding, source, kind| ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source,
        visibility: wgpu::ShaderStages::COMPUTE,
        kind,
    };
    let kernel = stage(
        IMPORTED_WGSL,
        "main",
        StageKind::Compute,
        StageDeclarations {
            resources: ResourceLayout::new(vec![
                binding(
                    0,
                    BindingSource::Engine(EngineSemantic::named("page")),
                    ResourceKind::Texture {
                        dimension: wgpu::TextureViewDimension::D2,
                        sample: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                    },
                ),
                binding(
                    1,
                    BindingSource::Unnamed,
                    ResourceKind::StorageImage {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: TextureFormat::Rg32Uint,
                        dimension: wgpu::TextureViewDimension::D2Array,
                    },
                ),
            ]),
            constants: Vec::new(),
        },
    );
    let program = runtime.compute_program(&kernel).unwrap();
    let scratch = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let destination = texture(&mut runtime, TextureFormat::Bc4RUnorm, EXTENT, 1, 1);
    let view = page.create_view(&wgpu::TextureViewDescriptor::default());
    let imports = [Import::texture(
        EngineSemantic::named("page"),
        page.clone(),
        view,
    )];
    let (bytes, index) = submit(&mut runtime, |runtime, encoder| {
        let dispatch = runtime.prepare_imported_dispatch(&ImportedDispatch {
            program: &program,
            bindings: &[BoundResource::texture(0, 1, scratch)],
            imports: &imports,
            groups: [GRID[0], GRID[1], 1],
            owners: &[],
        })?;
        dispatch.retain(encoder)?;
        dispatch.record(encoder);
        runtime.transfer_block_image(
            encoder,
            &transfer((scratch, 0, 0, [0, 0], GRID), (destination, 0, 0, EXTENT)),
        )?;
        Ok(read_blocks(runtime, encoder, destination, (0, 0), GRID))
    })
    .unwrap();
    let expected: Vec<u8> = reds
        .iter()
        .flat_map(|red| [*red, 0, 0, 0, 0, 0, 0, 0])
        .collect();
    assert_eq!(bytes.resolve(&gpu.device, index), expected);
}

#[test]
fn preparation_refuses_by_name_before_anything_is_recorded_or_counted() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let scratch = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let blocks = texture(&mut runtime, TextureFormat::Bc4RUnorm, EXTENT, 1, 1);
    let plain = texture(&mut runtime, TextureFormat::Rgba8Unorm, EXTENT, 1, 1);
    let before = runtime.live_bytes();
    let refusal = |result: Result<_, ProgramError>| match result {
        Err(ProgramError::BlockTransfer(error)) => error,
        Err(other) => panic!("expected a block transfer refusal, found {other}"),
        Ok(_) => panic!("the transfer was prepared"),
    };
    assert!(matches!(
        refusal(runtime.prepare_block_transfer(&transfer(
            (scratch, 0, 0, [0, 0], GRID),
            (blocks, 0, 0, [12, 8]),
        ))),
        BlockTransferError::ExtentPastMip { .. } | BlockTransferError::BlockCountMismatch { .. }
    ));
    assert!(matches!(
        refusal(runtime.prepare_block_transfer(&transfer(
            (scratch, 0, 0, [0, 0], GRID),
            (plain, 0, 0, EXTENT),
        ))),
        BlockTransferError::DestinationNotCompressed { .. }
    ));
    let uncompressed = lent(&gpu, TextureFormat::Rgba8Unorm);
    assert!(matches!(
        refusal(runtime.prepare_lent_block_transfer(
            &source(scratch),
            &LentBlockDestination {
                texture: uncompressed,
                mip: 0,
                layer: 0,
                extent: EXTENT,
            },
            &[],
        )),
        BlockTransferError::DestinationNotCompressed { .. }
    ));
    let program = writer_program(&runtime, GRID, &encoded()).unwrap();
    let limit = gpu.device.limits().max_compute_workgroups_per_dimension;
    let groups = [1, limit + 1, 1];
    let refused = runtime.prepare_dispatch(&DispatchRequest {
        program: &program,
        bindings: &[BoundResource::texture(0, 0, scratch)],
        groups,
    });
    assert!(
        matches!(refused, Err(ProgramError::Workgroups { groups: found, limit: max }) if found == groups && max == limit),
        "{:?}",
        refused.err()
    );
    assert_eq!(runtime.stats().transfer_buffers_built, 0);
    assert_eq!(runtime.live_bytes(), before);
}
