use super::ScenePages;
use worldrunner_render_plan::TerrainPageSettings;
use worldrunner_render_post::FrameError;

pub(super) fn bytes(settings: TerrainPageSettings, slots: u32) -> u64 {
    48 + (u64::from(settings.parts().maximum_selected_pages) + u64::from(slots)) * 16
}

pub(super) fn upload(
    pages: &ScenePages,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), FrameError> {
    let selection = pages.selection.as_ref().expect("selection prepared");
    let residents = &pages.draw_residents;
    let settings = pages.cache.settings();
    let mut words = vec![
        u32::try_from(selection.leaves().len()).expect("leaf count"),
        u32::try_from(residents.len()).expect("resident count"),
        pages.cache.product_count() - 2,
        pages
            .layered
            .as_ref()
            .map_or(0, |source| u32::from(source.anisotropy.level() - 1)),
        settings.parts().interior[0],
        settings.parts().interior[1],
        settings.parts().border,
        settings.maximum_sampling_mip(),
    ];
    words.extend(pages.bounds.map(f32::to_bits));
    for record in records(selection.leaves(), residents) {
        words.extend(record);
    }
    let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    let mut context = worldrunner_render_post::PrepareContext::new(device, encoder)?;
    context.upload_buffer(&pages.tables[pages.frame_slot], 0, &bytes);
    context.finish();
    Ok(())
}

pub(super) fn allocate(
    device: &wgpu::Device,
    bytes: u64,
) -> Result<Vec<wgpu::Buffer>, worldrunner_render_gpu::configuration::DriverAllocationError> {
    let scope = worldrunner_render_gpu::configuration::AllocationScope::new(device);
    let tables = (0..3)
        .map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("terrain page draw table"),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        })
        .collect();
    scope.finish()?;

    Ok(tables)
}

/// Each record holds a contiguous four-child block and an optional resident slot.
/// Only selected paths are emitted; retained off-tree residents are not searched.
pub(in crate::terrain::pages) fn records(
    leaves: &[crate::terrain::pages::selection::PageKey],
    residents: &[crate::terrain::pages::residency::ResidentPage],
) -> Vec<[u32; 4]> {
    use crate::terrain::pages::selection::PageKey;
    let resident: std::collections::BTreeMap<_, _> = residents
        .iter()
        .map(|page| (page.key(), page.slot() + 1))
        .collect();
    let mut records = vec![[0; 4]];
    records[0][1] = resident.get(&PageKey::ROOT).copied().unwrap_or(0);
    for leaf in leaves {
        let mut node = 0_usize;
        for depth in 1..=leaf.depth() {
            if records[node][0] == 0 {
                records[node][0] = u32::try_from(records.len()).expect("bounded page table");
                records.extend([[0; 4]; 4]);
            }
            let key = leaf.ancestor(depth).expect("ancestor");
            let [x, y] = key.coordinate();
            node = records[node][0] as usize + (x % 2 + 2 * (y % 2)) as usize;
            records[node][1] = resident.get(&key).copied().unwrap_or(0);
        }
    }
    records
}
