//! The terrain shadow caster: depth-only passes over the light frames of the four cascades, drawn
//! through the scene's vertex-only program and through the frozen fixed caster, with the depth
//! texels read back and compared byte for byte. The terrain is drawn whole, and streamed with the
//! cascades' volumes and a slice hull selecting which chunks each map receives.

use glam::{Mat4, Vec3};
use wgpu::{BindGroup, Buffer, RenderPipeline, TextureFormat};

use super::{Rig, Source, copy_plane, scenes, streaming::settle};
use crate::device::GpuContext;
use crate::draw::{DrawCount, SceneGpu, SliceHull};
use crate::dynamic::capture_tests::{CONFIGS, SIZE, build, context, digest, read_results};
use crate::frame::FrameUniform;
use crate::renderer::ScenePipelines;
use crate::renderer::legacy_terrain::{
    Chunk, Geometry, build_terrain_depth, draw_terrain_depth as draw_fixed,
};
use crate::shadows::{DirectionalShadowSettings, TerrainCaster};

/// A top-down orthographic map over a square of `half` metres about `centre`.
fn map(centre: (f32, f32), half: f32) -> Mat4 {
    let eye = Vec3::new(centre.0, centre.1, 6.0);
    glam::camera::rh::proj::directx::orthographic(-half, half, -half, half, 0.5, 12.0)
        * glam::camera::rh::view::look_at_mat4(eye, Vec3::new(centre.0, centre.1, 0.0), Vec3::Y)
}

/// The four cascades: two quadrants, the middle, and the whole terrain.
pub(in crate::terrain) fn cascades() -> [Mat4; 4] {
    [
        map((-2.0, -2.0), 2.6),
        map((2.0, 1.5), 3.0),
        map((0.0, 0.0), 3.2),
        map((0.0, 0.0), 7.0),
    ]
}

/// The caster policy a capture draws with: the depth test of the scene's casters and a depth
/// bias with a constant and a slope, so that the bias shows in the depth texels.
pub(in crate::terrain) fn configuration(
    bias: wgpu::DepthBiasState,
) -> worldrunner_render_shadow::DirectionalShadowConfig {
    let mut configuration = DirectionalShadowSettings {
        focus_meters: Vec3::ZERO,
        half_extent_meters: 4.0,
        depth_half_extent_meters: 4.0,
        resolution: 64,
        depth_bias: 0.0,
        normal_bias_meters: 0.0,
        refresh_policy: crate::shadows::CascadeRefreshPolicy::default(),
    }
    .configuration();
    configuration.caster_bias = bias;
    configuration
}

pub(in crate::terrain) const BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: 2,
    slope_scale: 1.5,
    clamp: 0.0,
};

/// The scene's caster, the fixed caster, and the light frame of each cascade for both.
pub(in crate::terrain) struct Casting {
    pub caster: TerrainCaster,
    pub fixed: RenderPipeline,
    pub frames: [(Buffer, BindGroup); 4],
    pub matrices: [Mat4; 4],
}

impl Casting {
    pub(in crate::terrain) fn new(
        context: &GpuContext,
        pipelines: &ScenePipelines,
        frame: &FrameUniform,
        bias: wgpu::DepthBiasState,
    ) -> Self {
        let device = context.device();
        let configuration = configuration(bias);
        let matrices = cascades();
        let frames = matrices.map(|matrix| {
            let buffer = crate::upload::upload_buffer(
                device,
                context.queue(),
                "capture shadow frame",
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
                &crate::shadows::light_frame(frame, matrix).to_bytes(),
            );
            let group = crate::draw::decals::empty_group(device, pipelines.frame_layout(), &buffer);
            (buffer, group)
        });
        Self {
            caster: TerrainCaster::new(
                &configuration,
                pipelines.terrain_context().family(),
                (device, context.submission()),
            ),
            fixed: build_terrain_depth(device, pipelines.frame_layout(), configuration),
            frames,
            matrices,
        }
    }

    /// What the fixed caster draws into cascade `index`, recorded into `pass`: nothing when the
    /// cascade's volume misses the terrain.
    pub(in crate::terrain) fn fixed_draw<C: crate::renderer::legacy_terrain::Commands>(
        &self,
        pass: &mut C,
        (context, gpu): (&GpuContext, &SceneGpu),
        index: usize,
    ) -> DrawCount {
        let Some(keep) =
            gpu.terrain_caster_selection(u32::try_from(index).unwrap(), self.matrices[index])
        else {
            return DrawCount::default();
        };
        let inputs = gpu
            .legacy_inputs(context.device(), context.queue())
            .expect("the scene has terrain");
        let chunks;
        let geometry = if inputs.streaming {
            chunks = gpu.legacy_chunks();
            Geometry::Streaming(&chunks)
        } else {
            let (vertices, indices, index_count) = inputs.mesh.as_ref().expect("a mesh");
            Geometry::Mesh {
                vertices,
                indices,
                index_count: *index_count,
            }
        };
        draw_fixed(
            pass,
            (&self.fixed, &self.frames[index].1),
            (&geometry, &inputs.instances),
            |chunk: &Chunk| keep(chunk.bounds),
        )
    }
}

/// Renders a depth-only pass over a map cleared to the far plane and returns its depth texels.
pub(in crate::terrain) fn render_map(
    context: &GpuContext,
    draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
) -> Vec<u8> {
    let device = context.device();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain caster capture"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = context.submission().begin(device).unwrap();
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain caster capture"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..wgpu::RenderPassDescriptor::default()
        });
        draw(&mut pass);
    }
    let staging = copy_plane(
        device,
        &mut encoder,
        &texture,
        wgpu::TextureAspect::DepthOnly,
    );
    let index = encoder.finish().submit().unwrap();
    read_results(device, index, vec![staging]).remove(0)
}

/// One cascade through the scene and through the fixed caster: the depth texels and the draw
/// calls must agree, and terrain must have drawn something when it drew a call.
fn capture_map(
    context: &GpuContext,
    (rig, casting): (&Rig<'_>, &Casting),
    (name, index): (&str, usize),
) -> (Vec<u8>, DrawCount) {
    let pipeline = casting
        .caster
        .pipeline(&casting.frames[index].0)
        .expect("the caster prepares against its light frame");
    let mut scene_count = DrawCount::default();
    let scene = render_map(context, |pass| {
        scene_count = rig.gpu.draw_terrain_depth(
            pass,
            &pipeline,
            (u32::try_from(index).unwrap(), casting.matrices[index]),
        );
    });
    let mut fixed_count = DrawCount::default();
    let fixed = render_map(context, |pass| {
        fixed_count = casting.fixed_draw(pass, (context, &rig.gpu), index);
    });
    let untouched = render_map(context, |_| {});
    assert!(
        scene == fixed,
        "{name} cascade {index}: the depth texels differ from the fixed caster's"
    );
    assert_eq!(
        scene_count, fixed_count,
        "{name} cascade {index}: draw calls"
    );
    assert_eq!(
        scene != untouched,
        scene_count.calls > 0,
        "{name} cascade {index}: terrain wrote depth exactly when it drew"
    );
    println!(
        "TERRAIN-CAPTURE adapter={:?} config=shadow-caster-{name}-cascade{index} depth=independent source=plain attachment=depth bytes={} hash={:016x} calls={}",
        context.adapter().get_info().name,
        scene.len(),
        digest(&scene),
        scene_count.calls
    );
    (scene, scene_count)
}

#[test]
fn the_whole_terrain_caster_matches_the_fixed_caster_byte_for_byte_in_every_cascade() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let frame = scenes::frame(scenes::pose());
    let rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::plain(),
        (Source::Plain, frame),
    );
    let mut whole = Vec::new();
    for (name, bias) in [("plain", wgpu::DepthBiasState::default()), ("biased", BIAS)] {
        let casting = Casting::new(&context, &built.pipelines, &frame, bias);
        let maps: Vec<_> = (0..4)
            .map(|index| capture_map(&context, (&rig, &casting), (name, index)))
            .collect();
        assert!(maps.iter().any(|(_, count)| count.calls == 1));
        whole.push(maps.into_iter().map(|(map, _)| map).collect::<Vec<_>>());
    }
    assert!(
        whole[0]
            .iter()
            .zip(&whole[1])
            .any(|(plain, biased)| plain != biased),
        "the depth bias moved no texel, so the captures cannot see it"
    );
    println!("TERRAIN-CAPTURE-SUMMARY set=shadow-caster mesh cascades=4 biases=2");
}

/// Settles a streamed terrain and captures its four cascades; returns the chunks each drew.
fn streamed_chunks(context: &GpuContext, hull: Option<(usize, &[Vec3])>) -> Vec<u64> {
    let built = build(context.device(), &CONFIGS[2]);
    let frame = scenes::frame(scenes::pose());
    let rig = Rig::new(
        context,
        &built.pipelines,
        &scenes::streamed(9),
        (Source::Plain, frame),
    );
    settle(context, &rig.gpu, &frame, 6);
    let casting = Casting::new(context, &built.pipelines, &frame, BIAS);
    let name = if hull.is_some() {
        "streamed-hull"
    } else {
        "streamed"
    };
    if let Some((index, corners)) = hull {
        let matrix = casting.matrices[index];
        rig.gpu.cascade_slice(
            index,
            matrix,
            Some(&SliceHull::new(corners, matrix, 0.0)),
            0.0,
        );
    }
    (0..4)
        .map(|index| {
            capture_map(context, (&rig, &casting), (name, index))
                .1
                .calls
        })
        .collect()
}

#[test]
fn the_streamed_terrain_caster_matches_the_fixed_caster_byte_for_byte_under_volume_and_slice_culling()
 {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let volume = streamed_chunks(&context, None);
    println!("TERRAIN-CAPTURE-SUMMARY set=shadow-caster streamed chunks_per_cascade={volume:?}");
    assert!(
        volume.iter().any(|&calls| calls > 1),
        "no cascade drew more than one chunk: {volume:?}"
    );
    assert!(
        volume.iter().any(|&calls| calls < volume[3]),
        "no cascade's volume culled a chunk the whole-terrain cascade draws: {volume:?}"
    );
    // A hull over one corner of the middle cascade's map keeps only the chunks that reach it.
    let corner = [
        Vec3::new(-3.0, -3.0, 0.0),
        Vec3::new(-1.8, -3.0, 0.0),
        Vec3::new(-1.8, -1.8, 0.0),
        Vec3::new(-3.0, -1.8, 0.0),
    ];
    let sliced = streamed_chunks(&context, Some((2, &corner)));
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=shadow-caster streamed-hull chunks_per_cascade={sliced:?}"
    );
    assert!(
        sliced[2] < volume[2],
        "the slice hull culled nothing from cascade two: {sliced:?} against {volume:?}"
    );
}
