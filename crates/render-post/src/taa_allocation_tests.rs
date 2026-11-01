//! CPU-only byte accounting of the TAA history banks.
use crate::{
    ColorGrading, FrameConfig, HdrPostSettings, PostResolve, UpscaleMode, UpscaleSettings,
    allocation::post_allocations,
};

const RENDER: [u32; 2] = [32, 16];
const OUTPUT: [u32; 2] = [64, 32];
const HALF_RGBA: u64 = 8;
const FLOAT_R: u64 = 4;
const SLOTS: u64 = 3;
const MASK_TEXEL: u64 = 2;

fn texels(extent: [u32; 2]) -> u64 {
    u64::from(extent[0]) * u64::from(extent[1])
}

fn settings(resolve: PostResolve, upscale: Option<UpscaleMode>) -> HdrPostSettings {
    HdrPostSettings {
        grading: ColorGrading {
            linear_scale: 1.5,
            ..ColorGrading::default()
        },
        resolve,
        upscale: upscale.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    }
}

fn config(samples: u32) -> FrameConfig {
    FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, samples).with_render_extent(RENDER)
}

/// Bytes the TAA resolve adds to a configuration, found by removing only the resolve.
fn taa_bytes(config: FrameConfig, upscale: Option<UpscaleMode>) -> u64 {
    let limits = wgpu::Limits::default();
    let bytes = |resolve| {
        post_allocations(config, settings(resolve, upscale))
            .validate(&limits)
            .unwrap()
    };
    bytes(PostResolve::taa(0.9)) - bytes(PostResolve::None)
}

#[test]
fn native_taa_counts_shared_history_once_and_slot_outputs_three_times() {
    let shared = texels(RENDER) * (HALF_RGBA + FLOAT_R);
    let per_slot = texels(RENDER) * (HALF_RGBA + FLOAT_R) * SLOTS;
    assert_eq!(taa_bytes(config(1), None), shared + per_slot);
}

#[test]
fn native_taa_history_does_not_grow_with_the_output_extent() {
    let same = FrameConfig::new(RENDER, wgpu::TextureFormat::Rgba16Float, 1);
    assert_eq!(taa_bytes(config(1), None), taa_bytes(same, None));
}

#[test]
fn native_taa_history_is_single_sampled_with_msaa_scene_targets() {
    assert_eq!(taa_bytes(config(4), None), taa_bytes(config(1), None));
}

/// The thin-feature lock state, measured: `Rg32Float` candidates at `R` per slot (8R), an
/// `Rgba16Float` lock bank at `O` per slot (8O) and one shared lock history (8O). The bank and the history
/// are 8O each, 32O with three slots, not the 16O the design counted for two 16 bit channels.
fn lock_bytes() -> u64 {
    let candidates = texels(RENDER) * 2 * FLOAT_R * SLOTS;
    let bank = texels(OUTPUT) * HALF_RGBA * SLOTS;
    let history = texels(OUTPUT) * HALF_RGBA;
    candidates + bank + history
}

#[test]
fn temporal_taa_counts_output_sized_history_once_and_render_sized_reconstruction_per_slot() {
    assert_eq!(
        taa_bytes(config(1), Some(UpscaleMode::Temporal)),
        unlocked_bytes()
    );
}

#[test]
fn temporal_taa_adds_the_lock_state_only_when_locks_are_on() {
    let _on = crate::taa_upscale_gpu::WithLocks::new();
    assert_eq!(
        taa_bytes(config(1), Some(UpscaleMode::Temporal)),
        unlocked_bytes() + lock_bytes()
    );
}

/// The temporal bytes of a configuration without locks.
fn unlocked_bytes() -> u64 {
    let shared = texels(OUTPUT) * HALF_RGBA;
    let per_slot_output = texels(OUTPUT) * HALF_RGBA * SLOTS;
    let dilated_depth = texels(RENDER) * FLOAT_R;
    let dilated_motion = texels(RENDER) * 2 * FLOAT_R;
    let reconstructed_depth = texels(RENDER) * FLOAT_R;
    let confidence = texels(RENDER) * FLOAT_R;
    let per_slot_reconstruction =
        (dilated_depth + dilated_motion + reconstructed_depth + confidence) * SLOTS;
    shared + per_slot_output + per_slot_reconstruction + mask_bytes()
}

/// The producer mask of temporal reconstruction: `Rg8Unorm` at `R`, one field shared by the
/// slots because it is cleared, written and read inside one encoded frame.
fn mask_bytes() -> u64 {
    texels(RENDER) * MASK_TEXEL
}

/// The same equality with locks on: the lock candidates, bank and history count too.
#[test]
fn locked_temporal_allocations_equal_the_bytes_of_the_real_stage_sets() {
    let _on = crate::taa_upscale_gpu::WithLocks::new();
    let (render, output) = ([10, 10], [16, 16]);
    let config =
        FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render);
    assert_eq!(
        taa_bytes(config, Some(UpscaleMode::Temporal)),
        built_temporal_bytes(render, output)
    );
}

/// A resize keeps the old generation alive beside the new one until its queue work completes,
/// and each owns a differently sized history bank, so the two allocation sets must combine.
#[test]
fn generations_with_differently_sized_taa_banks_validate_together() {
    let limits = wgpu::Limits::default();
    for upscale in [None, Some(UpscaleMode::Temporal)] {
        let resized = FrameConfig::new([12, 8], wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent([6, 4]);
        let mut generations = post_allocations(config(1), settings(PostResolve::taa(0.9), upscale));
        generations.extend(post_allocations(
            resized,
            settings(PostResolve::taa(0.9), upscale),
        ));
        generations.validate(&limits).unwrap();
    }
}

/// The bytes the stage sets of a real temporal build hold: every distinct texture and buffer
/// once, so the shared colour history counts once and each slot's own resources three times.
fn built_temporal_bytes(render: [u32; 2], output: [u32; 2]) -> u64 {
    use crate::hdr_temporal_build_tests as build;
    let (device, _) = crate::gpu_tests::gpu();
    let post = crate::hdr_post::HdrPost::new(&device, &build::settings()).unwrap();
    let targets = build::targets(&device, render, output);
    let sets = build::slots(&post, &device, &targets);
    let mut textures: Vec<&wgpu::Texture> = Vec::new();
    let mut bytes = targets.temporal_mask_texture().map_or(0, |mask| {
        u64::from(mask.width()) * u64::from(mask.height()) * MASK_TEXEL
    });
    for set in &sets {
        for texture in &set.resources.textures {
            if !textures.contains(&texture) {
                textures.push(texture);
                let texel = u64::from(texture.format().block_copy_size(None).unwrap());
                bytes += u64::from(texture.width()) * u64::from(texture.height()) * texel;
            }
        }
        bytes += set
            .resources
            .buffers
            .iter()
            .map(wgpu::Buffer::size)
            .sum::<u64>();
    }
    bytes
}

/// The allocation declaration must match what the build allocates, including the dilated depth,
/// the dilated motion and the reconstructed depth buffer, which are per slot and at the
/// internal extent, not shared.
#[test]
fn temporal_allocations_equal_the_bytes_of_the_real_stage_sets() {
    for (render, output) in [([10, 10], [16, 16]), ([11, 9], [17, 13]), ([3, 5], [9, 7])] {
        let config = FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let declared = taa_bytes(config, Some(UpscaleMode::Temporal));
        assert_eq!(
            declared,
            built_temporal_bytes(render, output),
            "render {render:?} output {output:?}"
        );
    }
}
