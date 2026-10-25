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
    if let Some(bloom) = settings.bloom {
        let mut sizes = vec![config.size];
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
    let fxaa = settings.fxaa.is_some_and(|weight| weight > 0.0);
    let upscale = settings
        .upscale
        .filter(|_| config.size != config.output_extent());
    for _ in 0..usize::from(fxaa) + usize::from(upscale.is_some()) {
        let [w, h] = config.size;
        set.texture(
            "display-linear post input",
            [w, h, 1],
            wgpu::TextureFormat::Rgba32Float,
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
