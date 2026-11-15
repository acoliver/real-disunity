//! Texture tiers: which levels a texture can lose, what it uploads, and the global choice.

use super::{TextureTierPolicy, TierChoice, choose};
use crate::texture::{PreparedTexture, prepare_texture};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};

/// An RGBA8 texture of `size` x `size` with its full chain down to 1x1, each level's bytes
/// filled with its own index so a slice is recognisable.
fn rgba(size: u32) -> SceneTexture {
    let levels = size.ilog2() + 1;
    let mut bytes = Vec::new();
    for level in 0..levels {
        let edge = usize::try_from(size >> level).expect("small side");
        bytes.resize(
            bytes.len() + 4 * edge * edge,
            u8::try_from(level).expect("few levels"),
        );
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: size,
        height: size,
        mips: levels,
        bytes: bytes.into(),
    }
}

fn prepared(texture: &SceneTexture) -> PreparedTexture<'_> {
    prepare_texture(texture).expect("a well-formed texel grid prepares")
}

/// Backing bytes of an RGBA8 square chain from `size` down to 1x1.
fn chain_bytes(size: u64) -> u64 {
    (0..=size.ilog2())
        .map(|level| 4 * (size >> level).pow(2))
        .sum()
}

const HALF: TextureTierPolicy = TextureTierPolicy {
    budget_fraction_micro: 500_000,
    max_dropped_levels: 3,
};

#[test]
fn a_texture_keeps_a_four_by_four_base() {
    let texture = rgba(64);
    // 32, 16, 8 and 4 can each become the base; 2 and 1 cannot.
    assert_eq!(prepared(&texture).droppable_levels(), 4);
    let tiny = rgba(4);
    assert_eq!(prepared(&tiny).droppable_levels(), 0);
    let single = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 64,
        height: 64,
        mips: 1,
        bytes: vec![0; 4 * 64 * 64].into(),
    };
    assert_eq!(prepared(&single).droppable_levels(), 0);
}

#[test]
fn a_tier_uploads_a_lower_level_as_the_base_without_resampling() {
    let texture = rgba(64);
    let authored = prepared(&texture);
    let tier = authored.clone().without_top_levels(2);
    assert_eq!([tier.width(), tier.height()], [16, 16]);
    assert_eq!(tier.mips().len(), authored.mips().len() - 2);
    // The new base is the authored level 2, the same bytes.
    assert!(std::ptr::eq(tier.mips()[0].bytes, authored.mips()[2].bytes));
    assert!(tier.mips()[0].bytes.iter().all(|&byte| byte == 2));
    // Asking for more than the texture can lose stops at its 4x4 base.
    let deepest = authored.without_top_levels(9);
    assert_eq!([deepest.width(), deepest.height()], [4, 4]);
}

#[test]
fn tier_bytes_follow_the_census_arithmetic() {
    let texture = rgba(64);
    let prepared = prepared(&texture);
    assert_eq!(prepared.tier_bytes(0), chain_bytes(64));
    assert_eq!(prepared.tier_bytes(1), chain_bytes(32));
    assert_eq!(prepared.tier_bytes(9), chain_bytes(4));
}

#[test]
fn without_a_policy_or_a_device_local_size_nothing_is_dropped() {
    let texture = rgba(64);
    let eligible = [prepared(&texture)];
    let full = chain_bytes(64);
    let untouched = TierChoice {
        dropped_levels: 0,
        authored_bytes: full,
        kept_bytes: full,
        fits: true,
    };
    for (policy, memory) in [(None, Some(1)), (Some(HALF), None), (None, None)] {
        assert_eq!(choose(policy, memory, 0, &eligible), untouched);
    }
}

#[test]
fn the_fewest_levels_that_fit_are_dropped_for_every_texture() {
    let (large, small) = (rgba(64), rgba(16));
    let eligible = [prepared(&large), prepared(&small)];
    let full = chain_bytes(64) + chain_bytes(16);
    // Room for everything: nothing is dropped.
    assert_eq!(
        choose(Some(HALF), Some(2 * full), 0, &eligible).dropped_levels,
        0
    );
    // Half the room: one level from each is enough.
    let one = choose(Some(HALF), Some(full), 0, &eligible);
    assert_eq!(one.dropped_levels, 1);
    assert_eq!(one.kept_bytes, chain_bytes(32) + chain_bytes(8));
    assert_eq!(one.saved_bytes(), full - one.kept_bytes);
    assert!(one.fits);
    // Memory already resident counts against the same budget.
    let crowded = choose(Some(HALF), Some(full), full / 3, &eligible);
    assert_eq!(crowded.dropped_levels, 2);
}

#[test]
fn when_nothing_fits_the_maximum_is_uploaded_and_reported() {
    let texture = rgba(64);
    let eligible = [prepared(&texture)];
    let choice = choose(Some(HALF), Some(64), 0, &eligible);
    assert_eq!(choice.dropped_levels, 3);
    assert!(!choice.fits);
    assert_eq!(choice.kept_bytes, chain_bytes(8));
}
