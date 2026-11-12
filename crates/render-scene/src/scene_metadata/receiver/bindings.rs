const LABEL: &str = "shadow with receiver displacements";
const OBJECT_LABEL: &str = "shadow with receiver displacements and card reject";

fn layout_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries = worldrunner_render_shadow::receiver_layout_entries(
        worldrunner_render_shadow::ReceiverDescriptor {
            label: Some("directional shadow"),
            dimension: worldrunner_render_shadow::ReceiverDimension::D2Array,
            texture_binding: 1,
            sampler_binding: 2,
            uniform_binding: 0,
            min_uniform_binding_size: None,
        },
    )
    .to_vec();
    entries.extend((3..5).map(|binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }));
    entries
}

pub(crate) fn shadow_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(LABEL),
        entries: &layout_entries(),
    })
}

/// Set 2 of object pipelines whose relief cards reject hidden fragments: the receiver set plus
/// the Opaque pass's depth attachment at (2,5), sampled while the pass holds it read-only.
pub(crate) fn object_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = layout_entries();
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 5,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    });
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(OBJECT_LABEL),
        entries: &entries,
    })
}

/// The shadow map resources every receiver group binds at 0..=2.
pub(crate) struct ShadowInputs<'a> {
    pub uniform: &'a wgpu::Buffer,
    pub depth: &'a wgpu::TextureView,
    pub sampler: &'a wgpu::Sampler,
}

/// The set-2 groups of one frame. `object` equals `terrain` unless the frame's object pipelines
/// reject relief cards, in which case it adds the Opaque depth at (2,5).
pub(crate) struct SetTwo {
    pub object: wgpu::BindGroup,
    pub terrain: wgpu::BindGroup,
}

/// The frame's set-2 groups. `card_depth` is the sampled view of the Opaque depth attachment,
/// given only when the selected pipelines were built with the card reject; the schedule then
/// attaches that depth read-only.
pub(crate) fn set_two(
    device: &wgpu::Device,
    payload: &super::ReceiverPayload,
    map: &ShadowInputs<'_>,
    card_depth: Option<&wgpu::TextureView>,
) -> SetTwo {
    let views = payload.views();
    let mut entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: map.uniform.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(map.depth),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(map.sampler),
        },
        wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(&views[0]),
        },
        wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::TextureView(&views[1]),
        },
    ];
    let terrain = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(LABEL),
        layout: &shadow_layout(device),
        entries: &entries,
    });
    let Some(depth) = card_depth else {
        return SetTwo {
            object: terrain.clone(),
            terrain,
        };
    };
    entries.push(wgpu::BindGroupEntry {
        binding: 5,
        resource: wgpu::BindingResource::TextureView(depth),
    });
    let object = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(OBJECT_LABEL),
        layout: &object_layout(device),
        entries: &entries,
    });
    SetTwo { object, terrain }
}
