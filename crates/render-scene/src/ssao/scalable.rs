use super::{ScreenPass, group, pipeline, pipeline_format, texture};
use crate::frame::metadata::FrameProduct;
use std::fmt::Write;
use worldrunner_render_plan::{SceneSsao, SceneSsaoFilter};
use worldrunner_render_post::PassSlot;

pub(super) fn source(levels: usize) -> String {
    assert!((1..=8).contains(&levels));
    let mut bindings =
        String::from("@group(0) @binding(1) var surface_normals: texture_2d<f32>;\n");
    let mut selection =
        String::from("fn depth_at(uv: vec2<f32>, level: i32) -> f32 { switch level {\n");
    for index in 0..levels {
        writeln!(
            bindings,
            "@group(0) @binding({}) var footprint{index}: texture_2d<f32>;",
            index + 2
        )
        .expect("string formatting");
        writeln!(
            selection,
            "case {index}: {{ return filtered_depth(footprint{index}, uv); }}"
        )
        .expect("string formatting");
    }
    selection.push_str("default: { return filtered_depth(footprint0, uv); } } }\n");
    include_str!("scalable.wgsl")
        .replace("/* DEPTH_BINDINGS */", &bindings)
        .replace("/* DEPTH_SELECT */", &selection)
        .replace("MAX_LEVEL", &(levels - 1).to_string())
}

pub(super) fn append(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    normals: &wgpu::Texture,
    settings: SceneSsao,
    stages: &mut Vec<ScreenPass>,
) -> wgpu::Texture {
    let depth = stages[0].target.clone();
    let mut previous = depth;
    let mut levels = Vec::new();
    let mut reads = Vec::new();
    for index in 0..8 {
        let size = [previous.width().div_ceil(2), previous.height().div_ceil(2)];
        let target = texture(device, size, wgpu::TextureFormat::R32Float);
        let pipeline = pipeline(device, include_str!("mean_depth.wgsl"));
        let group = group(device, &pipeline, None, &[&previous]);
        let resource = FrameProduct::averaged_depth(index).resource();
        stages.push(ScreenPass {
            slot: PassSlot::OcclusionMetadata(index),
            pipeline,
            group,
            target: target.clone(),
            reads: vec![if index == 0 {
                super::level(0)
            } else {
                reads[levels.len() - 1]
            }],
        });
        reads.push(resource);
        previous = target.clone();
        levels.push(target);
        if size == [1, 1] {
            break;
        }
    }
    let mut inputs = vec![normals];
    inputs.extend(levels.iter());
    let pipeline = pipeline_format(
        device,
        &source(levels.len()),
        wgpu::TextureFormat::Rg16Float,
    );
    let group = group(device, &pipeline, Some(uniform), &inputs);
    reads.insert(0, FrameProduct::normals().resource());
    let size = [levels[0].width(), levels[0].height()];
    let target = texture(device, size, wgpu::TextureFormat::Rg16Float);
    stages.push(ScreenPass {
        slot: PassSlot::Ssao,
        pipeline,
        group,
        target: target.clone(),
        reads,
    });
    super::kernels::append_filter(device, uniform, &levels[0], target, settings.filter, stages)
}

pub(super) fn blur_source(filter: SceneSsaoFilter, axis: [i32; 2]) -> String {
    let SceneSsaoFilter::ScalableBilateral {
        radius,
        step,
        sigma_scale_micro,
        sharpness_micro,
    } = filter
    else {
        unreachable!("validated scalable filter")
    };
    let sigma = super::super::scalar(u64::from(radius))
        * (1.0 / (super::super::scalar(u64::from(sigma_scale_micro)) / 1_000_000.0));
    let sharpness = super::super::scalar(u64::from(sharpness_micro)) / 1_000_000.0;
    include_str!("scalable_filter.wgsl")
        .replace("AXIS_X", &axis[0].to_string())
        .replace("AXIS_Y", &axis[1].to_string())
        .replace("TAP_RADIUS", &radius.to_string())
        .replace("TAP_STEP", &step.to_string())
        .replace("VARIANCE", &format!("{:.12}", 2.0 * sigma * sigma))
        .replace("DEPTH_COEFFICIENT", &format!("{sharpness:.9}"))
}
