use wgpu::util::DeviceExt;

use super::{
    BufferUse, BufferView, CheckedBindings, ContractError, RasterId, RasterPass, ResourceRegistry,
    ResourceUse, TextureCopy,
};

/// Work is executed in list order. No operation owns a queue or exposes an encoder.
#[derive(Clone, Debug)]
pub enum TypedWork {
    Raster(RasterPass),
    Fullscreen {
        pass: RasterPass,
        pipeline: wgpu::RenderPipeline,
    },
    Compute {
        pipeline: wgpu::ComputePipeline,
        bindings: Vec<CheckedBindings>,
        workgroups: [u32; 3],
    },
    CopyBuffer {
        source: BufferView,
        destination: BufferView,
    },
    Upload {
        destination: BufferView,
        bytes: Vec<u8>,
    },
    CopyTexture {
        source: TextureCopy,
        destination: TextureCopy,
    },
    CopyBufferToTexture {
        source: BufferView,
        destination: TextureCopy,
        bytes_per_row: u32,
        rows_per_image: u32,
    },
    CopyTextureToBuffer {
        source: TextureCopy,
        destination: BufferView,
        bytes_per_row: u32,
        rows_per_image: u32,
    },
    ReadbackBuffer {
        source: BufferView,
        destination: BufferView,
    },
    ReadbackTexture {
        source: TextureCopy,
        destination: BufferView,
        bytes_per_row: u32,
        rows_per_image: u32,
    },
}
#[derive(Clone, Debug)]
pub struct TypedOperation {
    pub id: RasterId,
    pub textures: Vec<ResourceUse>,
    pub buffers: Vec<BufferUse>,
    pub work: TypedWork,
}
impl TypedOperation {
    pub fn new(id: RasterId, work: TypedWork) -> Self {
        Self {
            id,
            textures: Vec::new(),
            buffers: Vec::new(),
            work,
        }
    }
    pub(super) fn raster(&self) -> Option<&RasterPass> {
        match &self.work {
            TypedWork::Raster(pass) | TypedWork::Fullscreen { pass, .. } => Some(pass),
            _ => None,
        }
    }
    pub(super) fn bindings(&self) -> &[CheckedBindings] {
        match &self.work {
            TypedWork::Compute { bindings, .. } => bindings,
            _ => self.raster().map_or(&[], |pass| &pass.bindings),
        }
    }
}
/// Immutable compiled bytes and resource handles. Revalidation precedes every encoding.
pub struct CompiledOperations {
    pub(super) operations: Vec<TypedOperation>,
    pub(super) accesses: Vec<(Vec<ResourceUse>, Vec<BufferUse>)>,
    pub(super) staging: Vec<Option<wgpu::Buffer>>,
}
impl CompiledOperations {
    pub fn ids(&self) -> impl Iterator<Item = RasterId> + '_ {
        self.operations.iter().map(|op| op.id)
    }
    pub fn encode(
        &self,
        registry: &ResourceRegistry,
        encoder: &mut wgpu::CommandEncoder,
        mut timer: Option<&mut crate::PassTimer>,
        mut draw: impl FnMut(RasterId, &mut wgpu::RenderPass<'_>),
    ) -> Result<(), ContractError> {
        for (textures, buffers) in &self.accesses {
            for usage in textures {
                registry.check_view(&usage.view)?;
            }
            for usage in buffers {
                registry.check_buffer(&usage.view)?;
            }
        }
        for (index, operation) in self.operations.iter().enumerate() {
            let stamp = timer
                .as_deref_mut()
                .and_then(|timer| timer.stamp(crate::PassId::Operation(operation.id)));
            operation.encode(
                encoder,
                self.staging[index].as_ref(),
                stamp.as_ref(),
                &mut draw,
            );
        }
        Ok(())
    }
}
impl TypedOperation {
    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        staging: Option<&wgpu::Buffer>,
        stamp: Option<&crate::timing::Stamp>,
        draw: &mut impl FnMut(RasterId, &mut wgpu::RenderPass<'_>),
    ) {
        match &self.work {
            TypedWork::Raster(pass) => pass.encode(encoder, stamp, |render| draw(self.id, render)),
            TypedWork::Fullscreen { pass, pipeline } => pass.encode(encoder, stamp, |render| {
                render.set_pipeline(pipeline);
                render.draw(0..3, 0..1);
            }),
            TypedWork::Compute {
                pipeline,
                bindings,
                workgroups,
            } => {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(self.id.stage.name),
                    timestamp_writes: stamp.map(crate::timing::Stamp::compute),
                });
                pass.set_pipeline(pipeline);
                for (index, binding) in bindings.iter().enumerate() {
                    pass.set_bind_group(
                        u32::try_from(index).expect("validated bind group count"),
                        binding.group(),
                        binding.dynamic_offsets(),
                    );
                }
                pass.dispatch_workgroups(workgroups[0], workgroups[1], workgroups[2]);
            }
            _ => {
                timestamp_boundary(encoder, stamp, true);
                self.encode_transfer(encoder, staging);
                timestamp_boundary(encoder, stamp, false);
            }
        }
    }
    fn encode_transfer(&self, encoder: &mut wgpu::CommandEncoder, staging: Option<&wgpu::Buffer>) {
        match &self.work {
            TypedWork::CopyBuffer {
                source,
                destination,
            }
            | TypedWork::ReadbackBuffer {
                source,
                destination,
            } => encoder.copy_buffer_to_buffer(
                &source.buffer,
                source.range.start,
                &destination.buffer,
                destination.range.start,
                source.range.end - source.range.start,
            ),
            TypedWork::Upload { destination, bytes } => encoder.copy_buffer_to_buffer(
                staging.expect("compiled upload staging"),
                0,
                &destination.buffer,
                destination.range.start,
                bytes.len() as u64,
            ),
            TypedWork::CopyTexture {
                source,
                destination,
            } => encoder.copy_texture_to_texture(source.info(), destination.info(), source.size()),
            TypedWork::CopyBufferToTexture {
                source,
                destination,
                bytes_per_row,
                rows_per_image,
            } => encoder.copy_buffer_to_texture(
                super::transfers::buffer_info(source, *bytes_per_row, *rows_per_image),
                destination.info(),
                destination.size(),
            ),
            TypedWork::CopyTextureToBuffer {
                source,
                destination,
                bytes_per_row,
                rows_per_image,
            }
            | TypedWork::ReadbackTexture {
                source,
                destination,
                bytes_per_row,
                rows_per_image,
            } => encoder.copy_texture_to_buffer(
                source.info(),
                super::transfers::buffer_info(destination, *bytes_per_row, *rows_per_image),
                source.size(),
            ),
            _ => unreachable!("render and compute operations handled separately"),
        }
    }
}
fn timestamp_boundary(
    encoder: &mut wgpu::CommandEncoder,
    stamp: Option<&crate::timing::Stamp>,
    begin: bool,
) {
    // Empty boundary passes use portable pass timestamps, without requiring
    // the optional encoder-timestamp feature for copy-only operations.
    if let Some(stamp) = stamp {
        let writes = stamp.compute();
        let _pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("transfer timing boundary"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: writes.query_set,
                beginning_of_pass_write_index: begin
                    .then_some(writes.beginning_of_pass_write_index.unwrap()),
                end_of_pass_write_index: (!begin)
                    .then_some(writes.end_of_pass_write_index.unwrap()),
            }),
        });
    }
}
pub(super) fn stage_uploads(
    device: &wgpu::Device,
    operations: &[TypedOperation],
) -> Vec<Option<wgpu::Buffer>> {
    operations
        .iter()
        .map(|op| match &op.work {
            TypedWork::Upload { bytes, .. } => Some(device.create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("ordered upload staging"),
                    contents: bytes,
                    usage: wgpu::BufferUsages::COPY_SRC,
                },
            )),
            _ => None,
        })
        .collect()
}
