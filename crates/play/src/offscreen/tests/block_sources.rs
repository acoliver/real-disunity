//! Block-compressed textures uploaded as their blocks sample to the texels the CPU decoder
//! produces. Each format is blitted to a render texture twice, once uploaded as blocks and once
//! decoded to RGBA8 by the adapter's existing path, and the two images must be equal.
//!
//! The blocks are synthetic and every texel selects an endpoint (or, for BC7, the first or last
//! palette entry), so the decoded texels do not depend on how a device rounds interpolated
//! colours. The test does not establish that interpolated texels agree.

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, OffscreenWork, TextureRef};

use super::super::adapter::{OffscreenAdapter, build};
use super::super::assets::{AssetError, Assets};
use super::super::frame::Settings;
use super::gpu::{Rig, SIDE, Synthetic, shader, vector};
use super::support::{render_texture, world};

const TEXTURE: u64 = 1 << 40;

/// One 4x4 texture stored under [`TEXTURE`].
struct Stored(PlayerTexture);

impl Assets for Stored {
    fn mesh(&self, _: u64, _: u64) -> Result<Option<MeshChannels>, AssetError> {
        Ok(None)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok((identity == TEXTURE && known != self.0.generation).then(|| self.0.clone()))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// Whether the texel at `index` (row major in the block) takes the second palette entry: a
/// pattern that differs along both axes.
fn second(index: usize) -> bool {
    (index / 4 + index % 4) % 2 == 1
}

/// The 3-bit indices of an alpha block: 0 selects the first endpoint, 1 the second.
fn alpha_indices() -> u64 {
    (0..16).fold(0, |bits, texel| {
        bits | (u64::from(second(texel)) << (3 * texel))
    })
}

fn alpha_block(first: u8, last: u8) -> [u8; 8] {
    let mut block = [0; 8];
    block[0] = first;
    block[1] = last;
    block[2..].copy_from_slice(&alpha_indices().to_le_bytes()[..6]);
    block
}

/// A colour block of two opaque endpoints (the first greater than the second, so the four-colour
/// form) whose texels take one or the other.
fn colour_block(red: u16, blue: u16) -> [u8; 8] {
    let indices = (0..16).fold(0_u32, |bits, texel| {
        bits | (u32::from(second(texel)) << (2 * texel))
    });
    let mut block = [0; 8];
    block[0..2].copy_from_slice(&red.to_le_bytes());
    block[2..4].copy_from_slice(&blue.to_le_bytes());
    block[4..].copy_from_slice(&indices.to_le_bytes());
    block
}

/// A BC7 block of the mode with one subset, seven-bit endpoints with a shared extra low bit and
/// four-bit indices. The texels take the first endpoint (index 0) or the second (index 15).
fn bc7_block(first: [u8; 4], last: [u8; 4]) -> [u8; 16] {
    let mut bits = 1_u128 << 6;
    for channel in 0..4 {
        bits |= u128::from(first[channel] & 127) << (7 + channel * 14);
        bits |= u128::from(last[channel] & 127) << (14 + channel * 14);
    }
    // Both extra low bits set.
    bits |= 3 << 63;
    // The first index is an anchor and has three bits, so it must be 0; the others have four.
    let mut at = 65;
    for texel in 0..16 {
        let width = if texel == 0 { 3 } else { 4 };
        if second(texel) {
            bits |= 15 << at;
        }
        at += width;
    }
    bits.to_le_bytes()
}

fn rig(blocks: bool) -> Rig {
    let mut rig = Rig::new();
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        ..Settings::default()
    };
    rig.adapter = OffscreenAdapter::with_supply(Synthetic, settings).with_block_sources(blocks);
    rig
}

/// The target's texels after a plain blit of the stored texture.
fn blitted(rig: &mut Rig, stored: &PlayerTexture) -> Vec<u8> {
    let mut world = world();
    let target = render_texture(&mut world, [SIDE, SIDE]);
    let blit = shader(&mut world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(&mut world, material, "_Tint", [1.0; 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Asset(TEXTURE),
            target,
            material: Some(material),
            pass: 0,
        })
        .unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    let mut adapted = rig.adapter.adapt(
        &snapshot,
        &Stored(stored.clone()),
        &mut rig.state,
        &mut rig.runtime,
    );
    assert!(
        adapted.report.failures.is_empty(),
        "{:?}",
        adapted.report.failures
    );
    let mut operations = build(&mut rig.state, &mut rig.runtime, &rig.arena, &mut adapted).unwrap();
    let image = operations
        .read_back(super::super::ids::render_texture(target), 0, 0)
        .unwrap();
    rig.run(operations);
    image.bytes(&rig.device)
}

/// A texture of a generation no earlier one had, so the adapter reads it again.
fn stored(format: u32, linear: bool, bytes: &[u8]) -> PlayerTexture {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    PlayerTexture {
        format,
        width: 4,
        height: 4,
        mips: 1,
        linear,
        generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        distance_spread: 0,
        bytes: bytes.to_vec(),
    }
}

/// The stored bytes of one block for each format, with the two colours or levels it holds.
fn blocks() -> [(u32, &'static str, Vec<u8>); 4] {
    [
        (10, "DXT1", colour_block(0xF800, 0x001F).to_vec()),
        (
            12,
            "DXT5",
            [
                alpha_block(255, 64).as_slice(),
                &colour_block(0x07E0, 0xF81F),
            ]
            .concat(),
        ),
        (26, "BC4", alpha_block(200, 40).to_vec()),
        (
            25,
            "BC7",
            bc7_block([100, 30, 70, 127], [10, 90, 120, 40]).to_vec(),
        ),
    ]
}

#[test]
fn blocks_sample_to_the_texels_the_cpu_decoder_gives_for_dxt1_dxt5_bc4_and_bc7() {
    let mut as_blocks = rig(true);
    assert!(
        as_blocks
            .runtime
            .device()
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        "the adapter must offer block compression for this test"
    );
    let mut decoded = rig(false);
    for (format, name, bytes) in blocks() {
        for linear in [false, true] {
            let stored = stored(format, linear, &bytes);
            let from_blocks = blitted(&mut as_blocks, &stored);
            let from_rgba8 = blitted(&mut decoded, &stored);
            assert_eq!(from_blocks.len(), (SIDE * SIDE * 4) as usize);
            assert_eq!(from_blocks, from_rgba8, "{name}, linear {linear}");
            // The block holds two different colours, so the equality is not of a flat image.
            let distinct: std::collections::BTreeSet<_> = from_blocks.chunks(4).collect();
            assert_eq!(distinct.len(), 2, "{name} has two colours: {from_blocks:?}");
        }
    }
}

#[test]
fn a_texture_uploaded_as_blocks_takes_a_quarter_to_an_eighth_of_the_rgba8_storage() {
    for (format, name, bytes) in blocks() {
        // BC4 goes up as blocks only as data.
        let stored = stored(format, format == 26, &bytes);
        let (mut as_blocks, mut decoded) = (rig(true), rig(false));
        blitted(&mut as_blocks, &stored);
        blitted(&mut decoded, &stored);
        let held = (as_blocks.runtime.live_bytes(), decoded.runtime.live_bytes());
        // Both hold the same render texture; the texture itself is 64 bytes decoded and one
        // block (8 or 16 bytes) as blocks.
        let block = bytes.len().min(16) as u64;
        assert_eq!(held.1 - held.0, 64 - block, "{name}: {held:?}");
    }
}
