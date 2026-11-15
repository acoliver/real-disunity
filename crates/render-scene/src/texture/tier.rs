//! Texture tiers (GitHub #376 step 3): dropping the top levels of a scene texture's explicit mip
//! chain at upload, so the resident textures fit a share of the adapter's device-local memory.
//!
//! A tier is a slice of the prepared chain, with no resampling: tier N uploads level N as the
//! base. One count N applies to every eligible texture, the fewest that fits.
use super::PreparedTexture;

/// The smallest base a tier may leave: one 4x4 block, the block size of every compressed
/// scene format.
const MIN_BASE: u32 = 4;

/// Host policy for texture tiers. Without one, every texture uploads its authored chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureTierPolicy {
    /// Share of device-local memory, in millionths, that resident GPU memory may use: the
    /// eligible textures' kept bytes plus everything else already resident.
    pub budget_fraction_micro: u32,
    /// The most top levels any texture may lose.
    pub max_dropped_levels: u8,
}

/// The chosen tier, as the statistics report it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TierChoice {
    /// Top levels dropped from every eligible texture (each loses at most what it can).
    pub dropped_levels: u8,
    /// Backing bytes of the eligible textures with the authored chains.
    pub authored_bytes: u64,
    /// Backing bytes of the eligible textures as uploaded.
    pub kept_bytes: u64,
    /// `false` when even the maximum exceeds the budget; the maximum is uploaded then.
    pub fits: bool,
}

impl TierChoice {
    /// Bytes the tier saves against the authored chains.
    #[must_use]
    pub const fn saved_bytes(&self) -> u64 {
        self.authored_bytes.saturating_sub(self.kept_bytes)
    }
}

impl PreparedTexture<'_> {
    /// How many top levels this texture can lose: a cube or a single level loses none, and the
    /// new base keeps at least one 4x4 block.
    #[must_use]
    pub fn droppable_levels(&self) -> u8 {
        if self.cube.is_some() {
            return 0;
        }
        let count = self
            .mips
            .iter()
            .skip(1)
            .take_while(|mip| mip.width >= MIN_BASE && mip.height >= MIN_BASE)
            .count();
        u8::try_from(count).unwrap_or(u8::MAX)
    }

    /// This texture with up to `levels` top levels removed; level `levels` becomes the base.
    #[must_use]
    pub fn without_top_levels(mut self, levels: u8) -> Self {
        let levels = usize::from(levels.min(self.droppable_levels()));
        if levels == 0 {
            return self;
        }
        self.mips.drain(..levels);
        self.width = self.mips[0].width;
        self.height = self.mips[0].height;
        self
    }

    /// Backing bytes this texture occupies after dropping up to `levels` top levels, with the
    /// census's own arithmetic, so a choice and the census agree.
    #[must_use]
    pub fn tier_bytes(&self, levels: u8) -> u64 {
        let tier = self.clone().without_top_levels(levels);
        let mip_level_count = u32::try_from(tier.mips.len()).unwrap_or(u32::MAX);
        worldrunner_render_gpu::census::texture_backing_bytes(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: tier.width,
                height: tier.height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: tier.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }
}

impl super::OwnedPreparedTexture {
    /// Drops up to `levels` top levels in place, freeing their bytes, with the same limits as
    /// [`PreparedTexture::without_top_levels`].
    pub fn drop_top_levels(&mut self, levels: u8) {
        let levels = usize::from(levels.min(self.as_prepared().droppable_levels()));
        if levels == 0 {
            return;
        }
        self.mips.drain(..levels);
        self.width = self.mips[0].width;
        self.height = self.mips[0].height;
    }
}

/// The fewest dropped levels, shared by every eligible texture, whose kept bytes plus
/// `other_resident` fit the policy's share of `device_local`. Without a policy or a known
/// device-local size, nothing is dropped.
#[must_use]
pub fn choose(
    policy: Option<TextureTierPolicy>,
    device_local: Option<u64>,
    other_resident: u64,
    eligible: &[PreparedTexture<'_>],
) -> TierChoice {
    let bytes = |levels: u8| eligible.iter().map(|t| t.tier_bytes(levels)).sum::<u64>();
    let authored_bytes = bytes(0);
    let choice = |dropped_levels: u8, fits: bool| TierChoice {
        dropped_levels,
        authored_bytes,
        kept_bytes: bytes(dropped_levels),
        fits,
    };
    let (Some(policy), Some(device_local)) = (policy, device_local) else {
        return choice(0, true);
    };
    let budget = u128::from(device_local) * u128::from(policy.budget_fraction_micro) / 1_000_000;
    let budget = u64::try_from(budget).unwrap_or(u64::MAX);
    (0..=policy.max_dropped_levels)
        .find(|&levels| bytes(levels).saturating_add(other_resident) <= budget)
        .map_or_else(
            || choice(policy.max_dropped_levels, false),
            |levels| choice(levels, true),
        )
}

#[cfg(test)]
#[path = "tier_tests.rs"]
mod tests;
