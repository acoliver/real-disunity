use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferUsages, Device, Queue, RenderPass, Sampler, Texture,
    TextureView, TextureViewDescriptor,
};
use worldrunner_render_plan::SceneTexture;
use worldrunner_render_program::runtime::{PreparedProgramDraw, TargetSignature};

use super::unlit::{QuadResources, UnlitRuntime, prepare_quads};
use super::validation::validate_quad;
use super::{DynamicError, DynamicQuad};
use crate::geometry::{BILLBOARD_FIXED, pack_model_instance, push_f32, push_vec3};
use crate::pipeline::MATERIAL_UNIFORM_BYTES;
use crate::renderer::ScenePipelines;
use crate::texture::prepare_texture;
use crate::upload::{MATERIAL_GROUP, material_group, upload_buffer, upload_texture};

/// What staging quads needs besides the quads: the scene's placeholder texture and sampler, and
/// where the quads are drawn.
pub(crate) struct DynamicBindings<'a> {
    pub placeholder: &'a Texture,
    pub sampler: &'a Sampler,
    pub target: QuadTarget<'a>,
}

/// Where staged quads are drawn.
pub(crate) enum QuadTarget<'a> {
    /// The scene's main pass, through the program runtime.
    Scene(SceneStaging<'a>),
    /// The retained-UI overlay, which keeps its own fixed pipeline over the material layout that
    /// pipeline was built with.
    Overlay(&'a BindGroupLayout),
}

/// What staging quads for the scene's main pass needs: the pipelines whose pass the first
/// preparation is for, the frame uniform of each frame slot and the runtime that prepares the
/// draws.
pub(crate) struct SceneStaging<'a> {
    pub pipelines: &'a ScenePipelines,
    pub frames: Vec<Buffer>,
    pub runtime: &'a mut UnlitRuntime,
    pub authority: &'a worldrunner_render_gpu::submission::SubmissionAuthority,
}

struct QuadBindings<'a> {
    placeholder: (&'a Texture, &'a TextureView),
    textures: (&'a [Texture], &'a [TextureView]),
    samplers: &'a [Sampler],
    sampler: &'a Sampler,
    indices: &'a Buffer,
    overlay_layout: Option<&'a BindGroupLayout>,
}

/// One instanced draw of adjacent compatible quads: what it draws with, and how many instances.
/// The scene's pass clips nothing, so the clip rectangle only keeps quads with different ones in
/// different draws; the overlay scissors each draw to it and binds `material`.
#[derive(Debug)]
struct QuadDraw {
    resources: Arc<QuadResources>,
    instance_count: u32,
    clip_rect: Option<[u32; 4]>,
    material: Option<BindGroup>,
}

/// The draws prepared for the pass a signature describes, a draw of each quad group for each
/// frame slot.
#[derive(Debug)]
struct Prepared {
    signature: TargetSignature,
    draws: Vec<Vec<PreparedProgramDraw>>,
}

#[derive(Debug, Default)]
pub(crate) struct DynamicGpu {
    textures: Vec<Texture>,
    samplers: Vec<Sampler>,
    draws: Vec<QuadDraw>,
    indices: Option<Buffer>,
    frames: Vec<Buffer>,
    prepared: Vec<Prepared>,
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
        self.prepared.clear();
        Ok(())
    }

    pub(crate) fn set_quads(
        &mut self,
        device: &Device,
        queue: &Queue,
        bindings: DynamicBindings<'_>,
        quads: &[DynamicQuad],
    ) -> Result<(), DynamicError> {
        let ordered = self.painter_order(device, quads)?;
        let placeholder = bindings
            .placeholder
            .create_view(&TextureViewDescriptor::default());
        let views: Vec<_> = self
            .textures
            .iter()
            .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
            .collect();
        let indices = self.quad_indices(device, queue);
        let (stage, overlay_layout) = match bindings.target {
            QuadTarget::Scene(stage) => (Some(stage), None),
            QuadTarget::Overlay(layout) => (None, Some(layout)),
        };
        let material_bindings = QuadBindings {
            placeholder: (bindings.placeholder, &placeholder),
            textures: (&self.textures, &views),
            samplers: &self.samplers,
            sampler: bindings.sampler,
            indices: &indices,
            overlay_layout,
        };
        let draws: Vec<_> = ordered
            .chunk_by(|a, b| {
                a.clip_rect == b.clip_rect
                    && a.texture == b.texture
                    && a.corners == b.corners
                    && a.rect.map(f32::to_bits) == b.rect.map(f32::to_bits)
                    && a.uv_rect.map(f32::to_bits) == b.uv_rect.map(f32::to_bits)
            })
            .map(|group| QuadDraw::upload(device, queue, &material_bindings, group))
            .collect();
        let (frames, prepared) = prepare_stage(device, stage, &draws)?;
        self.draws = draws;
        self.frames = frames;
        self.prepared = prepared;
        Ok(())
    }

    /// The quads in the order they paint, after checking them against the device's limits and
    /// the texture table. Equal keys keep their submission order.
    fn painter_order<'q>(
        &self,
        device: &Device,
        quads: &'q [DynamicQuad],
    ) -> Result<Vec<&'q DynamicQuad>, DynamicError> {
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
        ordered.sort_by_key(|quad| {
            (
                quad.overlay,
                quad.sorting_layer,
                quad.sorting_order,
                quad.hierarchy_order,
            )
        });
        Ok(ordered)
    }

    /// The index buffer of two triangles over a quad's four corners, made once and shared.
    fn quad_indices(&mut self, device: &Device, queue: &Queue) -> Buffer {
        self.indices
            .get_or_insert_with(|| {
                let indices: Vec<_> = [0u32, 1, 2, 0, 2, 3]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                upload_buffer(
                    device,
                    queue,
                    "dynamic quad indices",
                    BufferUsages::INDEX,
                    &indices,
                )
            })
            .clone()
    }

    /// Prepares the quads for the pass `pipelines` describe, if they are not yet, so the frame
    /// that draws into it records only state. Does nothing when there are no quads.
    ///
    /// # Errors
    /// Fails when the runtime refuses a draw for that pass.
    pub(crate) fn prepare_for(
        &mut self,
        runtime: &mut UnlitRuntime,
        pipelines: &ScenePipelines,
    ) -> Result<(), DynamicError> {
        if self.draws.is_empty() {
            return Ok(());
        }
        let signature = pipelines.unlit_signature()?;
        if self.prepared.iter().any(|set| set.signature == signature) {
            return Ok(());
        }
        let set = prepare_signature(
            None,
            runtime,
            pipelines,
            signature,
            (&self.frames, &self.draws),
        )?;
        self.prepared.push(set);
        Ok(())
    }

    /// Weak references to what the staged draws lend the runtime, for tests of its lifetime.
    #[cfg(test)]
    pub(crate) fn probes(&self) -> Vec<std::sync::Weak<dyn std::any::Any + Send + Sync>> {
        self.draws
            .iter()
            .map(|draw| {
                let owner: Arc<dyn std::any::Any + Send + Sync> = draw.resources.clone();
                Arc::downgrade(&owner)
            })
            .collect()
    }

    fn prepared_for(&self, signature: TargetSignature) -> Option<&Prepared> {
        self.prepared.iter().find(|set| set.signature == signature)
    }

    /// Makes the frame `encoder` records hold every prepared draw for the pass `pipelines`
    /// describe, and so their owners, until the frame is cancelled, dropped or completed on
    /// the GPU. A draw of each frame slot is held, since which slot a frame draws with is not
    /// settled until its transaction commits.
    ///
    /// # Errors
    /// Fails when the quads are not prepared for the pass, or the encoder is not a frame being
    /// recorded by the draws' authority.
    pub(crate) fn retain(
        &self,
        encoder: &wgpu::CommandEncoder,
        pipelines: &ScenePipelines,
    ) -> Result<(), DynamicError> {
        if self.draws.is_empty() {
            return Ok(());
        }
        let prepared = pipelines
            .unlit_signature()
            .ok()
            .and_then(|signature| self.prepared_for(signature))
            .ok_or(DynamicError::NotPrepared)?;
        for draw in prepared.draws.iter().flatten() {
            draw.retain(encoder)?;
        }
        Ok(())
    }

    /// Records the prepared draws for `slot`'s frame uniform into `pass`.
    ///
    /// # Panics
    /// Panics when the quads were not prepared for the pass `pipelines` describe. Staging them
    /// prepares them for the pipelines it is given, and the frame entry points prepare or refuse
    /// before they open a pass.
    pub(crate) fn draw(&self, pass: &mut RenderPass<'_>, slot: usize, pipelines: &ScenePipelines) {
        if self.draws.is_empty() {
            return;
        }
        let prepared = pipelines
            .unlit_signature()
            .ok()
            .and_then(|signature| self.prepared_for(signature))
            .expect("dynamic quads are prepared for the pass they draw into");
        for draw in &prepared.draws {
            draw[slot].record(pass);
        }
    }

    /// Draws the quads into the overlay's pass with its own fixed `pipeline`, each draw clipped
    /// to its rectangle, or to the whole `viewport` without one, and not at all when the
    /// rectangle leaves nothing of it. Returns the draw calls and instances recorded.
    ///
    /// # Panics
    /// Panics when the quads were not staged for the overlay.
    pub(crate) fn draw_overlay(
        &self,
        pass: &mut RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        viewport: [u32; 2],
    ) -> crate::draw::DrawCount {
        let mut count = crate::draw::DrawCount::default();
        if self.draws.is_empty() {
            return count;
        }
        pass.set_pipeline(pipeline);
        pass.set_index_buffer(
            self.indices
                .as_ref()
                .expect("resident quads have indices")
                .slice(..),
            wgpu::IndexFormat::Uint32,
        );
        for draw in &self.draws {
            if !set_scissor(pass, draw.clip_rect, viewport) {
                continue;
            }
            let material = draw
                .material
                .as_ref()
                .expect("overlay quads carry a material group");
            pass.set_bind_group(MATERIAL_GROUP, material, &[]);
            pass.set_vertex_buffer(0, draw.resources.vertices.slice(..));
            pass.set_vertex_buffer(1, draw.resources.instance.slice(..));
            pass.set_vertex_buffer(2, draw.resources.tint.slice(..));
            pass.draw_indexed(0..6, 0, 0..draw.instance_count);
            count.draw(u64::from(draw.instance_count));
        }
        count
    }
}

/// The frame uniforms of the scene's slots and the draws prepared for the pass its pipelines
/// describe, when the quads are for the scene's pass and there are any. The overlay has neither.
fn prepare_stage(
    device: &Device,
    stage: Option<SceneStaging<'_>>,
    draws: &[QuadDraw],
) -> Result<(Vec<Buffer>, Vec<Prepared>), DynamicError> {
    let Some(stage) = stage else {
        return Ok((Vec::new(), Vec::new()));
    };
    if draws.is_empty() {
        return Ok((stage.frames, Vec::new()));
    }
    let signature = stage.pipelines.unlit_signature()?;
    let set = prepare_signature(
        Some((device, stage.authority)),
        stage.runtime,
        stage.pipelines,
        signature,
        (&stage.frames, draws),
    )?;
    Ok((stage.frames, vec![set]))
}

/// A prepared draw of every quad group for each frame slot, for the pass `signature` describes.
fn prepare_signature(
    context: Option<(
        &Device,
        &worldrunner_render_gpu::submission::SubmissionAuthority,
    )>,
    runtime: &mut UnlitRuntime,
    pipelines: &ScenePipelines,
    signature: TargetSignature,
    (frames, draws): (&[Buffer], &[QuadDraw]),
) -> Result<Prepared, DynamicError> {
    let (runtime, program) = runtime.program(context, pipelines.unlit_source())?;
    let draws = draws
        .iter()
        .map(|draw| {
            prepare_quads(
                runtime,
                &program,
                signature,
                frames,
                (&draw.resources, draw.instance_count),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Prepared { signature, draws })
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
        let (albedo, albedo_view) = quad.texture.map_or(bindings.placeholder, |index| {
            (&bindings.textures.0[index], &bindings.textures.1[index])
        });
        let sampler = quad
            .texture
            .map_or(bindings.sampler, |index| &bindings.samplers[index]);
        let resources = QuadResources {
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
            material: upload_buffer(
                device,
                queue,
                "dynamic material",
                BufferUsages::UNIFORM,
                &[0u8; MATERIAL_UNIFORM_BYTES],
            ),
            albedo: albedo.clone(),
            albedo_view: albedo_view.clone(),
            sampler: sampler.clone(),
            indices: bindings.indices.clone(),
        };
        let material = bindings.overlay_layout.map(|layout| {
            overlay_material(device, queue, layout, (&resources, bindings.placeholder.1))
        });
        Self {
            resources: Arc::new(resources),
            instance_count: u32::try_from(group.len()).expect("quad count was validated"),
            clip_rect: quad.clip_rect,
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

/// The overlay's material group for one draw: the draw's texture and sampler over the shared
/// zero material block and the placeholder in the slots the unlit shader does not read.
fn overlay_material(
    device: &Device,
    queue: &Queue,
    layout: &BindGroupLayout,
    (resources, placeholder): (&QuadResources, &TextureView),
) -> BindGroup {
    let mut bound = [placeholder; 5];
    bound[0] = &resources.albedo_view;
    let heights = upload_buffer(
        device,
        queue,
        "dynamic contact heights",
        BufferUsages::STORAGE,
        &[0u8; 48],
    );
    material_group(
        device,
        layout,
        &resources.material,
        &bound,
        &resources.sampler,
        &heights,
    )
}

/// Clips the next draws to `clip`, a lower-left pixel rectangle, or to the whole target without
/// one. Returns whether anything of the target is left to draw into.
fn set_scissor(pass: &mut RenderPass<'_>, clip: Option<[u32; 4]>, size: [u32; 2]) -> bool {
    let [x, y, width, height] = clip.unwrap_or([0, 0, size[0], size[1]]);
    let width = width.min(size[0].saturating_sub(x));
    let height = height.min(size[1].saturating_sub(y));
    if width == 0 || height == 0 {
        return false;
    }
    pass.set_scissor_rect(x, size[1] - y - height, width, height);
    true
}
