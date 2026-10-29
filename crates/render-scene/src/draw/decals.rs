use super::{
    DrawError,
    visibility::{Bounds, Frustum},
};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use worldrunner_render_plan::{RenderScene, SceneDecal, SceneDecalAxis, SceneDecalWriteMask};

mod channels;
use channels::Channel;

const LIMIT: usize = 32;
pub(crate) const BYTES: usize = 16 + LIMIT * 144;

#[derive(Debug)]
struct Entry {
    id: u64,
    decal: SceneDecal,
    model: Mat4,
    bounds: Bounds,
    surface: [f32; 3],
}
#[derive(Debug)]
struct Palette {
    materials: Vec<u32>,
    group: wgpu::BindGroup,
}
#[derive(Debug)]
pub(super) struct Decals {
    scene: RenderScene,
    entries: Vec<Entry>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    palettes: RefCell<Vec<Palette>>,
    pub buffers: Vec<wgpu::Buffer>,
    pub slot: usize,
}

/// Checks only referenced decal materials and borrowed texture descriptors/mip ranges.
/// No world geometry preparation or texture decoding is performed.
pub fn validate_decal_materials(scene: &RenderScene) -> Result<(), DrawError> {
    let mut materials: Vec<_> = scene.decals().iter().map(|d| d.material.raw()).collect();
    materials.sort_unstable();
    materials.dedup();
    for &handle in &materials {
        validate_material(&scene.resources().materials()[handle as usize])?;
    }
    for channel in [Channel::Albedo, Channel::Normal, Channel::Surface] {
        channel.validate(scene, &materials)?;
    }
    Ok(())
}
fn validate_material(material: &worldrunner_render_plan::SceneMaterial) -> Result<(), DrawError> {
    if material.normal().is_some()
        && (material.normal_encoding()
            != worldrunner_render_plan::SceneNormalEncoding::TwoChannelXy
            || material.normal_y() != worldrunner_render_plan::SceneNormalY::Up
            || material.normal_scale_micro() != 1_000_000)
    {
        return Err(crate::frame::FrameError::UnsupportedEffect(
            "unsupported decal normal controls",
        )
        .into());
    }
    if material.features().is_some()
        || material.impostor().is_some()
        || material.self_illumination().is_some()
        || material.ambient_occlusion().is_some()
        || material.terrain_contact().is_some()
        || material.translucency_micro() != 0
    {
        return Err(crate::frame::FrameError::UnsupportedEffect(
            "unsupported decal material effect",
        )
        .into());
    }
    Ok(())
}

impl Decals {
    pub fn allocate(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &super::PreparedScene,
        layout: &wgpu::BindGroupLayout,
    ) -> Result<(Self, Vec<(wgpu::Buffer, wgpu::BindGroup)>), DrawError> {
        let slots = super::resources::allocate_frame_slots(device, queue, layout);
        let buffers: Vec<_> = slots.iter().map(|_| empty_buffer(device)).collect();
        let palettes = slots
            .iter()
            .zip(&buffers)
            .map(|((frame, _), buffer)| Palette {
                materials: Vec::new(),
                group: empty_group_with(device, layout, frame, buffer),
            })
            .collect();
        let mut decals = Self {
            scene: prepared.scene.clone(),
            entries: Vec::new(),
            device: device.clone(),
            queue: queue.clone(),
            layout: layout.clone(),
            palettes: RefCell::new(palettes),
            buffers,
            slot: 0,
        };
        decals.replace(
            prepared
                .scene
                .decals()
                .iter()
                .copied()
                .enumerate()
                .map(|(id, decal)| (id as u64, decal))
                .collect(),
        )?;
        Ok((decals, slots))
    }

    pub fn placements(&self) -> Vec<(u64, SceneDecal)> {
        self.entries.iter().map(|e| (e.id, e.decal)).collect()
    }

    pub fn replace(&mut self, placements: Vec<(u64, SceneDecal)>) -> Result<(), DrawError> {
        let scene = self
            .scene
            .with_decals(placements.iter().map(|p| p.1).collect())
            .map_err(|_| crate::frame::FrameError::UnsupportedEffect("invalid decal placement"))?;
        validate_decal_materials(&scene)?;
        self.entries = placements
            .into_iter()
            .map(|(id, decal)| entry(&scene, id, decal))
            .collect();
        self.scene = scene;
        Ok(())
    }

    pub fn group(&self) -> wgpu::BindGroup {
        self.palettes.borrow()[self.slot].group.clone()
    }

    pub fn prepare(&self, frame: &crate::frame::FrameUniform, buffer: &wgpu::Buffer) -> Vec<u8> {
        let selected = selected(&self.entries, frame.view_projection, frame.camera_position);
        let mut materials: Vec<_> = selected
            .iter()
            .map(|&i| self.entries[i].decal.material.raw())
            .collect();
        materials.sort_unstable();
        materials.dedup();
        let mut palettes = self.palettes.borrow_mut();
        let palette = &mut palettes[self.slot];
        if palette.materials != materials {
            let views = [Channel::Albedo, Channel::Normal, Channel::Surface].map(|channel| {
                channel
                    .texture(self, &materials)
                    .create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2Array),
                        ..Default::default()
                    })
            });
            palette.group = group_with(
                &self.device,
                &self.layout,
                buffer,
                &self.buffers[self.slot],
                &views,
                &crate::upload::material_sampler(&self.device),
            );
            palette.materials.clone_from(&materials);
        }
        self.bytes(&selected, &materials)
    }

    fn bytes(&self, selected: &[usize], materials: &[u32]) -> Vec<u8> {
        assert!(selected.len() <= LIMIT);
        let mut values = vec![
            f32::from_bits(u32::try_from(selected.len()).expect("bounded decals")),
            0.0,
            0.0,
            0.0,
        ];
        for &index in selected {
            let Entry {
                decal,
                model,
                surface,
                ..
            } = &self.entries[index];
            values.extend(model.inverse().to_cols_array());
            let axis = match decal.axis {
                SceneDecalAxis::X => Vec3::X,
                SceneDecalAxis::Y => Vec3::Y,
                SceneDecalAxis::Z => Vec3::Z,
            };
            values.extend(model.transform_vector3(axis).normalize().to_array());
            values.push(unit(decal.edge_fade_micro));
            values.extend([
                surface[0],
                surface[1],
                unit(decal.angle_cutoff_micro),
                unit(decal.angle_fade_micro),
            ]);
            let layer = materials
                .binary_search(&decal.material.raw())
                .expect("selected material");
            values.extend([
                f32::from_bits(receiver_and_alpha_test(decal)),
                f32::from_bits(u32::try_from(layer).expect("bounded palette")),
                f32::from_bits(match decal.axis {
                    SceneDecalAxis::X => 0,
                    SceneDecalAxis::Y => 1,
                    SceneDecalAxis::Z => 2,
                }),
                f32::from_bits(
                    Channel::Normal.layer(&self.scene, materials, decal.material.raw())
                        | (Channel::Surface.layer(&self.scene, materials, decal.material.raw())
                            << 8)
                        | ((SceneDecalWriteMask::ALL.bits() ^ decal.write_mask.bits()) << 16),
                ),
            ]);
            mapping_values(&mut values, decal, surface[2]);
        }
        let mut bytes: Vec<_> = values.into_iter().flat_map(f32::to_le_bytes).collect();
        assert!(bytes.len() <= BYTES);
        bytes.resize(BYTES, 0);
        bytes
    }
}

// Unused selection bits preserve the default uniform layout and bytes.
fn receiver_and_alpha_test(decal: &SceneDecal) -> u32 {
    decal.receivers.bits()
        | decal
            .alpha_test_cutoff_micro
            .map_or(0, |cutoff| (1 << 22) | (cutoff << 2))
}

fn mapping_values(values: &mut Vec<f32>, decal: &SceneDecal, alpha: f32) {
    let (threshold, exponent) = match decal.mapping.angle {
        worldrunner_render_plan::SceneDecalAngle::Linear => (0.0, 0.0),
        worldrunner_render_plan::SceneDecalAngle::Power {
            threshold_micro,
            exponent_micro,
        } => (unit(threshold_micro), unit(exponent_micro)),
    };
    values.extend([
        alpha,
        threshold,
        exponent,
        if decal.mapping.double_sided { 1.0 } else { 0.0 },
    ]);
    values.extend(decal.mapping.uv_scale_micro.map(|x| {
        crate::view::meters_scalar(x.unsigned_abs()) / 1000.0 * if x < 0 { -1.0 } else { 1.0 }
    }));
    values.extend(decal.mapping.uv_offset_micro.map(|x| {
        crate::view::meters_scalar(x.unsigned_abs()) / 1000.0 * if x < 0 { -1.0 } else { 1.0 }
    }));
}

fn entry(scene: &RenderScene, id: u64, decal: SceneDecal) -> Entry {
    let model = crate::geometry::model_matrix(&decal.transform)
        * Mat4::from_scale(Vec3::from_array(
            decal.dimensions_millimeters.map(crate::view::meters_scalar),
        ));
    let center = model.w_axis.truncate();
    let half = (model.x_axis.truncate().abs()
        + model.y_axis.truncate().abs()
        + model.z_axis.truncate().abs())
        * 0.5;
    let material = &scene.resources().materials()[decal.material.raw() as usize];
    Entry {
        id,
        decal,
        model,
        bounds: Bounds {
            min: center - half,
            max: center + half,
        },
        surface: [
            unit(material.roughness_micro()),
            unit(material.metalness_micro()),
            unit(material.alpha_factor_micro()),
        ],
    }
}
fn unit(value: u32) -> f32 {
    crate::view::meters_scalar(value) / 1000.0
}
fn empty_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("decal volumes"),
        size: BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
fn empty_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("empty decal array"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
pub(crate) fn empty_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
) -> wgpu::BindGroup {
    empty_group_with(device, layout, frame, &empty_buffer(device))
}
fn empty_group_with(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let texture = empty_texture(device);
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    group_with(
        device,
        layout,
        frame,
        buffer,
        &[view.clone(), view.clone(), view],
        &crate::upload::material_sampler(device),
    )
}
fn group_with(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
    decals: &wgpu::Buffer,
    views: &[wgpu::TextureView; 3],
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene frame and decals"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: decals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&views[0]),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&views[1]),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&views[2]),
            },
        ],
    })
}

fn selected(entries: &[Entry], matrix: Mat4, camera: Vec3) -> Vec<usize> {
    let frustum = Frustum::new(matrix);
    let mut selected: Vec<_> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| frustum.intersects(e.bounds))
        .map(|(i, _)| i)
        .collect();
    selected.sort_by(|&a, &b| {
        let a = &entries[a];
        let b = &entries[b];
        b.decal
            .priority
            .cmp(&a.decal.priority)
            .then_with(|| {
                a.model
                    .w_axis
                    .truncate()
                    .distance_squared(camera)
                    .total_cmp(&b.model.w_axis.truncate().distance_squared(camera))
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    selected.truncate(LIMIT);
    selected.sort_unstable();
    selected
}

#[cfg(test)]
mod tests;
