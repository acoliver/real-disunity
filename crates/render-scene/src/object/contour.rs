//! Contour entry points in the same object program context as the opaque producer.
use super::{
    BindGroupLayout, Context, Device, Entry, PipelineKey, RenderState, ResourceKind,
    ResourceLayout, ShaderStages, TextureFormat, declare,
};

pub(super) const SEMANTIC: &str = "object.contour_id";
pub(super) const BINDING: u32 = 30;

pub(super) fn layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("contour ID only"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: BINDING,
            visibility: ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(16),
            },
            count: None,
        }],
    })
}

pub(super) fn bindings() -> ResourceLayout {
    ResourceLayout::new(vec![declare(
        (2, BINDING),
        SEMANTIC,
        ShaderStages::FRAGMENT,
        ResourceKind::Uniform { min_size: 16 },
    )])
}

impl Context {
    /// The same vertex source, constants, winding and skin layout as the producer. The
    /// contour entry has no lighting dependency and lends only its ID at set two.
    pub(crate) fn contour(&self) -> Self {
        self.0
            .contour
            .borrow_mut()
            .get_or_insert_with(|| {
                let mut description = self.0.description.clone();
                assert!(
                    description.source.contains("@invariant @builtin(position)"),
                    "contour producers must keep invariant raster positions"
                );
                description.source = format!(
                    "{}\n{}",
                    description.source,
                    include_str!("../shader/contour.wgsl")
                )
                .into();
                description.colors = vec![TextureFormat::R8Uint];
                description.shadow = None;
                description.screen = None;
                description.entry = Entry::Contour;
                Self::new(description, &self.0.family)
            })
            .clone()
    }

    pub(crate) fn contour_group(&self, id: u8) -> wgpu::BindGroup {
        use wgpu::util::DeviceExt;
        self.0
            .contour_groups
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| {
                let buffer = self
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("immutable contour ID"),
                        contents: &[u32::from(id).to_le_bytes(), [0; 4], [0; 4], [0; 4]].concat(),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                self.device().create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("contour ID only"),
                    layout: self.contour_layout(),
                    entries: &[wgpu::BindGroupEntry {
                        binding: BINDING,
                        resource: buffer.as_entire_binding(),
                    }],
                })
            })
            .clone()
    }

    pub(crate) fn contour_layout(&self) -> &BindGroupLayout {
        self.0
            .contour_layout
            .as_ref()
            .expect("contour context owns its ID layout")
    }
}

pub(super) fn entry(key: PipelineKey) -> &'static str {
    match (key.relief, key.masked) {
        (false, false) => "fs_contour_opaque",
        (false, true) => "fs_contour_masked",
        (true, false) => "fs_contour_relief_opaque",
        (true, true) => "fs_contour_relief_masked",
    }
}

pub(super) fn state(mut state: RenderState) -> RenderState {
    let depth = state.depth.as_mut().expect("object depth state");
    depth.test = wgpu::CompareFunction::Equal;
    depth.write = false;
    state.stencil = None;
    state.raster.alpha_to_coverage = false;
    state
}

#[cfg(test)]
#[path = "contour_tests.rs"]
mod tests;
