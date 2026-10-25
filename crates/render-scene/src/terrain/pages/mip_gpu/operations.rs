use super::{PageWriter, id};
use crate::terrain::pages::residency::{PendingPage, ResidentPages};
use worldrunner_render_post::contracts::{
    AccessMode, BufferBinding, BufferBindingKind, BufferUse, BufferUseKind, BufferView,
    CheckedBindings, CheckedView, ContractError, ResourceRegistry, ResourceUse, Semantic,
    TextureBinding, TextureBindingKind, TextureRange, TypedOperation, TypedWork, UseKind,
};

pub(super) fn build(
    writer: &PageWriter,
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    cache: &ResidentPages,
    pending: &PendingPage,
    source: &BufferView,
) -> Result<Vec<TypedOperation>, ContractError> {
    let storage = cache.storage();
    let key = registry.import_leased(
        &storage.texture,
        &storage.owner.lease(),
        Semantic::Unspecified,
        None,
    )?;
    let mut result = Vec::new();
    let mut previous: Option<CheckedView> = None;
    for (mip, [width, height]) in cache.settings().mip_extents().into_iter().enumerate() {
        let mip = u32::try_from(mip).expect("mip count");
        let destination = registry.view(
            key,
            TextureRange {
                mips: mip..mip + 1,
                layers: pending.slot() * 2..pending.slot() * 2 + 2,
                aspect: wgpu::TextureAspect::All,
            },
            1,
        )?;
        let mut textures = vec![binding(1, destination.clone(), true)];
        let mut buffers = Vec::new();
        let mut uses = vec![ResourceUse::write(destination.clone(), UseKind::Storage)];
        if let Some(view) = previous {
            textures.push(binding(0, view.clone(), false));
            uses.push(ResourceUse::read(view, UseKind::Sampled));
        } else {
            buffers.push(BufferBinding {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                view: source.clone(),
                kind: BufferBindingKind::Storage(AccessMode::Read),
                dynamic_offset: None,
            });
        }
        let group = CheckedBindings::build_with_buffers(device, registry, &textures, &buffers)?;
        let pipeline = if mip == 0 {
            &writer.store
        } else {
            &writer.reduce
        };
        let mut operation = TypedOperation::new(
            id("terrain page mip", mip),
            TypedWork::Compute {
                pipeline: pipeline.clone(),
                bindings: vec![group],
                workgroups: [width.div_ceil(8), height.div_ceil(8), 1],
            },
        );
        operation.textures = uses;
        if mip == 0 {
            operation
                .buffers
                .push(BufferUse::read(source.clone(), BufferUseKind::Storage));
        }
        result.push(operation);
        previous = Some(destination);
    }
    Ok(result)
}
fn binding(index: u32, view: CheckedView, write: bool) -> TextureBinding {
    TextureBinding {
        binding: index,
        visibility: wgpu::ShaderStages::COMPUTE,
        view,
        samples: 1,
        semantic: Semantic::Unspecified,
        kind: if write {
            TextureBindingKind::Storage(wgpu::StorageTextureAccess::WriteOnly)
        } else {
            TextureBindingKind::Sampled(wgpu::TextureSampleType::Uint)
        },
    }
}
