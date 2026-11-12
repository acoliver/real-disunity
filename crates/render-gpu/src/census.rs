//! Resident GPU bytes by resource class, counted when a resource is created and released when
//! its handle drops.
//!
//! Admission budgets (see [`crate::configuration`]) count declared descriptors. The census
//! counts what was actually created, so a host can report where device memory goes.
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// What a GPU resource is for. Each class is reported separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceClass {
    /// Material and content textures, including environment maps.
    Texture,
    /// Vertex and index buffers.
    Mesh,
    /// Per-instance data such as transforms and draw records.
    Instance,
    /// Terrain height, splat and page storage.
    TerrainPage,
    /// Shadow map and cascade depth storage.
    ShadowMap,
    /// Frame attachments and post-processing targets.
    RenderTarget,
    /// Uniform and small storage buffers holding per-frame or per-material parameters.
    Uniform,
    /// Upload and readback staging.
    Staging,
}

impl ResourceClass {
    /// Every class, in report order.
    pub const ALL: [Self; 8] = [
        Self::Texture,
        Self::Mesh,
        Self::Instance,
        Self::TerrainPage,
        Self::ShadowMap,
        Self::RenderTarget,
        Self::Uniform,
        Self::Staging,
    ];

    /// A short lowercase name for logs.
    pub fn name(self) -> &'static str {
        match self {
            Self::Texture => "texture",
            Self::Mesh => "mesh",
            Self::Instance => "instance",
            Self::TerrainPage => "terrain_page",
            Self::ShadowMap => "shadow_map",
            Self::RenderTarget => "render_target",
            Self::Uniform => "uniform",
            Self::Staging => "staging",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

type Counters = [AtomicU64; ResourceClass::ALL.len()];

/// A shared tally of live tracked resources. Clones share one tally.
#[derive(Clone, Default)]
pub struct MemoryCensus(Arc<Counters>);

impl fmt::Debug for MemoryCensus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MemoryCensus")
            .field(&self.snapshot())
            .finish()
    }
}

impl MemoryCensus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts `bytes` under `class` until the returned entry drops.
    pub fn track(&self, class: ResourceClass, bytes: u64) -> CensusEntry {
        self.0[class.index()].fetch_add(bytes, Ordering::Relaxed);
        CensusEntry {
            counters: Arc::clone(&self.0),
            class,
            bytes,
        }
    }

    /// The bytes currently counted in each class.
    pub fn snapshot(&self) -> CensusSnapshot {
        CensusSnapshot(std::array::from_fn(|index| {
            self.0[index].load(Ordering::Relaxed)
        }))
    }

    /// Creates a buffer and counts its size under `class`.
    pub fn create_buffer(
        &self,
        device: &wgpu::Device,
        class: ResourceClass,
        descriptor: &wgpu::BufferDescriptor<'_>,
    ) -> Tracked<wgpu::Buffer> {
        let buffer = device.create_buffer(descriptor);
        let entry = self.track(class, buffer.size());
        Tracked::new(buffer, entry)
    }

    /// Creates an initialised buffer and counts its size under `class`.
    pub fn create_buffer_init(
        &self,
        device: &wgpu::Device,
        class: ResourceClass,
        descriptor: &wgpu::util::BufferInitDescriptor<'_>,
    ) -> Tracked<wgpu::Buffer> {
        use wgpu::util::DeviceExt;
        let buffer = device.create_buffer_init(descriptor);
        let entry = self.track(class, buffer.size());
        Tracked::new(buffer, entry)
    }

    /// Creates a texture and counts its backing bytes under `class`.
    pub fn create_texture(
        &self,
        device: &wgpu::Device,
        class: ResourceClass,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Tracked<wgpu::Texture> {
        let texture = device.create_texture(descriptor);
        let entry = self.track(class, texture_backing_bytes(descriptor));
        Tracked::new(texture, entry)
    }
}

/// Bytes a texture with `descriptor` occupies: every mip level and layer, times the sample
/// count, at the backing block size. Formats without a single-plane block size count zero.
pub fn texture_backing_bytes(descriptor: &wgpu::TextureDescriptor<'_>) -> u64 {
    let Some(block) = crate::configuration::backing_block_bytes(descriptor.format) else {
        return 0;
    };
    let (block_width, block_height) = descriptor.format.block_dimensions();
    let blocks: u64 = (0..descriptor.mip_level_count)
        .filter_map(|level| descriptor.mip_level_size(level))
        .map(|size| {
            u64::from(size.width.div_ceil(block_width))
                * u64::from(size.height.div_ceil(block_height))
                * u64::from(size.depth_or_array_layers)
        })
        .sum();
    blocks * u64::from(block) * u64::from(descriptor.sample_count)
}

/// Bytes per class at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CensusSnapshot([u64; ResourceClass::ALL.len()]);

impl CensusSnapshot {
    pub fn get(&self, class: ResourceClass) -> u64 {
        self.0[class.index()]
    }

    /// The sum over every class.
    pub fn total(&self) -> u64 {
        self.0.iter().sum()
    }

    /// Each class with its bytes, in report order.
    pub fn classes(&self) -> impl Iterator<Item = (ResourceClass, u64)> + '_ {
        ResourceClass::ALL
            .into_iter()
            .map(|class| (class, self.get(class)))
    }
}

impl fmt::Display for CensusSnapshot {
    /// One line: `total=N texture=N mesh=N ...`, in bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "total={}", self.total())?;
        for (class, bytes) in self.classes() {
            write!(f, " {}={bytes}", class.name())?;
        }
        Ok(())
    }
}

/// Keeps `bytes` counted under `class` while it lives.
pub struct CensusEntry {
    counters: Arc<Counters>,
    class: ResourceClass,
    bytes: u64,
}

impl CensusEntry {
    pub fn class(&self) -> ResourceClass {
        self.class
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl fmt::Debug for CensusEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CensusEntry")
            .field("class", &self.class)
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl Drop for CensusEntry {
    fn drop(&mut self) {
        self.counters[self.class.index()].fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

/// A GPU resource whose bytes stay counted until it drops.
#[derive(Debug)]
pub struct Tracked<T> {
    resource: T,
    entry: CensusEntry,
}

impl<T> Tracked<T> {
    pub fn new(resource: T, entry: CensusEntry) -> Self {
        Self { resource, entry }
    }

    pub fn entry(&self) -> &CensusEntry {
        &self.entry
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.resource
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(
        size: [u32; 3],
        format: wgpu::TextureFormat,
        mips: u32,
        dimension: wgpu::TextureDimension,
    ) -> wgpu::TextureDescriptor<'static> {
        wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: size[2],
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }
    }

    #[test]
    fn entries_count_until_they_drop() {
        let census = MemoryCensus::new();
        let mesh = census.track(ResourceClass::Mesh, 96);
        let page = census.track(ResourceClass::TerrainPage, 4096);
        let second = census.clone().track(ResourceClass::Mesh, 32);
        let snapshot = census.snapshot();
        assert_eq!(snapshot.get(ResourceClass::Mesh), 128);
        assert_eq!(snapshot.get(ResourceClass::TerrainPage), 4096);
        assert_eq!(snapshot.total(), 4224);
        drop((mesh, page));
        assert_eq!(census.snapshot().get(ResourceClass::Mesh), 32);
        drop(second);
        assert_eq!(census.snapshot(), CensusSnapshot::default());
    }

    #[test]
    fn clones_of_one_authority_share_its_census() {
        let gpu =
            crate::test_support::TestGpu::shared(&crate::device::DeviceRequirements::default())
                .unwrap();
        let authority = crate::submission::SubmissionAuthority::new(gpu.queue.clone());
        let clone = authority.clone();
        let entry = clone.census().track(ResourceClass::Instance, 48);
        assert_eq!(
            authority.census().snapshot().get(ResourceClass::Instance),
            48
        );
        let other = crate::submission::SubmissionAuthority::new(gpu.queue);
        assert_eq!(other.census().snapshot().total(), 0);
        drop(entry);
        assert_eq!(authority.census().snapshot().total(), 0);
    }

    #[test]
    fn the_report_line_names_every_class_in_order() {
        let census = MemoryCensus::new();
        let _shadow = census.track(ResourceClass::ShadowMap, 7);
        assert_eq!(
            census.snapshot().to_string(),
            "total=7 texture=0 mesh=0 instance=0 terrain_page=0 shadow_map=7 \
             render_target=0 uniform=0 staging=0"
        );
    }

    #[test]
    fn texture_bytes_cover_mips_layers_volumes_blocks_and_depth_backing() {
        use wgpu::TextureDimension::{D2, D3};
        use wgpu::TextureFormat::{Bc1RgbaUnorm, Depth24PlusStencil8, Rgba8Unorm};
        // 8x4 with 3 mips: 32 + 8 + 2 texels, two layers, four bytes.
        assert_eq!(
            texture_backing_bytes(&descriptor([8, 4, 2], Rgba8Unorm, 3, D2)),
            42 * 2 * 4
        );
        // A volume halves its depth per mip: 4x4x4 + 2x2x2 + 1x1x1 texels.
        assert_eq!(
            texture_backing_bytes(&descriptor([4, 4, 4], Rgba8Unorm, 3, D3)),
            (64 + 8 + 1) * 4
        );
        // BC1 packs 4x4 texels into 8 bytes; an 8x8 base is four blocks, its 4x4 mip one.
        assert_eq!(
            texture_backing_bytes(&descriptor([8, 8, 1], Bc1RgbaUnorm, 2, D2)),
            (4 + 1) * 8
        );
        // Depth/stencil counts the promoted 8-byte backing, as admission does.
        let mut msaa = descriptor([4, 4, 1], Depth24PlusStencil8, 1, D2);
        msaa.sample_count = 4;
        assert_eq!(texture_backing_bytes(&msaa), 16 * 8 * 4);
    }

    #[test]
    fn created_resources_count_until_their_handles_drop() {
        let gpu =
            crate::test_support::TestGpu::shared(&crate::device::DeviceRequirements::default())
                .unwrap();
        let census = MemoryCensus::new();
        let vertices = census.create_buffer(
            &gpu.device,
            ResourceClass::Mesh,
            &wgpu::BufferDescriptor {
                label: Some("census vertices"),
                size: 256,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            },
        );
        let uniform = census.create_buffer_init(
            &gpu.device,
            ResourceClass::Uniform,
            &wgpu::util::BufferInitDescriptor {
                label: Some("census uniform"),
                contents: &[0; 64],
                usage: wgpu::BufferUsages::UNIFORM,
            },
        );
        let target = descriptor(
            [16, 16, 1],
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            wgpu::TextureDimension::D2,
        );
        let target = census.create_texture(&gpu.device, ResourceClass::RenderTarget, &target);
        let snapshot = census.snapshot();
        assert_eq!(snapshot.get(ResourceClass::Mesh), vertices.size());
        assert_eq!(snapshot.get(ResourceClass::Uniform), 64);
        assert_eq!(snapshot.get(ResourceClass::RenderTarget), 16 * 16 * 4);
        assert_eq!(target.entry().bytes(), 1024);
        drop((vertices, uniform, target));
        assert_eq!(census.snapshot(), CensusSnapshot::default());
    }
}
