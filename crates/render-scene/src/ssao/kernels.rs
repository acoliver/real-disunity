use super::{ScreenPass, group, level, pipeline, pipeline_format, texture};
use crate::frame::metadata::FrameProduct;
use worldrunner_render_plan::{SceneSsao, SceneSsaoFilter, SceneSsaoKernel};
use worldrunner_render_post::{PassSlot, ResourceRef};

fn averaged(
    device: &wgpu::Device,
    size: [u32; 2],
    depth: &wgpu::Texture,
    normals: &wgpu::Texture,
    stages: &mut Vec<ScreenPass>,
) -> Vec<wgpu::Texture> {
    let mut levels: Vec<wgpu::Texture> = Vec::new();
    let mut extent = size;
    loop {
        let index = u32::try_from(levels.len()).expect("bounded metadata levels");
        let target = texture(device, extent, wgpu::TextureFormat::Rgba16Float);
        let (shader, inputs, reads) = if let Some(previous) = levels.last() {
            (
                include_str!("average.wgsl"),
                vec![previous],
                vec![FrameProduct::occlusion_metadata(index - 1).resource()],
            )
        } else {
            (
                include_str!("pack.wgsl"),
                vec![depth, normals],
                vec![level(0), FrameProduct::normals().resource()],
            )
        };
        let pipeline = pipeline_format(device, shader, wgpu::TextureFormat::Rgba16Float);
        let group = group(device, &pipeline, None, &inputs);
        levels.push(target.clone());
        stages.push(ScreenPass {
            slot: PassSlot::OcclusionMetadata(index),
            pipeline,
            group,
            target,
            reads,
        });
        if extent == [1, 1] || index == 6 {
            break;
        }
        extent = extent.map(|side| side.div_ceil(2));
    }
    levels
}

pub(super) fn append(
    device: &wgpu::Device,
    size: [u32; 2],
    uniform: &wgpu::Buffer,
    normals: &wgpu::Texture,
    settings: SceneSsao,
    stages: &mut Vec<ScreenPass>,
) -> wgpu::Texture {
    let depth = stages[0].target.clone();
    let (ao, inputs, reads) = match settings.kernel {
        SceneSsaoKernel::Hemisphere => (
            pipeline(device, include_str!("occlusion.wgsl")),
            vec![depth.clone(), normals.clone()],
            vec![level(0), FrameProduct::normals().resource()],
        ),
        SceneSsaoKernel::Spiral => {
            let levels = averaged(device, size, &depth, normals, stages);
            let indices = [0, 2, 4, 6].map(|index| index.min(levels.len() - 1));
            let mut source = include_str!("spiral.wgsl").to_owned();
            for (binding, index) in indices.iter().enumerate().skip(1) {
                source = source.replace(
                    &format!("LEVEL{binding}_SCALE"),
                    &(1u32 << index).to_string(),
                );
            }
            let inputs = indices.iter().map(|&index| levels[index].clone()).collect();
            let mut reads: Vec<_> = indices
                .iter()
                .map(|&index| {
                    FrameProduct::occlusion_metadata(u32::try_from(index).expect("bounded level"))
                        .resource()
                })
                .collect();
            reads.sort();
            reads.dedup();
            (pipeline(device, &source), inputs, reads)
        }
    };
    let inputs: Vec<_> = inputs.iter().collect();
    let group = group(device, &ao, Some(uniform), &inputs);
    let raw = texture(device, size, wgpu::TextureFormat::R32Float);
    stages.push(ScreenPass {
        slot: PassSlot::Ssao,
        pipeline: ao,
        group,
        target: raw.clone(),
        reads,
    });
    append_filter(device, uniform, &depth, raw, settings.filter, stages)
}

pub(super) fn append_filter(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    depth: &wgpu::Texture,
    raw: wgpu::Texture,
    filter: SceneSsaoFilter,
    stages: &mut Vec<ScreenPass>,
) -> wgpu::Texture {
    let axes: &[[i32; 2]] = match filter {
        SceneSsaoFilter::DepthAwareBox => &[[0, 0]],
        SceneSsaoFilter::SeparableBilateral => &[[1, 0], [0, 1]],
    };
    let mut input = raw;
    let mut input_resource = ResourceRef::new("raw ambient occlusion");
    for &axis in axes {
        let slot = if axis == [1, 0] {
            PassSlot::SsaoBlurHorizontal
        } else {
            PassSlot::SsaoBlur
        };
        let shader = if axis == [0, 0] {
            include_str!("blur.wgsl").to_owned()
        } else {
            crate::ssao::bilateral::source(axis)
        };
        let pipeline = pipeline(device, &shader);
        let group = group(
            device,
            &pipeline,
            (axis == [0, 0]).then_some(uniform),
            &[depth, &input],
        );
        let target = texture(
            device,
            [depth.width(), depth.height()],
            wgpu::TextureFormat::R32Float,
        );
        stages.push(ScreenPass {
            slot,
            pipeline,
            group,
            target: target.clone(),
            reads: vec![level(0), input_resource],
        });
        input_resource = super::ScreenPasses::resource(slot);
        input = target;
    }
    input
}
