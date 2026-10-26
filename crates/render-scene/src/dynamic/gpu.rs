use glam::{Mat4, Quat, Vec3};
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferUsages, Device, Queue, RenderPass, Sampler, Texture,
    TextureView, TextureViewDescriptor,
};
use worldrunner_render_plan::SceneTexture;

use super::validation::validate_quad;
use super::{DynamicError, DynamicQuad};
use crate::geometry::{BILLBOARD_FIXED, pack_model_instance, push_f32, push_vec3};
use crate::pipeline::MATERIAL_UNIFORM_BYTES;
use crate::renderer::ScenePipelines;
use crate::texture::prepare_texture;
use crate::upload::{FRAME_GROUP, MATERIAL_GROUP, upload_buffer, upload_texture};

#[derive(Clone, Copy)]
pub(crate) struct DynamicBindings<'a> {
    pub pipelines: &'a ScenePipelines,
    pub placeholder: &'a Texture,
    pub sampler: &'a Sampler,
}

struct QuadBindings<'a> {
    layout: &'a BindGroupLayout,
    placeholder: &'a TextureView,
    views: &'a [TextureView],
    samplers: &'a [Sampler],
    sampler: &'a Sampler,
}

#[derive(Debug)]
struct QuadDraw {
    vertices: Buffer,
    instance: Buffer,
    tint: Buffer,
    instance_count: u32,
    material: BindGroup,
}

#[derive(Debug, Default)]
pub(crate) struct DynamicGpu {
    textures: Vec<Texture>,
    samplers: Vec<Sampler>,
    draws: Vec<QuadDraw>,
    indices: Option<Buffer>,
}

impl DynamicGpu {
    pub(crate) fn set_textures(
        &mut self,
        device: &Device,
        queue: &Queue,
        textures: &[SceneTexture],
    ) -> Result<(), DynamicError> {
        let prepared = textures
            .iter()
            .enumerate()
            .map(|(index, texture)| {
                texture
                    .validate()
                    .map_err(|source| DynamicError::TextureDomain { index, source })?;
                let prepared = prepare_texture(texture)
                    .map_err(|source| DynamicError::Texture { index, source })?;
                super::validation::validate_texture(device, &prepared)?;
                Ok(prepared)
            })
            .collect::<Result<Vec<_>, DynamicError>>()?;
        let uploaded = prepared
            .iter()
            .map(|texture| upload_texture(device, queue, texture))
            .collect::<Result<Vec<_>, _>>()?;
        self.samplers = textures
            .iter()
            .map(|t| super::sampling::create_sampler(device, t.sampler()))
            .collect();
        self.textures = uploaded;
        self.draws.clear();
        Ok(())
    }

    pub(crate) fn set_quads(
        &mut self,
        device: &Device,
        queue: &Queue,
        bindings: DynamicBindings<'_>,
        quads: &[DynamicQuad],
    ) -> Result<(), DynamicError> {
        let count =
            u64::try_from(quads.len()).map_err(|_| DynamicError::DeviceLimit("quad count"))?;
        if count > u64::from(u32::MAX)
            || count.saturating_mul(crate::geometry::INSTANCE_STRIDE)
                > device.limits().max_buffer_size
        {
            return Err(DynamicError::DeviceLimit("instance buffer size"));
        }
        for (index, quad) in quads.iter().enumerate() {
            validate_quad(quad, index, self.textures.len())?;
        }
        let mut ordered: Vec<_> = quads.iter().collect();
        ordered.sort_by_key(|quad| (quad.overlay, quad.sorting_layer, quad.sorting_order));
        let placeholder = bindings
            .placeholder
            .create_view(&TextureViewDescriptor::default());
        let views: Vec<_> = self
            .textures
            .iter()
            .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
            .collect();
        let mut draws = Vec::with_capacity(ordered.len());
        let material_bindings = QuadBindings {
            layout: bindings.pipelines.material_layout(),
            placeholder: &placeholder,
            views: &views,
            samplers: &self.samplers,
            sampler: bindings.sampler,
        };
        for group in ordered.chunk_by(|a, b| {
            a.texture == b.texture
                && a.corners == b.corners
                && a.rect.map(f32::to_bits) == b.rect.map(f32::to_bits)
                && a.uv_rect.map(f32::to_bits) == b.uv_rect.map(f32::to_bits)
        }) {
            draws.push(QuadDraw::upload(device, queue, &material_bindings, group));
        }
        if self.indices.is_none() && !draws.is_empty() {
            let indices: Vec<_> = [0u32, 1, 2, 0, 2, 3]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            self.indices = Some(upload_buffer(
                device,
                queue,
                "dynamic quad indices",
                BufferUsages::INDEX,
                &indices,
            ));
        }
        self.draws = draws;
        Ok(())
    }

    pub(crate) fn draw(
        &self,
        pass: &mut RenderPass<'_>,
        frame_group: &BindGroup,
        pipelines: &ScenePipelines,
    ) {
        if self.draws.is_empty() {
            return;
        }
        pass.set_pipeline(pipelines.unlit_alpha());
        pass.set_bind_group(FRAME_GROUP, frame_group, &[]);
        pass.set_index_buffer(
            self.indices
                .as_ref()
                .expect("resident quads have indices")
                .slice(..),
            wgpu::IndexFormat::Uint32,
        );
        for draw in &self.draws {
            pass.set_bind_group(MATERIAL_GROUP, &draw.material, &[]);
            pass.set_vertex_buffer(0, draw.vertices.slice(..));
            pass.set_vertex_buffer(1, draw.instance.slice(..));
            pass.set_vertex_buffer(2, draw.tint.slice(..));
            pass.draw_indexed(0..6, 0, 0..draw.instance_count);
        }
    }
}

impl QuadDraw {
    fn upload(
        device: &Device,
        queue: &Queue,
        bindings: &QuadBindings<'_>,
        group: &[&DynamicQuad],
    ) -> Self {
        let quad = group[0];
        let vertices = pack_quad(quad);
        let mut instances = Vec::new();
        let mut tints = Vec::new();
        for quad in group {
            let model = Mat4::from_scale_rotation_translation(
                Vec3::from_array(quad.scale),
                Quat::from_array(quad.rotation).normalize(),
                Vec3::from_array(quad.position),
            );
            instances.extend_from_slice(&pack_model_instance(model));
            for channel in quad.tint {
                push_f32(&mut tints, channel);
            }
        }
        let albedo = quad
            .texture
            .map_or(bindings.placeholder, |index| &bindings.views[index]);
        let sampler = quad
            .texture
            .map_or(bindings.sampler, |index| &bindings.samplers[index]);
        let material = quad_material(device, queue, bindings, albedo, sampler);
        Self {
            vertices: upload_buffer(
                device,
                queue,
                "dynamic quad",
                BufferUsages::VERTEX,
                &vertices,
            ),
            instance: upload_buffer(
                device,
                queue,
                "dynamic instances",
                BufferUsages::VERTEX,
                &instances,
            ),
            tint: upload_buffer(device, queue, "dynamic tints", BufferUsages::VERTEX, &tints),
            instance_count: u32::try_from(group.len()).expect("quad count was validated"),
            material,
        }
    }
}

fn pack_quad(quad: &DynamicQuad) -> Vec<u8> {
    let [left, bottom, right, top] = quad.rect;
    let [u0, v0, u1, v1] = quad.uv_rect;
    let mut bytes = Vec::with_capacity(4 * crate::geometry::VERTEX_BYTES);
    for (index, (x, z, u, v)) in [
        (left, bottom, u0, v1),
        (right, bottom, u1, v1),
        (right, top, u1, v0),
        (left, top, u0, v0),
    ]
    .into_iter()
    .enumerate()
    {
        let position = quad.corners.map_or(Vec3::new(x, 0., z), |corners| {
            Vec3::from_array(corners[index])
        });
        push_vec3(&mut bytes, position);
        push_vec3(&mut bytes, Vec3::NEG_Y);
        push_f32(&mut bytes, u);
        push_f32(&mut bytes, v);
        bytes.extend_from_slice(&BILLBOARD_FIXED.to_le_bytes());
        push_f32(&mut bytes, 1.0);
        bytes.extend_from_slice(&[0; 16]);
    }
    bytes
}

fn quad_material(
    device: &Device,
    queue: &Queue,
    bindings: &QuadBindings<'_>,
    albedo: &TextureView,
    sampler: &Sampler,
) -> wgpu::BindGroup {
    let mut bound = [bindings.placeholder; 5];
    bound[0] = albedo;
    let uniform = upload_buffer(
        device,
        queue,
        "dynamic material",
        BufferUsages::UNIFORM,
        &[0u8; MATERIAL_UNIFORM_BYTES],
    );
    let heights = upload_buffer(
        device,
        queue,
        "dynamic contact heights",
        BufferUsages::STORAGE,
        &[0u8; 48],
    );
    crate::upload::material_group(device, bindings.layout, &uniform, &bound, sampler, &heights)
}
