//! A device with block compression, kernels that write encoded blocks, readbacks and draws.

use std::fmt::Write as _;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use wgpu::{ShaderStages, StorageTextureAccess, TextureFormat, TextureViewDimension};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::{BlockTransferError, ProgramError};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BlockDestination, BlockImageTransfer, BlockSource, BoundResource, DispatchRequest, DrawRequest,
    Geometry, ProgramRuntime, TargetImage, TextureHandle, TextureShape,
};
use worldrunner_render_program::state::RenderState;

const ADAPTER_BOUND: Duration = Duration::from_secs(30);

pub struct Gpu {
    pub device: wgpu::Device,
    pub authority: SubmissionAuthority,
}

impl Gpu {
    /// A device that has block compression, or `None` with the reason on stderr, so a test on an
    /// adapter without it says why it checked nothing.
    pub fn new() -> Option<Self> {
        let instance = worldrunner_render_gpu::test_support::test_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("the block transfer tests need a GPU adapter");
        let requirements = DeviceRequirements {
            optional_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
            ..DeviceRequirements::default()
        };
        let (device, queue, _) = pollster::block_on(request_device(&adapter, &requirements))
            .expect("the adapter gives a device");
        if !device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        {
            eprintln!(
                "SKIPPED: adapter {} lacks TEXTURE_COMPRESSION_BC, so there is no block-compressed texture to transfer into",
                adapter.get_info().name
            );
            return None;
        }
        Some(Self {
            device,
            authority: SubmissionAuthority::new(queue),
        })
    }

    pub fn runtime(&self) -> ProgramRuntime {
        self.runtime_with(AllocationBudget(1 << 24))
    }

    pub fn runtime_with(&self, budget: AllocationBudget) -> ProgramRuntime {
        ProgramRuntime::new(self.device.clone(), self.authority.clone(), budget)
    }
}

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

/// The source image's format for blocks of `bytes` bytes.
pub fn source_format(bytes: usize) -> TextureFormat {
    match bytes {
        8 => TextureFormat::Rg32Uint,
        16 => TextureFormat::Rgba32Uint,
        other => panic!("no source format for {other}-byte blocks"),
    }
}

pub fn texture(
    runtime: &mut ProgramRuntime,
    format: TextureFormat,
    size: [u32; 2],
    layers: u32,
    mips: u32,
) -> TextureHandle {
    let shape = TextureShape {
        size,
        layers,
        mips,
        format,
        storage: !format.is_compressed(),
        view: TextureViewDimension::D2Array,
    };
    runtime
        .create_empty_texture("block transfer", shape)
        .unwrap()
}

/// The block's bytes as the words a shader writes into one source texel.
fn words(block: &[u8]) -> [u32; 4] {
    let mut words = [0; 4];
    for (word, bytes) in words.iter_mut().zip(block.chunks(4)) {
        *word = u32::from_le_bytes(bytes.try_into().unwrap());
    }
    words
}

fn declarations(kind: ResourceKind, visibility: ShaderStages) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: BindingAddress { set: 0, binding: 0 },
            source: BindingSource::Unnamed,
            visibility,
            kind,
        }]),
        constants: Vec::new(),
    }
}

fn stage(source: &str, entry: &str, kind: StageKind, declared: StageDeclarations) -> StageProgram {
    let module = StageModule {
        source: StageSource::Wgsl(Arc::from(source)),
        entry_point: entry.to_owned(),
        kind,
    };
    StageProgram::new(module, declared).unwrap()
}

/// A kernel that writes `blocks`, laid out by layer, row and column, into a storage image of
/// `size` texels, as a game's encoder would write the blocks it made.
pub fn writer_source(format: TextureFormat, size: [u32; 2], blocks: &[Vec<u8>]) -> String {
    let name = if format == TextureFormat::Rg32Uint {
        "rg32uint"
    } else {
        "rgba32uint"
    };
    let mut source = format!(
        "@group(0) @binding(0) var image: texture_storage_2d_array<{name}, write>;\n\
         var<private> BLOCKS = array<vec4<u32>, {}>(",
        blocks.len()
    );
    for block in blocks {
        let [a, b, c, d] = words(block);
        write!(source, "vec4<u32>({a}u, {b}u, {c}u, {d}u),").unwrap();
    }
    write!(
        source,
        ");\n@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
         let block = BLOCKS[(id.z * {}u + id.y) * {}u + id.x];\n\
         textureStore(image, vec2<i32>(id.xy), i32(id.z), block);\n}}",
        size[1], size[0]
    )
    .unwrap();
    source
}

/// A source image holding `blocks` once the frame that `dispatch_writer` records has run.
pub fn dispatch_writer(
    runtime: &mut ProgramRuntime,
    encoder: &mut wgpu::CommandEncoder,
    texture: TextureHandle,
    size: [u32; 2],
    blocks: &[Vec<u8>],
) -> Result<(), ProgramError> {
    let format = source_format(blocks[0].len());
    let layers = u32::try_from(blocks.len()).unwrap() / (size[0] * size[1]);
    let image = ResourceKind::StorageImage {
        access: StorageTextureAccess::WriteOnly,
        format,
        dimension: TextureViewDimension::D2Array,
    };
    let kernel = stage(
        &writer_source(format, size, blocks),
        "main",
        StageKind::Compute,
        declarations(image, ShaderStages::COMPUTE),
    );
    let program = runtime.compute_program(&kernel)?;
    runtime.dispatch(
        encoder,
        &DispatchRequest {
            program: &program,
            bindings: &[BoundResource::texture(0, 0, texture)],
            groups: [size[0], size[1], layers],
        },
    )
}

pub fn transfer(
    source: (TextureHandle, u32, u32, [u32; 2], [u32; 2]),
    destination: (TextureHandle, u32, u32, [u32; 2]),
) -> BlockImageTransfer {
    BlockImageTransfer {
        source: BlockSource {
            texture: source.0,
            mip: source.1,
            layer: source.2,
            origin: source.3,
            extent: source.4,
        },
        destination: BlockDestination {
            texture: destination.0,
            mip: destination.1,
            layer: destination.2,
            extent: destination.3,
        },
    }
}

/// The refusal a transfer gets, recorded into a frame that is then dropped unsubmitted.
pub fn refusal(runtime: &mut ProgramRuntime, request: &BlockImageTransfer) -> BlockTransferError {
    let error = submit(runtime, |runtime, encoder| {
        runtime.transfer_block_image(encoder, request)
    })
    .unwrap_err();
    match error {
        ProgramError::BlockTransfer(error) => error,
        other => panic!("expected a block transfer refusal, found {other}"),
    }
}

/// A copy of one mip and layer of a texture into a mappable buffer, tightly addressed by block.
pub struct Readback {
    buffer: wgpu::Buffer,
    row: usize,
    padded_row: usize,
    rows: usize,
}

/// Records a copy of the physical blocks of a mip and layer: `blocks` columns and rows of
/// `block_bytes` bytes each.
pub fn read_blocks(
    runtime: &ProgramRuntime,
    encoder: &mut wgpu::CommandEncoder,
    texture: TextureHandle,
    at: (u32, u32),
    blocks: [u32; 2],
) -> Readback {
    let texture = runtime.gpu_texture(texture).unwrap();
    let block_bytes = texture.format().block_copy_size(None).unwrap();
    let row = (blocks[0] * block_bytes) as usize;
    let padded_row = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
    let buffer = runtime.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("block readback"),
        size: (padded_row * blocks[1] as usize) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: at.0,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: at.1,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(u32::try_from(padded_row).unwrap()),
                rows_per_image: Some(blocks[1]),
            },
        },
        wgpu::Extent3d {
            width: blocks[0] * 4,
            height: blocks[1] * 4,
            depth_or_array_layers: 1,
        },
    );
    Readback {
        buffer,
        row,
        padded_row,
        rows: blocks[1] as usize,
    }
}

impl Readback {
    /// The blocks' bytes, rows of blocks one after the other.
    pub fn resolve(self, device: &wgpu::Device, submission: wgpu::SubmissionIndex) -> Vec<u8> {
        let (sender, receiver) = mpsc::channel();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(ADAPTER_BOUND),
            })
            .unwrap();
        receiver.recv_timeout(ADAPTER_BOUND).unwrap().unwrap();
        let mapped = self.buffer.slice(..).get_mapped_range().unwrap();
        let bytes = mapped
            .chunks_exact(self.padded_row)
            .take(self.rows)
            .flat_map(|line| &line[..self.row])
            .copied()
            .collect();
        drop(mapped);
        self.buffer.unmap();
        bytes
    }
}

/// A full-screen triangle that loads each fragment's texel from one mip and layer of an array.
fn load_source(layer: u32, level: u32) -> String {
    format!(
        "@group(0) @binding(0) var source: texture_2d_array<f32>;\n\
         @vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {{\n\
             var corners = array<vec2<f32>, 3>(\n\
                 vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));\n\
             return vec4<f32>(corners[index], 0.0, 1.0);\n\
         }}\n\
         @fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {{\n\
             return textureLoad(source, vec2<i32>(at.xy), {layer}, {level});\n\
         }}"
    )
}

/// Records a draw of the texels of `texture` at `layer` and `level` into a new target of
/// `size`, and the read of that target.
pub fn sample(
    runtime: &mut ProgramRuntime,
    encoder: &mut wgpu::CommandEncoder,
    texture: TextureHandle,
    at: (u32, u32),
    size: [u32; 2],
) -> Result<worldrunner_render_program::runtime::PendingTargetRead, ProgramError> {
    let source = load_source(at.0, at.1);
    let resource = ResourceKind::Texture {
        dimension: TextureViewDimension::D2Array,
        sample: wgpu::TextureSampleType::Float { filterable: true },
        multisampled: false,
    };
    let vertex = stage(
        &source,
        "vs",
        StageKind::Vertex,
        StageDeclarations {
            resources: ResourceLayout::new(Vec::new()),
            constants: Vec::new(),
        },
    );
    let fragment = stage(
        &source,
        "fs",
        StageKind::Fragment,
        declarations(resource, ShaderStages::FRAGMENT),
    );
    let program = runtime.graphics_program(&vertex, &fragment)?;
    let target = runtime.create_render_target("sampled", size, TextureFormat::Rgba8Unorm, None)?;
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    runtime.clear_target(encoder, target, [0.0; 4], 1.0)?;
    runtime.draw(
        encoder,
        &DrawRequest {
            program: &program,
            state: &state,
            target,
            vertex_inputs: &[],
            bindings: &[BoundResource::texture(0, 0, texture)],
            geometry: Geometry::Vertices { first: 0, count: 3 },
        },
    )?;
    runtime.read_target(encoder, target)
}

pub fn pixel(image: &TargetImage, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).try_into().unwrap()
}
