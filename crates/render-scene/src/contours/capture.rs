//! Capture-only sample extraction. Integer IDs are never hardware-resolved.
use worldrunner_render_post::{
    FrameError, FullscreenDescriptor, fullscreen_pass, fullscreen_pipeline,
};

const SHADER: &str = "
@group(0) @binding(0) var ids: texture_multisampled_2d<u32>;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(p[index],0.0,1.0);
}
@fragment fn fs(@builtin(position) position: vec4<f32>) -> @location(0) u32 {
    let width = textureDimensions(ids).x;
    let pixel = vec2<u32>(position.xy);
    return textureLoad(ids, vec2<i32>(i32(pixel.x % width),i32(pixel.y)), i32(pixel.x / width)).r;
}";

pub(crate) fn encode(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
) -> Result<wgpu::Texture, FrameError> {
    let width = source
        .width()
        .checked_mul(source.sample_count())
        .filter(|width| *width <= device.limits().max_texture_dimension_2d)
        .ok_or(FrameError::FeaturePreparation {
            feature: "contour capture",
            reason: "sample-lane width exceeds device limits".into(),
        })?;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("contour ID sample capture"),
        size: wgpu::Extent3d {
            width,
            height: source.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: super::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let (pipeline, group) = pipeline(device, source);
    fullscreen_pass(
        encoder,
        Some("capture contour samples"),
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[&group],
    );
    Ok(target)
}

fn pipeline(
    device: &wgpu::Device,
    source: &wgpu::Texture,
) -> (wgpu::RenderPipeline, wgpu::BindGroup) {
    let multisampled = source.sample_count() > 1;
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("contour capture input"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Uint,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled,
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("contour capture input"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(
                &source.create_view(&wgpu::TextureViewDescriptor::default()),
            ),
        }],
    });
    let source = if multisampled {
        SHADER.to_owned()
    } else {
        SHADER.replace("texture_multisampled_2d<u32>", "texture_2d<u32>")
    };
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("capture contour samples"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("contour capture"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = fullscreen_pipeline(
        device,
        FullscreenDescriptor {
            label: Some("contour capture"),
            layout: &layout,
            module: &module,
            vertex_entry: "vs",
            fragment_entry: "fs",
            format: super::FORMAT,
            constants: &[],
        },
    );
    (pipeline, group)
}

#[cfg(test)]
#[test]
fn contour_capture_shaders_validate_single_and_multisampled_uint_loads() {
    for source in [
        SHADER.to_owned(),
        SHADER.replace("texture_multisampled_2d<u32>", "texture_2d<u32>"),
    ] {
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
