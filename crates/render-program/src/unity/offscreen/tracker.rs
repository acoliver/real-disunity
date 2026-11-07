//! The executor's view of a frame: a registry of the resources the frame uses, imported with the
//! arena's lease, and the version every cell of every texture has reached.
//!
//! Each frame builds a new registry, so it never accumulates retired storage. Every texture is
//! imported with version 0 as its contents, which is true of a texture that was created
//! earlier and of a new one, since wgpu fills new textures with zeros. Each write takes the next
//! version of the texture it writes.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use worldrunner_render_gpu::submission::FrameLease;
use worldrunner_render_post::contracts::{
    AllocationKey, BufferView, CheckedView, ContractError, ResourceLifetime, ResourceRegistry,
    Semantic, TextureRange,
};

use crate::ids::TextureId;

use super::error::OffscreenError;

/// The allocations of a texture in the registry. A depth-only render texture has no color; its
/// `primary` allocation, the one copies, readbacks and samples use, is its depth.
#[derive(Clone, Copy)]
pub(super) struct Imported {
    pub(super) primary: AllocationKey,
    pub(super) color: Option<AllocationKey>,
    pub(super) depth: Option<AllocationKey>,
    pub(super) layers: u32,
    pub(super) mips: u32,
    pub(super) size: [u32; 2],
    pub(super) format: wgpu::TextureFormat,
}

pub(super) struct Tracker {
    pub(super) registry: ResourceRegistry,
    lease: FrameLease,
    textures: HashMap<TextureId, Imported>,
    cells: BTreeMap<(AllocationKey, u32, u32), u32>,
    counters: BTreeMap<AllocationKey, u32>,
    buffers: HashMap<wgpu::Buffer, AllocationKey>,
}

impl Tracker {
    pub(super) fn new(lease: FrameLease) -> Self {
        Self {
            registry: ResourceRegistry::default(),
            lease,
            textures: HashMap::new(),
            cells: BTreeMap::new(),
            counters: BTreeMap::new(),
            buffers: HashMap::new(),
        }
    }

    pub(super) fn imported(&self, id: TextureId) -> Option<Imported> {
        self.textures.get(&id).copied()
    }

    /// Imports a texture the first time the frame uses it.
    pub(super) fn import(
        &mut self,
        id: TextureId,
        color: Option<&wgpu::Texture>,
        depth: Option<&wgpu::Texture>,
    ) -> Result<Imported, ContractError> {
        if let Some(imported) = self.textures.get(&id) {
            return Ok(*imported);
        }
        let mut import = |texture: &wgpu::Texture| {
            self.registry
                .import_leased(texture, &self.lease, Semantic::Unspecified, Some(0))
        };
        let color_key = color.map(&mut import).transpose()?;
        let depth_key = depth.map(&mut import).transpose()?;
        let (Some(primary), Some(texture)) = (color_key.or(depth_key), color.or(depth)) else {
            return Err(ContractError::Pipeline);
        };
        let imported = Imported {
            primary,
            color: color_key,
            depth: depth_key,
            layers: texture.depth_or_array_layers(),
            mips: texture.mip_level_count(),
            size: [texture.width(), texture.height()],
            format: texture.format(),
        };
        self.textures.insert(id, imported);
        Ok(imported)
    }

    /// A buffer of the runtime, imported once. `initial` is the version its contents already
    /// have, or `None` for a buffer the frame writes first.
    pub(super) fn import_buffer(
        &mut self,
        buffer: &wgpu::Buffer,
        lifetime: ResourceLifetime,
        initial: Option<u32>,
    ) -> Result<AllocationKey, ContractError> {
        if let Some(key) = self.buffers.get(buffer) {
            return Ok(*key);
        }
        let key = self.registry.import_buffer(buffer, lifetime, initial)?;
        self.buffers.insert(buffer.clone(), key);
        Ok(key)
    }

    pub(super) fn buffer_view(
        &self,
        key: AllocationKey,
        range: Range<u64>,
        version: u32,
    ) -> Result<BufferView, ContractError> {
        self.registry.buffer_view(key, range, version)
    }

    /// A view of one mip and layer at `version`.
    pub(super) fn cell_view(
        &self,
        key: AllocationKey,
        cell: (u32, u32),
        aspect: wgpu::TextureAspect,
        version: u32,
    ) -> Result<CheckedView, ContractError> {
        self.registry
            .view(key, TextureRange::single(cell.0, cell.1, aspect), version)
    }

    /// A view of every mip and layer of a texture, in the dimension its binding declares.
    pub(super) fn whole_view(
        &self,
        imported: Imported,
        mips: u32,
        dimension: wgpu::TextureViewDimension,
        version: u32,
    ) -> Result<CheckedView, ContractError> {
        let range = TextureRange {
            mips: 0..mips,
            layers: 0..imported.layers,
            aspect: wgpu::TextureAspect::All,
        };
        self.registry
            .view_dimension(imported.primary, range, version, dimension)
    }

    pub(super) fn cell(&self, key: AllocationKey, mip: u32, layer: u32) -> u32 {
        self.cells.get(&(key, mip, layer)).copied().unwrap_or(0)
    }

    /// The version a draw samples a texture at and how many levels, from the base, it binds. The
    /// levels bound are the leading ones whose layers all have the base level's version, since a
    /// view reads one version. A level written at another point, or never written when the base
    /// was, is left out: the engine writes the base level of a render texture and generates no
    /// other.
    ///
    /// # Errors
    /// Fails with `MixedVersions` when the layers of the base level differ, since no level can be
    /// bound then.
    pub(super) fn sampled_levels(
        &self,
        id: TextureId,
        imported: Imported,
    ) -> Result<(u32, u32), OffscreenError> {
        let level = |mip| {
            let mut versions =
                (0..imported.layers).map(|layer| self.cell(imported.primary, mip, layer));
            let first = versions.next().unwrap_or(0);
            versions.all(|version| version == first).then_some(first)
        };
        let version = level(0).ok_or(OffscreenError::MixedVersions { texture: id })?;
        let mips = (1..imported.mips)
            .take_while(|&mip| level(mip) == Some(version))
            .count();
        Ok((version, u32::try_from(mips).unwrap_or(u32::MAX) + 1))
    }

    /// Takes the next version of a cell, and returns the one it had.
    pub(super) fn write(&mut self, key: AllocationKey, mip: u32, layer: u32) -> (u32, u32) {
        let counter = self.counters.entry(key).or_insert(0);
        *counter += 1;
        let after = *counter;
        let before = self.cells.insert((key, mip, layer), after).unwrap_or(0);
        (before, after)
    }
}
