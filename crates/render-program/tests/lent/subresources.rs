//! One texture bound at several subresources in one dispatch: views of distinct mips may sit in
//! one dispatch while one of them is written, as a single-pass downsampler binds a source mip and
//! the mips it writes, and only the subresources a storage image covers count as written. Views
//! whose ranges overlap, where one writes, are still refused, and so is a view that states no
//! range, which covers the whole texture.
//!
//! wgpu's no-op device validates groups, passes and their usage as a real device does and runs
//! nothing.

use std::ops::Range;
use std::sync::Arc;

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::PreparedWrite;
use worldrunner_render_post::contracts::TextureRange;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    ComputeProgram, Import, ImportedDispatch, PreparedDispatch, ProgramRuntime,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// Reads the source at twice the invocation's texel and writes it into the first image and one
/// more into the second.
const WGSL: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var first: texture_storage_2d<r32float, write>;
@group(0) @binding(2) var second: texture_storage_2d<r32float, write>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let value = textureLoad(source, vec2<i32>(id.xy) * 2, 0);
    textureStore(first, vec2<i32>(id.xy), value);
    textureStore(second, vec2<i32>(id.xy), value + vec4<f32>(1.0));
}";

/// The same kernel with the images before the source: binding 0 and 2 are written and binding 1
/// is sampled.
const WRITTEN_FIRST_WGSL: &str = "
@group(0) @binding(0) var first: texture_storage_2d<r32float, write>;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var second: texture_storage_2d<r32float, write>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let value = textureLoad(source, vec2<i32>(id.xy) * 2, 0);
    textureStore(first, vec2<i32>(id.xy), value);
    textureStore(second, vec2<i32>(id.xy), value + vec4<f32>(1.0));
}";

const NAMES: [&str; 3] = ["sub.source", "sub.first", "sub.second"];

struct Rig {
    device: wgpu::Device,
    runtime: ProgramRuntime,
    program: ComputeProgram,
    /// An `R32Float` texture of 16 x 16 texels and four mips that kernels sample and write.
    texture: wgpu::Texture,
    /// The same, in two array layers.
    layered: wgpu::Texture,
}

impl Rig {
    fn new() -> Self {
        Self::with_order(false)
    }

    /// A rig whose kernel samples binding 0 and writes bindings 1 and 2, or, with
    /// `written_first`, writes bindings 0 and 2 and samples binding 1.
    fn with_order(written_first: bool) -> Self {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let runtime = ProgramRuntime::new(
            device.clone(),
            SubmissionAuthority::new(queue),
            AllocationBudget(1 << 20),
        );
        let binding = |binding: u32, kind| ResourceBinding {
            at: BindingAddress { set: 0, binding },
            source: BindingSource::Engine(EngineSemantic::named(NAMES[binding as usize])),
            visibility: wgpu::ShaderStages::COMPUTE,
            kind,
        };
        let image = ResourceKind::StorageImage {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: FORMAT,
            dimension: wgpu::TextureViewDimension::D2,
        };
        let sampled = ResourceKind::Texture {
            dimension: wgpu::TextureViewDimension::D2,
            sample: wgpu::TextureSampleType::Float { filterable: false },
            multisampled: false,
        };
        let (wgsl, kinds) = if written_first {
            (WRITTEN_FIRST_WGSL, [image, sampled, image])
        } else {
            (WGSL, [sampled, image, image])
        };
        let stage = StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(Arc::from(wgsl)),
                entry_point: "main".to_owned(),
                kind: StageKind::Compute,
            },
            StageDeclarations {
                resources: ResourceLayout::new(
                    kinds
                        .into_iter()
                        .enumerate()
                        .map(|(at, kind)| binding(u32::try_from(at).unwrap(), kind))
                        .collect(),
                ),
                constants: Vec::new(),
            },
        )
        .unwrap();
        let program = runtime.compute_program(&stage).unwrap();
        let texture = |label, layers| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: 16,
                    height: 16,
                    depth_or_array_layers: layers,
                },
                mip_level_count: 4,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };
        let (texture, layered) = (texture("four mips", 1), texture("four mips, two layers", 2));
        Self {
            device,
            runtime,
            program,
            texture,
            layered,
        }
    }

    fn view(&self, mips: &Range<u32>) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mips.start,
            mip_level_count: Some(mips.end - mips.start),
            ..wgpu::TextureViewDescriptor::default()
        })
    }

    /// The binding `binding` given the mips `mips` of the texture, with the range stated.
    fn ranged(&self, binding: usize, mips: Range<u32>) -> Import {
        Import::texture_range(
            EngineSemantic::named(NAMES[binding]),
            self.texture.clone(),
            self.view(&mips),
            wgpu::TextureViewDimension::D2,
            TextureRange {
                mips,
                layers: 0..1,
                aspect: wgpu::TextureAspect::All,
            },
        )
    }

    /// The binding `binding` given the mips `mips` of the texture, with no range stated.
    fn unranged(&self, binding: usize, mips: &Range<u32>) -> Import {
        Import::texture_view(
            EngineSemantic::named(NAMES[binding]),
            self.texture.clone(),
            self.view(mips),
            wgpu::TextureViewDimension::D2,
        )
    }

    fn prepare(&mut self, imports: &[Import]) -> Result<PreparedDispatch, ProgramError> {
        self.runtime.prepare_imported_dispatch(&ImportedDispatch {
            program: &self.program,
            bindings: &[],
            imports,
            groups: [4, 4, 1],
            owners: &[],
        })
    }

    fn written(&self, mip: u32) -> PreparedWrite {
        PreparedWrite::layer(&self.texture, mip, 0)
    }

    /// The binding `binding` given mip `mip` of layer `layer` of the layered texture, with that
    /// range stated.
    fn layer(&self, binding: usize, (mip, layer): (u32, u32)) -> Import {
        let view = self.layered.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..wgpu::TextureViewDescriptor::default()
        });
        Import::texture_range(
            EngineSemantic::named(NAMES[binding]),
            self.layered.clone(),
            view,
            wgpu::TextureViewDimension::D2,
            TextureRange::single(mip, layer, wgpu::TextureAspect::All),
        )
    }
}

fn aliased(binding: u32, other_binding: u32) -> ProgramError {
    ProgramError::AliasedTexture {
        set: 0,
        binding,
        other_set: 0,
        other_binding,
    }
}

#[test]
fn distinct_mips_of_one_texture_bind_in_one_dispatch_and_only_the_written_mips_count() {
    let mut rig = Rig::new();
    let imports = [
        rig.ranged(0, 0..1),
        rig.ranged(1, 1..2),
        rig.ranged(2, 2..3),
    ];
    let dispatch = rig.prepare(&imports).unwrap();
    assert_eq!(dispatch.writes(), [rig.written(1), rig.written(2)]);

    let scope = rig.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = rig
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    dispatch.record(&mut encoder).unwrap();
    drop(encoder.finish());
    let error = pollster::block_on(scope.pop());
    assert!(
        error.is_none(),
        "wgpu takes distinct mips in one dispatch: {error:?}"
    );
}

#[test]
fn a_sampled_range_that_covers_a_written_mip_is_refused() {
    let mut rig = Rig::new();
    let imports = [
        rig.ranged(0, 0..2),
        rig.ranged(1, 1..2),
        rig.ranged(2, 2..3),
    ];
    assert_eq!(rig.prepare(&imports).unwrap_err(), aliased(0, 1));
}

#[test]
fn one_subresource_sampled_and_written_is_refused() {
    let mut rig = Rig::new();
    let imports = [
        rig.ranged(0, 1..2),
        rig.ranged(1, 1..2),
        rig.ranged(2, 2..3),
    ];
    assert_eq!(rig.prepare(&imports).unwrap_err(), aliased(0, 1));
}

#[test]
fn one_subresource_written_twice_is_refused() {
    let mut rig = Rig::new();
    let imports = [
        rig.ranged(0, 0..1),
        rig.ranged(1, 2..3),
        rig.ranged(2, 2..3),
    ];
    assert_eq!(rig.prepare(&imports).unwrap_err(), aliased(1, 2));
}

#[test]
fn views_that_state_no_range_cover_the_whole_texture_and_are_refused_together() {
    let mut rig = Rig::new();
    let imports = [
        rig.unranged(0, &(0..1)),
        rig.unranged(1, &(1..2)),
        rig.unranged(2, &(2..3)),
    ];
    assert_eq!(rig.prepare(&imports).unwrap_err(), aliased(0, 1));
}

#[test]
fn one_mip_in_distinct_layers_binds_in_one_dispatch_and_only_the_written_layer_counts() {
    let mut rig = Rig::new();
    let imports = [
        rig.layer(0, (1, 0)),
        rig.layer(1, (1, 1)),
        rig.layer(2, (2, 1)),
    ];
    let dispatch = rig.prepare(&imports).unwrap();
    assert_eq!(
        dispatch.writes(),
        [
            PreparedWrite::layer(&rig.layered, 1, 1),
            PreparedWrite::layer(&rig.layered, 2, 1)
        ]
    );
    let scope = rig.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = rig
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    dispatch.record(&mut encoder).unwrap();
    drop(encoder.finish());
    let error = pollster::block_on(scope.pop());
    assert!(
        error.is_none(),
        "wgpu takes one mip of distinct layers in one dispatch: {error:?}"
    );
}

#[test]
fn one_mip_of_one_layer_sampled_and_written_is_refused_whichever_layer_it_is() {
    for layer in [0, 1] {
        let mut rig = Rig::new();
        let imports = [
            rig.layer(0, (1, layer)),
            rig.layer(1, (1, layer)),
            rig.layer(2, (2, 1 - layer)),
        ];
        assert_eq!(
            rig.prepare(&imports).unwrap_err(),
            aliased(0, 1),
            "layer {layer}"
        );
    }
}

#[test]
fn a_written_image_before_the_sampled_one_conflicts_with_it_as_the_other_way_round_does() {
    let mut rig = Rig::with_order(true);
    let clash = [
        rig.ranged(0, 1..2),
        rig.ranged(1, 1..2),
        rig.ranged(2, 2..3),
    ];
    assert_eq!(rig.prepare(&clash).unwrap_err(), aliased(0, 1));
    let apart = [
        rig.ranged(0, 1..2),
        rig.ranged(1, 0..1),
        rig.ranged(2, 2..3),
    ];
    assert_eq!(
        rig.prepare(&apart).unwrap().writes(),
        [rig.written(1), rig.written(2)]
    );
}
