//! The fixed unlit-alpha path as it was before dynamic quads drew through the program runtime,
//! with the clip rectangles and triangles the retained-UI overlay added to it.

use super::*;

/// The fixed unlit-alpha path as it was before dynamic quads drew through the program runtime.
/// [`Legacy::draw`] is the scene's pass, which clips nothing; [`Legacy::draw_scissored`] is the
/// same draws with the scissor the fixed path set for the overlay.
pub(super) struct Legacy {
    pipeline: RenderPipeline,
    draws: Vec<LegacyDraw>,
    indices: Buffer,
    frame: BindGroup,
}

struct LegacyDraw {
    vertices: Buffer,
    instance: Buffer,
    tint: Buffer,
    count: u32,
    index_count: u32,
    material: BindGroup,
    clip_rect: Option<[u32; 4]>,
}

/// What every group of quads shares: the material layout, the placeholder texel, the uploaded
/// textures and the sampler of each and the scene's default sampler.
struct Shared<'a> {
    layout: &'a wgpu::BindGroupLayout,
    placeholder: &'a TextureView,
    views: &'a [TextureView],
    samplers: &'a [wgpu::Sampler],
    sampler: &'a wgpu::Sampler,
}

impl LegacyDraw {
    fn new(
        (device, queue): (&wgpu::Device, &wgpu::Queue),
        bindings: &Shared<'_>,
        group: &[&DynamicQuad],
    ) -> Self {
        let quad = group[0];
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
        let mut bound = [bindings.placeholder; 5];
        bound[0] = albedo;
        let uniform = upload_buffer(
            device,
            queue,
            "legacy material",
            BufferUsages::UNIFORM,
            &[0u8; MATERIAL_UNIFORM_BYTES],
        );
        let heights = upload_buffer(
            device,
            queue,
            "legacy heights",
            BufferUsages::STORAGE,
            &[0u8; 48],
        );
        LegacyDraw {
            vertices: upload_buffer(
                device,
                queue,
                "legacy quad",
                BufferUsages::VERTEX,
                &pack_quad(quad),
            ),
            instance: upload_buffer(
                device,
                queue,
                "legacy instances",
                BufferUsages::VERTEX,
                &instances,
            ),
            tint: upload_buffer(device, queue, "legacy tints", BufferUsages::VERTEX, &tints),
            count: u32::try_from(group.len()).unwrap(),
            material: material_group(device, bindings.layout, &uniform, &bound, sampler, &heights),
            clip_rect: quad.clip_rect,
            index_count: if quad.triangle_uv.is_some() { 3 } else { 6 },
        }
    }
}

impl Legacy {
    pub(super) fn new(
        context: &GpuContext,
        pipelines: &ScenePipelines,
        textures: &[SceneTexture],
        quads: &[DynamicQuad],
        view: &SceneView,
    ) -> Self {
        let (device, queue) = (context.device(), context.queue());
        let uploaded: Vec<Texture> = textures
            .iter()
            .map(|texture| {
                upload_texture(device, queue, &prepare_texture(texture).unwrap()).unwrap()
            })
            .collect();
        let samplers: Vec<_> = textures
            .iter()
            .map(|texture| super::super::sampling::create_sampler(device, texture.sampler()))
            .collect();
        let views: Vec<_> = uploaded
            .iter()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();
        let (placeholder, placeholder_view) = placeholder_with_view(device, queue);
        let shared = material_sampler(device);
        let layout = material_bind_group_layout(device);
        let ordered = painter_order(quads);
        let shared = Shared {
            layout: &layout,
            placeholder: &placeholder_view,
            views: &views,
            samplers: &samplers,
            sampler: &shared,
        };
        let draws = ordered
            .chunk_by(|a, b| {
                a.clip_rect == b.clip_rect
                    && a.texture == b.texture
                    && a.corners == b.corners
                    && a.triangle_uv == b.triangle_uv
                    && a.rect.map(f32::to_bits) == b.rect.map(f32::to_bits)
                    && a.uv_rect.map(f32::to_bits) == b.uv_rect.map(f32::to_bits)
            })
            .map(|group| LegacyDraw::new((device, queue), &shared, group))
            .collect();
        drop(placeholder);
        let indices: Vec<u8> = [0u32, 1, 2, 0, 2, 3]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let frame_bytes = upload_buffer(
            device,
            queue,
            "legacy frame",
            BufferUsages::UNIFORM,
            &FrameUniform::unlit(view).to_bytes(),
        );
        Self {
            pipeline: pipelines.legacy_unlit_alpha(),
            draws,
            indices: upload_buffer(
                device,
                queue,
                "legacy indices",
                BufferUsages::INDEX,
                &indices,
            ),
            frame: crate::draw::decals::empty_group(device, pipelines.frame_layout(), &frame_bytes),
        }
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.record(pass, None);
    }

    /// The draws with each one clipped to its rectangle in a target of `viewport` pixels, and
    /// the calls and instances the fixed path counted.
    pub(super) fn draw_scissored(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        viewport: [u32; 2],
    ) -> DrawCount {
        self.record(pass, Some(viewport))
    }

    /// The fixed unlit pipeline, for drawing other staged quads with it.
    pub(super) fn pipeline(&self) -> &RenderPipeline {
        &self.pipeline
    }

    /// The frame uniform group the fixed unlit pipeline reads.
    pub(super) fn frame_group(&self) -> &BindGroup {
        &self.frame
    }

    fn record(&self, pass: &mut wgpu::RenderPass<'_>, viewport: Option<[u32; 2]>) -> DrawCount {
        let mut count = DrawCount::default();
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame, &[]);
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for draw in &self.draws {
            if viewport.is_some_and(|size| !set_scissor(pass, draw.clip_rect, size)) {
                continue;
            }
            pass.set_bind_group(1, &draw.material, &[]);
            pass.set_vertex_buffer(0, draw.vertices.slice(..));
            pass.set_vertex_buffer(1, draw.instance.slice(..));
            pass.set_vertex_buffer(2, draw.tint.slice(..));
            pass.draw_indexed(0..draw.index_count, 0, 0..draw.count);
            count.draw(u64::from(draw.count));
        }
        count
    }
}

/// The quads in the order the fixed path painted them: overlays last, then by layer, order and
/// hierarchy, with submission order between equal keys.
fn painter_order(quads: &[DynamicQuad]) -> Vec<&DynamicQuad> {
    let mut ordered: Vec<_> = quads.iter().collect();
    ordered.sort_by_key(|quad| {
        (
            quad.overlay,
            quad.sorting_layer,
            quad.sorting_order,
            quad.hierarchy_order,
        )
    });
    ordered
}

/// The fixed path's scissor: the lower-left rectangle clamped to the target, flipped to the
/// target's top-left origin, or the whole target without one. False when nothing is left of it.
fn set_scissor(pass: &mut wgpu::RenderPass<'_>, clip: Option<[u32; 4]>, size: [u32; 2]) -> bool {
    let [x, y, width, height] = clip.unwrap_or([0, 0, size[0], size[1]]);
    let width = width.min(size[0].saturating_sub(x));
    let height = height.min(size[1].saturating_sub(y));
    if width == 0 || height == 0 {
        return false;
    }
    pass.set_scissor_rect(x, size[1] - y - height, width, height);
    true
}

fn pack_quad(quad: &DynamicQuad) -> Vec<u8> {
    let [left, bottom, right, top] = quad.rect;
    let [u0, v0, u1, v1] = quad.uv_rect;
    let mut bytes = Vec::new();
    for (index, (x, z, u, v)) in [
        (left, bottom, u0, v1),
        (right, bottom, u1, v1),
        (right, top, u1, v0),
        (left, top, u0, v0),
    ]
    .into_iter()
    .enumerate()
    {
        if quad.triangle_uv.is_some() && index == 3 {
            break;
        }
        let [u, v] = quad.triangle_uv.map_or([u, v], |uv| uv[index]);
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
