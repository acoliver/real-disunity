use crate::ViewerError;
use worldrunner_render_scene::{Viewport, renderer::depth_texture};

/// Always use the shadow renderer's honest attachment contract. The final pass
/// handles channel ordering through a pipeline matching the negotiated surface.
pub(super) const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(super) struct Presentation {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    pub targets: Targets,
}

pub(super) struct Targets {
    pub color: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    group: wgpu::BindGroup,
}

impl Presentation {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: Viewport) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewer blit layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewer presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("presentation.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewer presentation"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewer presentation"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let targets = Targets::new(device, &layout, size);
        Self {
            pipeline,
            layout,
            targets,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: Viewport) {
        self.targets = Targets::new(device, &self.layout, size);
    }

    pub fn present(&self, encoder: &mut wgpu::CommandEncoder, surface: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewer presentation"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.targets.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Targets {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, size: Viewport) -> Self {
        let color = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("viewer RGBA intermediate"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth =
            depth_texture(device, size).create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewer blit source"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&color),
            }],
        });
        Self {
            color,
            depth,
            group,
        }
    }
}

/// Native presentation and scene attachments have different size contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Extents {
    pub surface: Viewport,
    pub render: Viewport,
}

impl Extents {
    pub fn new(size: [u32; 2], device_limit: u32) -> Result<Option<Self>, ViewerError> {
        if size.contains(&0) {
            return Ok(None);
        }
        if size.iter().any(|&side| side > device_limit) {
            return Err(ViewerError::SurfaceExtent { size, device_limit });
        }
        Ok(Some(Self {
            surface: Viewport {
                width: size[0],
                height: size[1],
            },
            render: bounded_viewport(size, device_limit).expect("nonzero supported extent"),
        }))
    }
}

/// Uniformly downscale attachments, not the native surface. Round each side to
/// the nearest pixel; projection uses the physical aspect, so rounding cannot
/// change scene geometry's apparent aspect after the full-surface blit.
fn bounded_viewport(size: [u32; 2], device_limit: u32) -> Option<Viewport> {
    let longest = *size.iter().max()?;
    let limit = device_limit.min(4096);
    if size.contains(&0) || limit == 0 {
        return None;
    }
    let denominator = u64::from(longest.max(limit));
    let scale = |side| {
        u32::try_from((u64::from(side) * u64::from(limit) + denominator / 2) / denominator)
            .expect("scaled dimension is at most 4096")
            .max(1)
    };
    Some(Viewport {
        width: scale(size[0]),
        height: scale(size[1]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_extents_are_preserved_or_rejected_before_configuration() {
        for (physical, render) in [
            ([5120, 2880], [4096, 2304]),
            ([2880, 5120], [2304, 4096]),
            ([8192, 1], [4096, 1]),
            ([257, 241], [257, 241]),
        ] {
            let extents = Extents::new(physical, 8192).unwrap().unwrap();
            assert_eq!([extents.surface.width, extents.surface.height], physical);
            assert_eq!([extents.render.width, extents.render.height], render);
        }
        for physical in [[8193, 100], [100, 8193], [u32::MAX, u32::MAX]] {
            assert!(
                matches!(Extents::new(physical, 8192), Err(ViewerError::SurfaceExtent { size, device_limit: 8192 }) if size == physical)
            );
        }
        assert_eq!(Extents::new([0, u32::MAX], 8192).unwrap(), None);
        assert!(matches!(
            Extents::new([1, 1], 0),
            Err(ViewerError::SurfaceExtent { .. })
        ));
        assert_eq!(
            bounded_viewport([u32::MAX, 8000], 8192),
            Some(Viewport {
                width: 4096,
                height: 1
            })
        );
    }

    #[test]
    fn zero_area_suspends_and_oversized_extents_are_bounded() {
        assert_eq!(bounded_viewport([0, 100], 8192), None);
        assert_eq!(bounded_viewport([100, 0], 8192), None);
        assert_eq!(bounded_viewport([100, 100], 0), None);
        assert_eq!(
            bounded_viewport([5120, 2880], 8192),
            Some(Viewport {
                width: 4096,
                height: 2304
            })
        );
        assert_eq!(
            bounded_viewport([500, 900], 256),
            Some(Viewport {
                width: 142,
                height: 256
            })
        );
        assert_eq!(
            bounded_viewport([257, 241], 8192),
            Some(Viewport {
                width: 257,
                height: 241
            })
        );
    }
}
