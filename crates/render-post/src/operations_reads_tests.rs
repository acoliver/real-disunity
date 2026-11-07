//! Reads that happen after a list, in work the list does not contain, are checked against the
//! versions the list leaves behind.
use super::operations_tests::{device, id, underlying};
use crate::contracts::*;

struct Rig {
    device: wgpu::Device,
    /// Keeps the device's queue alive.
    _queue: wgpu::Queue,
    registry: ResourceRegistry,
    source: AllocationKey,
    target: AllocationKey,
    untouched: AllocationKey,
}

fn texture(device: &wgpu::Device, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reads fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage,
        view_formats: &[],
    })
}

impl Rig {
    fn new() -> Self {
        let (device, queue) = device();
        let mut registry = ResourceRegistry::default();
        let mut import = |usage, initial| {
            registry
                .import(
                    &texture(&device, usage),
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    initial,
                )
                .unwrap()
        };
        let source = import(wgpu::TextureUsages::COPY_SRC, Some(0));
        let sampled = wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING;
        let target = import(sampled, None);
        let untouched = import(sampled, Some(0));
        Self {
            device,
            _queue: queue,
            registry,
            source,
            target,
            untouched,
        }
    }

    fn view(&self, key: AllocationKey, version: u32) -> CheckedView {
        self.registry
            .view(
                key,
                TextureRange::single(0, 0, wgpu::TextureAspect::All),
                version,
            )
            .unwrap()
    }

    /// Copies the source into the target, taking the target to `version`.
    fn copy(&self, name: &'static str, version: u32) -> TypedOperation {
        TypedOperation::new(
            id(name),
            TypedWork::CopyTexture {
                source: TextureCopy::whole(self.view(self.source, 0)),
                destination: TextureCopy::whole(self.view(self.target, version)),
            },
        )
    }

    fn sampled(&self, key: AllocationKey, version: u32) -> ResourceUse {
        ResourceUse::read(self.view(key, version), UseKind::Sampled)
    }

    fn compile(
        &self,
        operations: &[TypedOperation],
        reads: &[ResourceUse],
    ) -> Result<CompiledOperations, ContractError> {
        self.registry
            .compile_operations_with_reads(&self.device, operations, reads)
    }
}

#[test]
fn a_later_read_of_what_the_list_wrote_is_accepted() {
    let rig = Rig::new();
    let operations = [rig.copy("copy", 1)];
    assert!(
        rig.compile(&operations, &[rig.sampled(rig.target, 1)])
            .is_ok()
    );
    assert!(rig.compile(&operations, &[]).is_ok());
}

#[test]
fn contents_the_allocation_declares_initialized_can_be_read_without_a_writer() {
    let rig = Rig::new();
    assert!(
        rig.compile(&[], &[rig.sampled(rig.untouched, 0)]).is_ok(),
        "initial version 0"
    );
}

#[test]
fn a_later_read_without_a_producer_or_of_another_version_is_a_producer_error() {
    let rig = Rig::new();
    let operations = [rig.copy("copy", 1)];
    // The target has no initialized contents before the copy.
    assert!(matches!(
        rig.compile(&[], &[rig.sampled(rig.target, 0)])
            .err()
            .unwrap(),
        ContractError::Producer(_)
    ));
    // Nothing wrote version 2.
    assert!(matches!(
        rig.compile(&operations, &[rig.sampled(rig.target, 2)])
            .err()
            .unwrap(),
        ContractError::Producer(_)
    ));
}

#[test]
fn a_read_of_a_version_a_later_operation_replaced_is_stale() {
    let rig = Rig::new();
    let mut second = rig.copy("second copy", 2);
    second.id = id("second copy");
    let operations = [rig.copy("first copy", 1), second];
    assert!(matches!(
        rig.compile(&operations, &[rig.sampled(rig.target, 1)])
            .err()
            .unwrap(),
        ContractError::Producer(_)
    ));
    assert!(
        rig.compile(&operations, &[rig.sampled(rig.target, 2)])
            .is_ok()
    );
}

#[test]
fn only_sampled_reads_can_be_declared_and_they_need_the_usage() {
    let rig = Rig::new();
    let operations = [rig.copy("copy", 1)];
    let write = ResourceUse::write(rig.view(rig.target, 2), UseKind::Sampled);
    assert!(matches!(
        rig.compile(&operations, &[write]).err().unwrap(),
        ContractError::Binding(_)
    ));
    let copy = ResourceUse::read(rig.view(rig.source, 0), UseKind::Copy);
    assert!(matches!(
        rig.compile(&operations, &[copy]).err().unwrap(),
        ContractError::Binding(_)
    ));
    // The source texture was not created for sampling.
    assert!(matches!(
        rig.compile(&operations, &[rig.sampled(rig.source, 0)])
            .err()
            .unwrap(),
        ContractError::Binding(_)
    ));
}

#[test]
fn errors_in_the_operations_keep_their_pass() {
    let rig = Rig::new();
    let mut late = rig.copy("copy", 1);
    late.id = id("late copy");
    assert!(matches!(
        rig.compile(&[late.clone(), late], &[]).err().unwrap(),
        ContractError::Raster(_)
    ));
    let read_first = TypedOperation::new(
        id("read first"),
        TypedWork::CopyTexture {
            source: TextureCopy::whole(rig.view(rig.target, 0)),
            destination: TextureCopy::whole(rig.view(rig.untouched, 1)),
        },
    );
    assert!(matches!(
        underlying(rig.compile(&[read_first], &[]).err().unwrap()),
        ContractError::Producer(_)
    ));
}

#[test]
fn the_declared_reads_are_revalidated_when_the_list_is_encoded() {
    let mut rig = Rig::new();
    let compiled = rig.compile(&[], &[rig.sampled(rig.untouched, 0)]).unwrap();
    let mut encoder = rig
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(
        compiled
            .encode(&rig.registry, &mut encoder, None, |_, _| {})
            .is_ok()
    );
    let replacement = texture(
        &rig.device,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    rig.registry.replace(rig.untouched, &replacement).unwrap();
    assert!(matches!(
        compiled
            .encode(&rig.registry, &mut encoder, None, |_, _| {})
            .err()
            .unwrap(),
        ContractError::StaleAllocation(_)
    ));
}
