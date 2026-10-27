use super::{PageState, Storage};
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_gpu::{
    configuration::{AllocationScope, AllocationSet, ConfigurationError, DriverAllocationError},
    submission::{FrameOwner, SubmissionAuthority},
};
use worldrunner_render_plan::TerrainPageSettings;

pub(in crate::terrain::pages) fn allocations(
    settings: TerrainPageSettings,
    slots: u32,
    products: u32,
    source: &AllocationSet,
) -> Result<AllocationSet, ConfigurationError<DriverAllocationError>> {
    let layers = slots
        .checked_mul(products)
        .filter(|_| slots > 0)
        .ok_or(ConfigurationError::Invalid("page slot count"))?;
    let page_bytes = settings.page_bytes() / 2 * u64::from(products);
    if page_bytes
        .checked_mul(u64::from(slots))
        .is_none_or(|bytes| bytes > settings.parts().resident_bytes)
    {
        return Err(ConfigurationError::Budget {
            required: page_bytes.saturating_mul(u64::from(slots)),
            budget: settings.parts().resident_bytes,
        });
    }
    let [w, h] = settings.extent();
    if u64::from(w) * u64::from(h) > u64::from(u32::MAX / 256) {
        return Err(ConfigurationError::Invalid(
            "page extent exceeds exact GPU reduction range",
        ));
    }
    let uploads = settings.parts().upload_bytes_per_frame / page_bytes;
    let buffers = u32::try_from(
        uploads
            .checked_mul(2)
            .ok_or(ConfigurationError::Invalid("page upload count"))?,
    )
    .map_err(|_| ConfigurationError::Invalid("page upload count"))?;
    let mut set = AllocationSet::default();
    set.texture(
        "terrain material pages",
        [w, h, layers],
        wgpu::TextureFormat::Rgba8Uint,
        [
            u32::try_from(settings.mip_extents().len()).expect("mip count"),
            1,
        ],
        1,
    );
    // Packed composition output and its executor upload staging coexist.
    set.buffer(
        "terrain page input and staging",
        u64::from(w) * u64::from(h) * if products == 3 { 16 } else { 8 },
        buffers,
    );
    if products == 3 {
        set.extend(source.clone());
        set.buffer(
            "terrain layered positions and staging",
            u64::from(w) * u64::from(h) * 8 + 8,
            buffers,
        );
    }
    Ok(set)
}

pub(super) fn create(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    settings: TerrainPageSettings,
    slots: u32,
    products: u32,
) -> Result<Storage, DriverAllocationError> {
    let scope = AllocationScope::new(device);
    let [width, height] = settings.extent();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain material pages"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: slots * products,
        },
        mip_level_count: u32::try_from(settings.mip_extents().len()).expect("mip count"),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    scope.finish()?;
    Ok(Storage {
        texture,
        state: Rc::new(RefCell::new(PageState::new(
            slots,
            settings.parts().upload_bytes_per_frame
                / (settings.page_bytes() / 2 * u64::from(products)),
            authority,
        ))),
        owner: FrameOwner::new((), authority),
    })
}
