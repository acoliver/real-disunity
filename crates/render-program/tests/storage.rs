//! Storage images through the neutral interface: what a declaration must say to match a shader's
//! storage texture, and how a kernel's writes reach a later draw, a later kernel and a readback.
//!
//! The textures the kernels write hold patterns computed from the texel's coordinates, so every
//! expected byte is predicted from the formula in the kernel and none is read back from a
//! reference run.

use std::sync::Arc;

use wgpu::{
    ShaderStages, StorageTextureAccess, TextureDimension, TextureFormat, TextureViewDimension,
};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::{ProgramError, StorageImageError};
use worldrunner_render_program::layout::{LayoutError, ProgramLayout};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BoundResource, BufferHandle, ComputeProgram, DispatchRequest, DrawRequest, Geometry,
    GraphicsProgram, ProgramRuntime, TargetImage, TextureHandle,
};
use worldrunner_render_program::state::RenderState;

const WRITE_RGBA8: &str = "
@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(4, 4) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(image, vec2<i32>(id.xy), vec4<f32>(
        f32(16u * id.x + 1u) / 255.0,
        f32(16u * id.y + 2u) / 255.0,
        f32(id.x + 4u * id.y) / 255.0,
        1.0));
}";

fn declared(binding: u32, visibility: ShaderStages, kind: ResourceKind) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: BindingAddress { set: 0, binding },
            source: BindingSource::Unnamed,
            visibility,
            kind,
        }]),
        constants: Vec::new(),
    }
}

fn image(
    access: StorageTextureAccess,
    format: TextureFormat,
    dimension: TextureViewDimension,
) -> ResourceKind {
    ResourceKind::StorageImage {
        access,
        format,
        dimension,
    }
}

fn kernel(source: &str, declarations: StageDeclarations) -> StageProgram {
    let module = StageModule {
        source: StageSource::Wgsl(Arc::from(source)),
        entry_point: "main".to_owned(),
        kind: StageKind::Compute,
    };
    StageProgram::new(module, declarations).unwrap()
}

#[test]
fn a_declared_storage_image_is_a_slot_of_the_shaders_binding_type() {
    let declarations = declared(
        0,
        ShaderStages::COMPUTE,
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::Rgba8Unorm,
            TextureViewDimension::D2,
        ),
    );
    let stage = kernel(WRITE_RGBA8, declarations);
    let layout = ProgramLayout::build(&[&stage]).unwrap();
    let slot = layout.slot(0, 0).unwrap();
    assert_eq!(
        slot.ty,
        wgpu::BindingType::StorageTexture {
            access: StorageTextureAccess::WriteOnly,
            format: TextureFormat::Rgba8Unorm,
            view_dimension: TextureViewDimension::D2,
        }
    );
    assert_eq!(slot.visibility, ShaderStages::COMPUTE);
    assert_eq!(slot.min_size, 0);
}

#[test]
fn a_declaration_that_differs_in_access_format_or_dimension_is_a_mismatch() {
    let differing = [
        image(
            StorageTextureAccess::ReadWrite,
            TextureFormat::Rgba8Unorm,
            TextureViewDimension::D2,
        ),
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::Rgba16Float,
            TextureViewDimension::D2,
        ),
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::Rgba8Unorm,
            TextureViewDimension::D2Array,
        ),
        ResourceKind::Storage {
            access: worldrunner_render_program::binding::StorageAccess::ReadWrite,
            min_size: 0,
        },
    ];
    for wrong in differing {
        let stage = kernel(WRITE_RGBA8, declared(0, ShaderStages::COMPUTE, wrong));
        let error = ProgramLayout::build(&[&stage]).unwrap_err();
        assert_eq!(
            error,
            LayoutError::Mismatch {
                set: 0,
                binding: 0,
                declared: format!("{wrong:?}"),
                found: format!(
                    "{:?}",
                    wgpu::BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: TextureFormat::Rgba8Unorm,
                        view_dimension: TextureViewDimension::D2,
                    }
                ),
            }
        );
    }
}

struct Gpu {
    device: wgpu::Device,
    authority: SubmissionAuthority,
}

impl Gpu {
    fn new() -> Self {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("the storage image tests need a GPU adapter");
        eprintln!(
            "render-program storage tests adapter: {}",
            adapter.get_info().name
        );
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
                .expect("the adapter gives a device");
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
        }
    }

    fn runtime(&self) -> ProgramRuntime {
        ProgramRuntime::new(
            self.device.clone(),
            self.authority.clone(),
            AllocationBudget(1 << 24),
        )
    }
}

fn submit<T>(
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

fn dispatch(
    runtime: &mut ProgramRuntime,
    program: &ComputeProgram,
    bindings: &[BoundResource],
    groups: [u32; 3],
) -> Result<(), ProgramError> {
    submit(runtime, |runtime, frame| {
        runtime.dispatch(
            frame,
            &DispatchRequest {
                program,
                bindings,
                groups,
            },
        )
    })
    .map(|_| ())
}

fn writer(runtime: &ProgramRuntime) -> ComputeProgram {
    let declarations = declared(
        0,
        ShaderStages::COMPUTE,
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::Rgba8Unorm,
            TextureViewDimension::D2,
        ),
    );
    runtime
        .compute_program(&kernel(WRITE_RGBA8, declarations))
        .unwrap()
}

fn pattern(x: u32, y: u32) -> [u8; 4] {
    [
        u8::try_from(16 * x + 1).unwrap(),
        u8::try_from(16 * y + 2).unwrap(),
        u8::try_from(x + 4 * y).unwrap(),
        255,
    ]
}

fn assert_pattern(image: &TargetImage) {
    assert_eq!(image.size, [4, 4]);
    assert_eq!(image.format, TextureFormat::Rgba8Unorm);
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(image.pixel(x, y), pattern(x, y), "texel ({x}, {y})");
        }
    }
}

/// A full-screen triangle that loads the texel under each fragment from a sampled texture.
const COPY_TO_TARGET: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(corners[index], 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(at.xy), 0);
}";

fn sampling_program(runtime: &ProgramRuntime) -> GraphicsProgram {
    let stage = |entry: &str, kind, visibility, declared: bool| {
        let resources = if declared {
            vec![ResourceBinding {
                at: BindingAddress { set: 0, binding: 0 },
                source: BindingSource::Unnamed,
                visibility,
                kind: ResourceKind::Texture {
                    dimension: TextureViewDimension::D2,
                    sample: wgpu::TextureSampleType::Float { filterable: true },
                    multisampled: false,
                },
            }]
        } else {
            Vec::new()
        };
        let module = StageModule {
            source: StageSource::Wgsl(Arc::from(COPY_TO_TARGET)),
            entry_point: entry.to_owned(),
            kind,
        };
        let declarations = StageDeclarations {
            resources: ResourceLayout::new(resources),
            constants: Vec::new(),
        };
        StageProgram::new(module, declarations).unwrap()
    };
    let vertex = stage("vs", StageKind::Vertex, ShaderStages::VERTEX, false);
    let fragment = stage("fs", StageKind::Fragment, ShaderStages::FRAGMENT, true);
    runtime.graphics_program(&vertex, &fragment).unwrap()
}

fn no_cull() -> RenderState {
    let mut state = RenderState::opaque_depth();
    state.raster.cull = None;
    state
}

#[test]
fn a_kernels_writes_reach_a_later_draw_and_a_readback_in_one_submission() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let writer = writer(&runtime);
    let sampler = sampling_program(&runtime);
    let texture = runtime
        .create_storage_texture(
            "pattern",
            [4, 4, 1],
            TextureDimension::D2,
            TextureFormat::Rgba8Unorm,
        )
        .unwrap();
    let target = runtime
        .create_render_target("out", [4, 4], TextureFormat::Rgba8Unorm, None)
        .unwrap();
    let state = no_cull();
    let ((drawn, copy), index) = submit(&mut runtime, |runtime, frame| {
        runtime.clear_target(frame, target, [0.0; 4], 1.0)?;
        runtime.dispatch(
            frame,
            &DispatchRequest {
                program: &writer,
                bindings: &[BoundResource::texture(0, 0, texture)],
                groups: [1, 1, 1],
            },
        )?;
        runtime.draw(
            frame,
            &DrawRequest {
                program: &sampler,
                state: &state,
                target,
                vertex_inputs: &[],
                bindings: &[BoundResource::texture(0, 0, texture)],
                geometry: Geometry::Vertices { first: 0, count: 3 },
            },
        )?;
        let drawn = runtime.read_target(frame, target)?;
        let copy = runtime.read_texture(frame, texture)?;
        Ok((drawn, copy))
    })
    .unwrap();
    assert_pattern(&drawn.resolve(index.clone()).unwrap());
    assert_pattern(&copy.resolve(index).unwrap());
}

fn refusal(error: ProgramError) -> (u32, u32, StorageImageError) {
    match error {
        ProgramError::StorageImage {
            set,
            binding,
            error,
        } => (set, binding, error),
        other => panic!("expected a storage image refusal, found {other}"),
    }
}

fn texture_for(
    runtime: &mut ProgramRuntime,
    size: [u32; 3],
    dimension: TextureDimension,
    format: TextureFormat,
) -> TextureHandle {
    runtime
        .create_storage_texture("t", size, dimension, format)
        .unwrap()
}

#[test]
fn a_texture_in_another_format_than_the_shaders_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let writer = writer(&runtime);
    let texture = texture_for(
        &mut runtime,
        [4, 4, 1],
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
    );
    let error = dispatch(
        &mut runtime,
        &writer,
        &[BoundResource::texture(0, 0, texture)],
        [1, 1, 1],
    )
    .unwrap_err();
    assert_eq!(
        refusal(error),
        (
            0,
            0,
            StorageImageError::Format {
                declared: TextureFormat::Rgba8Unorm,
                found: TextureFormat::Rgba16Float,
            }
        )
    );
}

#[test]
fn a_texture_that_does_not_allow_storage_binding_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let writer = writer(&runtime);
    let uploaded = runtime
        .create_texture_2d("uploaded", [4, 4], TextureFormat::Rgba8Unorm, &[0; 64])
        .unwrap();
    let error = dispatch(
        &mut runtime,
        &writer,
        &[BoundResource::texture(0, 0, uploaded)],
        [1, 1, 1],
    )
    .unwrap_err();
    let (set, binding, found) = refusal(error);
    assert_eq!((set, binding), (0, 0));
    assert!(
        matches!(found, StorageImageError::Usage { usage } if !usage.contains(wgpu::TextureUsages::STORAGE_BINDING)),
        "{found}"
    );
}

#[test]
fn a_texture_of_another_dimension_than_the_shaders_view_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let writer = writer(&runtime);
    let volume = texture_for(
        &mut runtime,
        [4, 4, 4],
        TextureDimension::D3,
        TextureFormat::Rgba8Unorm,
    );
    let layers = texture_for(
        &mut runtime,
        [4, 4, 3],
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
    );
    for (texture, found, count) in [
        (volume, TextureDimension::D3, 4),
        (layers, TextureDimension::D2, 3),
    ] {
        let error = dispatch(
            &mut runtime,
            &writer,
            &[BoundResource::texture(0, 0, texture)],
            [1, 1, 1],
        )
        .unwrap_err();
        assert_eq!(
            refusal(error),
            (
                0,
                0,
                StorageImageError::Dimension {
                    declared: TextureViewDimension::D2,
                    found,
                    layers: count,
                }
            )
        );
    }
}

#[test]
fn a_format_the_device_cannot_bind_as_storage_is_refused_when_the_program_is_built() {
    // The device is made without the feature that lets BGRA8 be a storage format.
    let gpu = Gpu::new();
    assert!(
        !gpu.device
            .features()
            .contains(wgpu::Features::BGRA8UNORM_STORAGE)
    );
    let runtime = gpu.runtime();
    let source = "
        @group(0) @binding(0) var image: texture_storage_2d<bgra8unorm, write>;
        @compute @workgroup_size(1) fn main() {
            textureStore(image, vec2<i32>(0, 0), vec4<f32>(1.0));
        }";
    let declarations = declared(
        0,
        ShaderStages::COMPUTE,
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::Bgra8Unorm,
            TextureViewDimension::D2,
        ),
    );
    let error = runtime
        .compute_program(&kernel(source, declarations))
        .err()
        .unwrap();
    assert_eq!(
        refusal(error),
        (
            0,
            0,
            StorageImageError::Unsupported {
                format: TextureFormat::Bgra8Unorm,
                access: StorageTextureAccess::WriteOnly,
            }
        )
    );
    assert!(
        runtime.supports_storage_image(TextureFormat::Rgba8Unorm, StorageTextureAccess::WriteOnly)
    );
    assert!(
        !runtime.supports_storage_image(TextureFormat::Bgra8Unorm, StorageTextureAccess::WriteOnly)
    );
    assert!(!runtime.supports_storage_image(
        TextureFormat::Rgba8UnormSrgb,
        StorageTextureAccess::WriteOnly
    ));
}

#[test]
fn one_texture_bound_as_storage_and_as_sampled_in_one_dispatch_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let source = "
        @group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
        @group(0) @binding(1) var seen: texture_2d<f32>;
        @compute @workgroup_size(1) fn main() {
            textureStore(image, vec2<i32>(0, 0), textureLoad(seen, vec2<i32>(0, 0), 0));
        }";
    let kinds = [
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::Rgba8Unorm,
            TextureViewDimension::D2,
        ),
        ResourceKind::Texture {
            dimension: TextureViewDimension::D2,
            sample: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: false,
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
    let program = runtime
        .compute_program(&kernel(source, declarations))
        .unwrap();
    let shared = texture_for(
        &mut runtime,
        [4, 4, 1],
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
    );
    let other = texture_for(
        &mut runtime,
        [4, 4, 1],
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
    );
    let error = dispatch(
        &mut runtime,
        &program,
        &[
            BoundResource::texture(0, 0, shared),
            BoundResource::texture(0, 1, shared),
        ],
        [1, 1, 1],
    )
    .unwrap_err();
    assert_eq!(
        error,
        ProgramError::AliasedTexture {
            set: 0,
            binding: 0,
            other_set: 0,
            other_binding: 1,
        }
    );
    // Two textures are two resources, and the dispatch runs.
    dispatch(
        &mut runtime,
        &program,
        &[
            BoundResource::texture(0, 0, shared),
            BoundResource::texture(0, 1, other),
        ],
        [1, 1, 1],
    )
    .unwrap();
}

fn declared_all(kinds: &[(ResourceKind, ShaderStages)]) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(
            kinds
                .iter()
                .zip(0..)
                .map(|((kind, visibility), binding)| ResourceBinding {
                    at: BindingAddress { set: 0, binding },
                    source: BindingSource::Unnamed,
                    visibility: *visibility,
                    kind: *kind,
                })
                .collect(),
        ),
        constants: Vec::new(),
    }
}

/// Reads every texel of a 4-wide `r32uint` image of any dimension into `output` with a kernel
/// that only has read access, one invocation for each texel of an `extent` volume.
fn dump(
    runtime: &mut ProgramRuntime,
    declaration: (&str, TextureViewDimension, &str),
    texture: TextureHandle,
    extent: [u32; 3],
) -> Vec<u32> {
    let (type_name, dimension, load) = declaration;
    let source = format!(
        "@group(0) @binding(0) var image: {type_name};
         @group(0) @binding(1) var<storage, read_write> output: array<u32>;
         @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
             output[id.x + {w}u * (id.y + {h}u * id.z)] = {load};
         }}",
        w = extent[0],
        h = extent[1],
    );
    let declarations = declared_all(&[
        (
            image(
                StorageTextureAccess::ReadOnly,
                TextureFormat::R32Uint,
                dimension,
            ),
            ShaderStages::COMPUTE,
        ),
        (
            ResourceKind::Storage {
                access: worldrunner_render_program::binding::StorageAccess::ReadWrite,
                min_size: 0,
            },
            ShaderStages::COMPUTE,
        ),
    ]);
    let program = runtime
        .compute_program(&kernel(&source, declarations))
        .unwrap();
    let count = (extent[0] * extent[1] * extent[2]) as usize;
    let output: BufferHandle = runtime
        .create_buffer("output", &vec![0xff; count * 4])
        .unwrap();
    let (pending, index) = submit(runtime, |runtime, frame| {
        runtime.dispatch(
            frame,
            &DispatchRequest {
                program: &program,
                bindings: &[
                    BoundResource::texture(0, 0, texture),
                    BoundResource::buffer(0, 1, output),
                ],
                groups: extent,
            },
        )?;
        runtime.read_buffer(frame, output)
    })
    .unwrap();
    pending
        .resolve(index)
        .unwrap()
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
        .collect()
}

fn write_r32uint(
    runtime: &mut ProgramRuntime,
    declaration: (&str, TextureViewDimension, &str),
    texture: TextureHandle,
    extent: [u32; 3],
) {
    let (type_name, dimension, store) = declaration;
    let source = format!(
        "@group(0) @binding(0) var image: {type_name};
         @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
             {store}
         }}"
    );
    let declarations = declared(
        0,
        ShaderStages::COMPUTE,
        image(
            StorageTextureAccess::WriteOnly,
            TextureFormat::R32Uint,
            dimension,
        ),
    );
    let program = runtime
        .compute_program(&kernel(&source, declarations))
        .unwrap();
    dispatch(
        runtime,
        &program,
        &[BoundResource::texture(0, 0, texture)],
        extent,
    )
    .unwrap();
}

#[test]
fn a_2d_array_takes_a_write_in_every_layer_and_is_read_back_by_a_read_only_view() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let extent = [3, 2, 4];
    let texture = texture_for(
        &mut runtime,
        extent,
        TextureDimension::D2,
        TextureFormat::R32Uint,
    );
    write_r32uint(
        &mut runtime,
        (
            "texture_storage_2d_array<r32uint, write>",
            TextureViewDimension::D2Array,
            "textureStore(image, vec2<i32>(id.xy), i32(id.z), vec4<u32>(100u * id.z + 10u * id.y + id.x, 0u, 0u, 0u));",
        ),
        texture,
        extent,
    );
    let found = dump(
        &mut runtime,
        (
            "texture_storage_2d_array<r32uint, read>",
            TextureViewDimension::D2Array,
            "textureLoad(image, vec2<i32>(id.xy), i32(id.z)).x",
        ),
        texture,
        extent,
    );
    let mut expected = Vec::new();
    for z in 0..4 {
        for y in 0..2 {
            for x in 0..3 {
                expected.push(100 * z + 10 * y + x);
            }
        }
    }
    assert_eq!(found, expected);
}

#[test]
fn a_3d_texture_takes_a_write_in_every_slice_and_is_read_back_by_a_read_only_view() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let extent = [2, 3, 4];
    let texture = texture_for(
        &mut runtime,
        extent,
        TextureDimension::D3,
        TextureFormat::R32Uint,
    );
    write_r32uint(
        &mut runtime,
        (
            "texture_storage_3d<r32uint, write>",
            TextureViewDimension::D3,
            "textureStore(image, vec3<i32>(id), vec4<u32>(100u * id.z + 10u * id.y + id.x, 0u, 0u, 0u));",
        ),
        texture,
        extent,
    );
    let found = dump(
        &mut runtime,
        (
            "texture_storage_3d<r32uint, read>",
            TextureViewDimension::D3,
            "textureLoad(image, vec3<i32>(id)).x",
        ),
        texture,
        extent,
    );
    let mut expected = Vec::new();
    for z in 0..4 {
        for y in 0..3 {
            for x in 0..2 {
                expected.push(100 * z + 10 * y + x);
            }
        }
    }
    assert_eq!(found, expected);
}

#[test]
fn a_read_write_view_sees_what_the_dispatch_before_it_wrote() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let extent = [4, 4, 1];
    let texture = texture_for(
        &mut runtime,
        extent,
        TextureDimension::D2,
        TextureFormat::R32Uint,
    );
    write_r32uint(
        &mut runtime,
        (
            "texture_storage_2d<r32uint, write>",
            TextureViewDimension::D2,
            "textureStore(image, vec2<i32>(id.xy), vec4<u32>(10u * id.y + id.x, 0u, 0u, 0u));",
        ),
        texture,
        extent,
    );
    let source = "
        @group(0) @binding(0) var image: texture_storage_2d<r32uint, read_write>;
        @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
            let at = vec2<i32>(id.xy);
            textureStore(image, at, vec4<u32>(textureLoad(image, at).x + 1000u, 0u, 0u, 0u));
        }";
    let declarations = declared(
        0,
        ShaderStages::COMPUTE,
        image(
            StorageTextureAccess::ReadWrite,
            TextureFormat::R32Uint,
            TextureViewDimension::D2,
        ),
    );
    let increment = runtime
        .compute_program(&kernel(source, declarations))
        .unwrap();
    dispatch(
        &mut runtime,
        &increment,
        &[BoundResource::texture(0, 0, texture)],
        extent,
    )
    .unwrap();
    let (pending, index) = submit(&mut runtime, |runtime, frame| {
        runtime.read_texture(frame, texture)
    })
    .unwrap();
    let image = pending.resolve(index).unwrap();
    assert_eq!(image.format, TextureFormat::R32Uint);
    for y in 0..4u32 {
        for x in 0..4u32 {
            let texel = image.pixel(x, y);
            let value = u32::from_le_bytes([texel[0], texel[1], texel[2], texel[3]]);
            assert_eq!(value, 1000 + 10 * y + x, "texel ({x}, {y})");
        }
    }
}

#[test]
fn a_dispatch_can_be_checked_without_recording_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let writer = writer(&runtime);
    let good = texture_for(
        &mut runtime,
        [4, 4, 1],
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
    );
    let wrong = texture_for(
        &mut runtime,
        [4, 4, 1],
        TextureDimension::D2,
        TextureFormat::R32Float,
    );
    runtime
        .check_storage_images(&writer, &[BoundResource::texture(0, 0, good)])
        .unwrap();
    let error = runtime
        .check_storage_images(&writer, &[BoundResource::texture(0, 0, wrong)])
        .unwrap_err();
    assert_eq!(
        refusal(error),
        (
            0,
            0,
            StorageImageError::Format {
                declared: TextureFormat::Rgba8Unorm,
                found: TextureFormat::R32Float,
            }
        )
    );
}
