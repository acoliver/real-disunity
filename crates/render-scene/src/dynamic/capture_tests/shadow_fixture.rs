//! The directional shadow inputs the capture suites share: one cascade over a cleared depth map,
//! as the bind group of each of two frame slots, or as one group whose cascade a suite chooses.

use wgpu::{BindGroup, TextureFormat};

use crate::device::GpuContext;

/// The cascade scale of each frame slot: 0.25 for slot 0 and 0.2 for slot 1.
const SCALES: [f32; 2] = [0.25, 0.2];

/// The first cascade of a frame slot: `scale` in x and y, and scene depths mapped across the
/// cleared depth.
fn scaled(scale: f32) -> glam::Mat4 {
    glam::Mat4::from_cols_array(&[
        scale, 0.0, 0.0, 0.0, //
        0.0, scale, 0.0, 0.0, //
        0.0, 0.0, 0.2, 0.0, //
        0.0, 0.0, 0.5, 1.0,
    ])
}

/// One cascade whose clip space maps scene depths across the cleared depth. `scale` sets the
/// cascade's x and y scale, so the two frame slots carry different uniform values.
fn uniform(scale: f32) -> Vec<u8> {
    uniform_with_cascade(scaled(scale))
}

/// The shadow uniform with `first` as the one cascade in use and identity for the other three.
fn uniform_with_cascade(first: glam::Mat4) -> Vec<u8> {
    let mut values = Vec::with_capacity(crate::shadows::SHADOW_UNIFORM_BYTES / 4);
    for cascade in 0..4 {
        let matrix = if cascade == 0 {
            first
        } else {
            glam::Mat4::IDENTITY
        };
        values.extend(matrix.to_cols_array());
    }
    // multiplier, normal bias, grid radius, strength reduction.
    values.extend([1.0, 0.0, 1.0, 0.0]);
    // one cascade, no blend, light-space selection, camera near.
    values.extend([1.0, 0.0, 0.0, 0.1]);
    values.extend([10.0, 20.0, 30.0, 40.0]);
    values.extend([0.0, 0.0, -1.0, 0.0]);
    values.extend([0.001; 4]);
    // no fade toward lit at the end of the shadow distance.
    values.extend([0.0; 4]);
    let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_le_bytes).collect();
    assert_eq!(bytes.len(), crate::shadows::SHADOW_UNIFORM_BYTES);
    bytes
}

/// Four layers of depth, each cleared to one half.
fn cleared_depth(context: &GpuContext) -> wgpu::Texture {
    let device = context.device();
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture shadow depth"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 4,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let mut encoder = context.submission().begin(device).unwrap();
    for layer in 0..4 {
        let view = depth.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..wgpu::TextureViewDescriptor::default()
        });
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("capture shadow clear"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.5),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        }));
    }
    encoder.finish().submit().unwrap();
    depth
}

/// The shadow group of each of two frame slots, in the `water_layout` shape: uniform at binding
/// 0, the `D2Array` view of four 32x32 `Depth32Float` layers cleared to 0.5 at binding 1, and a
/// linear comparison sampler with `LessEqual` at binding 2. Slot 0 carries cascade scale 0.25
/// and slot 1 carries 0.2; both slots share one map and one sampler. The two uniforms hold
/// different values, which this fixture's contract test checks. Whether a given scene draws
/// different pixels under them depends on the scene and is not claimed here.
pub(crate) fn groups(context: &GpuContext) -> [BindGroup; 2] {
    bind(context, SCALES.map(uniform))
}

/// One shadow group in the shape [`groups`] describes, over its own cleared map, whose one
/// cascade in use is `first`. A suite whose surface lies at one depth passes a cascade that maps
/// position across the cleared depth, so part of the surface is lit and part is not.
pub(crate) fn group_with_cascade(context: &GpuContext, first: glam::Mat4) -> BindGroup {
    let [group] = bind(context, [uniform_with_cascade(first)]);
    group
}

/// A group for each of `uniforms`, sharing one cleared map and one comparison sampler.
fn bind<const N: usize>(context: &GpuContext, uniforms: [Vec<u8>; N]) -> [BindGroup; N] {
    let device = context.device();
    let view = cleared_depth(context).create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..wgpu::TextureViewDescriptor::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("capture shadow sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..wgpu::SamplerDescriptor::default()
    });
    let layout = crate::shadows::water_layout(device);
    uniforms.map(|bytes| {
        let buffer = crate::upload::upload_buffer(
            device,
            context.queue(),
            "capture shadow uniform",
            wgpu::BufferUsages::UNIFORM,
            &bytes,
        );
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("capture shadow"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{SCALES, uniform, uniform_with_cascade};

    /// The uniform of one slot spelled out as floats, independent of `SCALES` and `uniform`.
    fn expected(scale: f32) -> Vec<u8> {
        let identity = [
            1.0_f32, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ];
        let first = [
            scale, 0.0, 0.0, 0.0, //
            0.0, scale, 0.0, 0.0, //
            0.0, 0.0, 0.2, 0.0, //
            0.0, 0.0, 0.5, 1.0,
        ];
        let tail = [
            1.0_f32, 0.0, 1.0, 0.0, //
            1.0, 0.0, 0.0, 0.1, //
            10.0, 20.0, 30.0, 40.0, //
            0.0, 0.0, -1.0, 0.0, //
            0.001, 0.001, 0.001, 0.001, //
            0.0, 0.0, 0.0, 0.0,
        ];
        first
            .iter()
            .chain(&identity)
            .chain(&identity)
            .chain(&identity)
            .chain(&tail)
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn shadow_fixture_slots_carry_scales_one_quarter_and_one_fifth() {
        assert_eq!(
            SCALES.map(f32::to_bits),
            [0.25_f32, 0.2_f32].map(f32::to_bits),
            "the slot scales are the input"
        );
    }

    #[test]
    fn shadow_fixture_uniforms_encode_each_slot_scale_in_the_first_cascade_matrix() {
        let (quarter, fifth) = (0.25_f32, 0.2_f32);
        assert_eq!(
            uniform(SCALES[0]),
            expected(quarter),
            "slot 0 uniform bytes"
        );
        assert_eq!(uniform(SCALES[1]), expected(fifth), "slot 1 uniform bytes");
        assert_eq!(
            uniform(SCALES[0]).len(),
            crate::shadows::SHADOW_UNIFORM_BYTES
        );
        assert_ne!(
            uniform(SCALES[0]),
            uniform(SCALES[1]),
            "the slots differ in uniform bytes"
        );
    }

    #[test]
    fn shadow_fixture_chosen_cascade_replaces_only_the_first_matrix() {
        let first = glam::Mat4::from_cols_array(&[
            0.2, 0.0, 0.1, 0.0, //
            0.0, 0.2, 0.0, 0.0, //
            0.0, 0.0, 0.2, 0.0, //
            0.0, 0.0, 0.5, 1.0,
        ]);
        let chosen = uniform_with_cascade(first);
        let slot = uniform(SCALES[1]);
        let matrix_bytes = 16 * 4;
        let expected: Vec<u8> = first
            .to_cols_array()
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        assert_eq!(
            chosen[..matrix_bytes],
            expected[..],
            "the first cascade is the chosen one"
        );
        assert_eq!(
            chosen[matrix_bytes..],
            slot[matrix_bytes..],
            "every byte after the first cascade is the slot fixture's"
        );
    }
}
