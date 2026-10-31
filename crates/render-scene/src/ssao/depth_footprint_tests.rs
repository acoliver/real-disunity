use super::*;
use crate::scene_metadata::SceneMetadata;
use crate::ssao::tests::{context, draw, read, upload};
use worldrunner_render_plan::{SceneDownsampleRounding, SceneSsaoDepthFootprint};

fn settings(footprint: SceneSsaoDepthFootprint) -> worldrunner_render_plan::SceneSsao {
    worldrunner_render_plan::SceneSsao {
        depth_footprint: footprint,
        ..super::scalable_tests::recipe()
    }
}

#[test]
fn allocation_bytes_follow_the_selected_footprint() {
    use crate::compositor::RendererConfiguration;
    let mut config = RendererConfiguration::legacy([9, 7], wgpu::TextureFormat::Rgba8UnormSrgb);
    config.ssao = Some(settings(SceneSsaoDepthFootprint::default()));
    let limits = wgpu::Limits::default();
    let default_bytes = config.allocations().validate(&limits).unwrap();
    config.ssao = Some(settings(SceneSsaoDepthFootprint {
        level_count: 2,
        rounding: SceneDownsampleRounding::Floor,
    }));
    let floor_bytes = config.allocations().validate(&limits).unwrap();
    assert_eq!(default_bytes - floor_bytes, 156);
}

#[test]
fn gpu_floor_footprint_has_requested_levels_and_literal_reduction() {
    let context = context();
    let footprint = SceneSsaoDepthFootprint {
        level_count: 2,
        rounding: SceneDownsampleRounding::Floor,
    };
    let screen = SceneMetadata::new(context.device(), [9, 7], settings(footprint)).unwrap();
    let stages: Vec<_> = screen
        .passes
        .stages
        .iter()
        .filter(|stage| matches!(stage.slot, PassSlot::OcclusionMetadata(_)))
        .collect();
    assert_eq!(
        stages
            .iter()
            .map(|s| [s.target.width(), s.target.height()])
            .collect::<Vec<_>>(),
        [[4, 3], [2, 1]]
    );
    assert_eq!(
        [
            screen.passes.filtered.width(),
            screen.passes.filtered.height()
        ],
        [4, 3]
    );
    let seed: Vec<f32> = (0u16..63).map(f32::from).collect();
    let depth = &screen.passes.stages[0].target;
    write_depth(&context, depth, &seed);
    for stage in stages {
        draw(&context, &stage.pipeline, &stage.group, &stage.target);
    }
    let last = screen
        .passes
        .stages
        .iter()
        .find(|s| s.slot == PassSlot::OcclusionMetadata(1))
        .unwrap();
    // Exact on every conforming device: `mean_depth.wgsl` loads integer-addressed texels, adds
    // four values and multiplies by 0.25. Each stored value is a multiple of 0.25 far below 2^24,
    // so every sum and product is representable and correctly rounded arithmetic returns it
    // unchanged; R32Float readback is a transfer.
    assert_eq!(read(&context, &last.target), [15.0, 19.0]);
    assert!(scalable::source(2).contains("- 3, 0, 1)"));
}

fn write_depth(context: &crate::device::GpuContext, target: &wgpu::Texture, values: &[f32]) {
    context.queue().write_texture(
        target.as_image_copy(),
        &values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(target.width() * 4),
            rows_per_image: Some(target.height()),
        },
        target.size(),
    );
}

fn bytes(context: &crate::device::GpuContext, target: &wgpu::Texture) -> Vec<u8> {
    let config = worldrunner_render_post::FrameConfig::new(
        [target.width(), target.height()],
        target.format(),
        1,
    );
    worldrunner_render_post::Readback::new(context.device(), config, 100_000)
        .unwrap()
        .read(context.device(), context.submission(), target)
        .unwrap()
}

fn legacy_levels(context: &crate::device::GpuContext, seed: &wgpu::Texture) -> Vec<wgpu::Texture> {
    let mut previous = seed.clone();
    let mut levels = Vec::new();
    for _ in 0..8 {
        let size = [previous.width().div_ceil(2), previous.height().div_ceil(2)];
        let target = texture(context.device(), size, wgpu::TextureFormat::R32Float);
        let pipeline = pipeline(context.device(), include_str!("mean_depth.wgsl"));
        let inputs = group(context.device(), &pipeline, None, &[&previous]);
        draw(context, &pipeline, &inputs, &target);
        previous = target.clone();
        levels.push(target);
        if size == [1, 1] {
            break;
        }
    }
    levels
}

#[test]
fn gpu_default_depth_and_occlusion_bytes_match_legacy_allocation() {
    let context = context();
    let screen = SceneMetadata::new(
        context.device(),
        [9, 7],
        settings(SceneSsaoDepthFootprint::default()),
    )
    .unwrap();
    super::scalable_tests::camera(&context, &screen);
    let values: Vec<_> = (0u16..63)
        .map(|i| if i % 9 < 4 { 2.0 } else { 1.5 })
        .collect();
    let seed = upload(&context, [9, 7], &values);
    write_depth(&context, &screen.passes.stages[0].target, &values);
    write_normals(&context, &screen);
    let legacy = legacy_levels(&context, &seed);
    for stage in screen
        .passes
        .stages
        .iter()
        .filter(|s| !matches!(s.slot, PassSlot::DepthPyramid(_)))
    {
        draw(&context, &stage.pipeline, &stage.group, &stage.target);
    }
    for (index, level) in legacy.iter().enumerate() {
        let actual = screen
            .passes
            .stages
            .iter()
            .find(|s| s.slot == PassSlot::OcclusionMetadata(u32::try_from(index).unwrap()))
            .unwrap();
        assert_eq!(bytes(&context, &actual.target), bytes(&context, level));
    }
    compare_occlusion(&context, &screen, &legacy);
}

fn write_normals(context: &crate::device::GpuContext, screen: &SceneMetadata) {
    let normals: Vec<u16> = (0..63).flat_map(|_| [0, 0, 0x3c00, 0x3c00]).collect();
    context.queue().write_texture(
        screen.normals.as_image_copy(),
        &normals
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(9 * 8),
            rows_per_image: Some(7),
        },
        screen.normals.size(),
    );
}

fn compare_occlusion(
    context: &crate::device::GpuContext,
    screen: &SceneMetadata,
    legacy: &[wgpu::Texture],
) {
    let pipeline = pipeline_format(
        context.device(),
        &scalable::source(legacy.len()),
        wgpu::TextureFormat::Rg16Float,
    );
    let mut inputs = vec![&screen.normals];
    inputs.extend(legacy.iter());
    let inputs = group(context.device(), &pipeline, Some(&screen.uniform), &inputs);
    let raw = texture(context.device(), [5, 4], wgpu::TextureFormat::Rg16Float);
    draw(context, &pipeline, &inputs, &raw);
    let actual = screen
        .passes
        .stages
        .iter()
        .find(|s| s.slot == PassSlot::Ssao)
        .unwrap();
    assert_eq!(bytes(context, &actual.target), bytes(context, &raw));
    let mut filters = Vec::new();
    let filtered = kernels::append_filter(
        context.device(),
        &screen.uniform,
        &legacy[0],
        raw,
        screen.settings.filter,
        &mut filters,
    );
    for stage in filters {
        draw(context, &stage.pipeline, &stage.group, &stage.target);
    }
    assert_eq!(
        bytes(context, &screen.passes.filtered),
        bytes(context, &filtered)
    );
}
