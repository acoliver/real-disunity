//! Stock post storage declarations, independent of GPU allocation.
use crate::{FrameConfig, HdrPostSettings};
use worldrunner_render_gpu::configuration::AllocationSet;

pub fn target_allocations(config: FrameConfig) -> AllocationSet {
    let mut set = AllocationSet::default();
    let [w, h] = config.size;
    set.texture("resolved color", [w, h, 1], config.format, [1, 1], 1);
    if config.samples > 1 {
        set.texture(
            "multisampled color",
            [w, h, 1],
            config.format,
            [1, config.samples],
            1,
        );
    }
    set.texture(
        "depth",
        [w, h, 1],
        wgpu::TextureFormat::Depth32Float,
        [1, config.samples],
        1,
    );
    set
}
pub fn post_allocations(config: FrameConfig, settings: HdrPostSettings) -> AllocationSet {
    let mut set = AllocationSet::default();
    if !settings.enabled() {
        return set;
    }
    // These buffers are retained by the factory across replacement.
    set.extend(exposure_allocations());
    set.buffer("exposure staging", 16, 3);
    if settings.adaptive.is_some() {
        set.buffer("histograms", 1024, 3);
    }
    gaussian_allocations(&mut set, config, &settings);
    tent_allocations(&mut set, config, &settings);
    let upscale = settings.upscale.filter(|_| {
        config.size != config.output_extent() && !settings.reconstructs_temporally(config)
    });
    display_input_allocations(&mut set, config, &settings);
    taa_allocations(&mut set, config, &settings);
    if settings.resolve.active_cmaa2().is_some() {
        crate::cmaa2_gpu::allocations(&mut set, settings.post_extent(config));
    }
    if settings.sharpen.is_some() {
        // Sharpening is one more stage between tone mapping and the output, so it adds one
        // intermediate: the stage before it can no longer write the output.
        let [w, h] = settings.post_extent(config);
        set.texture(
            "CAS input",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            3,
        );
    }
    if upscale.is_some_and(|settings| settings.sharpness.is_some()) {
        let [w, h] = config.output_extent();
        set.texture(
            "RCAS input",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            3,
        );
    }
    set
}
/// The TAA history bank and the slot outputs. The history is shared by the three slots and
/// counted once. Native TAA works on the render grid and keeps a linear-depth history for the
/// disocclusion test. Temporal reconstruction keeps its colour history on the output grid and
/// has no depth history; its dilated depth, dilated motion and reconstructed depth are per slot
/// at the render grid.
fn taa_allocations(set: &mut AllocationSet, config: FrameConfig, settings: &HdrPostSettings) {
    if settings.resolve.active_taa().is_none() {
        return;
    }
    let temporal = settings.reconstructs_temporally(config);
    let [w, h] = if temporal {
        config.output_extent()
    } else {
        config.size
    };
    // The history bank is shared by the three slots but belongs to its history key (extents,
    // mode, samples), not to every generation: a resize builds a new bank while the retiring
    // generation still holds the old one. A shared entry must keep one descriptor across
    // generations, so the bank is a plain single entry per generation. When the key is unchanged
    // and the bank is reused, a retiring generation is counted twice until it retires.
    let colour = crate::taa_gpu::FORMAT;
    set.texture("taa history", [w, h, 1], colour, [1, 1], 1);
    set.texture("taa output", [w, h, 1], colour, [1, 1], 3);
    if temporal {
        // Held by the frame targets, one field for every slot: it is cleared, written and read
        // inside one encoded frame. At equal extents the chain is the native TAA and has none.
        let [rw, rh] = config.size;
        set.texture(
            "frame temporal mask",
            [rw, rh, 1],
            crate::TEMPORAL_MASK_FORMAT,
            [1, 1],
            1,
        );
        // Dilated from the frame mask into a field per slot, see `taa_mask_gpu`.
        set.texture(
            "temporal prepared mask",
            [rw, rh, 1],
            crate::TEMPORAL_MASK_FORMAT,
            [1, 1],
            3,
        );
        temporal_reconstruction_allocations(set, config.size);
        if crate::taa_upscale_gpu::locks_enabled() {
            temporal_lock_allocations(set, config.size, [w, h]);
        }
    } else {
        let depth = crate::taa_gpu::DEPTH_FORMAT;
        set.texture("taa depth history", [w, h, 1], depth, [1, 1], 1);
        set.texture("taa depth output", [w, h, 1], depth, [1, 1], 3);
    }
}
/// What the temporal reconstruction passes write, per slot at the render extent `R`: the dilated
/// depth, the dilated motion, the buffer the previous-coordinate depth is scattered into
/// (4 bytes per texel) and the history confidence plane. They are written and read within one frame of a slot, so they are
/// per-generation entries with no shared descriptor.
fn temporal_reconstruction_allocations(set: &mut AllocationSet, [w, h]: [u32; 2]) {
    set.texture(
        "temporal dilated depth",
        [w, h, 1],
        crate::taa_reconstruct_gpu::DEPTH_FORMAT,
        [1, 1],
        3,
    );
    set.texture(
        "temporal dilated motion",
        [w, h, 1],
        crate::taa_reconstruct_gpu::MOTION_FORMAT,
        [1, 1],
        3,
    );
    set.buffer(
        "temporal reconstructed depth",
        4 * u64::from(w) * u64::from(h),
        3,
    );
    set.texture(
        "temporal depth confidence",
        [w, h, 1],
        crate::taa_reconstruct_gpu::CONFIDENCE_FORMAT,
        [1, 1],
        3,
    );
}
/// The thin-feature lock state, allocated only with locks on: the candidate field per slot at `R`, the lock bank per slot at
/// `O` and the lock history, shared like the colour history and counted once per generation.
fn temporal_lock_allocations(set: &mut AllocationSet, [rw, rh]: [u32; 2], [w, h]: [u32; 2]) {
    set.texture(
        "temporal thin candidates",
        [rw, rh, 1],
        crate::taa_thin_gpu::CANDIDATE_FORMAT,
        [1, 1],
        3,
    );
    let lock = crate::taa_upscale_gpu::FORMAT;
    set.texture("temporal lock bank", [w, h, 1], lock, [1, 1], 3);
    set.texture("temporal lock history", [w, h, 1], lock, [1, 1], 1);
}
/// The display-linear intermediates between tone mapping and a spatial resolve or upscale: FXAA
/// reads an `Rgba16Float` copy, the upscale and native RCAS read `Rgba32Float`.
fn display_input_allocations(
    set: &mut AllocationSet,
    config: FrameConfig,
    settings: &HdrPostSettings,
) {
    let fxaa = settings.resolve.active_fxaa().is_some();
    let upscale = settings.upscale.is_some()
        && config.size != config.output_extent()
        && !settings.reconstructs_temporally(config);
    let [w, h] = settings.post_extent(config);
    if fxaa {
        set.texture(
            "display-linear post input",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            3,
        );
    }
    if upscale {
        set.texture(
            "display-linear post input",
            [w, h, 1],
            wgpu::TextureFormat::Rgba32Float,
            [1, 1],
            3,
        );
    }
    let temporal_rcas = settings.temporal_rcas(config).is_some();
    if temporal_rcas || (settings.native_rcas.is_some() && config.size == config.output_extent()) {
        let [w, h] = settings.post_extent(config);
        set.texture(
            "native RCAS input",
            [w, h, 1],
            wgpu::TextureFormat::Rgba32Float,
            [1, 1],
            3,
        );
    }
}
fn tent_allocations(set: &mut AllocationSet, config: FrameConfig, settings: &HdrPostSettings) {
    if let Some(bloom) = settings
        .bloom
        .filter(|_| matches!(settings.bloom_kernel, crate::BloomKernel::Tent))
    {
        let mut sizes = vec![settings.post_extent(config)];
        while sizes.len() < usize::try_from(bloom.levels).unwrap()
            && *sizes.last().unwrap() != [1, 1]
        {
            sizes.push(sizes.last().unwrap().map(|side| side.div_ceil(2)));
        }
        for (index, &[w, h]) in sizes.iter().enumerate() {
            set.texture(
                "bloom downsample",
                [w, h, 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                3,
            );
            if index + 1 < sizes.len() {
                set.texture(
                    "bloom upsample",
                    [w, h, 1],
                    wgpu::TextureFormat::Rgba16Float,
                    [1, 1],
                    3,
                );
            }
        }
    }
}
fn gaussian_allocations(set: &mut AllocationSet, config: FrameConfig, settings: &HdrPostSettings) {
    if let (Some(bloom), crate::BloomKernel::SeparablePyramid(_)) =
        (settings.bloom, settings.bloom_kernel)
        && let Ok(extents) = crate::gaussian_extents_with_terminal(
            settings.post_extent(config),
            bloom.levels,
            bloom.policies.terminal,
        )
    {
        for (index, [w, h]) in extents.into_iter().enumerate() {
            let blurred = u32::try_from(index).unwrap() >= bloom.policies.selection.first_blurred;
            set.texture(
                "Gaussian level and scratch",
                [w, h, 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                if blurred { 6 } else { 3 },
            );
            if !blurred {
                continue;
            }
            set.buffer(
                "Gaussian folded weights",
                u64::try_from(crate::gaussian::GAUSSIAN_WEIGHT_BYTES).unwrap(),
                if bloom.policies.kernel == crate::GaussianKernelRule::RadiusThird {
                    3
                } else {
                    6
                },
            );
        }
        if settings.adaptive.is_none()
            && matches!(
                bloom.policies.extraction,
                crate::BrightExtraction::ToneCurve(_)
            )
        {
            set.buffer("stock fixed bloom exposure", 4, 3);
        }
    }
}
pub fn timing_allocations() -> AllocationSet {
    let mut set = AllocationSet::default();
    set.shared_buffer("timestamp queries", 16_384, 3);
    set.shared_buffer("timestamp resolve", 16_384, 3);
    set.shared_buffer("timestamp readback", 16_384, 3);
    set
}

pub fn exposure_allocations() -> AllocationSet {
    let mut set = AllocationSet::default();
    set.shared_buffer("exposure carry", 16, 1);
    set.shared_buffer("slot exposure", 16, 3);
    set.shared_buffer("slot exposure step", 16, 3);
    set
}
